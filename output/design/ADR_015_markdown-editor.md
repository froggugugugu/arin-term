# ADR-015: Markdown のファイルを、ターミナルのタブと並ぶエディタのタブで編集・プレビューする(M4.8)

- 状態: Proposed(本人が 2026-11-17 に求めた。形は AI が決めた。【要確認】3 点と PRD への反映は本人の了承待ち)
- 日付: 2026-11-17
- 関係: ADR-001(xterm.js の隔離)、ADR-003(画面の部品を React の外に置く)、ADR-007(ファイル操作)、ADR-011(タイル表示)、ADR-012(ショートカットの表)、ADR-013(設定画面・`state.json` の `ui`)、ADR-014(表は 32 行、`reset_appearance`)

## 背景(本人の要望と回答。原文)

- 「mdslideプロジェクトのようなマークダウンエディタ機能が欲しい(マークダウン限定で良い)。」
- 範囲: 「編集+プレビュー」
- キー操作: 「通常が既定、Vim に切替可」

PRD(`output/prd/PRD_multi-terminal.md:52`・`:58`)は非ゴール N1(テキストエディタ)と N7(ファイルの中身に触れる機能)を持つ。この ADR は、**拡張子 `.md`・`.markdown` のファイルに限って** N1 と N7 を外す。**PRD の書き換えは本人の了承のあと**(【要確認】3)。全文検索・差分・LSP・ほかの言語の編集は非ゴールのまま。

## 調べて分かった事実(決定の根拠)

| 事実 | 出どころ |
| ---- | -------- |
| タブは Rust の `TerminalStore`(プロジェクトごとの並びと表示中)が持ち、写し `AppSnapshot` で画面へ渡す。画面は写しを書き換えない | `crates/arin-core/src/terminal/store.rs`、`crates/arin-core/src/workspace/state.rs:245-295` |
| ⌘W はメニューから Rust が直接 `close_active_terminal` を呼ぶ(確認なし)。⌘T・前後のタブも Rust | `src-tauri/src/menu.rs:335-357` |
| 終了の確認は Rust の `quit_or_ask` が動作中のターミナルだけを見て `quit-requested` を送る | `src-tauri/src/lifecycle.rs:105-124` |
| 起動時はプロジェクトごとにタブを 1 枚作るだけで、タブの並びは保存しない | `crates/arin-core/src/workspace/state.rs:62-114` |
| タイルはターミナルだけ。タブバーはタブ表示のときだけ描く | `src/ui/terminals/TerminalArea.tsx:90-116` |
| 「切り取り・コピー・ペースト」はメニューの標準の項目(`PredefinedMenuItem`)。「取り消す」の項目は無い | `src-tauri/src/menu.rs:287-289` |
| `tree-changed` は**選択中のプロジェクトだけ**を監視し、中身はプロジェクトの ID と変わったフォルダ(相対)か `fullReload` | `src-tauri/src/tree_watch.rs:4`、`crates/arin-core/src/tree/types.rs:37-44` |
| 「開く」(`open_entry`)はふつうのファイルだけで、リンク・実行できるもの・ルートの外は開かない(`PathScopeGuard`) | `src-tauri/src/commands/desktop.rs:76-111` |
| ファイルの書き込みは親を `O_NOFOLLOW_ANY` で開いて fd 相対。移動は `RENAME_EXCL`、作成は `O_EXCL`。削除は無い | `crates/arin-platform/src/fs/local.rs:1-40`、`fs/mod.rs:6` |
| キーの受け手は表の受け口(`menu-item`・`edit-item` は `app`、`screen` はその場の部品)で決める | `src/logic/keys/keyOwner.ts:21-26` |
| CSP は `style-src-elem 'self' 'unsafe-inline'`・`style-src-attr 'unsafe-inline'`、スクリプトは `'self'` だけ | `src-tauri/tauri.conf.json:26-36`、`scripts/check-boundaries.mjs:70-79` |
| `⌘S` は予約のキーに無い | `crates/arin-core/src/shortcuts/reserved.rs:10-28` |

## 決定

