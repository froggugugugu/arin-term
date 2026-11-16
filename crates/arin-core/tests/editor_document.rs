//! 文書の規則と型(ADR-015 の 3・5、計画書 T81a)。
//!
//! 対象の拡張子、大きさの上限、文字の検査と BOM・改行、版(`FileStamp`)の比べ方、エディタのタブの並び(`EditorStore`)。
//! どれも入出力を持たない。

use arin_core::editor::{
    DOCUMENT_LIMIT_BYTES, DocumentError, EditorError, EditorId, EditorStore, FileStamp, FileStat,
    LineEnding, ReadOnlyReason, decode_document, digest, encode_document, is_markdown_path,
    same_content,
};
use arin_core::paths::RelPath;
use arin_core::project::ProjectId;
use serde_json::json;

fn rel(text: &str) -> RelPath {
    RelPath::parse(text).expect("相対パスにできなかった")
}

fn project(n: u8) -> ProjectId {
    ProjectId::from_random_bytes([n; 16])
}

fn stat(ino: u64, size: u64, mtime_ns: i64) -> FileStat {
    FileStat {
        dev: 1,
        ino,
        size,
        mtime_ns,
    }
}

fn stamp_of(bytes: &[u8]) -> FileStamp {
    FileStamp::new(stat(10, bytes.len() as u64, 1_000), bytes)
}

#[test]
fn 拡張子_md_と_markdown_だけ_大文字も可() {
    for path in [
        "README.md",
        "docs/guide.markdown",
        "NOTES.MD",
        "a/b/Mixed.MarkDown",
        "日本語.md",
    ] {
        assert!(is_markdown_path(&rel(path)), "{path} は対象");
    }
    for path in [
        "",
        "README",
        "README.txt",
        "a.md.txt",
        "a.mdx",
        "md",
        "docs.md/readme",
        ".md",
    ] {
        assert!(!is_markdown_path(&rel(path)), "{path:?} は対象でない");
    }
}

#[test]
fn _1_mib_を超えると大きすぎる() {
    assert_eq!(DOCUMENT_LIMIT_BYTES, 1024 * 1024);
    let limit = usize::try_from(DOCUMENT_LIMIT_BYTES).unwrap();
    let just = vec![b'a'; limit];
    assert!(decode_document(&just).is_ok(), "ちょうど 1 MiB は読める");

    let over = vec![b'a'; limit + 1];
    assert_eq!(
        decode_document(&over),
        Err(DocumentError::TooLarge {
            size: DOCUMENT_LIMIT_BYTES + 1
        })
    );
}

#[test]
fn 不正な_utf8_と_nul_は文字でない() {
    assert_eq!(
        decode_document(&[b'a', 0xFF, b'b']),
        Err(DocumentError::NotText)
    );
    assert_eq!(decode_document(b"a\0b"), Err(DocumentError::NotText));
    // BOM のあとに不正な並び。
    assert_eq!(
        decode_document(&[0xEF, 0xBB, 0xBF, 0xC3]),
        Err(DocumentError::NotText)
    );
    // 空の文書は読める。
    let empty = decode_document(b"").unwrap();
    assert_eq!(empty.text, "");
    assert!(!empty.bom);
    assert_eq!(empty.line_ending, LineEnding::Lf);
}

#[test]
fn bom_を外して覚え保存で付け直す() {
    let bytes = [&[0xEF, 0xBB, 0xBF][..], "# 見出し\n".as_bytes()].concat();
    let decoded = decode_document(&bytes).unwrap();
    assert_eq!(decoded.text, "# 見出し\n");
    assert!(decoded.bom);

    assert_eq!(encode_document(&decoded.text, true), bytes);
    assert_eq!(
        encode_document("# 見出し\n", false),
        "# 見出し\n".as_bytes()
    );

    // BOM の無い文書は BOM を持たない。
    let plain = decode_document("本文".as_bytes()).unwrap();
    assert!(!plain.bom);
}

