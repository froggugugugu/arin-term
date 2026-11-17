//! エスケープした挿入の文字列を実 zsh に渡す往復の結合テスト(計画書 T44a、FR-016・FR-024、設計書 §12「シェルへの注入」、
//! 【仮定】16・17)。T38b の表(`crates/arin-core/tests/shell_escape.rs`)の名前すべてと、改行などの制御文字を含む名前を、
//! 実ファイルとして作ってから `insertion_text` で挿入の文字列にし、実 PTY の対話の zsh に入力として送る。
//! zsh が受け取った引数を 16 進で書き戻させ、**1 つの引数として元の名前に戻る**こと、そのパスのファイルがあること、
//! コマンドとして実行されない(目印のファイルができない)ことを確かめる。
//!
//! 書き方は `tests/pty.rs` にならう。
//! - シェルは `/bin/zsh` に固定し、`HOME` を一時フォルダに向けて利用者の設定ファイルを読ませない
//! - 出力は目印(`ARIN<番号><…>NIRA`)で囲ませて取り出す。目印は `printf` が組み立てるので、入力のエコーには
//!   そのままの形では現れない
//! - テストは共通のロックで直列にする。どの待ちにも上限時間(`WAIT`)を付ける

use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use arin_core::paths::format::insertion_text;
use arin_core::paths::{PathKind, RelPath};
use arin_core::ports::{PtyPort, PtySession, PtySize};
use arin_core::terminal::ShellLaunchSpec;
use arin_platform::pty::NativePty;

/// 1 回の待ちの上限。
const WAIT: Duration = Duration::from_secs(20);
/// テストで起動するシェル。利用者の既定のシェルに左右されないよう固定する。
const ZSH: &str = "/bin/zsh";
const SIZE: PtySize = PtySize { rows: 24, cols: 80 };

/// T38b の表の名前(`shell_escape.rs`)と、往復で確かめたい名前。`/` を含むものはフォルダの中のファイル。
const NAMES: &[&str] = &[
    // 空白
    "src/a b.ts",
    "a  b",
    // 特殊文字の無い名前(先頭の `-` は `./` が付く)
    "README.md",
    "a_b-c.d+e,f:g@h%i",
    "-rf",
    "--help",
    "v1.2.3",
    // シェルの特殊文字(21 個)
    "a'b",
    "a\"b",
    "a\\b",
    "a$b",
    "a`b",
    "a!b",
    "a&b",
    "a*b",
    "a?b",
    "a(b",
    "a)b",
    "a[b",
    "a]b",
    "a{b",
    "a}b",
    "a;b",
    "a<b",
    "a>b",
    "a|b",
    "a#b",
    "a^b",
    // 並んだ特殊文字(コマンドの置き換え・変数の展開・履歴の展開)
    "$(touch PWNED)",
    "`touch PWNED`",
    "a;b&&c||d",
    "${HOME}",
    "'\"'",
    "!!",
    "!",
    "!$",
    "^a^b",
    "$PWD",
    "#comment",
    // 先頭のチルダとイコール
    "~",
    "~root",
    "=ls",
    "a~b=c",
    "*",
    // 改行・タブ・制御文字(ドル引用符)
    "a\nb",
    "a\rb",
    "a\tb",
    "a\u{1b}[31mb",
    "a\u{7f}b",
    "a\u{3}b",
    "it's\n",
    "a\\b\n",
    "!!\n",
    "$(touch PWNED) `touch PWNED` a b;c\n",
    "a\u{9b}b",
    "\n-rf",
    // 日本語と絵文字
    "資料/📝メモ.md",
    "👨‍👩‍👧.txt",
    "資料/📝 メモ.md",
    "メモ\n.md",
    "日本語 ファイル(1).txt",
];

/// コマンドとして実行されたら作られる目印のファイル(`touch PWNED` が名前に入っている)。
const PWNED: &str = "PWNED";

static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

