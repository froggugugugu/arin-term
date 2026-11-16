//! Arin の組み立て(設計書 §11)。
//!
//! ここでは部品を生成して渡すことだけを行い、規則を書かない。
//! 置き場所と起動の通知先を管理状態に登録し、起動時に状態ファイルを読んで(`persistence.rs`)`Workspace` と
//! セッション管理と保存のスレッドとプロジェクトの操作(`projects.rs`)とルートへの追従の見回り(`patrol.rs`)を作って登録し、
//! ツリーの操作(`tree.rs`)とファイル操作(`fileops.rs`。切り取りの状態をアプリに 1 つ持つ)と、選択中のプロジェクトのツリーの監視の張り替え(`tree_watch.rs`。写しを送るたびに張り替えを頼む)を登録し、
//! デスクトップとパスの操作(`commands/desktop.rs`。Finder で表示・開く・パスのコピー・ターミナルへの挿入)を登録し、
//! 設定ファイルを読んで(`settings.rs`)ショートカットの表(`menu::Shortcuts`。メニューバーと `get_shortcuts` が同じものを見る)と
//! 設定の知らせ(`settings::AppSettings`)を登録し、状態フォルダの監視(`settings_watch.rs`。`settings.toml` の保存で読み直す)を起動し、
//! コマンドとメニューバー(`menu.rs`。項目が選ばれたときも `menu.rs`)と終了の確認の状態(`lifecycle.rs`)を登録して
//! tauri.conf.json のウィンドウを 1 枚出す。Finder からのドロップは `lifecycle.rs` が受ける。

mod commands;
pub mod diagnostics;
pub mod events;
pub mod fileops;
pub mod launch;
pub mod lifecycle;
pub mod menu;
pub mod patrol;
pub mod persistence;
pub mod projects;
pub mod sessions;
pub mod settings;
pub mod settings_watch;
pub mod tree;
pub mod tree_watch;
pub mod verify_log;

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use arin_core::persistence::codec::encode;
use arin_core::ports::PtyPort;
use arin_core::ports::desktop::DesktopPort;
use arin_core::ports::fs::FileSystemPort;
use arin_core::ports::pasteboard::FilePasteboardPort;
use arin_core::ports::watch::WatcherPort;
use arin_core::tree::TreeChange;
use arin_core::workspace::{AppSnapshot, Workspace};
use arin_platform::desktop::{MacDesktop, MacFilePasteboard};
use arin_platform::fs::LocalFileSystem;
use arin_platform::pty::{FALLBACK_SHELL, NativePty, default_shell, system_locale};
use arin_platform::watch::FsEventsWatcher;
use tauri::{Builder, Emitter, Manager, Runtime};

pub use crate::commands::app::SNAPSHOT_EVENT;
use crate::commands::desktop::Desktop;
use crate::events::TREE_CHANGED_EVENT;
use crate::fileops::FileOps;
use crate::launch::LaunchConfig;
use crate::lifecycle::{FirstOutputNotice, QuitGate, ReadyNotice};
use crate::menu::Shortcuts;
use crate::patrol::{PATROL_INTERVAL, PatrolThread, RootPatrol};
use crate::persistence::{Persistence, PersistencePorts, load_startup};
use crate::projects::{FolderPicker, NativeFolderPicker, ProjectPorts, Projects};
use crate::sessions::{SessionConfig, SessionRegistry, ShellDefaults, ShellProbe};
use crate::settings::AppSettings;
use crate::settings_watch::SettingsWatch;
use crate::tree::Tree;
use crate::tree_watch::{TreeChangeSink, TreeWatch};

/// アプリの組み立て。ランタイムを型引数で受け取り、テストでは模擬ランタイムに差し替える。
///
/// 起動時の設定(`LaunchConfig`)は引数で受け取る。環境変数を読むのは `run` の側だけ。
/// PTY は OS の PTY(`NativePty`)、シェルはユーザー情報に登録された既定のシェルを使う。
pub fn builder<R: Runtime>(launch: LaunchConfig) -> Builder<R> {
    let user = launch.terminal().user.clone();
    let probe: ShellProbe = Arc::new(move || ShellDefaults {
        shell: user
            .as_deref()
            .map_or_else(|| PathBuf::from(FALLBACK_SHELL), default_shell),
        locale: system_locale(),
    });
    builder_with_pty(launch, Arc::new(NativePty), probe)
}

