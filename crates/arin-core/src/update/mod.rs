//! 新しい版の確認(ADR-018)。入出力はしない。
//!
//! GitHub の Releases の最新版の JSON(`releases/latest`)から版を読み、いまの版と比べる。
//! 問い合わせはポート([`crate::ports::release::ReleasePort`])、確かめる時機(起動時・メニュー)は `src-tauri` が決める。
//! 新しい版が見つかっても入れ替えはしない。画面は Releases のページを開くボタンを出すだけ。

use std::cmp::Ordering;
use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 最新版を問い合わせる先(GitHub の REST API)。
pub const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/froggugugugu/arin-term/releases/latest";

/// Releases のページの頭。開いてよいのはこの下の URL だけ。
pub const RELEASES_PAGE: &str = "https://github.com/froggugugugu/arin-term/releases";

/// 版(`major.minor.patch`)。プレリリース・ビルドの印の付いた版は扱わない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    /// `0.3.0` または `v0.3.0` を読む。3 つの数字だけを受け付ける(先頭の 0 は 1 桁の `0` だけ)。
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let body = text.strip_prefix('v').unwrap_or(text);
        let mut parts = body.split('.');
        let major = number(parts.next()?)?;
        let minor = number(parts.next()?)?;
        let patch = number(parts.next()?)?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            major,
            minor,
            patch,
        })
    }

    /// この版の Releases のページ(`…/releases/tag/vX.Y.Z`)。
    #[must_use]
    pub fn release_page(&self) -> String {
        format!("{RELEASES_PAGE}/tag/v{self}")
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// 開いてよい Releases のページか(`RELEASES_PAGE` の下で、英数字と `/`・`.`・`-`・`_` だけ)。
#[must_use]
pub fn is_release_page(url: &str) -> bool {
    url.strip_prefix(RELEASES_PAGE).is_some_and(|rest| {
        (rest.is_empty() || rest.starts_with('/'))
            && !rest.contains("..")
            && rest
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'-' | b'_'))
    })
}

fn number(part: &str) -> Option<u64> {
    let ok = !part.is_empty()
        && part.len() <= 9
        && part.bytes().all(|b| b.is_ascii_digit())
        && (part == "0" || !part.starts_with('0'));
    ok.then(|| part.parse().ok()).flatten()
}

/// `releases/latest` の応答のうち読むところ。
#[derive(Deserialize)]
struct LatestRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// `releases/latest` の JSON から版を読む。読めない・下書き・プレリリースは `None`。
#[must_use]
pub fn parse_latest_release(json: &str) -> Option<Version> {
    let release: LatestRelease = serde_json::from_str(json).ok()?;
    if release.draft || release.prerelease {
        return None;
    }
    Version::parse(&release.tag_name)
}

/// 確かめた結果(画面の知らせ。`get_update_notice` の応答と `update-changed` の中身)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UpdateNotice {
    /// 新しい版がある。`version` は `0.4.0` の形(`v` なし)。
    Available { version: String },
    /// いまの版が最新(メニューから確かめたときだけ出す)。
    UpToDate { version: String },
    /// 確かめられなかった(メニューから確かめたときだけ出す)。`reason` は短い文。
    Failed { reason: String },
}

/// いまの版と最新の版を比べた結果。いまの版が読めなければ、どれも新しいとはみなさない。
#[must_use]
pub fn compare(current: &str, latest: Version) -> Ordering {
    Version::parse(current).map_or(Ordering::Equal, |current| latest.cmp(&current))
}

