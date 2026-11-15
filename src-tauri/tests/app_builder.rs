//! アプリの組み立てが失敗しないことを、Tauri の模擬ランタイム(`tauri::test`)で確かめる。
//!
//! 本物の `tauri.conf.json` から作った設定で組み立て、イベントループを 1 度起こして、
//! 起動時の処理(設定にあるウィンドウの生成と setup)まで通ることを見る。
//!
//! macOS ではメニューの部品をメインスレッドでしか作れないため、標準のテストハーネスを使わず
//! (Cargo.toml で `harness = false`)、この `main` をメインスレッドで実行する。

mod support;

use std::cell::Cell;
use std::rc::Rc;

use tauri::test::MockRuntime;
use tauri::{Manager, RunEvent};

fn アプリを組み立てて起動するとメインのウィンドウができる() {
    let state_dir = support::TempDir::new("builder");
    let app = arin_lib::builder::<MockRuntime>(support::launch_in(&state_dir, false))
        .build(tauri::generate_context!())
        .unwrap_or_else(|e| panic!("組み立てに失敗した: {e}"));

    let window_found = Rc::new(Cell::new(false));
    let seen = Rc::clone(&window_found);

    // 起動時の処理が失敗すると、run_return の中で panic する。
    app.run_return(move |handle, event| {
        if let RunEvent::Ready = event {
            seen.set(handle.get_webview_window("main").is_some());
            // 状態ファイルの読み込みは T33a。それまで通常の起動はプロジェクト 0 件・ターミナル 0 枚で始まる。
            let snapshot = handle
                .state::<arin_lib::sessions::SessionRegistry>()
                .snapshot();
            assert!(
                snapshot.projects.is_empty(),
                "プロジェクトがある: {snapshot:?}"
            );
            assert!(
                snapshot.terminals.is_empty(),
                "ターミナルがある: {snapshot:?}"
            );
            // 模擬ランタイムのループは、ウィンドウが全部無くなると終わる。
            // 1 枚も無いと終わらないので、その場合はここで失敗させる。
            let windows = handle.webview_windows();
            assert!(!windows.is_empty(), "ウィンドウが 1 枚も作られなかった");
            for window in windows.into_values() {
                window.destroy().expect("ウィンドウを閉じられなかった");
            }
        }
    });

    assert!(
        window_found.get(),
        "設定にあるウィンドウ main が作られなかった"
    );
}

fn main() {
    アプリを組み立てて起動するとメインのウィンドウができる();
    println!("test アプリを組み立てて起動するとメインのウィンドウができる ... ok");
}
