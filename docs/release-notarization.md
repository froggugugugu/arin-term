# macOS の公証付き配布手順(Developer ID 署名 + 公証 + DMG)

> **状態(2026-11-17)**: §4 のリポジトリの変更は済み(計画書 T51・T52)。§1〜§3・§5〜§7 は本人の作業で未実施(T53・T54)。
> tadcap の同名の手順書を Arin 向けに移したもの。証明書と API キーは tadcap と同じものを使え、Secrets の名前も同じ。

## 0. 全体の流れ

```text
1. Apple Developer Program に登録                         (人間。tadcap で済んでいれば不要)
2. Developer ID Application 証明書を作る                  (人間。tadcap のものを使える)
3. 公証用の認証情報を作る(App Store Connect API キー)    (人間。tadcap のものを使える)
4. リポジトリを変更する(署名 ID の扱い・DMG の公証・検証) (済み)
5. 手元でビルド → 公証 → 検証                             (人間)
6. CI に載せる(GitHub Secrets / Environment)            (人間)
7. tag を push して Releases を確かめる                    (人間)
```

各手順の先頭に、どこで操作するかを書く:

- **【Web】**: ブラウザで開くサイト。URL を併記する(Apple ID でサインイン)
- **【Mac アプリ】**: Mac に入っているアプリ。Spotlight(⌘ + Space)で名前を入れて開く
- **【ターミナル】**: ターミナルで実行するコマンド

## 1. Apple Developer Program に登録

- 【Web】https://developer.apple.com/programs/ から登録する。年額 99 USD(日本の価格は Apple の表示に従う)
- **個人**で登録すると配布者名に本名が出る。**組織**で登録すると組織名が出るが、D-U-N-S 番号が必要
- tadcap で登録済みなら、そのまま使う(Developer ID はアプリごとではなくチームごと)

## 2. Developer ID Application 証明書

tadcap で作った **Developer ID Application** の証明書をそのまま使える(1 枚で複数のアプリに署名できる)。手元のキーチェーンに入っているかを確かめる:

```bash
security find-identity -v -p codesigning
# → "Developer ID Application: <name> (<team id>)" が出れば OK
```

新しく作るときは:

1. 【Mac アプリ】「キーチェーンアクセス」→ メニューの「キーチェーンアクセス」→「証明書アシスタント」→「認証局に証明書を要求」。「ディスクに保存」で CSR を保存する(秘密鍵がログインキーチェーンに作られる)
2. 【Web】https://developer.apple.com/account/resources/certificates/add で **Developer ID Application** を選び、CSR をアップロードする(Account Holder 権限が要る)。中間証明書は **G2 Sub-CA (Xcode 11.4.1 or later)**。**Developer ID Installer** は `.pkg` 用なので不要
3. 【Web】`developerID_application.cer` をダウンロードし、ダブルクリックでキーチェーン(ログイン)に入れる
4. `0 valid identities found` のとき:
   - 中間証明書(Developer ID G2)が無い → 1 行ずつ実行する:

     ```bash
     curl -fsSLo ~/DeveloperIDG2CA.cer https://www.apple.com/certificateauthority/DeveloperIDG2CA.cer
     security import ~/DeveloperIDG2CA.cer -k ~/Library/Keychains/login.keychain-db
     ```

   - 秘密鍵が無い → CSR を作った Mac・ユーザーで入れ直す
5. 【Mac アプリ】CI 用に、この証明書を**秘密鍵ごと** `.p12` に書き出す(「自分の証明書」→ 右クリック →「書き出す」→「個人情報交換(.p12)」、パスワードを付ける)。tadcap 用に書き出した `.p12` があればそれでよい。**`.p12` とパスワードはリポジトリに入れない**

> 「Mac App Distribution」や「Apple Development」の証明書では App Store の外に配布できない。必ず **Developer ID** を選ぶ。

## 3. 公証用の認証情報(App Store Connect API キー)

tadcap で作った API キー(`AuthKey_<key id>.p8`、Key ID、Issuer ID)をそのまま使える。新しく作るときは:

1. 【Web】https://appstoreconnect.apple.com/access/integrations/api の「チームキー」で **+**。アクセスは **Developer**
2. 【Web】`AuthKey_<key id>.p8` をダウンロードする(**1 回しかダウンロードできない**)。**Key ID** と、ページ上部の **Issuer ID** を控える
3. 【ターミナル】(任意)手元のキーチェーンに保存しておくと、公証の記録を読むときに使える:

   ```bash
   xcrun notarytool store-credentials arin-notary \
     --key <path>/AuthKey_<key id>.p8 --key-id <key id> --issuer <issuer id>
   ```

