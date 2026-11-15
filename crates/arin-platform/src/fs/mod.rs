//! ファイルシステム(設計書 §4 の `fs/`、§5.4「上書きしない移動・コピー」、計画書 T40a)。
//!
//! [`LocalFileSystem`] が `arin_core::ports::fs::FileSystemPort` を実装する。フォルダの読み込みと、
//! 上書きしない作成・移動・コピーを置く。上書きしないことは、確認してから実行するのではなく、
//! OS が「既にあれば失敗する」ことを保証する呼び出し(`O_CREAT | O_EXCL`、`mkdir(2)`、
//! `renamex_np(2)` の `RENAME_EXCL`)で守る(設計書 §1.3 #11・§1.4 #14)。削除の操作は置かない。
//!
//! ゴミ箱(`trash`)は T40c で `trash.rs` に足す。それまでは未実装のエラーを返し、何も変えない。

mod local;

pub use local::{LocalFileSystem, STAGING_PREFIX};