**`.md`・`.markdown` のファイルを、Rust の `Workspace` が持つ「エディタのタブ」(ターミナルのタブの後ろに並ぶ)で開き、CodeMirror 6 で編集し、`@lezer/markdown` の構文木から組み立てた React の要素でプレビューし、保存は同じフォルダの一時ファイルからの改名で行う。**

### 1. 部品と境界

- 依存(版は正確に固定。mdslide と同じ): `codemirror` 6.0.2、`@codemirror/state` 6.7.2、`@codemirror/view` 6.43.10、`@codemirror/lang-markdown` 6.5.2、`@codemirror/language` 6.12.4、`@lezer/highlight` 1.2.3、`@replit/codemirror-vim` 6.4.0。直接 import する推移的な依存も直接の依存に足して固定する: `@lezer/markdown` 1.7.2、`@lezer/common` 1.5.2、`@codemirror/commands` 6.11.0、`@codemirror/search` 6.7.2(どれも mdslide の `package-lock.json` の版)。どれも公開から 2 週間を過ぎている(ADR-003 の規則。着手時に `npm view <名前>@<版> time` で確かめる)
- **境界**(`@xterm/*` と同じ形。`scripts/check-boundaries.mjs` に規則と見本を足す):
  - `codemirror`・`@codemirror/*`・`@lezer/*`・`@replit/*` を import できるのは `src/editor/codemirror/` だけ
  - `src/ui/` は `src/editor/codemirror/` を import しない(`src/editor/` の口と `src/state/` 経由)。`src/editor/codemirror/` は `src/ipc/` を import しない
- 置き場所: `src/editor/engine.ts`(`EditorSurface` の口。`src/terminal/engine.ts` にならう)、`src/editor/EditorManager.ts`(エディタの部品を React の外で持ち、置き場所へ付け外しする。ADR-003 の xterm と同じ。タブを替えても取り消しの履歴とカーソルが残る)、`src/editor/codemirror/`(CodeMirror の組み立て、Markdown の解析)、`src/ui/editor/`(パネル・プレビュー・切替)、`src/logic/editor/`(純粋な規則)
- エディタの JS は**初めてエディタを開くときに動的 import で読む**(`import("./codemirror/…")`)。起動とターミナルだけの使い方には重さが乗らない。`script-src 'self'` のままで読める
- `markdown()` には `codeLanguages` を渡さない(コードの言語を動的に読み込ませない)。`base` は GFM(`markdownLanguage`)

### 2. プレビュー(HTML の文字列を作らない)

- `src/editor/codemirror/markdownAst.ts` が `@lezer/markdown` の構文木(GFM: 表・タスクリスト・取り消し線・自動リンク)を、**自前の型の木**(`src/editor/markdownTypes.ts` の `MdBlock`・`MdInline`。React も lezer も含まない純粋なデータ)に変える。`src/ui/editor/MarkdownPreview.tsx` がその木を React の要素にする。HTML の文字列・`innerHTML`・`dangerouslySetInnerHTML`・無害化の部品はどこにも無い
- 出すもの: 見出し 1〜6、段落、強調・太字・取り消し線、インラインのコード、コードの塊(色付けなし、等幅)、引用、番号つき・なしのリスト、タスク(印は読み取り専用の `☑`/`☐` の文字。`<input>` を作らない)、表(揃えを守る)、水平線、改行
- 生の HTML(`HTMLBlock`・`HTMLTag`・`Comment`・`ProcessingInstruction`)は**書かれた文字のまま**出す(淡い等幅)
- 画像は読み込まない。`[画像: 代わりの文字]` を出す(`<img>` を作らない)
- リンクは開かない。`<a>` を作らず、`<span class="md-link">` に下線だけ付け、`title` に URL の文字を出す(クリックしても何もしない)
- **方針の穴として見つけたものと手当て**:
  1. 文字参照(`&amp;`・`&#x3C;`)は lezer が `Entity` の節にするだけで、変換しない。自前で数値の参照と `amp`・`lt`・`gt`・`quot`・`apos`・`nbsp` だけを文字に直し、ほかの名前の参照は書かれたまま出す(HTML の名前の表 2000 件余りを持ち込まない)
  2. 深い入れ子(`>` を数千重ねるなど)で React の描画が再帰しすぎる。入れ子は 32 段で打ち切り、それより深い中身は文字のまま出す
  3. 大きい文書でキーのたびに解析すると重い。プレビューが見えているときだけ、最後の変更から 150 ms 後に解析する(上限 1 MiB の文書で計測を残す。合否にしない)
  4. 見出しから `id` を作らない(文書の中身で DOM の名前を作らない)
  5. CodeMirror の Markdown のモードはコードの塊の言語名で別の言語を読み込みうる → `codeLanguages` を渡さないことで止める(1 に書いた)
  6. リンクを見た目だけにすると、本人は「押せそうで押せない」に戸惑いうる → `title` に「リンクは開けません: <URL>」を出す。開く機能は後の版で `open_entry` 相当の Rust の口を通して足せる(却下した案の表)

