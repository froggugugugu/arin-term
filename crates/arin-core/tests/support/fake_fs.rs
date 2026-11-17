//! 偽のファイルシステム(`FileSystemPort` のメモリの中の実装。計画書 T39a。T39b も使う)。
//!
//! フォルダ・ファイル・シンボリックリンクを絶対パスで持つ。途中の要素のシンボリックリンクはたどり、
//! 最後の要素のリンクはたどらない(リンク自体を項目として扱う)。`real_dir` はすべてのリンクを解決する。
//!
//! **上書きと削除はしない**: 中の表に入れる補助は、同じ場所に何かがあれば panic する(黙って置き換えない)。
//! 作成・移動・コピーは宛先があれば `AlreadyExists` を返し、項目が消えるのは移動の元とゴミ箱だけ。
//! 呼ばれた操作は順に記録し、テストが「どの操作が呼ばれたか」を確かめられる。
//!
//! ボリューム: `volume` で決めたフォルダの下は別のボリュームとみなす。ボリュームをまたぐ移動は `CrossDevice` で
//! 何も変えずに失敗する(コピーはまたげる)。`same_volume` はこの区切りで答える。
//!
//! 同時操作の再現(T39b): `fail_on` で決めた呼び出しを 1 回だけ失敗させ(何も変えない)、`after_call_file` で
//! 決めた呼び出しのあとに外からファイルが作られたことにする(メニューを開いたあとの変化や、置き換えの途中の変化)。
//!
//! テストのクレートごとに `#[path = "support/fake_fs.rs"] mod fake_fs;` で取り込む。
//! クレートによって使わない関数があるので、未使用の警告はこのファイルの中で止める。

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

use arin_core::ports::fs::{DirEntry, EntryKind, FileSystemPort, FsError};

/// シンボリックリンクをたどる回数の上限(超えたらループとみなす)。
const MAX_LINK_HOPS: usize = 40;

/// ゴミ箱の場所(偽物。`trash` はこの下の場所を返す)。
pub const TRASH_DIR: &str = "/.Trash";

#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    Dir,
    File,
    Link(PathBuf),
}

/// 呼ばれた操作(記録用)。パスは渡されたままの絶対パス。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    ReadDir(PathBuf),
    EntryKind(PathBuf),
    RealDir(PathBuf),
    CreateFile(PathBuf),
    CreateDir(PathBuf),
    Move(PathBuf, PathBuf),
    Copy(PathBuf, PathBuf),
    SameVolume(PathBuf, PathBuf),
    Trash(PathBuf),
}

#[derive(Debug, Default)]
struct Inner {
    nodes: BTreeMap<PathBuf, Node>,
    calls: Vec<Call>,
    trashed: Vec<PathBuf>,
    /// 1 回だけ失敗させる呼び出し(何も変えずにこの失敗を返す)。
    failures: Vec<(Call, FsError)>,
    /// 呼び出しのあとに外から作られるファイル(1 回だけ)。
    triggers: Vec<(Call, PathBuf)>,
    /// 別のボリュームの頂点のフォルダ(実パス)。
    volumes: Vec<PathBuf>,
}

/// メモリの中のファイルシステム。`/` は最初からある。
#[derive(Debug)]
pub struct FakeFs {
    inner: Mutex<Inner>,
}

impl Default for FakeFs {
    fn default() -> Self {
        Self::new()
    }
}

fn abs(path: &str) -> PathBuf {
    let path = PathBuf::from(path);
    assert!(
        path.is_absolute(),
        "偽のファイルシステムには絶対パスで渡す: {path:?}"
    );
    path
}

fn display(path: &Path) -> String {
    path.display().to_string()
}

impl FakeFs {
    /// `/` だけがある状態で作る。
    #[must_use]
    pub fn new() -> Self {
        let mut nodes = BTreeMap::new();
        nodes.insert(PathBuf::from("/"), Node::Dir);
        Self {
            inner: Mutex::new(Inner {
                nodes,
                ..Inner::default()
            }),
        }
    }

    // ---- 組み立て(テストの準備。親のフォルダは無ければ作る) ----

