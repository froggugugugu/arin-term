//! 設定ファイル `settings.toml` の見本の文(計画書 T64c。ADR-012 の 6)。
//!
//! 「設定ファイルを開く…」(T65d。M4.6 からは表の外のキーの無い項目と設定画面のボタン。ADR-013 の 6)が、ファイルが**無いときだけ**この文で見本を作る。見本は既定の表から作るので、
//! 表とずれない。どの行もコメントか `[shortcuts]` の見出しだけで、そのまま読むと既定の表と同じになる。
//! 変えられるコマンドは `# "ID" = "既定のキー"` の行で並べ、`#` と空白を消せばそのまま設定の行になる。
//! 変えられないコマンド(受け口が「編集」の標準の項目)は、コメントを外しても設定の行にならない形で説明に書く。
//! 入出力を持たない。

use std::fmt::Write as _;

use super::reserved::RESERVED_KEYS;
use super::table::{ShortcutBinding, ShortcutReceiver, ShortcutTable};

/// 見本の説明(見出しより前に置くコメント。行ごと。空の文字列は `#` だけの行)。
const INTRO: &[&str] = &[
    "Arin の設定ファイル(settings.toml)",
    "文字の大きさはこのファイルではなく、アプリの「設定…」(⌘,)で変えます",
    "",
    "ショートカットのキーを変えるには、[shortcuts] の下の行の先頭の「# 」を消して、キーを書き換えます。",
    "保存すると、アプリを動かしたまますぐに効きます。",
    "",
    "書き方の決まり:",
    "  - 1 行に 1 つ、\"コマンドの ID\" = \"キー\" の形で書きます。ID は . と - を含むので引用符で囲みます",
    "  - キーは修飾キーとキーを + でつなぎます(例 \"Cmd+Shift+T\")。修飾キーは Cmd・Shift・Alt・Ctrl",
    "    (別名 Command・Option・Control)。大文字と小文字、修飾キーの順は問いません",
    "  - キーは英字・数字・記号 [ ] - = , . / ; ' ` \\ と、Enter・Backspace・Delete・Escape・Tab・Space・",
    "    ArrowUp・ArrowDown・ArrowLeft・ArrowRight・F1〜F12。+ のキーは書けません",
    "  - 書かなかった行は既定のキーのままです。既定に戻すには、行を消すか先頭に # を付けます",
    "  - メニューの項目のキーは Cmd を含めてください。ツリーのキーは Cmd を含むか、",
    "    修飾キーの無い Enter・Space・Backspace・Delete・F1〜F12 にしてください",
    "  - 同じキーを 2 つのコマンドに割り当てると、関わる行は既定に戻ります。入れ替えるときは両方の行を書きます",
    "  - 問題のある行はその行だけ既定に戻り、画面に 1 行で知らせます。このファイルをアプリが書き換えることはありません",
    "",
];

/// 予約のキーの説明の前置き。
const RESERVED_INTRO: &[&str] = &[
    "",
    "macOS やアプリの標準の項目が使うため、割り当てられないキー(書くと既定に戻ります):",
];

/// 効かないときの確かめ方(見出しの直前に置く)。
const TROUBLESHOOTING: &[&str] = &[
    "",
    "キーが効かないとき:",
    "  - 画面の知らせ(1 行)に、使わなかった行の ID と理由が出ます",
    "  - 上の一覧に無くても、macOS の「システム設定」>「キーボード」>「キーボードショートカット」で",
    "    使われているキーや、入力ソースの切り替えのキーは、先に macOS が受けるので効きません。別のキーを試してください",
    "  - このファイルがシンボリックリンクのときは、リンクの先の変更は拾えません。",
    "    保存のあとにリンク自体を触る(touch)か、アプリを再起動してください",
    "",
];

/// TOML の基本の文字列の中身に書ける形にする(`\` と `"` を逃がす)。
fn toml_basic(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// コメントの 1 行を足す(空なら `#` だけ)。
fn push_comment(out: &mut String, line: &str) {
    if line.is_empty() {
        out.push_str("#\n");
    } else {
        // `String` への書き込みは失敗しない。
        let _ = writeln!(out, "# {line}");
    }
}

/// 既定の表から `settings.toml` の見本の文を作る(末尾に改行 1 つ)。
///
/// 先頭に文字の大きさはこのファイルで変えないこと(ADR-013 の 6)・書き方の決まり・変えられないコマンド・予約のキー・効かないときの確かめ方・リンクの先の変更は拾えないことを
/// コメントで書き、`[shortcuts]` の下に変えられるコマンドを表の順に `# "ID" = "既定のキー"` のコメントで並べる。
#[must_use]
pub fn sample_settings_toml(defaults: &ShortcutTable) -> String {
    let mut out = String::new();
    for line in INTRO {
        push_comment(&mut out, line);
    }

    push_comment(
        &mut out,
        "変えられないコマンド(「編集」の標準の項目。書いても使わず、知らせます):",
    );
    for binding in defaults.by_receiver(ShortcutReceiver::EditItem) {
        push_comment(
            &mut out,
            &format!(
                "  - {}({}。変えられません)",
                binding.command(),
                binding.accelerator()
            ),
        );
    }

    for line in RESERVED_INTRO {
        push_comment(&mut out, line);
    }
    push_comment(&mut out, &format!("  {}", RESERVED_KEYS.join("  ")));

    for line in TROUBLESHOOTING {
        push_comment(&mut out, line);
    }

    out.push_str("[shortcuts]\n");
    for binding in defaults
        .bindings()
        .iter()
        .filter(|b| b.receiver() != ShortcutReceiver::EditItem)
    {
        push_row(&mut out, binding);
    }
    out
}

/// `# "ID" = "既定のキー"` の 1 行を足す。
fn push_row(out: &mut String, binding: &ShortcutBinding) {
    let _ = writeln!(
        out,
        "# \"{}\" = \"{}\"",
        toml_basic(binding.command().as_str()),
        toml_basic(&binding.accelerator().to_string())
    );
}