### 3. エディタのタブ(Rust の状態に入れる。いちばん小さい形)

- **Rust の `Workspace` が持つ**。`crates/arin-core/src/editor/`(新): `EditorId`(`"editor-<n>"`)、`EditorTab { id, project: ProjectId, path: RelPath, dirty: bool, read_only: Option<ReadOnlyReason>, stamp: FileStamp }`、`EditorStore`(プロジェクトごとの並び)。`TerminalStore` は変えない
- プロジェクトごとに `shown_editor: Option<EditorId>` を持つ。`Some` のとき、タブ表示ではそのエディタを表示中のターミナルの代わりに出す。ターミナルのタブを選ぶ・⌘T で作ると `None` に戻る
- 理由: ⌘W・前後のタブ・終了の確認・プロジェクトを外す確認がすべて Rust にあるので、Rust がエディタを知らないと ⌘W が裏のターミナルを閉じる(`menu.rs:335-340`)。画面だけで持つと Rust に「表示中はエディタ」を別に伝える口が要り、正本が 2 つになる
- 写し: `AppSnapshot.editors: EditorTabSnapshot[]`(`id`・`projectId`・`path`・`label`(ファイル名)・`dirty`・`readOnly`)。`ProjectCardSnapshot` に `editorIds`・`shownEditorId`。文書の中身は写しに載せない(コマンドの応答で渡す)
- タブバー: ターミナルのタブの後ろにエディタのタブを並べる(順は開いた順。ターミナルとの混在の並べ替えはしない)。エディタのタブはプロジェクト色の丸の代わりに文書の印、未保存なら閉じるボタンの位置に「●」(macOS と同じ)
- 前後のタブ(既存の `terminal.previous-tab`・`next-tab`)は、ターミナル → エディタの順で 1 周する
- **タイル表示**: タイルはターミナルだけのまま(ADR-011 の規則と `tauriMock.ts` の写しを変えない)。タイル表示でエディタを開くと**タブ表示に替えて**そのエディタを出す(【要確認】1)。タイル表示に戻すとエディタは隠れる(閉じない)
- **再起動**: エディタのタブは保存せず、開き直さない(ターミナルのタブも保存していない。`state.json` の版は 1 のまま。【要確認】2)。未保存の変更は 5 の終了の確認で守る
- アテンションの印: 表示中がエディタのとき、そのプロジェクトのターミナルは「見ている」に数えない
- ルートが見つからなくなったプロジェクトのエディタは残す。保存は「フォルダが見つかりません」で失敗する。ルートへの追従でパスが変わっても相対パスで続けて使える
- ツリーでの名称変更・移動・ゴミ箱で開いている文書が動いても、エディタは追わない(保存のとき「見つからない」になる。6)

### 4. コマンドとイベント(引数はプロジェクトの ID・相対パス・エディタの ID だけ。絶対パスを画面に渡さない)