## 4. リポジトリの変更(済み)

| 対象 | 内容 |
| ---- | ---- |
| `scripts/package-mac.sh`(`npm run dist:mac`) | `APPLE_SIGNING_IDENTITY` が無いときは `-`(ad-hoc)で最後まで作る。あるときは、Tauri が app を署名(Hardened Runtime)・公証・staple したあと、**DMG に署名 → 公証 → staple** し、`spctl` で受け入れを確かめる。出力は `release/Arin-<版>-arm64.dmg`・`.zip` |
| `src-tauri/tauri.conf.json` | 署名 ID は書かない(環境変数で渡す)。`bundle.targets` は `app` のまま(`npm run app`・`npm run smoke` を速く保つ)で、dmg は `package-mac.sh` が `--bundles app,dmg` で作る。`bundle.resources` でライセンスの 3 ファイルを `Contents/Resources/licenses/` に入れる |
| `.github/workflows/release.yml` | `v*` の tag の push で、lint・テスト・`licenses:check` → 使い捨てのキーチェーン → `npm run dist:mac` → 公証の検証 → インストーラの検証 → Releases に公開 |
| `scripts/install.sh` | Releases の zip を sha256 で照合して `/Applications` に入れる。リポジトリの場所は先頭の `REPO` の 1 行(いまは仮に `froggugugugu/arin-term`) |

**DMG を別に公証する理由**: Tauri は環境変数があると `.app` を署名・公証・staple する。DMG 自体が公証されるかは未確認【仮定】なので、DMG を明示して公証・staple する。zip には staple できないが、staple 済みの `.app` を zip にするので問題ない。

### エンタイトルメント(設計書 §15.3 #2)

いまは**何も付けない**(A。tadcap と同じ)。確かめたこと(2026-11-17): `package-mac.sh` の ad-hoc の app は Hardened Runtime の印(`flags=…(adhoc,runtime)`)が付いた状態で、`npm run smoke -- --app=<その app>` と `-- --tiles`(シェル 9 個)が通った。PTY での子プロセスの起動と WKWebView は、エンタイトルメント無しで動く。ゴミ箱は `NSFileManager` で、Apple Events を使わない(ADR-007)。

**本人が決めること(B を採るか)**: ターミナルの中で動く CLI(`osascript`・カメラやマイクを使う道具など)の権限の確認は、Arin の名前で出る。Hardened Runtime では、Apple Events(`com.apple.security.automation.apple-events`)・カメラ・マイクなどはエンタイトルメントが無いと使えない見込み【仮定。実機で未確認】。要るなら `src-tauri/Arin.entitlements` を作って `bundle.macOS.entitlements` に指定し、`Info.plist` に利用目的の文を足す。T54 の確認(NFR-008)で、Claude Code などが `osascript` を使う場面を試してから決める。

## 5. 手元でのビルドと公証

【ターミナル】リポジトリのルートで実行する:

```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: <name> (<team id>)"
export APPLE_API_ISSUER=<issuer id>
export APPLE_API_KEY=<key id>
export APPLE_API_KEY_PATH=<path>/AuthKey_<key id>.p8

npm run dist:mac
```

公証は 2 回(app と DMG)あり、それぞれ数分かかる。署名 ID があるのに `APPLE_API_*` が欠けていると、ビルド前に止まる。Tauri はキーチェーンのプロファイルを使えないので、`.p8` のパスを渡す。

公証が `Invalid` になったら、`xcrun notarytool log <submission id> --keychain-profile arin-notary` で理由を見る。よくある原因: タイムスタンプが無い・Hardened Runtime が無効・署名されていないバイナリが混ざっている。

## 6. 検証(完了の証拠にする)

【ターミナル】

```bash
dmg=release/Arin-0.2.0-arm64.dmg
spctl -a -vvv -t install "$dmg"          # → accepted / source=Notarized Developer ID
xcrun stapler validate "$dmg"            # → The validate action worked!

mnt="$(mktemp -d)"
hdiutil attach -nobrowse -readonly -mountpoint "$mnt" "$dmg"
codesign --verify --deep --strict --verbose=2 "$mnt/Arin.app"
spctl -a -vvv "$mnt/Arin.app"            # → source=Notarized Developer ID
hdiutil detach "$mnt"
```

