//! プロジェクトのコマンド(`src-tauri/src/commands/project.rs`、計画書 T33c)を、Tauri の模擬ランタイムで画面から呼ぶ形で確かめる。
//! FR-001(ダイアログ・ドロップ)、FR-002、FR-003、FR-007(再リンク)、FR-010、FR-059、FR-064。
//!
//! - フォルダ選択のダイアログは偽物(決めた答えを順に返す)。本物の `NSOpenPanel` は開かない
//! - ペーストボードは名前付きのもの(`MacPasteboard::named`)。ユーザーのクリップボードには触れない。最後に解放する
//! - フォルダの正規化・ブックマーク・状態ファイルは本物(`arin-platform`)で、すべて一時フォルダの中で行う
//! - PTY は偽物。前面のプロセスを「シェル以外」にでき、削除の確認(動作中)を起こせる
//! - 確認は順に行い、前の確認で作ったプロジェクトを次で使う
//!
//! macOS ではメニューの部品をメインスレッドでしか作れないため、`harness = false` で走らせる(`app_builder` と同じ)。

mod support;

use std::cell::Cell;
use std::collections::VecDeque;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use arin_core::ports::{ChildExit, Foreground, PtyError, PtyPort, PtySession, PtySize, SpawnedPty};
use arin_core::project::ProjectColor;
use arin_core::terminal::ShellLaunchSpec;
use arin_core::workspace::{
    AddProjectOutcome, AppSnapshot, CloseTabsOutcome, RemoveProjectOutcome,
};
use arin_lib::persistence::PersistencePorts;
use arin_lib::projects::{FolderPicker, FolderPurpose, ProjectPorts};
use arin_lib::sessions::ShellDefaults;
use arin_platform::desktop::MacPasteboard;
use arin_platform::system::UrandomIdSource;
use serde_json::json;
use support::{request, request_with};
use tauri::ipc::{InvokeBody, InvokeResponseBody};
use tauri::test::{MockRuntime, get_ipc_response};
use tauri::{AppHandle, Manager, RunEvent, WebviewWindow};

const WAIT: Duration = Duration::from_secs(10);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn wait_until(mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        thread::sleep(Duration::from_millis(2));
    }
    condition()
}

// ---------------------------------------------------------------------------
// 偽のダイアログ
// ---------------------------------------------------------------------------

/// 決めた答えを順に返すダイアログ。何の目的で開かれたかを記録する。
#[derive(Default)]
struct FakePicker {
    answers: Mutex<VecDeque<Option<PathBuf>>>,
    opened: Mutex<Vec<FolderPurpose>>,
}

impl FakePicker {
    fn answer(&self, path: Option<&Path>) {
        lock(&self.answers).push_back(path.map(Path::to_path_buf));
    }
}

impl FolderPicker for FakePicker {
    fn pick_folder(&self, purpose: FolderPurpose) -> Result<Option<PathBuf>, String> {
        lock(&self.opened).push(purpose);
        lock(&self.answers)
            .pop_front()
            .ok_or_else(|| "答えを用意していないのにダイアログが開かれた".to_owned())
    }
}

// ---------------------------------------------------------------------------
// 偽の PTY
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Control {
    /// 前面で動いているのがシェル以外か(削除の確認を起こす)。
    busy: AtomicBool,
    killed: Mutex<usize>,
    spawned_cwds: Mutex<Vec<PathBuf>>,
}

struct FakePty(Arc<Control>);

impl PtyPort for FakePty {
    fn spawn(&self, spec: &ShellLaunchSpec, _size: PtySize) -> Result<SpawnedPty, PtyError> {
        lock(&self.0.spawned_cwds).push(spec.cwd().to_path_buf());
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        Ok(SpawnedPty {
            session: Box::new(FakeSession {
                control: Arc::clone(&self.0),
                feed: Some(tx),
            }),
            output: Box::new(FakeOutput(rx)),
        })
    }
}

struct FakeOutput(Receiver<Vec<u8>>);

impl Read for FakeOutput {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.0.recv() {
            Ok(chunk) => {
                buf[..chunk.len()].copy_from_slice(&chunk);
                Ok(chunk.len())
            }
            Err(_) => Ok(0),
        }
    }
}

struct FakeSession {
    control: Arc<Control>,
    /// 落とすと出力が終わる。
    feed: Option<Sender<Vec<u8>>>,
}