/// このテストだけが使う一時フォルダ。抜けるときに自分が作ったフォルダだけを消す。
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("時計が 1970 年より前")
            .as_nanos();
        let base = fs::canonicalize(std::env::temp_dir()).expect("一時領域の実パスを取れなかった");
        let dir = base.join(format!(
            "arin-platform-shell-{name}-{}-{nanos}",
            std::process::id()
        ));
        assert!(
            dir.starts_with(&base) && dir != base,
            "一時領域の外を使おうとした"
        );
        fs::create_dir(&dir).expect("一時フォルダを作れなかった");
        Self(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// `HOME` と、プロジェクトのルート(作業フォルダ)を持つ一式。
struct Sandbox {
    dir: TempDir,
    root: PathBuf,
}

impl Sandbox {
    /// `root_name` はルートのフォルダの名前(絶対パスの挿入で、ルートの名前もエスケープされることを見る)。
    fn new(name: &str, root_name: &str) -> Self {
        let dir = TempDir::new(name);
        let home = dir.path().join("home");
        fs::create_dir(&home).expect("HOME を作れなかった");
        // 設定ファイルが 1 つも無いと、zsh が初回の設定の案内(zsh-newuser-install)を出すことがある。
        fs::write(home.join(".zshrc"), "").expect(".zshrc を作れなかった");
        let root = dir.path().join(root_name);
        fs::create_dir(&root).expect("ルートを作れなかった");
        Self { dir, root }
    }

    fn spec(&self) -> ShellLaunchSpec {
        let mut parent_env: Vec<(OsString, OsString)> = ["USER", "LOGNAME", "TMPDIR"]
            .into_iter()
            .filter_map(|name| std::env::var_os(name).map(|value| (name.into(), value)))
            .collect();
        parent_env.push(("HOME".into(), self.dir.path().join("home").into_os_string()));
        ShellLaunchSpec::new(Path::new(ZSH), &self.root, parent_env, Some("ja_JP"))
            .expect("起動の仕様を組み立てられなかった")
    }

    /// 表の名前をすべて、ルートの中のファイルとして作る。
    fn create_all(&self) -> Vec<RelPath> {
        NAMES
            .iter()
            .map(|name| {
                let rel = RelPath::parse(name).unwrap_or_else(|e| panic!("{name:?}: {e}"));
                let path = self.root.join(name);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).expect("フォルダを作れなかった");
                }
                fs::write(&path, b"x").unwrap_or_else(|e| panic!("{name:?} を作れなかった: {e}"));
                rel
            })
            .collect()
    }
}

/// 起動したシェル 1 つ。出力は別スレッドで読み、チャネルで受け取る。
struct Shell {
    session: Box<dyn PtySession>,
    output: Receiver<Vec<u8>>,
    received: Vec<u8>,
    tags: u32,
}

/// 受け取った引数を書き戻す関数。`ARIN<番号><引数の数>` に続けて、引数ごとに `:<y|n>:<16 進>` を書き、`>NIRA` で閉じる。
/// `y` はそのパスの項目がある(リンクはたどらない)。16 進は引数のバイトそのもの(制御文字も崩さずに比べるため)。
const DEFINE_PROBE: &str = "arin_probe() { local tag=$1 a; shift; printf 'ARIN%s<%s' \"$tag\" \"$#\"; \
for a in \"$@\"; do if [[ -e $a || -L $a ]]; then printf ':y:'; else printf ':n:'; fi; \
print -rn -- \"$a\" | /usr/bin/od -An -v -tx1 | /usr/bin/tr -d ' \\n'; done; printf '>NIRA\\n'; }\n";

impl Shell {
    fn spawn(spec: &ShellLaunchSpec) -> Self {
        let spawned = NativePty
            .spawn(spec, SIZE)
            .expect("シェルを起動できなかった");
        let (sender, output) = mpsc::channel();
        let mut reader = spawned.output;
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if sender.send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        let mut shell = Self {
            session: spawned.session,
            output,
            received: Vec::new(),
            tags: 0,
        };
        shell.send(DEFINE_PROBE);
        shell
    }

    fn send(&mut self, input: &str) {
        self.session
            .write(input.as_bytes())
            .expect("入力を書き込めなかった");
    }