/// PTY のポートと、シェルと言語設定の取得を差し替えて組み立てる(テスト用。本物は `builder`)。
/// 状態ファイルは置き場所(`launch.paths()`)の OS の実装を使う。
pub fn builder_with_pty<R: Runtime>(
    launch: LaunchConfig,
    pty: Arc<dyn PtyPort>,
    probe: ShellProbe,
) -> Builder<R> {
    let ports = PersistencePorts::native(launch.paths().state_dir().to_path_buf());
    builder_with_ports(launch, pty, probe, ports)
}

/// `builder_with_pty` に加えて、状態の読み書きのポート(状態ファイル・ブックマーク・ルート・時計)も差し替えて組み立てる(テスト用)。
pub fn builder_with_ports<R: Runtime>(
    launch: LaunchConfig,
    pty: Arc<dyn PtyPort>,
    probe: ShellProbe,
    ports: PersistencePorts,
) -> Builder<R> {
    builder_with_project_ports(launch, pty, probe, ports, ProjectPorts::native())
}

/// `builder_with_ports` に加えて、プロジェクトの操作のポート(乱数・ペーストボード・フォルダ選択のダイアログ)も
/// 差し替えて組み立てる(テスト用)。フォルダの正規化とブックマークは `ports` のものを使う。
pub fn builder_with_project_ports<R: Runtime>(
    launch: LaunchConfig,
    pty: Arc<dyn PtyPort>,
    probe: ShellProbe,
    ports: PersistencePorts,
    project_ports: ProjectPorts,
) -> Builder<R> {
    builder_with_file_system(
        launch,
        pty,
        probe,
        ports,
        project_ports,
        Arc::new(LocalFileSystem::with_leftover_report(
            fileops::report_staging_leftover,
        )),
    )
}

/// `builder_with_project_ports` に加えて、ファイルシステムのポート(ツリーの読み込み)も差し替えて組み立てる(テスト用)。
/// 本物は `LocalFileSystem`。監視は本物(`FsEventsWatcher`)。
pub fn builder_with_file_system<R: Runtime>(
    launch: LaunchConfig,
    pty: Arc<dyn PtyPort>,
    probe: ShellProbe,
    ports: PersistencePorts,
    project_ports: ProjectPorts,
    fs: Arc<dyn FileSystemPort>,
) -> Builder<R> {
    builder_with_watcher(
        launch,
        pty,
        probe,
        ports,
        project_ports,
        fs,
        Arc::new(FsEventsWatcher::new()),
    )
}

/// `builder_with_file_system` に加えて、ツリーの監視のポートも差し替えて組み立てる(テスト用)。本物は `FsEventsWatcher`。
/// ファイルのペーストボードは本物(一般のペーストボード)。
pub fn builder_with_watcher<R: Runtime>(
    launch: LaunchConfig,
    pty: Arc<dyn PtyPort>,
    probe: ShellProbe,
    ports: PersistencePorts,
    project_ports: ProjectPorts,
    fs: Arc<dyn FileSystemPort>,
    watcher: Arc<dyn WatcherPort>,
) -> Builder<R> {
    builder_with_tree_ports(
        launch,
        pty,
        probe,
        ports,
        project_ports,
        TreePorts {
            fs,
            watcher,
            file_pasteboard: Arc::new(MacFilePasteboard::general()),
        },
    )
}

/// ツリーとファイル操作のポート(`builder_with_tree_ports` で差し替える)。
pub struct TreePorts {
    /// ファイルシステム(ツリーの読み込みとファイル操作。ゴミ箱を含む)。本物は `LocalFileSystem`。
    pub fs: Arc<dyn FileSystemPort>,
    /// ツリーの監視。本物は `FsEventsWatcher`。
    pub watcher: Arc<dyn WatcherPort>,
    /// ファイルの URL のペーストボード(ツリーのコピー・切り取り・ペースト)。本物は一般のペーストボード
    /// (`MacFilePasteboard::general`)。テストは名前付きのものを渡す。
    pub file_pasteboard: Arc<dyn FilePasteboardPort>,
}

