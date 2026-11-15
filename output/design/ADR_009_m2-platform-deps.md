# ADR-009: M2 の OS 連携(ブックマーク・ルートの現在位置・ペーストボード)に、Tauri がすでに引き込んでいるクレートを同じ版で使う

- ステータス: Accepted
- 日付: 2026-11-15
- 決定者: AI(司令塔とサブエージェント)。新しいクレートが `Cargo.lock` に増えないため、本人から任された自律の範囲で決めた。本人の了承は M2 の報告で求める
- 関連: `ARCH_multi-terminal.md` §1.4 #12・#13・#19、§5.4、§7.1「ルートフォルダへの追従」。計画 T31b・T31c。`PROGRESS.md` の「M2 の着手時の確認(T28)」の (c)

## コンテキスト

M2 では `arin-platform` に、次の OS 連携を足す。

- 登録したフォルダのブックマークを作って解決する(FR-007。`NSURL`)
- 登録するフォルダのパスを正規化し(FR-010。二重登録の判定に使う)、開いたままにしたフォルダの現在のパスを見回りで調べる(FR-007。`fcntl` の `F_GETPATH`)
- ルートの絶対パスをペーストボードに書く(FR-064。`NSPasteboard`)
- プロジェクト ID の乱数(`/dev/urandom`)と時計(`Instant`・`SystemTime`)

M1 までの `arin-platform` の依存は `portable-pty` だけだった。外部通信をしない(NFR-008)ため、通信のクレートを組み込まないことと、依存の版を正確に固定すること(ADR-003。公開から 2 週間未満の版を避ける)を守る。

## 判断

ルートの `Cargo.toml` の `[workspace.dependencies]` に次を足し、`arin-platform` だけが `workspace = true` で使う。どれも Tauri(と、その依存)が同じ版ですでに `Cargo.lock` に載せているクレートで、新しい `[[package]]` は増えない。

| クレート | 版 | feature | 使うところ | 理由 | タスク |
| -------- | -- | ------- | ---------- | ---- | ------ |
| `objc2` | =0.6.5 | 既定 | `bookmark/`、`desktop/` | Objective-C のオブジェクトの保持(`Retained`)と autoreleasepool、`msg_send!` | T31b |
| `objc2-foundation` | =0.3.2 | `std`・`NSURL`・`NSData`・`NSError`・`NSString`・`NSArray`(既定の feature は切る) | `bookmark/`、`desktop/` | ブックマークの作成と解決(`NSURL`)。Tauri が同じ feature で使っている | T31b |
| `base64` | =0.23.1 | 既定 | `bookmark/` | ブックマークのバイト列を `state.json` に入れる文字列にする(標準の符号化)。`arin-core` には文字列として渡す | T31b |
| `libc` | =0.2.191 | 既定 | `watch/` | `fcntl(F_GETPATH)`、`O_EVTONLY`・`O_DIRECTORY`、`ENOENT` | T31b |
| `objc2-app-kit` | =0.3.2 | `std`・`NSPasteboard`(既定の feature は切る) | `desktop/` | ペーストボードに文字列を書く。Tauri が `NSPasteboard` の feature をすでに有効にしている | T31c |

- `unsafe` は 4 か所で、どれも `// SAFETY:` を付けた: ブックマークの解決(`bookmarkDataIsStale` のポインタ)、`fcntl(F_GETPATH)`、`NSPasteboardTypeString`(外部の定数)、名前付きペーストボードの解放(`releaseGlobally`。objc2-app-kit 0.3.2 が包んでいないため `msg_send!`)
- 乱数と時計は標準ライブラリだけで書き、クレートを足さない(`uuid`・`rand`・`getrandom` は足さない。計画 M2 の【仮定】1)

### 実装で確かめたこと(2026-11-15、この Mac。macOS 26、APFS)

- パスの正規化は `std::fs::canonicalize`(realpath(3))を使う。シンボリックリンクを解決し、大文字・小文字の表記が違うパス(親のフォルダの名前も含む)をディスク上の表記にそろえた。開いた記述子の `F_GETPATH` もディスク上の表記を返したが、記述子を開かずに済む(読む権限の無いフォルダでも使える)方を選んだ
- `F_GETPATH` は改名・別の親への移動のあとも現在のパスを返した。**削除したあとも消える前のパスを返した**(設計書 §1.4 #13 の「削除後は取得に失敗する」と違う)。削除後は記述子の `fstat` が `ENOENT` になるので、それで「見つからない」と判定する。念のため、返ったパスにいまあるものの装置番号と inode 番号が、開いたフォルダと一致するかも確かめる(一致しなければ 3 回まで取り直し、だめなら「見つからない」)
- フォルダは `O_EVTONLY` で開く(外付けのボリュームの取り外しを妨げないため)
- 名前付きのペーストボードに書いて読み戻せた。テストはユーザーのクリップボード(一般のペーストボード)に触れず、終わったら解放する

### 確かめたこと(依存)

- `git diff Cargo.lock` は `arin-platform` の依存の一覧に `libc`・`objc2-app-kit` の 2 行が増えただけで、新しい `[[package]]` は 0 件
- `cargo tree -i reqwest --target aarch64-apple-darwin` は該当なし(通信のクレートは組み込まれない)
- 境界検査(`node scripts/check-boundaries.mjs`)は違反なし。`arin-core` が `objc2`・`libc` を推移的に持たないことも検査している

## 検討した代替案

| 案 | 採らなかった理由 |
| -- | ---------------- |
| ブックマークに `core-foundation-sys` 0.8.7 を使う | T28 で同じ結果を確かめたが、ペーストボードで `objc2` の系統が要るので揃えた |
| `F_GETPATH` の代わりに `nix` クレート | `Cargo.lock` に無く、新しいクレートが増える。呼ぶのは 1 か所だけ |
| ペーストボードを `pbcopy` の起動で書く | 子プロセスを起こすのは重く、テストで書き込み先(名前付きのペーストボード)を差し替えられない |
| ペーストボードに Tauri の clipboard プラグインを使う | プラグインの権限を画面に与える形になり、Rust から直接書く設計(§9.1)と合わない。クレートも増える |
| base64 を `arin-core` に小さな関数で書く | `arin-core` はブックマークの中身を知らなくてよい(T28 の決定)。`arin-platform` に置けば既存のクレートで済む |

## 影響

- Tauri を上げたら、上の 5 つが同じ版のままか(`Cargo.lock` に 2 つの版が載らないか)を確かめ、ずれたら Tauri の版に合わせて上げる
- 版の公開日はこの ADR では確かめていない。どれも M0(2026-11-14)の時点で Tauri の依存として `Cargo.lock` に入っていた版
- 見直す条件: ブックマークや `F_GETPATH` の動作が OS の更新で変わったとき、外付け・ネットワーク・iCloud Drive のフォルダで問題が出たとき(未確認)