impl PtySession for FakeSession {
    fn write(&mut self, _data: &[u8]) -> Result<(), PtyError> {
        Ok(())
    }

    fn resize(&mut self, _size: PtySize) -> Result<(), PtyError> {
        Ok(())
    }

    fn kill(&mut self) -> Result<(), PtyError> {
        *lock(&self.control.killed) += 1;
        self.feed.take();
        Ok(())
    }

    fn try_wait(&mut self) -> Result<Option<ChildExit>, PtyError> {
        Ok(None)
    }

    fn foreground(&mut self) -> Foreground {
        if self.control.busy.load(Ordering::SeqCst) {
            Foreground::Other
        } else {
            Foreground::Shell
        }
    }
}

// ---------------------------------------------------------------------------
// 共有するもの
// ---------------------------------------------------------------------------

struct Ctx {
    picker: Arc<FakePicker>,
    pasteboard: MacPasteboard,
    control: Arc<Control>,
    /// プロジェクトのフォルダを置く一時フォルダ(状態フォルダとは別)。
    work: PathBuf,
    state_dir: PathBuf,
}

impl Ctx {
    /// 一時フォルダの中にフォルダを作り、正規化したパス(`/private/var/...`)を返す。
    fn folder(&self, name: &str) -> (PathBuf, PathBuf) {
        let path = self.work.join(name);
        fs::create_dir_all(&path).expect("フォルダを作れなかった");
        let canonical = fs::canonicalize(&path).expect("正規化できなかった");
        (path, canonical)
    }
}

/// 抜けるとき(確認が失敗して panic したときも)に名前付きのペーストボードを OS から解放する。
struct ReleaseOnDrop(MacPasteboard);

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        self.0.release();
    }
}

type Check = fn(&AppHandle<MockRuntime>, &WebviewWindow<MockRuntime>, &Ctx);

fn call(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    cmd: &str,
    args: serde_json::Value,
) -> Result<InvokeResponseBody, serde_json::Value> {
    get_ipc_response(window, request_with(handle, cmd, InvokeBody::Json(args)))
}

fn call_ok<T: serde::de::DeserializeOwned>(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    cmd: &str,
    args: serde_json::Value,
) -> T {
    call(handle, window, cmd, args)
        .unwrap_or_else(|e| panic!("{cmd} が失敗した: {e}"))
        .deserialize()
        .unwrap_or_else(|e| panic!("{cmd} の戻り値を読めなかった: {e}"))
}

fn error_text(result: Result<InvokeResponseBody, serde_json::Value>) -> String {
    match result {
        Ok(_) => panic!("失敗するはずが成功した"),
        Err(e) => e
            .as_str()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| e.to_string()),
    }
}

fn snapshot(handle: &AppHandle<MockRuntime>, window: &WebviewWindow<MockRuntime>) -> AppSnapshot {
    get_ipc_response(window, request(handle, "get_app_snapshot"))
        .unwrap_or_else(|e| panic!("get_app_snapshot が失敗した: {e}"))
        .deserialize()
        .unwrap_or_else(|e| panic!("AppSnapshot として読めなかった: {e}"))
}

/// 表示用のパスが `display` のプロジェクトの ID。
fn project_id_by_name(snapshot: &AppSnapshot, name: &str) -> String {
    snapshot
        .projects
        .iter()
        .find(|p| p.name == name)
        .unwrap_or_else(|| panic!("{name} のプロジェクトが無い"))
        .id
        .as_str()
        .to_owned()
}

fn selected(snapshot: &AppSnapshot) -> Option<String> {
    snapshot
        .selected_project_id
        .as_ref()
        .map(|id| id.as_str().to_owned())
}

/// フォルダの中身(相対パスの一覧。並べ替え済み)。
fn tree(dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(&current).expect("フォルダを読めなかった") {
            let path = entry.expect("項目を読めなかった").path();
            let rel = path
                .strip_prefix(dir)
                .expect("外の項目")
                .display()
                .to_string();
            let contents = if path.is_dir() {
                stack.push(path.clone());
                String::from("/")
            } else {
                fs::read_to_string(&path).unwrap_or_default()
            };
            found.push(format!("{rel}:{contents}"));
        }
    }
    found.sort();
    found
}

// ---------------------------------------------------------------------------
// 確認
// ---------------------------------------------------------------------------