/// 問い合わせの本文から知らせを作る。`manual` はメニューから確かめたか(最新・失敗も知らせる)。
/// 起動時の確認で新しい版が無いとき・確かめられなかったときは `None`(黙る)。
#[must_use]
pub fn notice_for(
    current: &str,
    fetched: Result<&str, &str>,
    manual: bool,
) -> Option<UpdateNotice> {
    if Version::parse(current).is_none() {
        // いまの版が読めない(開発版など)と比べられない。「最新です」とは言わない
        return manual.then(|| UpdateNotice::Failed {
            reason: format!("いまの版 {current} を読めません"),
        });
    }
    let notice = match fetched {
        Err(reason) => UpdateNotice::Failed {
            reason: reason.to_owned(),
        },
        Ok(json) => match parse_latest_release(json) {
            None => UpdateNotice::Failed {
                reason: "応答を読めませんでした".to_owned(),
            },
            Some(latest) if compare(current, latest) == Ordering::Greater => {
                UpdateNotice::Available {
                    version: latest.to_string(),
                }
            }
            Some(_) => UpdateNotice::UpToDate {
                version: current.to_owned(),
            },
        },
    };
    match notice {
        UpdateNotice::Available { .. } => Some(notice),
        _ if manual => Some(notice),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(major: u64, minor: u64, patch: u64) -> Version {
        Version {
            major,
            minor,
            patch,
        }
    }

    #[test]
    fn 版を読む() {
        assert_eq!(Version::parse("0.3.0"), Some(v(0, 3, 0)));
        assert_eq!(Version::parse("v1.20.3"), Some(v(1, 20, 3)));
        for bad in [
            "",
            "v",
            "1.2",
            "1.2.3.4",
            "1.2.3-beta.1",
            "1.2.3+build",
            "01.2.3",
            "1..3",
            "v-1.2.3",
            "1.2.x",
            " 1.2.3",
            "vv1.2.3",
            "1234567890.0.0",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn 版は数字の順に比べる() {
        assert!(v(0, 10, 0) > v(0, 9, 9));
        assert!(v(1, 0, 0) > v(0, 99, 99));
        assert_eq!(compare("0.3.0", v(0, 4, 0)), Ordering::Greater);
        assert_eq!(compare("0.3.0", v(0, 3, 0)), Ordering::Equal);
        assert_eq!(compare("0.4.0", v(0, 3, 9)), Ordering::Less);
        assert_eq!(compare("読めない", v(9, 9, 9)), Ordering::Equal);
    }

    #[test]
    fn リリースのページは版から作る() {
        assert_eq!(
            v(0, 4, 0).release_page(),
            "https://github.com/froggugugugu/arin-term/releases/tag/v0.4.0"
        );
    }

    #[test]
    fn 開いてよいのは_releases_のページだけ() {
        assert!(is_release_page(RELEASES_PAGE));
        assert!(is_release_page(&v(0, 4, 0).release_page()));
        assert!(is_release_page(
            "https://github.com/froggugugugu/arin-term/releases/latest"
        ));
        for bad in [
            "https://github.com/froggugugugu/arin-term/releasesX",
            "https://github.com/froggugugugu/arin-term/releases/../issues",
            "https://github.com/froggugugugu/arin-term/releases/tag/v1?x=1",
            "https://github.com/froggugugugu/arin-term/releases/tag/v1#a",
            "https://github.com/froggugugugu/arin-term/releases/a b",
            "http://github.com/froggugugugu/arin-term/releases",
            "https://evil.example/",
            "",
        ] {
            assert!(!is_release_page(bad), "{bad}");
        }
    }

    #[test]
    fn 最新版の_json_を読む() {
        let json = r#"{"tag_name":"v0.4.0","html_url":"https://evil.example/","draft":false,"prerelease":false,"body":"…"}"#;
        assert_eq!(parse_latest_release(json), Some(v(0, 4, 0)));
        assert_eq!(
            parse_latest_release(r#"{"tag_name":"v0.4.0"}"#),
            Some(v(0, 4, 0))
        );
        assert_eq!(
            parse_latest_release(r#"{"tag_name":"v0.4.0","prerelease":true}"#),
            None
        );
        assert_eq!(
            parse_latest_release(r#"{"tag_name":"v0.4.0","draft":true}"#),
            None
        );
        assert_eq!(parse_latest_release(r#"{"tag_name":"nightly"}"#), None);
        assert_eq!(parse_latest_release(r#"{"message":"Not Found"}"#), None);
        assert_eq!(parse_latest_release("<html>"), None);
    }

    #[test]
    fn 起動時は新しい版があるときだけ知らせる() {
        let newer = r#"{"tag_name":"v0.4.0"}"#;
        let same = r#"{"tag_name":"v0.3.0"}"#;
        assert_eq!(
            notice_for("0.3.0", Ok(newer), false),
            Some(UpdateNotice::Available {
                version: "0.4.0".to_owned()
            })
        );
        assert_eq!(notice_for("0.3.0", Ok(same), false), None);
        assert_eq!(notice_for("0.3.0", Err("通信できません"), false), None);
        assert_eq!(notice_for("0.3.0", Ok("{}"), false), None);
        assert_eq!(notice_for("0.3.0-dev", Ok(newer), false), None);
    }

    #[test]
    fn いまの版が読めなければ最新とは言わない() {
        assert_eq!(
            notice_for("0.3.0-dev", Ok(r#"{"tag_name":"v0.3.0"}"#), true),
            Some(UpdateNotice::Failed {
                reason: "いまの版 0.3.0-dev を読めません".to_owned()
            })
        );
    }

    #[test]
    fn メニューから確かめたら最新と失敗も知らせる() {
        assert_eq!(
            notice_for("0.3.0", Ok(r#"{"tag_name":"v0.3.0"}"#), true),
            Some(UpdateNotice::UpToDate {
                version: "0.3.0".to_owned()
            })
        );
        assert_eq!(
            notice_for("0.3.0", Err("通信できません"), true),
            Some(UpdateNotice::Failed {
                reason: "通信できません".to_owned()
            })
        );
        assert_eq!(
            notice_for("0.3.0", Ok("{}"), true),
            Some(UpdateNotice::Failed {
                reason: "応答を読めませんでした".to_owned()
            })
        );
    }
}
