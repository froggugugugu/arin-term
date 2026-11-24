//! 最新版の問い合わせ(`arin_core::ports::release::ReleasePort` の実装。ADR-018)。
//!
//! macOS 標準の `/usr/bin/curl` を絶対パスで起動する(HTTP の crate を足さない。NFR-008 の例外はこの 1 か所だけ)。
//! - 問い合わせ先は `arin_core::update::LATEST_RELEASE_API` に固定。https だけ(`--proto =https`)
//! - 利用者の `.curlrc` を読まない(`-q` は最初の引数でないと効かない)
//! - 応答の大きさ(1 MiB)と時間(curl に 10 秒、全体で 15 秒)に上限を付ける。超えたら子プロセスを kill する
//! - 送る頭は `Accept` と固定の `User-Agent` だけ(利用者の情報は送らない)
//! - 環境変数は引き継ぐ(`https_proxy` などのプロキシの設定が効く。会社のプロキシの下でも確かめられるように)

use std::time::Duration;

use arin_core::ports::release::{ReleaseFetchError, ReleasePort};
use arin_core::update::LATEST_RELEASE_API;

use crate::command::{Outcome, Probe};

const CURL: &str = "/usr/bin/curl";

/// 読む応答の大きさの上限(1 MiB)。最新版の JSON は数十 KiB。
const MAX_BODY: u64 = 1024 * 1024;

/// curl に渡す引数(問い合わせ先の前まで)。
const CURL_ARGS: &[&str] = &[
    "-q",
    "--silent",
    "--show-error",
    "--fail",
    "--proto",
    "=https",
    "--tlsv1.2",
    "--max-time",
    "10",
    "--max-filesize",
    "1048576",
    "--header",
    "Accept: application/vnd.github+json",
    "--user-agent",
    "Arin-update-check",
    "--",
];

const CURL_PROBE: Probe<'static> = Probe {
    program: CURL,
    leading: CURL_ARGS,
    limit: Duration::from_secs(15),
};

/// `/usr/bin/curl` で GitHub の Releases の最新版を問い合わせる。
#[derive(Debug, Clone, Copy, Default)]
pub struct CurlReleases;

impl CurlReleases {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl ReleasePort for CurlReleases {
    fn fetch_latest(&self) -> Result<String, ReleaseFetchError> {
        fetch_with(&CURL_PROBE)
    }
}

fn fetch_with(probe: &Probe<'_>) -> Result<String, ReleaseFetchError> {
    match probe.run_capped(&[LATEST_RELEASE_API], Some(MAX_BODY)) {
        Outcome::Output(body) => Ok(body),
        Outcome::Failed => Err(ReleaseFetchError(
            "GitHub に問い合わせられませんでした".to_owned(),
        )),
        Outcome::TimedOut { .. } => Err(ReleaseFetchError(
            "GitHub から時間内に応答がありませんでした".to_owned(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(script: &'static str, limit: Duration) -> Probe<'static> {
        Probe {
            program: "/bin/sh",
            leading: Box::leak(Box::new(["-c", script, "sh"])),
            limit,
        }
    }

    #[test]
    fn 問い合わせ先と引数は固定() {
        assert_eq!(CURL_PROBE.program, "/usr/bin/curl");
        assert_eq!(CURL_PROBE.leading.first(), Some(&"-q"));
        assert_eq!(CURL_PROBE.leading.last(), Some(&"--"));
        assert!(
            CURL_PROBE
                .leading
                .windows(2)
                .any(|w| w == ["--proto", "=https"])
        );
        assert_eq!(CURL_PROBE.limit, Duration::from_secs(15));
    }

    #[test]
    fn 成功したら本文を返す_問い合わせ先は最後に渡る() {
        let echo = fake("echo \"$1\"", Duration::from_secs(5));
        assert_eq!(fetch_with(&echo), Ok(format!("{LATEST_RELEASE_API}\n")));
    }

    #[test]
    fn 上限より大きな応答は読まずに失敗にする() {
        let big = fake(
            "/usr/bin/yes x | /usr/bin/head -c 2000000",
            Duration::from_secs(10),
        );
        assert!(fetch_with(&big).is_err());
        let small = fake(
            "/usr/bin/yes x | /usr/bin/head -c 1000",
            Duration::from_secs(10),
        );
        assert_eq!(fetch_with(&small).map(|b| b.len()), Ok(1000));
    }

    #[test]
    fn 失敗と時間切れは理由の文になる() {
        assert!(fetch_with(&fake("exit 22", Duration::from_secs(5))).is_err());
        let slow = fake("exec /bin/sleep 30", Duration::from_millis(200));
        assert_eq!(
            fetch_with(&slow),
            Err(ReleaseFetchError(
                "GitHub から時間内に応答がありませんでした".to_owned()
            ))
        );
    }

    /// 本物の GitHub に問い合わせる(手で回す: `cargo test -p arin-platform --lib release -- --ignored`)。
    #[test]
    #[ignore = "ネットワークに出る"]
    fn 本物の最新版を読める() {
        let body = CurlReleases::new()
            .fetch_latest()
            .expect("問い合わせられなかった");
        assert!(
            arin_core::update::parse_latest_release(&body).is_some(),
            "{body}"
        );
    }
}