fn ダイアログで選んだフォルダが登録され_1_タブ開き選択される(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let (alpha, canonical) = ctx.folder("alpha");
    assert!(snapshot(handle, window).projects.is_empty());
    ctx.picker.answer(Some(&alpha));

    let outcome: AddProjectOutcome = call_ok(handle, window, "add_project_by_dialog", json!({}));

    let s = snapshot(handle, window);
    let AddProjectOutcome::Added { project_id } = outcome else {
        panic!("登録されなかった: {outcome:?}");
    };
    assert_eq!(lock(&ctx.picker.opened).as_slice(), &[FolderPurpose::Add]);
    assert_eq!(s.projects.len(), 1);
    assert_eq!(s.projects[0].id, project_id);
    assert_eq!(s.projects[0].name, "alpha");
    assert_eq!(selected(&s).as_deref(), Some(project_id.as_str()));
    assert_eq!(
        s.projects[0].terminal_ids.len(),
        1,
        "タブが 1 枚開いていない"
    );
    assert_eq!(s.terminals.len(), 1);
    assert!(!s.projects[0].missing);

    // 開いたタブはルート(正規化したパス)で起動する。
    let terminal = s.projects[0].terminal_ids[0].as_str().to_owned();
    call(
        handle,
        window,
        "connect_terminal",
        json!({ "terminalId": terminal, "rows": 24, "cols": 80, "onOutput": "__CHANNEL__:7001" }),
    )
    .unwrap_or_else(|e| panic!("connect_terminal が失敗した: {e}"));
    assert_eq!(lock(&ctx.control.spawned_cwds).as_slice(), &[canonical]);
}

fn 登録でプロジェクトのフォルダにファイルができず_状態ファイルに保存される(
    handle: &AppHandle<MockRuntime>,
    _window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let (alpha, canonical) = ctx.folder("alpha");
    assert!(
        tree(&alpha).is_empty(),
        "プロジェクトのフォルダに何かができた"
    );

    // 前面でなくなったときは待たずに書く(保存は写しを送ったときに予約されている)。
    arin_lib::lifecycle::on_window_focus_changed(handle, false);
    let state = ctx.state_dir.join("state.json");
    assert!(
        wait_until(|| fs::read_to_string(&state).is_ok_and(|t| t.contains("alpha"))),
        "状態ファイルに登録が保存されなかった"
    );
    let saved: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&state).expect("状態ファイルを読めなかった"))
            .expect("状態ファイルが JSON でない");
    let project = &saved["projects"][0];
    assert_eq!(project["lastKnownPath"], json!(canonical));
    assert!(
        project["bookmark"].as_str().is_some_and(|b| !b.is_empty()),
        "ブックマークが保存されていない: {project}"
    );
    assert!(
        tree(&alpha).is_empty(),
        "プロジェクトのフォルダに何かができた"
    );
}

fn ダイアログを取り消すと何も変わらない(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let before = snapshot(handle, window);
    ctx.picker.answer(None);

    let outcome: AddProjectOutcome = call_ok(handle, window, "add_project_by_dialog", json!({}));

    assert_eq!(outcome, AddProjectOutcome::Cancelled);
    assert_eq!(snapshot(handle, window), before);
}

fn 同じフォルダは増えず既存が選択される(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let (beta, _) = ctx.folder("beta");
    ctx.picker.answer(Some(&beta));
    let added: AddProjectOutcome = call_ok(handle, window, "add_project_by_dialog", json!({}));
    assert!(
        matches!(added, AddProjectOutcome::Added { .. }),
        "{added:?}"
    );
    let before = snapshot(handle, window);
    let alpha_id = project_id_by_name(&before, "alpha");
    assert_ne!(selected(&before).as_deref(), Some(alpha_id.as_str()));

    // シンボリックリンクを経由して同じフォルダを選ぶ。
    let link = ctx.work.join("alpha-link");
    std::os::unix::fs::symlink(ctx.work.join("alpha"), &link).expect("リンクを作れなかった");
    ctx.picker.answer(Some(&link));
    let outcome: AddProjectOutcome = call_ok(handle, window, "add_project_by_dialog", json!({}));

    let s = snapshot(handle, window);
    assert_eq!(
        serde_json::to_value(&outcome).unwrap(),
        json!({ "kind": "alreadyRegistered", "projectId": alpha_id })
    );
    assert_eq!(s.projects.len(), before.projects.len(), "カードが増えた");
    assert_eq!(selected(&s).as_deref(), Some(alpha_id.as_str()));
    assert_eq!(s.terminals.len(), before.terminals.len(), "タブが増えた");
}