    /// フォルダを作る(親も)。
    #[must_use]
    pub fn dir(self, path: &str) -> Self {
        self.add(&abs(path), Node::Dir);
        self
    }

    /// 空のファイルを作る(親のフォルダも)。
    #[must_use]
    pub fn file(self, path: &str) -> Self {
        self.add(&abs(path), Node::File);
        self
    }

    /// シンボリックリンクを作る(親のフォルダも)。`target` は絶対パスか、リンクのあるフォルダからの相対パス。
    #[must_use]
    pub fn link(self, path: &str, target: &str) -> Self {
        self.add(&abs(path), Node::Link(PathBuf::from(target)));
        self
    }

    fn add(&self, path: &Path, node: Node) {
        let mut inner = self.lock();
        let mut current = PathBuf::from("/");
        let parents: Vec<_> = path
            .parent()
            .into_iter()
            .flat_map(Path::components)
            .collect();
        for component in parents {
            if let Component::Normal(name) = component {
                current.push(name);
                match inner.nodes.get(&current) {
                    None => insert_new(&mut inner.nodes, current.clone(), Node::Dir),
                    Some(Node::Dir) => {}
                    Some(other) => panic!("準備で親にフォルダ以外がある: {current:?}({other:?})"),
                }
            }
        }
        insert_new(&mut inner.nodes, path.to_path_buf(), node);
    }

    /// `path` のフォルダ(無ければ作る)から下を、別のボリュームにする。
    #[must_use]
    pub fn volume(self, path: &str) -> Self {
        let path = abs(path);
        if !self.lock().nodes.contains_key(&path) {
            self.add(&path, Node::Dir);
        }
        self.lock().volumes.push(path);
        self
    }

    /// `call` と同じ呼び出しを 1 回だけ、何も変えずに `error` で失敗させる。
    #[must_use]
    pub fn fail_on(self, call: Call, error: FsError) -> Self {
        self.lock().failures.push((call, error));
        self
    }

    /// `call` と同じ呼び出しが終わった直後に 1 回だけ、`path` に空のファイルが外から作られたことにする。
    #[must_use]
    pub fn after_call_file(self, call: Call, path: &str) -> Self {
        let path = abs(path);
        self.lock().triggers.push((call, path));
        self
    }

    // ---- 調べる(テストの確認) ----

    /// その場所(最後の要素のリンクはたどらない)に何かがあるか。
    #[must_use]
    pub fn exists(&self, path: &str) -> bool {
        let inner = self.lock();
        locate(&inner.nodes, &abs(path)).is_ok_and(|p| inner.nodes.contains_key(&p))
    }

    /// 呼ばれた操作の一覧(呼ばれた順)。
    #[must_use]
    pub fn calls(&self) -> Vec<Call> {
        self.lock().calls.clone()
    }

    /// 変える操作(作成・移動・コピー・ゴミ箱)だけの一覧。
    #[must_use]
    pub fn mutations(&self) -> Vec<Call> {
        self.calls()
            .into_iter()
            .filter(|c| {
                matches!(
                    c,
                    Call::CreateFile(_)
                        | Call::CreateDir(_)
                        | Call::Move(..)
                        | Call::Copy(..)
                        | Call::Trash(_)
                )
            })
            .collect()
    }