#[test]
fn crlf_があれば_crlf() {
    assert_eq!(
        decode_document(b"a\r\nb\r\n").unwrap().line_ending,
        LineEnding::Crlf
    );
    // 混ざっていても 1 つでもあれば crlf(改行は画面がそろえる。ADR-015 のリスク)。
    assert_eq!(
        decode_document(b"a\nb\r\nc\n").unwrap().line_ending,
        LineEnding::Crlf
    );
    assert_eq!(
        decode_document(b"a\nb\n").unwrap().line_ending,
        LineEnding::Lf
    );
    // \r だけは crlf ではない。
    assert_eq!(
        decode_document(b"a\rb").unwrap().line_ending,
        LineEnding::Lf
    );
    assert_eq!(
        serde_json::to_value(LineEnding::Crlf).unwrap(),
        json!("crlf")
    );
    assert_eq!(serde_json::to_value(LineEnding::Lf).unwrap(), json!("lf"));
}

#[test]
fn stat_が同じなら同じ() {
    let a = FileStamp::new(stat(10, 3, 1_000), b"abc");
    // 要約が違っても stat の 4 つが同じなら変わっていないとみなす(読み直さずに済ませる)。
    let b = FileStamp {
        digest: a.digest ^ 1,
        ..a
    };
    assert!(a.same_stat(&b.stat));
    assert!(same_content(&a, &b));
}

#[test]
fn stat_が違っても要約が同じなら同じ中身() {
    let a = FileStamp::new(stat(10, 3, 1_000), b"abc");
    // touch(時刻だけが変わった)・同じ中身で置き換えられた(inode が変わった)。
    let touched = FileStamp::new(stat(10, 3, 2_000), b"abc");
    let replaced = FileStamp::new(stat(11, 3, 2_000), b"abc");
    assert!(!a.same_stat(&touched.stat));
    assert!(same_content(&a, &touched));
    assert!(same_content(&a, &replaced));

    // 中身が違えば違う。
    let edited = FileStamp::new(stat(10, 3, 2_000), b"abd");
    assert!(!same_content(&a, &edited));
}

#[test]
fn 要約は決まった値で中身ごとに違う() {
    assert_eq!(digest(b"abc"), digest(b"abc"));
    assert_ne!(digest(b"abc"), digest(b"abd"));
    assert_ne!(digest(b""), digest(b"\0"));
    let stamp = FileStamp::new(stat(1, 3, 0), b"abc");
    assert_eq!(stamp.digest, digest(b"abc"));
}

#[test]
fn 読み取り専用の理由は画面と同じ文字列になる() {
    assert_eq!(
        serde_json::to_value(ReadOnlyReason::HardLink).unwrap(),
        json!("hard-link")
    );
    assert_eq!(
        serde_json::to_value(ReadOnlyReason::NotWritable).unwrap(),
        json!("not-writable")
    );
    assert_eq!(
        serde_json::to_value(ReadOnlyReason::NotOwner).unwrap(),
        json!("not-owner")
    );
    assert_eq!(
        serde_json::to_value(ReadOnlyReason::FolderNotWritable).unwrap(),
        json!("folder-not-writable")
    );
}