fn 入れ子のフォルダは別のカードとして増える(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let (nested, _) = ctx.folder("alpha/frontend");
    let before = snapshot(handle, window).projects.len();
    ctx.picker.answer(Some(&nested));

    let outcome: AddProjectOutcome = call_ok(handle, window, "add_project_by_dialog", json!({}));

    assert!(
        matches!(outcome, AddProjectOutcome::Added { .. }),
        "{outcome:?}"
    );
    assert_eq!(snapshot(handle, window).projects.len(), before + 1);
}

fn ドロップしたフォルダだけが登録されファイルは無視される(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let (gamma, _) = ctx.folder("gamma");
    let file = ctx.work.join("note.txt");
    fs::write(&file, "memo").expect("ファイルを作れなかった");
    let before = snapshot(handle, window).projects.len();

    arin_lib::lifecycle::on_paths_dropped(
        handle,
        vec![file.clone(), gamma, ctx.work.join("no-such-folder")],
    );

    let s = snapshot(handle, window);
    assert_eq!(
        s.projects.len(),
        before + 1,
        "フォルダだけが 1 件増えるはず"
    );
    let gamma_id = project_id_by_name(&s, "gamma");
    assert_eq!(selected(&s).as_deref(), Some(gamma_id.as_str()));
    assert!(s.projects.iter().all(|p| p.name != "note.txt"));
    assert_eq!(fs::read_to_string(&file).unwrap(), "memo");
}

fn 選択を変えてもターミナルの状態は変わらない(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    _ctx: &Ctx,
) {
    let before = snapshot(handle, window);
    let alpha_id = project_id_by_name(&before, "alpha");

    call_ok::<()>(
        handle,
        window,
        "select_project",
        json!({ "projectId": alpha_id }),
    );

    let s = snapshot(handle, window);
    assert_eq!(selected(&s).as_deref(), Some(alpha_id.as_str()));
    assert_eq!(s.terminals, before.terminals);
    let unknown = error_text(call(
        handle,
        window,
        "select_project",
        json!({ "projectId": "00000000-0000-4000-8000-000000000000" }),
    ));
    assert!(unknown.contains("見つからない"), "{unknown}");
}

fn プロジェクトを指定して新しいタブを開くとそのプロジェクトが選ばれる(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    _ctx: &Ctx,
) {
    let before = snapshot(handle, window);
    let selected_before = selected(&before);
    let target = before
        .projects
        .iter()
        .find(|p| !p.missing && Some(p.id.as_str()) != selected_before.as_deref())
        .unwrap_or_else(|| panic!("選んでいないプロジェクトが無い"))
        .clone();

    let opened: String = call_ok(
        handle,
        window,
        "new_terminal",
        json!({ "projectId": target.id }),
    );

    let s = snapshot(handle, window);
    assert_eq!(selected(&s).as_deref(), Some(target.id.as_str()));
    let card = s
        .projects
        .iter()
        .find(|p| p.id == target.id)
        .unwrap_or_else(|| panic!("カードが無くなった"));
    assert_eq!(card.terminal_ids.len(), target.terminal_ids.len() + 1);
    assert_eq!(
        card.active_terminal_id.as_ref().map(|id| id.as_str()),
        Some(opened.as_str())
    );
    assert_eq!(s.terminals.len(), before.terminals.len() + 1);

    // 無い ID には開けず、選択も変わらない。
    let unknown = error_text(call(
        handle,
        window,
        "new_terminal",
        json!({ "projectId": "00000000-0000-4000-8000-000000000000" }),
    ));
    assert!(unknown.contains("見つからない"), "{unknown}");
    let after = snapshot(handle, window);
    assert_eq!(selected(&after), selected(&s));
    assert_eq!(after.terminals.len(), s.terminals.len());

    // あとの確認は元の選択を前提にしているので戻す。
    call_ok::<()>(
        handle,
        window,
        "select_project",
        json!({ "projectId": selected_before }),
    );
}

fn 色を変えられる(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    _ctx: &Ctx,
) {
    let beta_id = project_id_by_name(&snapshot(handle, window), "beta");

    call_ok::<()>(
        handle,
        window,
        "set_project_color",
        json!({ "projectId": beta_id, "color": "purple" }),
    );

    let s = snapshot(handle, window);
    let beta = s
        .projects
        .iter()
        .find(|p| p.id.as_str() == beta_id)
        .expect("beta が無い");
    assert_eq!(beta.color, ProjectColor::Purple);
    let bad = call(
        handle,
        window,
        "set_project_color",
        json!({ "projectId": beta_id, "color": "black" }),
    );
    assert!(bad.is_err(), "パレットに無い色を受け付けた");
}