/// `builder_with_watcher` に加えて、ファイルのペーストボードも差し替えて組み立てる(テスト用)。
/// Finder とアプリへの受け渡しは本物(`MacDesktop`)。
pub fn builder_with_tree_ports<R: Runtime>(
    launch: LaunchConfig,
    pty: Arc<dyn PtyPort>,
    probe: ShellProbe,
    ports: PersistencePorts,
    project_ports: ProjectPorts,
    tree_ports: TreePorts,
) -> Builder<R> {
    builder_with_desktop(
        launch,
        pty,
        probe,
        ports,
        project_ports,
        tree_ports,
        Arc::new(MacDesktop::new()),
    )
}

/// `builder_with_tree_ports` に加えて、Finder で表示・既定のアプリで開くのポートも差し替えて組み立てる(テスト用。
/// 自動テストで Finder を開いたりアプリを起動したりしないため)。本物は `MacDesktop`。
/// パスのコピーは `project_ports` の文字列のペーストボードに書く。設定ファイルの監視は本物(`FsEventsWatcher`)。
pub fn builder_with_desktop<R: Runtime>(
    launch: LaunchConfig,
    pty: Arc<dyn PtyPort>,
    probe: ShellProbe,
    ports: PersistencePorts,
    project_ports: ProjectPorts,
    tree_ports: TreePorts,
    desktop: Arc<dyn DesktopPort>,
) -> Builder<R> {
    builder_with_settings_watcher(
        launch,
        pty,
        probe,
        ports,
        project_ports,
        tree_ports,
        desktop,
        Arc::new(FsEventsWatcher::new()),
    )
}