    /// ゴミ箱に入れた項目(入れたときの場所)。
    #[must_use]
    pub fn trashed(&self) -> Vec<PathBuf> {
        self.lock().trashed.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// 呼び出しを記録し、失敗させると決めたものならその失敗を返す(何も変えない)。
    fn record(&self, call: Call) -> Result<(), FsError> {
        let mut inner = self.lock();
        inner.calls.push(call.clone());
        match inner.failures.iter().position(|(c, _)| *c == call) {
            Some(index) => Err(inner.failures.remove(index).1),
            None => Ok(()),
        }
    }

    /// 呼び出しのあとに外から作られるファイルを置く。
    fn fire(&self, call: &Call) {
        let path = {
            let mut inner = self.lock();
            match inner.triggers.iter().position(|(c, _)| c == call) {
                Some(index) => inner.triggers.remove(index).1,
                None => return,
            }
        };
        self.add(&path, Node::File);
    }

    /// 記録 → 失敗の注入 → 本体 → 外からの変化、の順に行う。
    fn run<T>(
        &self,
        call: Call,
        body: impl FnOnce(&Self) -> Result<T, FsError>,
    ) -> Result<T, FsError> {
        self.record(call.clone())?;
        let result = body(self);
        self.fire(&call);
        result
    }
}

/// 表に新しく入れる。すでに何かあれば panic する(上書きしない)。
fn insert_new(nodes: &mut BTreeMap<PathBuf, Node>, path: PathBuf, node: Node) {
    if let Some(old) = nodes.get(&path) {
        panic!("偽のファイルシステムに上書きを求めた: {path:?}(既存: {old:?})");
    }
    nodes.insert(path, node);
}

/// 項目とその子孫を表から取り出す(移動の元とゴミ箱だけが使う)。
fn take_subtree(nodes: &mut BTreeMap<PathBuf, Node>, path: &Path) -> Vec<(PathBuf, Node)> {
    let keys: Vec<PathBuf> = nodes
        .keys()
        .filter(|k| k.starts_with(path))
        .cloned()
        .collect();
    keys.into_iter()
        .map(|k| {
            let node = nodes.remove(&k).expect("直前に見つけた");
            (k, node)
        })
        .collect()
}

/// パスのすべての要素のリンクを解決した実パス(無い要素があれば `NotFound`)。
fn resolve(nodes: &BTreeMap<PathBuf, Node>, path: &Path) -> Result<PathBuf, FsError> {
    resolve_with(nodes, path, true, 0)
}

/// 最後の要素のリンクはたどらずに、親を解決した場所(最後の要素は無くてもよい)。
fn locate(nodes: &BTreeMap<PathBuf, Node>, path: &Path) -> Result<PathBuf, FsError> {
    resolve_with(nodes, path, false, 0)
}

fn resolve_with(
    nodes: &BTreeMap<PathBuf, Node>,
    path: &Path,
    follow_last: bool,
    hops: usize,
) -> Result<PathBuf, FsError> {
    if hops > MAX_LINK_HOPS {
        return Err(FsError::Io(format!("リンクが多すぎる: {}", display(path))));
    }
    let components: Vec<Component<'_>> = path.components().collect();
    let mut current = PathBuf::from("/");
    for (index, component) in components.iter().enumerate() {
        let last = index + 1 == components.len();
        match component {
            Component::RootDir => current = PathBuf::from("/"),
            Component::CurDir => {}
            Component::ParentDir => {
                current.pop();
            }
            Component::Prefix(_) => return Err(FsError::Io("想定外の要素".to_owned())),
            Component::Normal(name) => {
                let next = current.join(name);
                match nodes.get(&next) {
                    Some(Node::Link(target)) if !last || follow_last => {
                        let joined = if target.is_absolute() {
                            target.clone()
                        } else {
                            current.join(target)
                        };
                        current = resolve_with(nodes, &joined, true, hops + 1)?;
                    }
                    Some(Node::File) if !last => {
                        return Err(FsError::NotADirectory(display(&next)));
                    }
                    None if !last || follow_last => {
                        return Err(FsError::NotFound(display(&next)));
                    }
                    _ => current = next,
                }
            }
        }
    }
    Ok(current)
}

/// 操作の本体(記録と注入は `FileSystemPort` の実装が `run` で行う)。
impl FakeFs {
    fn read_dir_now(&self, dir: &Path) -> Result<Vec<DirEntry>, FsError> {
        let inner = self.lock();
        let real = resolve(&inner.nodes, dir)?;
        if inner.nodes.get(&real) != Some(&Node::Dir) {
            return Err(FsError::NotADirectory(display(dir)));
        }
        let children: Vec<PathBuf> = inner
            .nodes
            .keys()
            .filter(|k| k.parent() == Some(real.as_path()))
            .cloned()
            .collect();
        Ok(children
            .into_iter()
            .map(|child| DirEntry {
                name: child
                    .file_name()
                    .and_then(|n| n.to_str())
                    .expect("名前は UTF-8")
                    .to_owned(),
                kind: kind_of(&inner.nodes, &child),
            })
            .collect())
    }