| コマンド | 引数 | 応答 |
| -------- | ---- | ---- |
| `open_document` | `{ projectId, path }` | `OpenDocumentOutcome`: `opened { editorId, text, lineEnding, readOnly }` / `already-open { editorId }`(表示するだけ) / `not-allowed { reason }`(`not-markdown`・`link`・`directory`・`too-large`・`not-text`・`outside-root`) / `not-found` / `failed { message }` |
| `save_document` | `{ editorId, text, overwriteChanged }` | `SaveDocumentOutcome`: `saved` / `changed-outside` / `missing` / `read-only` / `too-large` / `failed { message }` |
| `check_document` | `{ editorId, withText }` | `CheckDocumentOutcome`: `unchanged` / `changed { text?, lineEnding }` / `missing` |
| `set_document_dirty` | `{ editorId, dirty }` | なし(変わったら `notify`)。画面は変わり目だけ送る |
| `select_editor` | `{ editorId }` | なし |
| `close_editor` | `{ editorId, discard }` | `closed` / `needs-confirmation`(未保存で `discard` が偽) |
| `set_editor_vim` | `{ enabled }` | なし |

- 「既定のアプリで開く」は既存の `open_entry` のまま(コマンドを足さない)
- 新しいイベント `editor-close-requested { editorId }`: ⌘W(メニュー)で表示中のエディタが未保存のとき、Rust は閉じずにこれを送り、画面が確認する
- `quit-requested` の中身 `QuitRequest` に `unsaved: UnsavedEditor[]`(`editorId`・`projectName`・`path`)を足す。動作中のターミナルが無くても未保存があれば確認する
- プロジェクトを外す確認(`RemoveOutcome::NeedsConfirmation`)に未保存のエディタも載せる
- 標準エラーの頭 `[arin:editor]`: 保存の一時ファイルを片付けられずに残した(場所を出す。`[arin:fileops]` と同じ扱い)

### 5. 読み込みと保存の規則(`arin-platform`。`fs/local.rs` の fd 相対の作りに乗せる)

- 対象: `PathScopeGuard` を通る(ルートの外へ出るリンクを経由しない)、**ふつうのファイル**(リンク自体は開かない。`open_entry` と同じ)、拡張子 `.md`・`.markdown`(大文字小文字を問わない)、大きさ **1 MiB 以下**、**UTF-8**(不正な並び・NUL を含むものは `not-text`)
- 先頭の BOM は外して渡し、保存で付け直す。改行は文書に `\r\n` があれば `crlf`、無ければ `lf` を返し、画面は CodeMirror の `EditorState.lineSeparator` をそれに合わせる(混在は多いほうにそろう。リスク参照)
- 読み取り専用で開くもの(`readOnly`): ハードリンクのあるファイル(`st_nlink > 1`。改名で置き換えるとリンクが切れる)、自分が書けない・持ち主が自分でない・フォルダに書けないもの(一時ファイルを作れない、または持ち主を写せない)
- **版**(`FileStamp`): `dev`・`ino`・`size`・`mtime`(ナノ秒)と中身の 64 ビットの要約(`std::hash::DefaultHasher`。固定の鍵なので決定的。改ざんの検出ではなく変更の検出に使う)。`arin-core` は要約の計算と比べ方だけを持ち(入出力なし)、読むのは `arin-platform`。`stat` の 4 つが同じなら変わっていない。違っても要約が同じ(`touch` など)なら変わっていないとみなし、版を新しくする
- **保存の手順**(`save_document`。本人が ⌘S・保存のボタン・確認の「上書き保存」を押したときだけ。自動保存はしない):
  1. 親を `O_NOFOLLOW_ANY` で開き、対象を `fstatat(AT_SYMLINK_NOFOLLOW)`。ふつうのファイルでない → `failed`。無い → `missing`。版が違い `overwriteChanged` が偽 → `changed-outside`(何も書かない)
  2. 同じフォルダに `.arin-save-<pid>-<連番>` を `O_CREAT | O_EXCL | O_NOFOLLOW` で作り、元のファイルから `fcopyfile(COPYFILE_METADATA)` で権限・ACL・拡張属性(Finder のタグなど)を写す(libc 0.2.191 に宣言が無ければ `copyfile` と同じく自前で宣言する。着手時に確かめる)
  3. 中身を書き `fsync` する
  4. もう一度 1 の検査をしてから、`renameatx_np(dir, tmp, dir, name, 0)` で**置き換える**。1 で `missing` を本人が「作る」と決めたときは `RENAME_EXCL`(上書きしない)
  5. 新しい版を `fstat` して持ち直し、`saved` を返す
  6. 2〜4 で失敗したら、一時ファイルは消さずに残し `[arin:editor]` で場所を出す(`.arin-copy-` と同じ。削除の経路を作らない)。元のファイルは変わっていない
