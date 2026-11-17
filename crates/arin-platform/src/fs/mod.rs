//! ファイルシステム(設計書 §4 の `fs/`、§5.4「上書きしない移動・コピー」、計画書 T40a)。
//!
//! [`LocalFileSystem`] が `arin_core::ports::fs::FileSystemPort` を実装する。フォルダの読み込みと、
//! 上書きしない作成・移動・コピーを置く。上書きしないことは、確認してから実行するのではなく、
//! OS が「既にあれば失敗する」ことを保証する呼び出し(`O_CREAT | O_EXCL`、`mkdir(2)`、
//! `renamex_np(2)` の `RENAME_EXCL`)で守る(設計書 §1.3 #11・§1.4 #14)。削除の操作は置かない。
//!
//! ゴミ箱(`trash`)は `trash.rs`(T40c。`NSFileManager` の `trashItemAtURL`。ADR-007 の (a))。
//! 入れられなかったときは失敗を返し、別の方式や削除に切り替えない。
//!
//! Markdown の文書の読み書き(`document.rs`。[`LocalDocuments`]。ADR-015 の 5)は、ここで唯一の置き換えの経路を持つ
//! (一時ファイルに書いてから改名で置き換える。その場では書かない。消す操作は置かない)。

mod document;
mod local;
mod trash;

pub use document::{LocalDocuments, SAVE_PREFIX};
pub use local::{LeftoverReport, LocalFileSystem, STAGING_PREFIX};