fn 絶対パスのコピーは偽のペーストボードに正規化したパスを書き選択を変えない(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let before = snapshot(handle, window);
    let beta_id = project_id_by_name(&before, "beta");
    assert_ne!(selected(&before).as_deref(), Some(beta_id.as_str()));
    let (_, canonical) = ctx.folder("beta");

    call_ok::<()>(
        handle,
        window,
        "copy_project_path",
        json!({ "projectId": beta_id }),
    );

    assert_eq!(
        ctx.pasteboard.read_text().as_deref(),
        Some(canonical.to_str().expect("UTF-8 のパス"))
    );
    assert_eq!(snapshot(handle, window), before, "コピーで状態が変わった");
}

fn 再リンクで_id_と色を保ち_取り消すと何も変わらない(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let before = snapshot(handle, window);
    let beta_id = project_id_by_name(&before, "beta");
    let beta_before = before
        .projects
        .iter()
        .find(|p| p.id.as_str() == beta_id)
        .cloned()
        .expect("beta が無い");

    ctx.picker.answer(None);
    let cancelled: bool = call_ok(
        handle,
        window,
        "relink_project",
        json!({ "projectId": beta_id }),
    );
    assert!(!cancelled);
    assert_eq!(snapshot(handle, window), before);

    let (delta, _) = ctx.folder("delta");
    ctx.picker.answer(Some(&delta));
    let relinked: bool = call_ok(
        handle,
        window,
        "relink_project",
        json!({ "projectId": beta_id }),
    );

    assert!(relinked);
    assert_eq!(
        lock(&ctx.picker.opened).last(),
        Some(&FolderPurpose::Relink)
    );
    let s = snapshot(handle, window);
    let beta = s
        .projects
        .iter()
        .find(|p| p.id.as_str() == beta_id)
        .expect("再リンクで ID が変わった");
    assert_eq!(beta.color, beta_before.color);
    assert_eq!(beta.name, "delta");
    assert_eq!(
        beta.terminal_ids, beta_before.terminal_ids,
        "再リンクでタブが変わった"
    );
    assert_eq!(selected(&s), selected(&before), "再リンクで選択が変わった");

    // 別のプロジェクトのフォルダには再リンクしない。
    let (alpha, _) = ctx.folder("alpha");
    ctx.picker.answer(Some(&alpha));
    let clash = error_text(call(
        handle,
        window,
        "relink_project",
        json!({ "projectId": beta_id }),
    ));
    assert!(clash.contains("登録済み"), "{clash}");
    assert_eq!(snapshot(handle, window), s);
}

fn names(snapshot: &AppSnapshot) -> Vec<String> {
    snapshot.projects.iter().map(|p| p.name.clone()).collect()
}