- 監視と一覧は `.arin-save-` で始まる名前を `.arin-copy-` と同じに扱う(監視は知らせない、隠しファイル表示が OFF なら一覧に出さない)
- **`docs/development-patterns.md` §1「削除と上書きの経路を作らない」の例外**(この ADR の採用で §1 に書き足す。条件をすべて満たすときだけ):
  - 経路は `save_document` の 1 つだけ。置き換えるのは `open_document` で開いた文書と同じ相対パスの、拡張子が `.md`・`.markdown` のふつうのファイルだけ
  - 本人の保存の操作があったときだけ。外で変わっていたら、本人が「上書き保存」を選ばない限り置き換えない
  - その場で切り詰めて書かない。必ず同じフォルダの新しい一時ファイルに書き終えてから改名で置き換える(途中で落ちても元の中身か新しい中身のどちらか)
  - ハードリンク・書けない・持ち主が違うファイルは置き換えない(読み取り専用)
  - ほかの項目(一時ファイルを含む)を消す経路は作らない
- **外での変更**:
  - 選択中のプロジェクト: 画面が `tree-changed` を受け、開いている文書の親フォルダが `dirs` にある(または `fullReload`)なら `check_document` を呼ぶ
  - ほかのプロジェクト(監視していない): そのエディタのタブを表示に戻したとき、ウィンドウが前面に戻ったときに `check_document` を呼ぶ
  - 未保存の変更が無ければ `withText: true` で読み直して置き換える(カーソルは同じ行に寄せる)。未保存があれば読み直さず、エディタの上に「ほかのアプリで変更されました」の帯を出す(保存すると 1 の `changed-outside` の確認になる)
  - `missing` なら帯で「ファイルが見つかりません」。保存すると「同じ名前で作る」か聞く
- 保存の確認(`changed-outside`)の選択肢: 「上書き保存」「読み直す(変更を捨てる)」「キャンセル」(既定はキャンセル)

### 6. キー

- 表に 1 行足す: `editor.save`、既定 `Cmd+S`、受け口 `screen`(ADR-014 の 32 行 → **33 行**)。メニューの項目は作らない(「ファイル」メニューを足さない。保存のボタンのツールチップに表から引いたキーを出す)。設定ファイルの見本に 1 行
- エディタにフォーカスがあるときの受け手 `editorKeyOwner(table, chord)`(`src/logic/keys/`。`terminalKeyOwner` と同じ考え):
  - 表の受け口が `menu-item`・`edit-item` の行のキー → `app`(CodeMirror は処理せず、メニューへ。⌘W・⌘T・⌘Q・⌘,・⇧⌘G・⌘1〜9・前後のタブなど)。CodeMirror の一番前の `keydown` の受け口で、`app` のキーは CodeMirror に渡さない(`preventDefault` もしない)
  - `editor.save` のキー → 保存
  - それ以外(`screen` の木のキーを含む) → CodeMirror(通常は既定の keymap、Vim はその keymap)
- ⌘C・⌘X・⌘V はメニューの標準の項目が WebKit の `copy`・`cut`・`paste` を起こし、CodeMirror が受ける。⌘A(`edit.select-all`)は `menu-command` の `select-all` を、画面がフォーカスのある部品(エディタならエディタの全選択)へ振り分ける。⌘Z・⇧⌘Z は表に無いので CodeMirror の取り消し
- Vim: `@replit/codemirror-vim` を `Compartment` で付け外しする(mdslide と同じ。エディタを作り直さない)。`:w` は保存。`Esc` は Vim に渡す
- Vim の切替は `state.json` の `ui.editorVim`(真のときだけ書く。無い・真偽値でないは偽。退避しない。版は 1 のまま)。写し `AppSnapshot.editor = { vim }`。「既定に戻す」(`reset_appearance`)は Vim を戻さない(見た目ではなく操作の選択なので)