    fn entry_kind_now(&self, path: &Path) -> Result<EntryKind, FsError> {
        let inner = self.lock();
        let located = locate(&inner.nodes, path)?;
        if !inner.nodes.contains_key(&located) {
            return Err(FsError::NotFound(display(path)));
        }
        Ok(kind_of(&inner.nodes, &located))
    }

    fn real_dir_now(&self, dir: &Path) -> Result<PathBuf, FsError> {
        let inner = self.lock();
        let real = resolve(&inner.nodes, dir)?;
        match inner.nodes.get(&real) {
            Some(Node::Dir) => Ok(real),
            Some(_) => Err(FsError::NotADirectory(display(dir))),
            None => Err(FsError::NotFound(display(dir))),
        }
    }

    fn create_file_now(&self, path: &Path) -> Result<(), FsError> {
        self.create(path, Node::File)
    }

    fn create_dir_now(&self, path: &Path) -> Result<(), FsError> {
        self.create(path, Node::Dir)
    }

    fn move_entry_now(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        let mut inner = self.lock();
        let source = existing(&inner.nodes, from)?;
        let dest = free_place(&inner.nodes, to)?;
        if volume_of(&inner.volumes, &source) != volume_of(&inner.volumes, &dest) {
            return Err(FsError::CrossDevice(display(from)));
        }
        if dest.starts_with(&source) {
            return Err(FsError::Io(format!(
                "自分の中へは移せない: {} → {}",
                display(from),
                display(to)
            )));
        }
        for (key, node) in take_subtree(&mut inner.nodes, &source) {
            let rest = key.strip_prefix(&source).expect("部分木の中");
            insert_new(&mut inner.nodes, dest.join(rest), node);
        }
        Ok(())
    }

    fn copy_entry_now(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        let mut inner = self.lock();
        let source = existing(&inner.nodes, from)?;
        let dest = free_place(&inner.nodes, to)?;
        if dest.starts_with(&source) {
            return Err(FsError::Io(format!(
                "自分の中へは写せない: {} → {}",
                display(from),
                display(to)
            )));
        }
        let copies: Vec<(PathBuf, Node)> = inner
            .nodes
            .iter()
            .filter(|(k, _)| k.starts_with(&source))
            .map(|(k, n)| {
                (
                    dest.join(k.strip_prefix(&source).expect("部分木の中")),
                    n.clone(),
                )
            })
            .collect();
        for (key, node) in copies {
            insert_new(&mut inner.nodes, key, node);
        }
        Ok(())
    }

    fn same_volume_now(&self, path: &Path, dir: &Path) -> Result<bool, FsError> {
        let inner = self.lock();
        let item = existing(&inner.nodes, path)?;
        let folder = resolve(&inner.nodes, dir)?;
        if !inner.nodes.contains_key(&folder) {
            return Err(FsError::NotFound(display(dir)));
        }
        Ok(volume_of(&inner.volumes, &item) == volume_of(&inner.volumes, &folder))
    }

    fn trash_now(&self, path: &Path) -> Result<Option<PathBuf>, FsError> {
        let mut inner = self.lock();
        let source = existing(&inner.nodes, path)?;
        take_subtree(&mut inner.nodes, &source);
        inner.trashed.push(source.clone());
        let name = source.file_name().expect("ルートはゴミ箱に入れない");
        Ok(Some(Path::new(TRASH_DIR).join(name)))
    }
}

impl FileSystemPort for FakeFs {
    fn read_dir(&self, dir: &Path) -> Result<Vec<DirEntry>, FsError> {
        self.run(Call::ReadDir(dir.to_path_buf()), |fs| fs.read_dir_now(dir))
    }