    /// 挿入の文字列を、利用者がコマンドの途中に入れたのと同じように送り、zsh が受け取った引数を返す。
    /// 挿入の文字列は Enter を含まないので、行の確定(改行)はテストが別に送る。
    fn probe(&mut self, inserted: &str) -> Vec<(bool, Vec<u8>)> {
        self.tags += 1;
        let tag = self.tags;
        self.send(&format!("arin_probe {tag} "));
        self.send(inserted);
        self.send("\n");
        let start = format!("ARIN{tag}<");
        let deadline = Instant::now() + WAIT;
        loop {
            let text = String::from_utf8_lossy(&self.received).into_owned();
            if let Some(begin) = text.find(&start) {
                let rest = &text[begin + start.len()..];
                if let Some(end) = rest.find(">NIRA") {
                    return parse_probe(&rest[..end], inserted);
                }
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            match self.output.recv_timeout(remaining) {
                Ok(chunk) => self.received.extend_from_slice(&chunk),
                Err(RecvTimeoutError::Timeout) => panic!(
                    "{inserted:?} の引数が {WAIT:?} 以内に返らなかった(続きの行を待っている・コマンドとして読まれたなど)。出力: {text:?}"
                ),
                Err(RecvTimeoutError::Disconnected) => {
                    panic!("{inserted:?} の引数が返る前に出力が終わった。出力: {text:?}")
                }
            }
        }
    }

    fn exit(&mut self) {
        self.send("exit\n");
        let deadline = Instant::now() + WAIT;
        while self
            .session
            .try_wait()
            .expect("終了を確かめられなかった")
            .is_none()
        {
            assert!(
                Instant::now() < deadline,
                "シェルが {WAIT:?} 以内に終わらなかった"
            );
            thread::sleep(Duration::from_millis(20));
        }
    }
}

/// `<数>:<y|n>:<16 進>…` を読む。
fn parse_probe(body: &str, inserted: &str) -> Vec<(bool, Vec<u8>)> {
    let mut parts = body.split(':');
    let count: usize = parts
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("{inserted:?}: 引数の数を読めなかった: {body:?}"));
    let rest: Vec<&str> = parts.collect();
    assert_eq!(
        rest.len(),
        count * 2,
        "{inserted:?}: 書き戻しの形が違う: {body:?}"
    );
    rest.chunks(2)
        .map(|pair| {
            let exists = match pair[0] {
                "y" => true,
                "n" => false,
                other => panic!("{inserted:?}: 有無の印が違う: {other:?}"),
            };
            (exists, from_hex(pair[1], inserted))
        })
        .collect()
}

