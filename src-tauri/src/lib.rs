//! Arin の組み立て(設計書 §11)。
//!
//! ここでは部品を生成して渡すことだけを行い、規則を書かない。
//! 置き場所と起動の通知先を管理状態に登録し、起動時に `Workspace` とセッション管理を作って登録し、
//! コマンドとメニューバー(`menu.rs`)を登録して tauri.conf.json のウィンドウを 1 枚出す。

mod commands;
pub mod diagnostics;
pub mod launch;
pub mod lifecycle;
pub mod menu;
pub mod sessions;
pub mod verify_log;

use std::path::PathBuf;
use std::sync::Arc;

use arin_core::ports::PtyPort;
use arin_core::workspace::{AppSnapshot, Workspace};
use arin_platform::pty::{FALLBACK_SHELL, NativePty, default_shell, system_locale};
use tauri::{Builder, Emitter, Manager, Runtime};

pub use crate::commands::app::SNAPSHOT_EVENT;
use crate::launch::LaunchConfig;
use crate::lifecycle::{FirstOutputNotice, ReadyNotice};
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
pub fn builder_with_pty<R: Runtime>(
    launch: LaunchConfig,
    pty: Arc<dyn PtyPort>,
    probe: ShellProbe,
) -> Builder<R> {
    let ready = ReadyNotice::new(launch.ready_marker().map(ToOwned::to_owned));
    let first_output = FirstOutputNotice::new(launch.terminal_marker().map(ToOwned::to_owned));
    let terminal = launch.terminal().clone();
    Builder::<R>::new()
        .manage(launch.paths().clone())
        .manage(launch.options())
        .manage(ready)
        .manage(first_output)
        .menu(menu::build)
        .setup(move |app| {
            let workspace = Workspace::new(
                app.package_info().version.to_string(),
                terminal.home.unwrap_or_default(),
                terminal.vars,
            );
            let handle = app.handle().clone();
            let listener = Arc::new(move |snapshot: AppSnapshot| {
                // 画面がまだ無いときなどは送れない。写しは画面が取得のコマンドでも取れるので捨ててよい。
                let _ = handle.emit(SNAPSHOT_EVENT, snapshot);
            });
            // 検証用の記録は開発ビルドだけ(リリースビルドでは `None`。T23)。
            app.manage(SessionRegistry::with_verify(
                workspace,
                pty,
                probe,
                listener,
                SessionConfig::default(),
                verify_log::default_verify(),
            ));
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
