# src/ipc/generated

Rust の型から `ts-rs` で生成した TypeScript の型。**手で編集しない。**

- 入口の型は `crates/arin-core/tests/generated_types.rs` の `collect_generated` に書く(参照している型はたどって全部生成する)
- 再生成: `ARIN_UPDATE_GENERATED=1 cargo test -p arin-core --test generated_types`
- `cargo test`(`npm test`)は、生成し直した内容とここにあるファイルが同じであることを確かめる。違えば失敗する
