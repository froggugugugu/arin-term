//! `NSURL`(objc2-foundation)を呼ぶ部分。unsafe はここの 1 か所だけに置く。

use std::path::{Path, PathBuf};

use objc2::rc::autoreleasepool;
use objc2::runtime::Bool;
use objc2_foundation::{
    NSData, NSError, NSString, NSURL, NSURLBookmarkCreationOptions, NSURLBookmarkResolutionOptions,
};

use super::{BookmarkError, Resolved, classify, path_str};

fn from_ns_error(e: &NSError) -> BookmarkError {
    classify(
        e.domain().to_string(),
        e.code(),
        e.localizedDescription().to_string(),
    )
}

/// フォルダのブックマークを作る(セキュリティスコープなし。オプションは空)。
pub fn create_bookmark(dir: &Path) -> Result<Vec<u8>, BookmarkError> {
    let path = path_str(dir)?;
    autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath_isDirectory(&NSString::from_str(path), true);
        let data = url
            .bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
                NSURLBookmarkCreationOptions::empty(),
                None,
                None,
            )
            .map_err(|e| from_ns_error(&e))?;
        Ok(data.to_vec())
    })
}

/// ブックマークを解決する。UI を出さず、ボリュームのマウントもしない(`WithoutUI | WithoutMounting`)。
pub fn resolve_bookmark(data: &[u8]) -> Result<Resolved, BookmarkError> {
    if data.is_empty() {
        // OS も 259(形式が違う)を返すが、呼ぶまでもない
        return Err(BookmarkError::Corrupt);
    }
    autoreleasepool(|_| {
        let ns_data = NSData::with_bytes(data);
        let mut stale = Bool::NO;
        // SAFETY: objc2-foundation がこのメソッドを unsafe にしているのは、`bookmarkDataIsStale` の
        // ポインタの有効性を型で保証できないため。渡すのはこの関数のローカル変数への排他参照で、
        // 呼び出しの間ずっと有効であり、OS はそこに BOOL を 1 つ書くだけ。他の引数は所有済みの値か None。
        let url = unsafe {
            NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
                &ns_data,
                NSURLBookmarkResolutionOptions::WithoutUI
                    | NSURLBookmarkResolutionOptions::WithoutMounting,
                None,
                &mut stale,
            )
        }
        .map_err(|e| from_ns_error(&e))?;
        // ファイルのブックマークから解決した URL は必ずパスを持つ。持たなければデータがおかしい
        let path = url.path().ok_or(BookmarkError::Corrupt)?.to_string();
        Ok(Resolved {
            path: PathBuf::from(path),
            stale: stale.as_bool(),
        })
    })
}
