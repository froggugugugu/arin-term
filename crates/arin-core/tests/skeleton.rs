//! テストの土台が動くことを確かめる最小のテスト。

#[test]
fn クレートを読み込めて版を返す() {
    let version = arin_core::version();

    assert!(!version.is_empty());
    assert_eq!(version.split('.').count(), 3);
}
