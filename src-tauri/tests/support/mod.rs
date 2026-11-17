//! `src-tauri/tests/` の共通の補助。
//!
//! テストは状態フォルダをシステムの一時領域の下に差し替え、本物の `~/Library/Application Support/Arin/` に触れない。
//! 環境変数は読まず、組み立て(`LaunchConfig::from_inputs`)に値を引数で渡す。

#![allow(dead_code)] // テストごとに使う補助が違う。

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use arin_core::persistence::codec::PersistedState;
use arin_core::project::{Project, ProjectColor, ProjectId, RootReference};
use arin_core::workspace::{RootResolution, Startup, Workspace};
use arin_lib::launch::LaunchConfig;
use tauri::AppHandle;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::MockRuntime;
use tauri::webview::InvokeRequest;

/// 画面(自アプリの画面)からコマンドを呼んだときの要求を作る。
pub fn request(handle: &AppHandle<MockRuntime>, cmd: &str) -> InvokeRequest {
    request_with(handle, cmd, InvokeBody::default())
}

/// 引数 `body`(JSON。キーは camelCase)を付けてコマンドを呼んだときの要求を作る。
pub fn request_with(handle: &AppHandle<MockRuntime>, cmd: &str, body: InvokeBody) -> InvokeRequest {
    InvokeRequest {
        cmd: cmd.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: "tauri://localhost".parse().expect("URL が不正"),
        body,
        headers: Default::default(),
        invoke_key: handle.invoke_key().to_string(),
    }
}

/// このテストだけが使う一時フォルダ。抜けるときに自分が作ったフォルダだけを消す。
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("時計が 1970 年より前")
            .as_nanos();
        let base = std::env::temp_dir();
        let dir = base.join(format!(
            "arin-tauri-test-{name}-{}-{nanos}",
            std::process::id()
        ));
        assert!(dir.starts_with(&base), "一時領域の外を使おうとした");
        fs::create_dir(&dir).expect("一時フォルダを作れなかった");
        Self(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// フォルダの中の名前の一覧から、状態ファイル(`state.json` と書き込み途中の `state.json.tmp`)を除いたもの。
    /// 状態ファイルを置いて起動したテストが、印のファイルだけを見るのに使う。
    pub fn entries_except_state(&self) -> Vec<String> {
        self.entries()
            .into_iter()
            .filter(|name| !name.starts_with(STATE_FILE_NAME))
            .collect()
    }

    /// フォルダの中の名前の一覧(並べ替え済み)。
    pub fn entries(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(&self.0)
            .expect("一時フォルダを読めなかった")
            .map(|e| {
                e.expect("項目を読めなかった")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// 状態フォルダを `dir` に差し替えた組み立ての設定。`smoke` が真ならスモークの印を有効にする。
pub fn launch_in(dir: &TempDir, smoke: bool) -> LaunchConfig {
    // ホームは渡さない。差し替えがあるのでホームは使われない(本物のホームを指す余地を残さない)。
    LaunchConfig::from_inputs(
        None,
        Some(dir.path().as_os_str()),
        smoke.then(|| OsStr::new("1")),
    )
    .unwrap_or_else(|e| panic!("組み立ての設定を作れなかった: {e}"))
}

/// 状態フォルダの中の状態ファイルの名前。
pub const STATE_FILE_NAME: &str = "state.json";

/// `n` 番目(1 から)のプロジェクトの ID。`workspace_with_projects`・`write_projects_state` が使う。
pub fn project_id(n: u8) -> ProjectId {
    ProjectId::from_random_bytes([n; 16])
}

/// 通常の起動の `Workspace`。`roots` の 1 件ごとにプロジェクトを 1 件持ち(ID は `project_id(1)` から順、色は緑)、
/// 各プロジェクトにまだ起動していないタブが 1 枚ずつある(`terminal-1` から順。cwd はそのルート)。
/// 1 件目を選んでいる。シェルが終わったタブは閉じる。
///
/// M1 の 9 枚の検証画面(`Workspace::for_verify_grid`。M4 の T49d で撤去)の代わりの足場。
/// ルートは解決できたものとして扱う(フォルダがあるかは確かめない)。実 PTY で起動するなら、あるフォルダを渡す。
pub fn workspace_with_projects<I, K, V>(
    app_version: &str,
    home: PathBuf,
    parent_env: I,
    roots: &[PathBuf],
) -> Workspace
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    let projects: Vec<(ProjectId, RootReference)> = roots
        .iter()
        .zip(1u8..)
        .map(|(root, n)| (project_id(n), RootReference::new(root.clone(), None)))
        .collect();
    Workspace::restore(
        app_version,
        home,
        parent_env,
        Startup {
            state: PersistedState {
                projects: projects
                    .iter()
                    .map(|(id, root)| {
                        Project::restore(id.clone(), root.clone(), None, ProjectColor::Green)
                    })
                    .collect(),
                selected_project_id: projects.first().map(|(id, _)| id.clone()),
                ..PersistedState::default()
            },
            roots: projects
                .into_iter()
                .map(|(id, root)| (id, RootResolution::Found(root)))
                .collect(),
            notice: None,
        },
    )
}

/// `roots` の 1 件ごとにプロジェクトを 1 件持つ状態ファイルを `state_dir` に置く(ID は `project_id(1)` から順、
/// ブックマークは無し、色は緑。1 件目を選ぶ)。起動すると各プロジェクトにタブが 1 枚ずつできる(`terminal-1` から順)。
pub fn write_projects_state(state_dir: &Path, roots: &[PathBuf]) {
    let projects: Vec<serde_json::Value> = roots
        .iter()
        .zip(1u8..)
        .map(|(root, n)| {
            serde_json::json!({
                "id": project_id(n).as_str(),
                "bookmark": null,
                "lastKnownPath": root,
                "customName": null,
                "colorId": "green"
            })
        })
        .collect();
    let text = serde_json::to_string_pretty(&serde_json::json!({
        "schemaVersion": 1,
        "projects": projects,
        "ui": { "selectedProjectId": roots.first().map(|_| project_id(1).as_str().to_owned()) }
    }))
    .expect("JSON にできなかった");
    fs::write(state_dir.join(STATE_FILE_NAME), text).expect("状態ファイルを置けなかった");
}

/// `count` 件のプロジェクトのフォルダ(`work` の下の `project-1` から順)を作り、そのルートの一覧を返す。
/// 状態フォルダとは別の一時フォルダに作る(状態フォルダの中身を見るテストに混ざらないように)。
pub fn make_project_roots(work: &TempDir, count: u8) -> Vec<PathBuf> {
    (1..=count)
        .map(|n| {
            let root = work.path().join(format!("project-{n}"));
            fs::create_dir(&root).expect("プロジェクトのフォルダを作れなかった");
            root
        })
        .collect()
}

/// `launch_in` の状態フォルダに、`roots` のプロジェクトを持つ状態ファイルを置いた設定(`write_projects_state`)。
///
/// 空の状態フォルダでの通常の起動はプロジェクト 0 件・ターミナル 0 枚で始まるので、
/// 起動直後からターミナルが要るテストはこちらを使い、1 枚目(`terminal-1`)を操作する。
/// M1 の 9 枚の検証画面の指定(`ARIN_VERIFY_GRID=1`。M4 の T49d で撤去)の代わり。
pub fn launch_with_projects_in(dir: &TempDir, roots: &[PathBuf], smoke: bool) -> LaunchConfig {
    write_projects_state(dir.path(), roots);
    launch_in(dir, smoke)
}