fn 並び順を変えると写しと状態ファイルの並びが変わる(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let before = snapshot(handle, window);
    let order = names(&before);
    assert!(
        order.len() >= 2,
        "並べ替えるプロジェクトが足りない: {order:?}"
    );
    let first = before.projects[0].id.as_str().to_owned();
    let last = order.len() - 1;

    call_ok::<()>(
        handle,
        window,
        "move_project",
        json!({ "projectId": first, "toIndex": last }),
    );
    let s = snapshot(handle, window);
    let mut expected = order.clone();
    let moved = expected.remove(0);
    expected.push(moved.clone());
    assert_eq!(names(&s), expected);
    assert_eq!(selected(&s), selected(&before), "選択が変わった");

    arin_lib::lifecycle::on_window_focus_changed(handle, false);
    let state = ctx.state_dir.join("state.json");
    let saved_order = || -> Vec<String> {
        let Ok(text) = fs::read_to_string(&state) else {
            return Vec::new();
        };
        let saved: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        saved["projects"]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(|p| p["id"].as_str().map(ToOwned::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    let expected_ids: Vec<String> = s
        .projects
        .iter()
        .map(|p| p.id.as_str().to_owned())
        .collect();
    assert!(
        wait_until(|| saved_order() == expected_ids),
        "状態ファイルの並びが変わらなかった: {:?}",
        saved_order()
    );

    // 無い ID(ドラッグのあいだに外された)は失敗にせず、何も変えない。元の並びに戻す。
    let s = snapshot(handle, window);
    call_ok::<()>(
        handle,
        window,
        "move_project",
        json!({ "projectId": "00000000-0000-4000-8000-000000000000", "toIndex": 0 }),
    );
    assert_eq!(snapshot(handle, window), s);
    call_ok::<()>(
        handle,
        window,
        "move_project",
        json!({ "projectId": first, "toIndex": 0 }),
    );
    assert_eq!(names(&snapshot(handle, window)), order);
}

fn タブを並べ替え_まとめて閉じるとき動作中があれば確認を求める(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    // alpha のタブ 1 枚目は最初の確認で接続済み。まだ接続していないタブを 2 枚足す。
    let before = snapshot(handle, window);
    let alpha_id = project_id_by_name(&before, "alpha");
    let alpha_tabs = |s: &AppSnapshot| -> Vec<String> {
        s.projects
            .iter()
            .find(|p| p.id.as_str() == alpha_id)
            .map(|p| {
                p.terminal_ids
                    .iter()
                    .map(|t| t.as_str().to_owned())
                    .collect()
            })
            .unwrap_or_default()
    };
    let connected = alpha_tabs(&before)[0].clone();
    let second: String = call_ok(
        handle,
        window,
        "new_terminal",
        json!({ "projectId": alpha_id }),
    );
    let third: String = call_ok(
        handle,
        window,
        "new_terminal",
        json!({ "projectId": alpha_id }),
    );

    // 並べ替え: 3 枚目を先頭へ。
    call_ok::<()>(
        handle,
        window,
        "move_terminal_tab",
        json!({ "terminalId": third, "toIndex": 0 }),
    );
    assert_eq!(
        alpha_tabs(&snapshot(handle, window)),
        [third.clone(), connected.clone(), second.clone()]
    );
    // 閉じたタブ(ドラッグのあいだに閉じた)は失敗にせず、何も変えない。
    let s = snapshot(handle, window);
    call_ok::<()>(
        handle,
        window,
        "move_editor_tab",
        json!({ "editorId": "editor-999", "toIndex": 0 }),
    );
    call_ok::<()>(
        handle,
        window,
        "move_terminal_tab",
        json!({ "terminalId": "terminal-999", "toIndex": 0 }),
    );
    assert_eq!(snapshot(handle, window), s);

    ctx.control.busy.store(true, Ordering::SeqCst);
    // 接続していないタブは動作中に数えないので、確認なしで閉じる。
    let outcome: CloseTabsOutcome = call_ok(
        handle,
        window,
        "close_tabs",
        json!({ "projectId": alpha_id, "terminalIds": [third], "editorIds": [], "dirtyEditorIds": [], "confirmedTerminalIds": [], "confirmedEditorIds": [] }),
    );
    assert_eq!(outcome, CloseTabsOutcome::Closed);
    // 動作中のタブを含むと、何も変えずに確認を求める。
    let mid = snapshot(handle, window);
    let killed_before = *lock(&ctx.control.killed);
    let outcome: CloseTabsOutcome = call_ok(
        handle,
        window,
        "close_tabs",
        json!({
            "projectId": alpha_id,
            "terminalIds": [connected, second],
            "editorIds": [],
            "dirtyEditorIds": [],
            "confirmedTerminalIds": [],
            "confirmedEditorIds": []
        }),
    );
    let CloseTabsOutcome::NeedsConfirmation { running, unsaved } = outcome else {
        panic!("確認を求めなかった: {outcome:?}");
    };
    assert_eq!(
        running
            .iter()
            .map(|t| t.terminal_id.as_str().to_owned())
            .collect::<Vec<_>>(),
        std::slice::from_ref(&connected)
    );
    assert!(unsaved.is_empty());
    assert_eq!(snapshot(handle, window), mid, "確認の前に何か変わった");
    assert_eq!(*lock(&ctx.control.killed), killed_before);
    ctx.control.busy.store(false, Ordering::SeqCst);

    // 残りの確認のため、接続済みの 1 枚だけを残す。
    let outcome: CloseTabsOutcome = call_ok(
        handle,
        window,
        "close_tabs",
        json!({ "projectId": alpha_id, "terminalIds": [second], "editorIds": [], "dirtyEditorIds": [], "confirmedTerminalIds": [], "confirmedEditorIds": [] }),
    );
    assert_eq!(outcome, CloseTabsOutcome::Closed);
    assert_eq!(alpha_tabs(&snapshot(handle, window)), [connected]);
}

fn 動作中があれば確認を求め_中止なら何も変わらない(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    // alpha のタブは最初の確認で接続済み(偽の PTY が動いている)。
    let before = snapshot(handle, window);
    let alpha_id = project_id_by_name(&before, "alpha");
    ctx.control.busy.store(true, Ordering::SeqCst);
    let killed_before = *lock(&ctx.control.killed);

    let outcome: RemoveProjectOutcome = call_ok(
        handle,
        window,
        "remove_project",
        json!({ "projectId": alpha_id, "confirmed": false }),
    );

    let RemoveProjectOutcome::NeedsConfirmation { running, unsaved } = outcome else {
        panic!("確認を求めなかった: {outcome:?}");
    };
    assert_eq!(running.len(), 1);
    assert!(unsaved.is_empty(), "エディタを開いていないので未保存は無い");
    assert_eq!(running[0].project_name.as_deref(), Some("alpha"));
    assert_eq!(running[0].label, "ターミナル 1");
    assert_eq!(snapshot(handle, window), before, "確認の前に何か変わった");
    assert_eq!(
        *lock(&ctx.control.killed),
        killed_before,
        "確認の前にシェルを終わらせた"
    );
}

fn 確認して削除するとカードが消えシェルが終わりフォルダの中身は変わらない(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    ctx: &Ctx,
) {
    let (alpha, _) = ctx.folder("alpha");
    fs::write(alpha.join("README.md"), "keep me").expect("ファイルを作れなかった");
    let contents = tree(&alpha);
    let before = snapshot(handle, window);
    let alpha_id = project_id_by_name(&before, "alpha");
    let killed_before = *lock(&ctx.control.killed);

    let outcome: RemoveProjectOutcome = call_ok(
        handle,
        window,
        "remove_project",
        json!({ "projectId": alpha_id, "confirmed": true }),
    );

    assert_eq!(outcome, RemoveProjectOutcome::Removed);
    let s = snapshot(handle, window);
    assert_eq!(s.projects.len(), before.projects.len() - 1);
    assert!(s.projects.iter().all(|p| p.id.as_str() != alpha_id));
    assert!(
        s.terminals
            .iter()
            .all(|t| t.project_id.as_ref().map(|p| p.as_str()) != Some(alpha_id.as_str())),
        "削除したプロジェクトのタブが残った"
    );
    assert_eq!(
        *lock(&ctx.control.killed),
        killed_before + 1,
        "シェルを終わらせなかった"
    );
    assert_eq!(tree(&alpha), contents, "フォルダの中身が変わった");
    ctx.control.busy.store(false, Ordering::SeqCst);
}

fn 動作中が無ければ確認なしで削除できる(
    handle: &AppHandle<MockRuntime>,
    window: &WebviewWindow<MockRuntime>,
    _ctx: &Ctx,
) {
    let before = snapshot(handle, window);
    let gamma_id = project_id_by_name(&before, "gamma");

    let outcome: RemoveProjectOutcome = call_ok(
        handle,
        window,
        "remove_project",
        json!({ "projectId": gamma_id, "confirmed": false }),
    );

    assert_eq!(outcome, RemoveProjectOutcome::Removed);
    assert_eq!(
        snapshot(handle, window).projects.len(),
        before.projects.len() - 1
    );
}

const CHECKS: &[(&str, Check)] = &[
    (
        "ダイアログで選んだフォルダが登録され_1_タブ開き選択される",
        ダイアログで選んだフォルダが登録され_1_タブ開き選択される,
    ),
    (
        "登録でプロジェクトのフォルダにファイルができず_状態ファイルに保存される",
        登録でプロジェクトのフォルダにファイルができず_状態ファイルに保存される,
    ),
    (
        "ダイアログを取り消すと何も変わらない",
        ダイアログを取り消すと何も変わらない,
    ),
    (
        "同じフォルダは増えず既存が選択される",
        同じフォルダは増えず既存が選択される,
    ),
    (
        "入れ子のフォルダは別のカードとして増える",
        入れ子のフォルダは別のカードとして増える,
    ),
    (
        "ドロップしたフォルダだけが登録されファイルは無視される",
        ドロップしたフォルダだけが登録されファイルは無視される,
    ),
    (
        "選択を変えてもターミナルの状態は変わらない",
        選択を変えてもターミナルの状態は変わらない,
    ),
    (
        "プロジェクトを指定して新しいタブを開くとそのプロジェクトが選ばれる",
        プロジェクトを指定して新しいタブを開くとそのプロジェクトが選ばれる,
    ),
    ("色を変えられる", 色を変えられる),
    (
        "絶対パスのコピーは偽のペーストボードに正規化したパスを書き選択を変えない",
        絶対パスのコピーは偽のペーストボードに正規化したパスを書き選択を変えない,
    ),
    (
        "再リンクで_id_と色を保ち_取り消すと何も変わらない",
        再リンクで_id_と色を保ち_取り消すと何も変わらない,
    ),
    (
        "並び順を変えると写しと状態ファイルの並びが変わる",
        並び順を変えると写しと状態ファイルの並びが変わる,
    ),
    (
        "タブを並べ替え_まとめて閉じるとき動作中があれば確認を求める",
        タブを並べ替え_まとめて閉じるとき動作中があれば確認を求める,
    ),
    (
        "動作中があれば確認を求め_中止なら何も変わらない",
        動作中があれば確認を求め_中止なら何も変わらない,
    ),
    (
        "確認して削除するとカードが消えシェルが終わりフォルダの中身は変わらない",
        確認して削除するとカードが消えシェルが終わりフォルダの中身は変わらない,
    ),
    (
        "動作中が無ければ確認なしで削除できる",
        動作中が無ければ確認なしで削除できる,
    ),
];

fn main() {
    let temp = support::TempDir::new("project-commands");
    let state_dir = temp.path().join("state");
    let work = temp.path().join("work");
    fs::create_dir(&state_dir).expect("状態フォルダを作れなかった");
    fs::create_dir(&work).expect("作業フォルダを作れなかった");

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("時計が 1970 年より前")
        .as_nanos();
    let pasteboard = MacPasteboard::named(format!(
        "dev.arin.test.project-commands.{}.{nanos}",
        std::process::id()
    ));
    assert!(
        !pasteboard.is_general(),
        "ユーザーのクリップボードを使おうとした"
    );
    let _release = ReleaseOnDrop(pasteboard.clone());

    let picker = Arc::new(FakePicker::default());
    let control = Arc::new(Control::default());
    let launch =
        arin_lib::launch::LaunchConfig::from_inputs(None, Some(state_dir.as_os_str()), None)
            .unwrap_or_else(|e| panic!("組み立ての設定を作れなかった: {e}"))
            .with_terminal_env(arin_lib::launch::TerminalEnv {
                home: Some(PathBuf::from("/Users/someone")),
                user: None,
                vars: vec![("HOME".into(), "/Users/someone".into())],
            });
    let project_ports = ProjectPorts {
        ids: Arc::new(UrandomIdSource::new()),
        pasteboard: Arc::new(pasteboard.clone()),
        picker: Some(Arc::clone(&picker) as Arc<dyn FolderPicker>),
    };
    let app = arin_lib::builder_with_project_ports::<MockRuntime>(
        launch,
        Arc::new(FakePty(Arc::clone(&control))),
        Arc::new(|| ShellDefaults {
            shell: PathBuf::from("/bin/zsh"),
            locale: None,
        }),
        PersistencePorts::native(state_dir.clone()),
        project_ports,
    )
    .build(tauri::generate_context!())
    .unwrap_or_else(|e| panic!("組み立てに失敗した: {e}"));

    let ctx = Ctx {
        picker,
        pasteboard: pasteboard.clone(),
        control,
        work,
        state_dir,
    };
    let passed = Rc::new(Cell::new(0_usize));
    let count = Rc::clone(&passed);

    app.run_return(move |handle, event| {
        if let RunEvent::Ready = event {
            let window = handle
                .get_webview_window("main")
                .expect("設定にあるウィンドウ main が作られなかった");
            for (name, check) in CHECKS {
                check(handle, &window, &ctx);
                println!("test {name} ... ok");
                count.set(count.get() + 1);
            }
            for window in handle.webview_windows().into_values() {
                window.destroy().expect("ウィンドウを閉じられなかった");
            }
        }
        arin_lib::lifecycle::on_run_event(handle, event);
    });
    assert_eq!(passed.get(), CHECKS.len(), "起動の完了まで進まなかった");
    println!("test result: ok. {} passed", CHECKS.len());
}