/// `builder_with_desktop` に加えて、設定ファイルの監視(状態フォルダに張る)のポートも差し替えて組み立てる(テスト用。T65c)。
/// 本物は `FsEventsWatcher`。ツリーの監視(`tree_ports.watcher`)とは別に渡す(張った場所の記録が混ざらないように)。
#[allow(clippy::too_many_arguments)] // 差し替えの口を 1 つずつ足してきた組み立ての末端(ほかの `builder_with_*` が呼ぶ)。
pub fn builder_with_settings_watcher<R: Runtime>(
    launch: LaunchConfig,
    pty: Arc<dyn PtyPort>,
    probe: ShellProbe,
    ports: PersistencePorts,
    project_ports: ProjectPorts,
    tree_ports: TreePorts,
    desktop: Arc<dyn DesktopPort>,
    settings_watcher: Arc<dyn WatcherPort>,
) -> Builder<R> {
    let TreePorts {
        fs,
        watcher,
        file_pasteboard,
    } = tree_ports;
    let ready = ReadyNotice::new(launch.ready_marker().map(ToOwned::to_owned));
    let first_output = FirstOutputNotice::new(launch.terminal_marker().map(ToOwned::to_owned));
    let terminal = launch.terminal().clone();
    let state_dir = launch.paths().state_dir().to_path_buf();
    // 設定ファイル(状態フォルダの `settings.toml`。差し替えたら差し替え先だけ)を、メニューを作る前に読む(T65b)。
    // 読むだけで、書き換え・退避はしない。問題は標準エラーに 1 つ 1 行(`[arin:settings]`)。
    let (settings, resolved) = AppSettings::load(launch.paths());
    settings::report(&resolved);
    Builder::<R>::new()
        .manage(launch.paths().clone())
        .manage(ready)
        .manage(first_output)
        .manage(QuitGate::default())
        // ショートカットの表(T42e)。メニュー(`menu::build`)と `get_shortcuts` がこれを見る。
        // 設定ファイルの値を当てた表(T65b)。動作中の差し替えは `Shortcuts::replace` と `menu::rebuild`(T65c)。
        .manage(Shortcuts::new(resolved.table))
        // 設定の知らせ(`get_settings_notice`)と、読み直し(T65c)のための直前の内容。
        .manage(settings)
        // 「設定ファイルを開く…」が Finder・既定のアプリに渡す口(T65d)。ツリーの「開く」と同じポート。
        .manage(settings::SettingsOpener::new(Arc::clone(&desktop)))
        .menu(menu::build)
        .on_menu_event(|app, event| menu::on_menu_event(app, event.id().as_ref()))
        .setup(move |app| {
            let version = app.package_info().version.to_string();
            let home = terminal.home.unwrap_or_default();
            let load = load_startup(&ports);
            let workspace = Workspace::restore(version, home, terminal.vars, load.startup);
            let persistence = Persistence::new(load.scheduler, Arc::clone(&ports.clock));
            let needs_save = load.needs_save;
            // いまの状態ファイルの中身とみなすもの。読んだまま変わっていなければ、同じ中身を書き直さない。
            let baseline = if needs_save {
                None
            } else {
                encode(&workspace.persisted_state()).ok()
            };
            let handle = app.handle().clone();
            let saver = persistence.clone();
            // ツリーの監視の張り替え(T42b)。セッション管理を作ったあとに起動するので、受け口からは後で入れたものを使う。
            let tree_watch: Arc<OnceLock<TreeWatch>> = Arc::new(OnceLock::new());
            let rewatch = Arc::clone(&tree_watch);
            let listener = Arc::new(move |snapshot: AppSnapshot| {
                // 画面がまだ無いときなどは送れない。写しは画面が取得のコマンドでも取れるので捨ててよい。
                let _ = handle.emit(SNAPSHOT_EVENT, snapshot);
                // 状態が変わったら保存を予約する。保存する内容が前回と同じなら書かない(`persistence.rs`)。
                saver.mark_changed();
                // 選択・ルート・見つからない・隠しファイル表示が変わっていれば張り替える(待たない。張り替えは専用のスレッド)。
                if let Some(watch) = rewatch.get() {
                    watch.refresh();
                }
            });
            // 検証用の記録は開発ビルドだけ(リリースビルドでは `None`。T23)。
            let sessions = SessionRegistry::with_verify(
                workspace,
                pty,
                probe,
                listener,
                SessionConfig::default(),
                verify_log::default_verify(),
            );
            let source = sessions.clone();
            persistence.start(
                Arc::clone(&ports.state_file),
                Box::new(move || source.persisted_state()),
                baseline,
            );
            if needs_save {
                persistence.mark_changed();
            }
            let picker: Arc<dyn FolderPicker> = project_ports
                .picker
                .clone()
                .unwrap_or_else(|| Arc::new(NativeFolderPicker::new(app.handle().clone())));
            app.manage(Projects::new(
                sessions.clone(),
                Arc::clone(&ports.roots),
                Arc::clone(&ports.bookmarks),
                Arc::clone(&project_ports.ids),
                Arc::clone(&project_ports.pasteboard),
                picker,
            ));
            app.manage(Tree::new(sessions.clone(), Arc::clone(&fs)));
            app.manage(FileOps::new(
                sessions.clone(),
                Arc::clone(&fs),
                Arc::clone(&file_pasteboard),
            ));
            app.manage(Desktop::new(
                sessions.clone(),
                Arc::clone(&fs),
                Arc::clone(&desktop),
                Arc::clone(&project_ports.pasteboard),
            ));
            let emitter = app.handle().clone();
            let emit: TreeChangeSink = Arc::new(move |change: TreeChange| {
                // 画面が無いときは送れない。画面は表示したときに読み込むので捨ててよい。
                let _ = emitter.emit(TREE_CHANGED_EVENT, change);
            });
            let watch = TreeWatch::spawn(sessions.clone(), watcher, fs, emit)?;
            // 見回りより先に入れる(見回りの追従の写しで張り替えを頼めるように)。
            let _ = tree_watch.set(watch.clone());
            app.manage(watch);
            // ルートへの追従の見回り(1 秒ごと。T33d)。開いたフォルダは見回りのスレッドが持ち、終了時に止める(`lifecycle.rs`)。
            let patrol = PatrolThread::spawn(
                sessions.clone(),
                RootPatrol::new(Arc::clone(&ports.roots), Arc::clone(&ports.bookmarks)),
                PATROL_INTERVAL,
            )?;
            app.manage(patrol);
            // 設定ファイルの保存の検知(T65c)。状態フォルダに張り、`settings.toml` が保存されたら読み直して
            // 表・メニュー・画面に反映する。張れなくても起動は続ける(標準エラーに `[arin:settings]` で 1 行)。
            let reloader = app.handle().clone();
            match SettingsWatch::spawn(
                state_dir,
                settings_watcher,
                Arc::new(move || {
                    settings_watch::reload_and_publish(&reloader);
                }),
            ) {
                Ok(watch) => {
                    app.manage(watch);
                }
                Err(e) => eprintln!(
                    "{} 設定ファイルの監視のスレッドを起動できないため、保存しても読み直さない({:?})",
                    settings::DIAGNOSTIC_PREFIX,
                    e.kind()
                ),
            }
            app.manage(sessions);
            app.manage(persistence);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::get_app_snapshot,
            commands::app::get_shortcuts,
            commands::app::get_settings_notice,
            commands::app::notify_app_ready,
            commands::app::report_frontend_error,
            commands::app::confirm_quit,
            commands::app::open_settings_file,
            commands::terminal::connect_terminal,
            commands::terminal::write_terminal,
            commands::terminal::ack_terminal_output,
            commands::terminal::resize_terminal,
            commands::terminal::write_terminal_bytes,
            commands::terminal::new_terminal,
            commands::terminal::close_terminal,
            commands::terminal::select_terminal,
            commands::project::add_project_by_dialog,
            commands::project::select_project,
            commands::project::remove_project,
            commands::project::relink_project,
            commands::project::set_project_color,
            commands::project::copy_project_path,
            commands::layout::set_pane_sizes,
            commands::layout::set_sidebar_collapsed,
            commands::layout::set_display_mode,
            commands::layout::focus_terminal,
            commands::layout::next_tile_page,
            commands::layout::previous_tile_page,
            commands::layout::toggle_tile_maximize,
            commands::layout::step_font_size,
            commands::layout::step_translucency,
            commands::layout::reset_appearance,
            commands::tree::list_directory,
            commands::tree::set_show_hidden,
            commands::fileops::create_entry,
            commands::fileops::rename_entry,
            commands::fileops::move_entry,
            commands::fileops::copy_entry,
            commands::fileops::trash_entry,
            commands::fileops::copy_entries,
            commands::fileops::cut_entries,
            commands::fileops::paste_entries,
            commands::desktop::reveal_entry,
            commands::desktop::open_entry,
            commands::desktop::copy_entry_path,
            commands::desktop::insert_entry_path,
        ])
}