#[test]
fn 同じパスを開くと同じ_id() {
    let mut store = EditorStore::default();
    let p = project(1);
    let (first, created) = store.open(&p, rel("README.md"), stamp_of(b"a"), None);
    assert!(created);
    assert_eq!(first, EditorId::new("editor-1"));
    assert_eq!(store.shown(&p), Some(&first));

    let (other, created) = store.open(&p, rel("docs/a.md"), stamp_of(b"b"), None);
    assert!(created);
    assert_eq!(store.shown(&p), Some(&other));

    // 既に開いているものは作らずに表示する(版と読み取り専用は初めに開いたときのまま)。
    let (again, created) = store.open(
        &p,
        rel("README.md"),
        stamp_of(b"zzz"),
        Some(ReadOnlyReason::HardLink),
    );
    assert!(!created);
    assert_eq!(again, first);
    assert_eq!(store.shown(&p), Some(&first));
    let tab = store.tab(&first).unwrap();
    assert_eq!(tab.stamp, stamp_of(b"a"));
    assert_eq!(tab.read_only, None);

    // 別のプロジェクトの同じ相対パスは別のタブ。
    let (elsewhere, created) = store.open(&project(2), rel("README.md"), stamp_of(b"a"), None);
    assert!(created);
    assert_ne!(elsewhere, first);

    let ids: Vec<&EditorId> = store.tabs(&p).map(|t| &t.id).collect();
    assert_eq!(ids, vec![&first, &other], "開いた順");
    assert_eq!(store.find(&p, &rel("docs/a.md")), Some(&other));
    assert_eq!(store.find(&p, &rel("nothing.md")), None);
}

#[test]
fn 閉じると並びから消え次のエディタ_id_は使い回さない() {
    let mut store = EditorStore::default();
    let p = project(1);
    let (a, _) = store.open(&p, rel("a.md"), stamp_of(b"a"), None);
    let (b, _) = store.open(&p, rel("b.md"), stamp_of(b"b"), None);
    let (c, _) = store.open(&p, rel("c.md"), stamp_of(b"c"), None);

    // 表示中の b を閉じると右隣の c が表示中になる。
    store.show(&b).unwrap();
    let closed = store.close(&b).unwrap();
    assert_eq!(closed.id, b);
    assert_eq!(closed.path, rel("b.md"));
    assert_eq!(store.shown(&p), Some(&c));
    assert!(store.tab(&b).is_none());

    // 右隣が無ければ左隣。
    store.close(&c).unwrap();
    assert_eq!(store.shown(&p), Some(&a));
    // 最後の 1 つを閉じると表示中のエディタは無い(ターミナルが出る)。
    store.close(&a).unwrap();
    assert_eq!(store.shown(&p), None);
    assert_eq!(store.tabs(&p).count(), 0);

    // 同じパスを開き直しても新しい ID。
    let (again, _) = store.open(&p, rel("a.md"), stamp_of(b"a"), None);
    assert_eq!(again, EditorId::new("editor-4"));

    assert_eq!(
        store.close(&b),
        Err(EditorError::NotFound(b.clone())),
        "閉じたものは見つからない"
    );
}

#[test]
fn 表示中でないエディタを閉じても表示中は変わらない() {
    let mut store = EditorStore::default();
    let p = project(1);
    let (a, _) = store.open(&p, rel("a.md"), stamp_of(b"a"), None);
    let (b, _) = store.open(&p, rel("b.md"), stamp_of(b"b"), None);
    store.close(&a).unwrap();
    assert_eq!(store.shown(&p), Some(&b));

    // 隠したあと(ターミナルを選んだ)に閉じても、表示中のエディタは無いまま。
    let (c, _) = store.open(&p, rel("c.md"), stamp_of(b"c"), None);
    store.hide(&p);
    store.close(&c).unwrap();
    assert_eq!(store.shown(&p), None);
}

#[test]
fn 未保存の印と版を替えられる() {
    let mut store = EditorStore::default();
    let p = project(1);
    let (a, _) = store.open(&p, rel("a.md"), stamp_of(b"a"), None);

    assert_eq!(store.set_dirty(&a, true), Ok(true));
    assert_eq!(store.set_dirty(&a, true), Ok(false), "変わらなければ偽");
    assert!(store.tab(&a).unwrap().dirty);

    store.set_stamp(&a, stamp_of(b"new")).unwrap();
    assert_eq!(store.tab(&a).unwrap().stamp, stamp_of(b"new"));

    let missing = EditorId::new("editor-99");
    assert_eq!(
        store.set_dirty(&missing, true),
        Err(EditorError::NotFound(missing.clone()))
    );
    assert_eq!(store.show(&missing), Err(EditorError::NotFound(missing)));
}

