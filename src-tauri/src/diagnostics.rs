//! 画面で起きた失敗の診断(常に有効)。
//!
//! 画面(`src/state/errorReporter.ts`)は CSP 違反、未処理の例外、未処理の Promise の失敗、ターミナルの接続の失敗を
//! `report_frontend_error` で送ってくる。ここでは決まった頭の文字列を付けた 1 行にして標準エラーに書く。
//! 実アプリでは画面のコンソールを見られないので、起動スモーク(`scripts/smoke.mjs`)はこの行で画面の失敗を検出する。

/// 画面の失敗の行の頭。`scripts/smoke.mjs` の `FRONTEND_ERROR_PREFIX` と合わせる。
pub const FRONTEND_ERROR_PREFIX: &str = "[arin:frontend-error]";

/// 1 行に載せる本文の上限(文字数)。画面から長い文字列が届いても標準エラーを埋めない。
pub const FRONTEND_ERROR_MAX_CHARS: usize = 1000;

/// 上限を超えて切り詰めたときに末尾に付ける印。
const TRUNCATED: &str = "…(切り詰めた)";

/// 画面から届いた文を、頭の文字列を付けた 1 行にする。
///
/// 改行・タブなどの制御文字は空白に置き換え(1 件が必ず 1 行になる)、本文が上限を超えたら切り詰める。
#[must_use]
pub fn frontend_error_line(message: &str) -> String {
    let mut body = String::new();
    let mut truncated = false;
    for (count, c) in message.chars().enumerate() {
        if count == FRONTEND_ERROR_MAX_CHARS {
            truncated = true;
            break;
        }
        body.push(if c.is_control() { ' ' } else { c });
    }
    let body = body.trim();
    let body = if body.is_empty() {
        "(空の報告)"
    } else {
        body
    };
    if truncated {
        format!("{FRONTEND_ERROR_PREFIX} {body}{TRUNCATED}")
    } else {
        format!("{FRONTEND_ERROR_PREFIX} {body}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 頭の文字列を付けて_1_行にする() {
        assert_eq!(
            frontend_error_line("CSP 違反: style-src-elem が inline を止めた"),
            "[arin:frontend-error] CSP 違反: style-src-elem が inline を止めた"
        );
    }

    #[test]
    fn 改行とタブなどの制御文字は空白にして_1_行に収める() {
        let line = frontend_error_line("a\nb\r\nc\td\u{1b}[31m");

        assert_eq!(line, "[arin:frontend-error] a b  c d [31m");
        assert!(!line.contains('\n'));
    }

    #[test]
    fn 上限を超えた本文は切り詰めて印を付ける() {
        let long = "あ".repeat(FRONTEND_ERROR_MAX_CHARS + 5);

        let line = frontend_error_line(&long);

        let body = line
            .strip_prefix("[arin:frontend-error] ")
            .expect("頭の文字列が無い");
        assert_eq!(
            body,
            format!("{}{TRUNCATED}", "あ".repeat(FRONTEND_ERROR_MAX_CHARS))
        );
    }

    #[test]
    fn ちょうど上限の本文は切り詰めない() {
        let exact = "x".repeat(FRONTEND_ERROR_MAX_CHARS);

        assert_eq!(
            frontend_error_line(&exact),
            format!("[arin:frontend-error] {exact}")
        );
    }

    #[test]
    fn 空や空白だけの報告もそれと分かる_1_行にする() {
        assert_eq!(frontend_error_line(""), "[arin:frontend-error] (空の報告)");
        assert_eq!(
            frontend_error_line(" \n "),
            "[arin:frontend-error] (空の報告)"
        );
    }

    #[test]
    fn 頭の文字列は_scripts_smoke_mjs_と合わせる() {
        assert_eq!(FRONTEND_ERROR_PREFIX, "[arin:frontend-error]");
    }
}