最後に、**別のユーザーアカウントで、ブラウザでダウンロードした DMG から入れて開く**(M5 の完了条件)。「インターネットからダウンロードされました。開きますか?」だけが出て、「開発元を確認できません」「壊れています」が出なければ成功。続けて、ターミナルでシェルが動くこと・ファイルツリーで書類フォルダなどを開けることを見る。

## 7. CI(GitHub の Environment と Secrets)

【Web】リポジトリ(以下 `<owner>/<repo>`。いまは `froggugugugu/arin-term`。2026-11-19 に設定済み)に Environment `release` を作り、その Environment secrets に下の 6 つを入れる(`release.yml` の build ジョブは `environment: release` が前提):

1. `https://github.com/<owner>/<repo>/settings/environments` → **New environment** → 名前 `release`
2. 「Deployment branches and tags」を **Selected branches and tags** にし、tag のパターン `v*` だけを許可する
3. (任意)「Required reviewers」に自分を入れると、リリースのたびに承認してから Secrets が使われる
4. 「Environment secrets」で **Add environment secret** を押し、下表を 1 つずつ登録する(名前は tadcap と同じ)
5. `https://github.com/<owner>/<repo>/settings/rules` で tag の ruleset を作り、`v*` の作成・更新・削除を自分だけに制限する(tag は任意のコミットに打てるため、Environment の制限だけでは書き込める人が Secrets を使えてしまう)

| Secret | 中身 |
| ------ | ---- |
| `APPLE_CERTIFICATE` | 【ターミナル】`base64 -i <書き出した .p12>` の出力 |
| `APPLE_CERTIFICATE_PASSWORD` | `.p12` のパスワード |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: <name> (<team id>)`(前後に引用符や空白を付けない) |
| `APPLE_API_ISSUER` / `APPLE_API_KEY` | Issuer ID / Key ID |
| `APPLE_API_KEY_P8` | `.p8` の中身(ジョブ内でファイルに書き出し、`APPLE_API_KEY_PATH` に渡す) |

`release.yml` の流れ:

1. tag と、`package.json`・`tauri.conf.json`・ルートの `Cargo.toml`(`[workspace.package]`)の版が同じか確かめる
2. `npm run lint`・`npm test`・`npm run licenses:check`
3. **署名のキーチェーン**: `APPLE_CERTIFICATE` を使い捨てのキーチェーンに読み込む。証明書とパスワードはこのステップにだけ渡す
4. **配布物を作り、署名・公証する**: 署名 ID と API キーだけを渡して `npm run dist:mac`
5. 検証: DMG と中の app が `source=Notarized Developer ID` でなければ失敗する(Secrets が欠けて ad-hoc になったときも失敗する)。インストーラで入れた app の署名と版、app の中のライセンスの 3 ファイルも見る
6. キーチェーンを消し、publish ジョブが下書きの Release に dmg と zip を付けてから公開する

**残るリスク**: Tauri はビルドの途中で app を署名・公証するため、4 のあいだは `.p8` とロック解除したキーチェーンがあり、npm・cargo の依存のコード(`build.rs` など)からも使える。ビルドと署名を別ジョブに分ければ避けられるが、手間が大きいので採らない(tadcap と同じ)。代わりに Environment の制限・tag の ruleset・(任意で)承認者で、Secrets が使われる場面を絞る。漏えいが疑われたら、API キーは App Store Connect で取り消し、証明書は developer.apple.com で revoke する(tadcap も使えなくなる)。

### tag を打つ

【ターミナル】版(`package.json`・`src-tauri/tauri.conf.json`・ルートの `Cargo.toml`)をそろえてコミットしてから:

```bash
git tag v<版>
git push origin v<版>
```

Actions の `release` が緑になり、Releases に `Arin-<版>-arm64.dmg`・`.zip` が載ったら、§6 の最後の確認(別ユーザーで開く)をする。

## 8. 付随して更新するもの

- `scripts/install.sh` の `REPO` と、§7 の `<owner>/<repo>` を、公開する場所に合わせる
- 期限: Developer ID 証明書は 5 年で失効する。API キーはいつでも取り消せる。Program の会費は毎年更新する
