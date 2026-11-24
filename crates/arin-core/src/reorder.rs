//! 並びの中の 1 つを別の位置へ動かす(カードとタブのドラッグ。ADR-019)。

/// `items` の `from` の位置の要素を、動かしたあとに `to` の位置に来るように動かす。`to` が末尾より大きければ末尾。
/// 位置が変わったら `true`(`from` が範囲の外なら何もせず `false`)。
pub(crate) fn move_item<T>(items: &mut Vec<T>, from: usize, to: usize) -> bool {
    if from >= items.len() {
        return false;
    }
    let to = to.min(items.len() - 1);
    if from == to {
        return false;
    }
    let item = items.remove(from);
    items.insert(to, item);
    true
}

#[cfg(test)]
mod tests {
    use super::move_item;

    #[test]
    fn 後ろへも前へも動かせ末尾より先は末尾になる() {
        let mut v = vec!['a', 'b', 'c', 'd'];
        assert!(move_item(&mut v, 0, 2));
        assert_eq!(v, ['b', 'c', 'a', 'd']);
        assert!(move_item(&mut v, 3, 0));
        assert_eq!(v, ['d', 'b', 'c', 'a']);
        assert!(move_item(&mut v, 1, 99));
        assert_eq!(v, ['d', 'c', 'a', 'b']);
    }

    #[test]
    fn 同じ位置と範囲の外は何もしない() {
        let mut v = vec![1, 2, 3];
        assert!(!move_item(&mut v, 1, 1));
        assert!(!move_item(&mut v, 5, 0));
        assert!(!move_item(&mut v, 2, 7));
        assert_eq!(v, [1, 2, 3]);
    }
}