/// アプリを起動する。
pub fn run() {
    let launch = match LaunchConfig::from_env() {
        Ok(launch) => launch,
        Err(e) => {
            // 置き場所を決められないまま起動すると、意図しない場所を読み書きしかねないので起動しない。
            eprintln!("Arin を起動できない: {e}");
            std::process::exit(2);
        }
    };
    let app = builder::<tauri::Wry>(launch)
        .build(tauri::generate_context!())
        .expect("Tauri のアプリを組み立てられなかった");
    apply_window_effects(&app);
    app.run(lifecycle::on_run_event);
}

/// ウィンドウのラベル(`tauri.conf.json` の `app.windows[0].label`)。
const MAIN_WINDOW_LABEL: &str = "main";

/// ウィンドウの背景に macOS のサイドバーの素材(すりガラス)を重ねる(ADR-010)。
///
/// `tauri.conf.json` の `windowEffects` には書かず、本物のランタイムで起動したときだけここで付ける。
/// 設定に書くと、模擬ランタイム(`src-tauri/tests/`)のウィンドウにも付けようとして落ちるため。
/// 付けられなくても起動は続ける(見た目だけの機能)。
fn apply_window_effects(app: &tauri::App) {
    use tauri::window::{Effect, EffectState, EffectsBuilder};

    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };
    let effects = EffectsBuilder::new()
        .effect(Effect::Sidebar)
        .state(EffectState::FollowsWindowActiveState)
        .build();
    if let Err(e) = window.set_effects(effects) {
        eprintln!("[arin:launch] ウィンドウの半透過を付けられなかった: {e}");
    }
}