#[test]
fn プロジェクトを外すとそのエディタだけ消える() {
    let mut store = EditorStore::default();
    let (a, _) = store.open(&project(1), rel("a.md"), stamp_of(b"a"), None);
    let (b, _) = store.open(&project(2), rel("b.md"), stamp_of(b"b"), None);

    let removed = store.remove_project(&project(1));
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].id, a);
    assert_eq!(store.shown(&project(1)), None);
    assert!(store.tab(&b).is_some());
    assert_eq!(store.shown(&project(2)), Some(&b));
}

#[test]
fn エディタの_id_は文字列として_json_になる() {
    assert_eq!(
        serde_json::to_value(EditorId::new("editor-3")).unwrap(),
        json!("editor-3")
    );
    let back: EditorId = serde_json::from_value(json!("editor-3")).unwrap();
    assert_eq!(back.as_str(), "editor-3");
}

// ---- 文書のコマンドの応答の形(ADR-015 の 4、計画書 T81d) ----

#[test]
fn 文書のコマンドの応答の_json_の形() {
    use arin_core::editor::{
        CheckDocumentOutcome, DocumentNotAllowedReason, EditorCloseRequest, OpenDocumentOutcome,
        SaveDocumentOutcome,
    };

    assert_eq!(
        serde_json::to_value(OpenDocumentOutcome::Opened {
            editor_id: EditorId::new("editor-1"),
            text: "# a\n".to_owned(),
            line_ending: LineEnding::Lf,
            read_only: Some(ReadOnlyReason::HardLink),
        })
        .unwrap(),
        json!({
            "kind": "opened",
            "editorId": "editor-1",
            "text": "# a\n",
            "lineEnding": "lf",
            "readOnly": "hard-link"
        })
    );
    assert_eq!(
        serde_json::to_value(OpenDocumentOutcome::AlreadyOpen {
            editor_id: EditorId::new("editor-2"),
        })
        .unwrap(),
        json!({ "kind": "already-open", "editorId": "editor-2" })
    );
    assert_eq!(
        serde_json::to_value(OpenDocumentOutcome::NotAllowed {
            reason: DocumentNotAllowedReason::OutsideRoot,
        })
        .unwrap(),
        json!({ "kind": "not-allowed", "reason": "outside-root" })
    );
    assert_eq!(
        serde_json::to_value(OpenDocumentOutcome::NotFound).unwrap(),
        json!({ "kind": "not-found" })
    );
    assert_eq!(
        serde_json::to_value(SaveDocumentOutcome::ChangedOutside).unwrap(),
        json!({ "kind": "changed-outside" })
    );
    assert_eq!(
        serde_json::to_value(SaveDocumentOutcome::Failed {
            message: "保存できませんでした".to_owned(),
        })
        .unwrap(),
        json!({ "kind": "failed", "message": "保存できませんでした" })
    );
    assert_eq!(
        serde_json::to_value(CheckDocumentOutcome::Changed {
            text: None,
            line_ending: LineEnding::Crlf,
        })
        .unwrap(),
        json!({ "kind": "changed", "text": null, "lineEnding": "crlf" })
    );
    assert_eq!(
        serde_json::to_value(CheckDocumentOutcome::Unchanged).unwrap(),
        json!({ "kind": "unchanged" })
    );
    assert_eq!(
        serde_json::to_value(EditorCloseRequest {
            editor_id: EditorId::new("editor-3"),
        })
        .unwrap(),
        json!({ "editorId": "editor-3" })
    );
    for (reason, text) in [
        (DocumentNotAllowedReason::NotMarkdown, "not-markdown"),
        (DocumentNotAllowedReason::Link, "link"),
        (DocumentNotAllowedReason::Directory, "directory"),
        (DocumentNotAllowedReason::TooLarge, "too-large"),
        (DocumentNotAllowedReason::NotText, "not-text"),
    ] {
        assert_eq!(serde_json::to_value(reason).unwrap(), json!(text));
    }
}