### 7. 文字の大きさ

- **ターミナルの文字の大きさ(`fontSizes.terminal`)を、エディタの本文とプレビューの本文の基準に使う**(司令塔の第一案のまま)。理由: どちらも右ペインの「中身」で、ターミナルと並べて見るものだから。画面の文字の大きさ(`fontSizes.interface`)はタブ・切替・帯など枠の部品だけに効く(ADR-013 の分け方を保つ)
- エディタの本文はターミナルと同じ等幅の書体。プレビューの本文はシステムの書体、見出しは `em` で相対、コードは等幅
- 設定画面の行の名前を「ターミナル」から「ターミナルとエディタ」に替える(testid は変えない)

### 8. 見た目

- タブの中の上端に細い帯を 1 本: 左に相対パス(控えめな文字の色)、右に表示の切替(「編集」「並べて」「プレビュー」の 3 つの区切りのある小さなボタン。macOS のセグメントにならう)と「保存」(未保存のときだけ押せる)
- 既定は「並べて」。切替はタブごとに `uiStore` が持つ(保存しない)。並べたときは左右半分、境は細い線 1 本(寸法は変えられない)
- 既存のトークンの色だけ。グラデーション・絵文字・大きな角丸のカード・影の重ねを使わない。プレビューは文字の組みで見せる(行の高さ 1.6、段落の間は余白、表は横線だけ)
- エディタとプレビューの地は、ライト・ダークに合う不透明の地にする(既存の面のトークン。名前は T83b で `tokens.css` から選ぶ)。ADR-014 のターミナルの透過はエディタに当てない(文字の読みやすさを優先)
- 切替は `role="radiogroup"`、帯の「保存」はツールチップにキー、未保存の「●」は読み上げで「未保存」

## 却下した案

| 案 | 長所 | 短所 | 却下の理由 |
| -- | ---- | ---- | ---------- |
| エディタのタブを画面(`uiStore`)だけで持つ | Rust の変更が小さく見える | ⌘W・前後のタブ・終了の確認が Rust にあり、裏のターミナルを閉じる。Rust に「表示中はエディタ」を伝える別の口が要る | 正本が 2 つになり、写しを画面で書き換えない規則と合わない |
| エディタを `TerminalStore` のタブの種類にする(タブを 1 つの並びにする) | ターミナルとエディタを混ぜて並べ替えられる | `TerminalStore`・`sessions.rs`・タイルの規則・`tauriMock.ts` の写しがみな種類の分岐を持つ | 変更が大きすぎる。並べ替えは求められていない |
| エディタをタイルの 1 枚にする | タイル表示のままで見られる | ADR-011 の規則・⌘1〜9 の番号・ページ・`tauriMock.ts` が変わる | 大きい。【要確認】1 の B として残す |
| プレビューを Markdown → HTML の文字列(markdown-it など)+ 無害化 | 対応が広い | `innerHTML` が要り、無害化の部品を足す | 規則(`docs/development-patterns.md` §1)に反する |
| 保存をその場の上書き(`O_TRUNC`) | 権限・持ち主・リンクがそのまま | 途中で落ちると中身が壊れる | 本人のデータを壊しうる |
| `RENAME_SWAP` で入れ替え、古い中身をゴミ箱へ | 元の中身が残る | 保存のたびにゴミ箱が増える | 過剰。改名の置き換えで途中の破損は防げる |
| 版を中身の SHA-256 で取る | 強い | 依存(`sha2` など)が増える | 変更の検出には `stat` と std の要約で足りる |
| リンクを既定のブラウザで開く | 便利 | 外へ出る経路(URL の検査・確認)が増える | 最初の版の範囲の外。あとから Rust の口で足せる |
| `⌘S` をメニューの項目にする(「ファイル」メニュー) | macOS の慣れた場所 | メニューの並び・`app_menu.rs` のテスト・メニューから画面への振り分けが増える | 受け口 `screen` で足りる。あとから受け口だけ替えられる |
| エディタの文字の大きさを画面の文字の大きさに追従 | 枠と揃う | 既定 13 は本文には小さく、ターミナルと並べると差が目立つ | 7 の理由 |

