//! Arin の組み立て(設計書 §11)。
//!
//! ここでは部品を生成して渡すことだけを行い、規則を書かない。
//! 置き場所と起動の通知先を管理状態に登録し、起動時に状態ファイルを読んで(`persistence.rs`)`Workspace` と
//! セッション管理と保存のスレッドとプロジェクトの操作(`projects.rs`)を作って登録し、コマンドとメニューバー(`menu.rs`)を登録して
//! tauri.conf.json のウィンドウを 1 枚出す。Finder からのドロップは `lifecycle.rs` が受ける。

mod commands;
pub mod diagnostics;
pub mod launch;
pub mod lifecycle;
pub mod menu;
pub mod persistence;
pub mod projects;
pub mod sessions;
pub mod verify_log;

use std::path::PathBuf;
use std::sync::Arc;

use arin_core::persistence::codec::encode;
use arin_core::ports::PtyPort;
use arin_core::workspace::{AppSnapshot, Workspace};
use arin_platform::pty::{FALLBACK_SHELL, NativePty, default_shell, system_locale};
use tauri::{Builder, Emitter, Manager, Runtime};

pub use crate::commands::app::SNAPSHOT_EVENT;
use crate::launch::LaunchConfig;
use crate::lifecycle::{FirstOutputNotice, ReadyNotice};
use crate::persistence::{Persistence, PersistencePorts, load_startup};
use crate::projects::{FolderPicker, NativeFolderPicker, ProjectPorts, Projects};
use crate::sessions::{SessionConfig, SessionRegistry, ShellDefaults, ShellProbe};

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
    let ready = ReadyNotice::new(launch.ready_marker().map(ToOwned::to_owned));
    let first_output = FirstOutputNotice::new(launch.terminal_marker().map(ToOwned::to_owned));
    let terminal = launch.terminal().clone();
    let verify_grid = launch.options().verify_grid;
    Builder::<R>::new()
        .manage(launch.paths().clone())
        .manage(launch.options())
        .manage(ready)
        .manage(first_output)
        .menu(menu::build)
        .setup(move |app| {
            let version = app.package_info().version.to_string();
            let home = terminal.home.unwrap_or_default();
            let (workspace, persistence, needs_save) = if verify_grid {
                // 9 枚の検証画面(`ARIN_VERIFY_GRID`。M1 の検証用、M4 の T49 で外す)。プロジェクトを持たず、状態ファイルを読まず書かない。
                (
                    Workspace::for_verify_grid(version, home, terminal.vars),
                    Persistence::disabled(),
                    false,
                )
            } else {
                let load = load_startup(&ports);
                let workspace = Workspace::restore(version, home, terminal.vars, load.startup);
                let persistence = Persistence::new(load.scheduler, Arc::clone(&ports.clock));
                (workspace, persistence, load.needs_save)
            };
            // いまの状態ファイルの中身とみなすもの。読んだまま変わっていなければ、同じ中身を書き直さない。
            let baseline = if needs_save {
                None
            } else {
                workspace
                    .persisted_state()
                    .and_then(|state| encode(&state).ok())
            };
            let handle = app.handle().clone();
            let saver = persistence.clone();
            let listener = Arc::new(move |snapshot: AppSnapshot| {
                // 画面がまだ無いときなどは送れない。写しは画面が取得のコマンドでも取れるので捨ててよい。
                let _ = handle.emit(SNAPSHOT_EVENT, snapshot);
                // 状態が変わったら保存を予約する。保存する内容が前回と同じなら書かない(`persistence.rs`)。
                saver.mark_changed();
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
            app.manage(sessions);
            app.manage(persistence);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::get_app_snapshot,
            commands::app::get_launch_options,
            commands::app::notify_app_ready,
            commands::app::report_frontend_error,
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
    builder::<tauri::Wry>(launch)
        .build(tauri::generate_context!())
        .expect("Tauri のアプリを組み立てられなかった")
        .run(lifecycle::on_run_event);
}