fn from_hex(hex: &str, inserted: &str) -> Vec<u8> {
    assert!(
        hex.len().is_multiple_of(2),
        "{inserted:?}: 16 進の長さが奇数: {hex:?}"
    );
    (0..hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex[i..i + 2], 16)
                .unwrap_or_else(|_| panic!("{inserted:?}: 16 進でない: {hex:?}"))
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// 相対パスの挿入で zsh が受け取るはずの引数(`-` で始まるものは `./` が付く)。
fn expected_relative(rel: &RelPath) -> String {
    if rel.as_str().starts_with('-') {
        format!("./{}", rel.as_str())
    } else {
        rel.as_str().to_owned()
    }
}

/// 送るバイトに、PTY の行の確定や制御になるもの(C0 と DEL)が無いこと。
fn assert_no_control_bytes(inserted: &str, name: &str) {
    assert!(
        inserted.bytes().all(|b| b >= 0x20 && b != 0x7f),
        "{name:?}: 挿入の文字列に制御のバイトがある: {inserted:?}"
    );
}

#[test]
fn エスケープした挿入の文字列を_zsh_に渡すと元の名前に戻る() {
    let _g = serial();
    let sandbox = Sandbox::new("relative", "project");
    let rels = sandbox.create_all();
    let mut shell = Shell::spawn(&sandbox.spec());

    for rel in &rels {
        let inserted = insertion_text(&sandbox.root, std::slice::from_ref(rel), PathKind::Relative)
            .expect("挿入の文字列を作れなかった");
        assert!(
            inserted.ends_with(' '),
            "末尾に半角スペースが付く: {inserted:?}"
        );
        assert_no_control_bytes(&inserted, rel.as_str());

        let args = shell.probe(&inserted);
        let expected = expected_relative(rel);
        assert_eq!(
            args.len(),
            1,
            "{:?}: 1 つの引数にならなかった(挿入 {inserted:?}): {args:?}",
            rel.as_str()
        );
        assert_eq!(
            hex(&args[0].1),
            hex(expected.as_bytes()),
            "{:?}: 元の名前に戻らなかった(挿入 {inserted:?}、受け取った {:?})",
            rel.as_str(),
            String::from_utf8_lossy(&args[0].1)
        );
        assert!(
            args[0].0,
            "{:?}: zsh から見てそのパスの項目が無い",
            rel.as_str()
        );
    }
    shell.exit();
    assert!(
        !sandbox.root.join(PWNED).exists() && !sandbox.dir.path().join("home").join(PWNED).exists(),
        "名前の中のコマンドが実行された"
    );
}

#[test]
fn エスケープしない名前は_zsh_で別の引数に分かれる_検査が効いていることの確かめ() {
    let _g = serial();
    let sandbox = Sandbox::new("control", "project");
    let mut shell = Shell::spawn(&sandbox.spec());

    let args = shell.probe("a b ");
    let got: Vec<Vec<u8>> = args.into_iter().map(|(_, bytes)| bytes).collect();
    assert_eq!(got, [b"a".to_vec(), b"b".to_vec()]);
    shell.exit();
}

#[test]
fn 絶対パスの挿入もルートの名前ごと_zsh_で元に戻る() {
    let _g = serial();
    // ルートの名前にも空白・引用符・日本語・`$` を入れる(別のプロジェクトのターミナルへの挿入。§15.2 #6 の A)
    let sandbox = Sandbox::new("absolute", "プロジェクト $x 'a' (b)");
    let rels = sandbox.create_all();
    let mut shell = Shell::spawn(&sandbox.spec());

    for rel in &rels {
        let inserted = insertion_text(&sandbox.root, std::slice::from_ref(rel), PathKind::Absolute)
            .expect("挿入の文字列を作れなかった");
        assert_no_control_bytes(&inserted, rel.as_str());

        let args = shell.probe(&inserted);
        let expected = sandbox.root.join(rel.as_str());
        let expected = expected.to_str().expect("UTF-8 のパス");
        assert_eq!(
            args.len(),
            1,
            "{:?}: 1 つの引数にならなかった: {args:?}",
            rel.as_str()
        );
        assert_eq!(
            hex(&args[0].1),
            hex(expected.as_bytes()),
            "{:?}: 元の絶対パスに戻らなかった(挿入 {inserted:?})",
            rel.as_str()
        );
        assert!(
            args[0].0,
            "{:?}: zsh から見てそのパスの項目が無い",
            rel.as_str()
        );
    }
    shell.exit();
    assert!(
        !sandbox.root.join(PWNED).exists(),
        "名前の中のコマンドが実行された"
    );
}

#[test]
fn 複数のパスを一度に挿入すると同じ数の引数としてそれぞれ元に戻る() {
    let _g = serial();
    let sandbox = Sandbox::new("multi", "project");
    let rels = sandbox.create_all();
    let mut shell = Shell::spawn(&sandbox.spec());

    let inserted = insertion_text(&sandbox.root, &rels, PathKind::Relative)
        .expect("挿入の文字列を作れなかった");
    assert_no_control_bytes(&inserted, "(すべて)");
    let args = shell.probe(&inserted);

    assert_eq!(args.len(), rels.len(), "引数の数が名前の数と違う");
    for (rel, (exists, got)) in rels.iter().zip(&args) {
        assert_eq!(
            hex(got),
            hex(expected_relative(rel).as_bytes()),
            "{:?}: 元の名前に戻らなかった(受け取った {:?})",
            rel.as_str(),
            String::from_utf8_lossy(got)
        );
        assert!(
            *exists,
            "{:?}: zsh から見てそのパスの項目が無い",
            rel.as_str()
        );
    }
    shell.exit();
    assert!(
        !sandbox.root.join(PWNED).exists(),
        "名前の中のコマンドが実行された"
    );
}