## 受け入れ基準(仮。AC-E)

| AC | 受け入れ基準 |
| -- | ------------ |
| AC-E01 | ツリーで `.md`・`.markdown` をダブルクリック・⌘O・右クリックの「開く」でエディタのタブが開き、表示される。同じファイルは同じタブを出す。ほかの拡張子は既定のアプリのまま。`.md` の右クリックに「既定のアプリで開く」がある |
| AC-E02 | リンク・1 MiB 超・UTF-8 でない・NUL を含む・ルートの外は開かず、理由を知らせる。ハードリンク・書けないファイルは読み取り専用で開き、その旨を帯に出す |
| AC-E03 | 「編集」「並べて」「プレビュー」を切り替えられる。プレビューは見出し・段落・強調・取り消し線・リスト・タスク・表・引用・コード・水平線を出す。生の HTML は文字のまま、画像は代わりの文字、リンクは開かない。プレビューの DOM に `a[href]`・`img`・`iframe`・`script`・`input`・`style` 属性つきの要素が無い |
| AC-E04 | ⌘S(表から引く)・保存のボタン・Vim の `:w` で保存できる。保存のあと権限と拡張属性が同じ、改行コードと BOM が保たれる、タブの「●」が消える。一時ファイルが残らない |
| AC-E05 | 外で変わったファイルを保存すると上書きせずに「上書き保存」「読み直す」「キャンセル」を聞く。消えていたら同じ名前で作るか聞き、作るときは上書きしない |
| AC-E06 | 未保存の変更が無いとき、外での変更を読み直す(選択中のプロジェクトは `tree-changed`、ほかはタブを表示に戻したときとウィンドウが前面に戻ったとき)。未保存があるときは読み直さず帯で知らせる |
| AC-E07 | 未保存のタブを ⌘W・× で閉じると「保存」「保存しない」「キャンセル」を聞く。終了とプロジェクトを外すときの確認に未保存の文書が並ぶ。未保存が無ければ今までどおり |
| AC-E08 | 設定画面に「Vim のキー操作」の切替があり、すぐ効き(エディタを作り直さない)、`state.json` の `ui.editorVim` に残る。既定は通常。「既定に戻す」では変わらない |
| AC-E09 | エディタにフォーカスがあるとき、表の受け口が `menu-item`・`edit-item` のキーはメニューへ届き、それ以外はエディタが受ける。⌘C・⌘X・⌘V・⌘A・⌘Z はエディタの中で効く |
| AC-E10 | エディタとプレビューの本文はターミナルの文字の大きさに追従する。枠は画面の文字の大きさ |
| AC-E11 | タイル表示で開くとタブ表示に替わる(【要確認】1)。タイルにエディタは出ない。前後のタブのキーはターミナル → エディタの順で回る |
| AC-E12 | 再起動でエディタのタブは開き直さない(【要確認】2)。`state.json` の版は 1 のまま |
| AC-E13 | CSP を変えずに動き、`[arin:frontend-error]` が出ない。`codemirror`・`@codemirror/*`・`@lezer/*`・`@replit/*` の import は `src/editor/codemirror/` だけ(境界検査)。エディタを開くまでエディタの JS を読まない |
| AC-E14 | 既存のテストは、理由のある書き換え(表の行数 32→33、写しの形、設定画面の行の名前)だけで通る |
| AC-E15 | 見た目が控えめで、ライト・ダークで読める(本人の確認) |

## 【要確認】(本人に聞く。計画は推奨案で組む)