    fn entry_kind(&self, path: &Path) -> Result<EntryKind, FsError> {
        self.run(Call::EntryKind(path.to_path_buf()), |fs| {
            fs.entry_kind_now(path)
        })
    }

    fn real_dir(&self, dir: &Path) -> Result<PathBuf, FsError> {
        self.run(Call::RealDir(dir.to_path_buf()), |fs| fs.real_dir_now(dir))
    }

    fn create_file(&self, path: &Path) -> Result<(), FsError> {
        self.run(Call::CreateFile(path.to_path_buf()), |fs| {
            fs.create_file_now(path)
        })
    }

    fn create_dir(&self, path: &Path) -> Result<(), FsError> {
        self.run(Call::CreateDir(path.to_path_buf()), |fs| {
            fs.create_dir_now(path)
        })
    }

    fn move_entry(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        self.run(Call::Move(from.to_path_buf(), to.to_path_buf()), |fs| {
            fs.move_entry_now(from, to)
        })
    }

    fn copy_entry(&self, from: &Path, to: &Path) -> Result<(), FsError> {
        self.run(Call::Copy(from.to_path_buf(), to.to_path_buf()), |fs| {
            fs.copy_entry_now(from, to)
        })
    }

    fn same_volume(&self, path: &Path, dir: &Path) -> Result<bool, FsError> {
        self.run(
            Call::SameVolume(path.to_path_buf(), dir.to_path_buf()),
            |fs| fs.same_volume_now(path, dir),
        )
    }

    fn trash(&self, path: &Path) -> Result<Option<PathBuf>, FsError> {
        self.run(Call::Trash(path.to_path_buf()), |fs| fs.trash_now(path))
    }
}

impl FakeFs {
    fn create(&self, path: &Path, node: Node) -> Result<(), FsError> {
        let mut inner = self.lock();
        let place = free_place(&inner.nodes, path)?;
        insert_new(&mut inner.nodes, place, node);
        Ok(())
    }
}

/// 項目のあるボリューム(いちばん深い頂点のフォルダ。どれの下でもなければ `/` のボリューム)。
/// ボリュームの頂点そのものは、そのボリュームの項目とみなす。
fn volume_of<'v>(volumes: &'v [PathBuf], path: &Path) -> Option<&'v PathBuf> {
    volumes
        .iter()
        .filter(|v| path.starts_with(v))
        .max_by_key(|v| v.components().count())
}

/// 項目の種類(リンクは指す先で分ける。壊れたリンクは `Other`)。
fn kind_of(nodes: &BTreeMap<PathBuf, Node>, path: &Path) -> EntryKind {
    match nodes.get(path) {
        Some(Node::Dir) => EntryKind::Directory,
        Some(Node::File) => EntryKind::File,
        Some(Node::Link(_)) => match resolve(nodes, path).ok().and_then(|r| nodes.get(&r)) {
            Some(Node::Dir) => EntryKind::DirectoryLink,
            Some(Node::File) => EntryKind::FileLink,
            _ => EntryKind::Other,
        },
        None => EntryKind::Other,
    }
}

/// すでにある項目の場所(最後のリンクはたどらない)。
fn existing(nodes: &BTreeMap<PathBuf, Node>, path: &Path) -> Result<PathBuf, FsError> {
    let located = locate(nodes, path)?;
    if nodes.contains_key(&located) {
        Ok(located)
    } else {
        Err(FsError::NotFound(display(path)))
    }
}

/// 新しく置ける場所(親がフォルダで、その名前がまだ無い)。
fn free_place(nodes: &BTreeMap<PathBuf, Node>, path: &Path) -> Result<PathBuf, FsError> {
    let located = locate(nodes, path)?;
    let parent = located
        .parent()
        .ok_or_else(|| FsError::Io(format!("ルートには置けない: {}", display(path))))?;
    match nodes.get(parent) {
        Some(Node::Dir) => {}
        Some(_) => return Err(FsError::NotADirectory(display(parent))),
        None => return Err(FsError::NotFound(display(parent))),
    }
    if nodes.contains_key(&located) {
        return Err(FsError::AlreadyExists(display(path)));
    }
    Ok(located)
}