1. **タイル表示のときにエディタを開いたら** — 推奨 A: タブ表示に替えてエディタを出す(表示モードの保存値も替わる。⇧⌘G で戻せる)。B: タイル表示のまま、拡大のようにエディタを全面に出し「元に戻す」で戻る(タイルの規則は変えないが、画面の分岐と確認が増える)。C: タイル表示では開かず「タブ表示で開けます」と知らせる
2. **再起動で開いていたエディタを開き直すか** — 推奨 A: 開き直さない(ターミナルのタブと同じ。未保存は終了の確認で守る)。B: 開いていた相対パスを `state.json` に保存して開き直す(版を上げずに `ui` か `projects` に足す形になる)
3. **PRD への反映** — 推奨: N1・N7 に「`.md`・`.markdown` の編集とプレビューを除く(ADR-015)」と書き足し、FR を 1 つ足す(仮の受け入れ基準は AC-E01〜E15)。了承まで PRD は書き換えない

## リスク

- **CSP**: CodeMirror(`style-mod`)は、`adoptedStyleSheets` が使えればそれを、無ければ実行時に `<style>` 要素を作る。行の高さや位置などに `style` 属性も書く。どちらも xterm.js と同じ形(`docs/development-patterns.md` §6 の xterm の行)で、いまの `style-src-elem 'self' 'unsafe-inline'`・`style-src-attr 'unsafe-inline'` で足りる見込み。`style-src`・`script-src` は変えない。`EditorView.cspNonce` は使わない。着手時(T82a)に `node_modules` で `eval(`・`new Function` が無いことを確かめ、実機(`npm run app:debug`)で `[arin:frontend-error]` が出ないことを T86a で確かめる。E2E(Vite)では CSP が当たらないので実機の確認が要る
- **バンドルの大きさ**: CodeMirror の中核と Markdown と lezer で圧縮前 350〜450 KB、Vim で 150 KB ほど増える見込み(【仮定】。T82a で `vite build` の出力を記録する)。動的 import で別の塊にし、起動の重さには乗らない
- **メモリ**: エディタのタブ 1 枚ごとに `EditorView` と文書(最大 1 MiB)と取り消しの履歴を持つ。ターミナルのスクロールバックより小さい見込み。プレビューは見えているタブだけ解析する。`npm run perf` に 1 MiB の文書を開いたときの記録を足す(合否にしない)
- **改行の混在**: CodeMirror は読み込みで改行をそろえるので、`\r\n` と `\n` が混ざった文書は保存で 1 種類にそろう(知らせは出さない。`docs/development-patterns.md` §6 に落とし穴として書く)
- **改名の置き換えの隙**: 4 の再検査から改名までのあいだに外で書かれた中身は失われる(ほかのエディタと同じ程度の隙)
- **持ち主・作成日**: 改名で置き換えると inode と作成日が新しくなる。持ち主は自分のファイルだけを書けるものにしているので変わらない
- **ADR-014 との重なり**: 表(32 行)・`reset_appearance`・`state.rs`・設定画面を M4.7 が触る。M4.8 の Rust と設定画面のタスクは M4.7 の実装が済んでから始める
- **copy・paste のイベント**: ターミナル用に画面全体で `copy`・`paste` を受けている処理があれば、エディタにフォーカスがあるときに奪わないか T82d の着手時に確かめる

## 影響

- 良い: 本人が Claude Code などと並べて Markdown を直せる。ターミナル・タイルの規則は変わらない。HTML の差し込みの規則と CSP を保つ
- 悪い: 依存が 11 個増える(境界で隔離)。ファイルを置き換える経路が 1 つできる(5 の条件で限る)。表が 33 行になり、行数を前提にしたテスト・文書を直す
- 文書: `docs/development-patterns.md` §1(例外の条件、境界)・§1.1(`[arin:editor]`)、`docs/data-model.md`(`ui.editorVim`、写しの `editors`・`editor`、コマンド)、`docs/architecture.md`(`src/editor/`・`src/ui/editor/`・`crates/arin-core/src/editor/`)、`docs/project.md`(技術スタックに CodeMirror)
