//! `ProjectStore` の振る舞い(FR-001・003・007・010・059、設計書 §7.2「プロジェクトの登録」、§9.2)。

use std::path::{Path, PathBuf};

use super::*;

const HOME: &str = "/Users/me";

/// 連番から ID を作る(`n` を末尾のバイトに入れる)。
fn id(n: u8) -> ProjectId {
    let mut bytes = [0u8; 16];
    bytes[15] = n;
    ProjectId::from_random_bytes(bytes)
}

fn root(path: &str) -> RootReference {
    RootReference::new(PathBuf::from(path), Some(format!("bm:{path}")))
}

/// `paths` を順に登録したストア。ID は 1 から順。
fn store_with(paths: &[&str]) -> ProjectStore {
    let mut store = ProjectStore::new();
    for (i, path) in paths.iter().enumerate() {
        let n = u8::try_from(i + 1).expect("テストの件数は 255 件まで");
        store.register(id(n), root(path)).expect("登録できるはず");
    }
    store
}

fn colors(store: &ProjectStore) -> Vec<ProjectColor> {
    store.projects().iter().map(Project::color).collect()
}

fn names(store: &ProjectStore) -> Vec<String> {
    store.projects().iter().map(Project::name).collect()
}

// --- 登録(FR-001) ---

#[test]
fn 登録すると末尾に_1_件増えて選択される() {
    let mut store = store_with(&["/Users/me/dev/a"]);

    let added = store
        .register(id(2), root("/Users/me/dev/b"))
        .expect("登録できるはず");

    assert_eq!(added, id(2));
    assert_eq!(names(&store), ["a", "b"]);
    assert_eq!(store.selected(), Some(&id(2)));
    let project = store.get(&id(2)).expect("登録したものがある");
    assert_eq!(project.path(), Path::new("/Users/me/dev/b"));
    assert_eq!(project.root().bookmark(), Some("bm:/Users/me/dev/b"));
    assert_eq!(
        project.root().last_known_path(),
        Path::new("/Users/me/dev/b")
    );
    assert!(!project.is_missing());
}

#[test]
fn 空のストアは一覧が空で選択も無い() {
    let store = ProjectStore::default();

    assert!(store.projects().is_empty());
    assert_eq!(store.selected(), None);
    assert_eq!(store.get(&id(1)), None);
}

#[test]
fn 登録したパスは正規化して持つ() {
    let store = store_with(&["/Users/me//dev/./app/"]);

    assert_eq!(store.projects()[0].path(), Path::new("/Users/me/dev/app"));
    assert_eq!(
        store.projects()[0].path().to_str(),
        Some("/Users/me/dev/app")
    );
}

#[test]
fn 相対パスと_親を指す部分を含むパスは登録できない() {
    let mut store = ProjectStore::new();

    assert_eq!(
        store.register(id(1), root("dev/app")),
        Err(ProjectError::RelativePath("dev/app".into()))
    );
    assert_eq!(
        store.register(id(1), root("/Users/me/dev/../app")),
        Err(ProjectError::ParentComponent("/Users/me/dev/../app".into()))
    );
    assert!(store.projects().is_empty());
}

#[cfg(unix)]
#[test]
fn utf_8_でないパスは登録できない() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let mut store = ProjectStore::new();
    let path = PathBuf::from(OsStr::from_bytes(b"/Users/me/\xff"));

    assert_eq!(
        store.register(id(1), RootReference::new(path, None)),
        Err(ProjectError::NotUtf8Path)
    );
}

#[test]
fn ゴミ箱の中のフォルダは登録できない() {
    let mut store = ProjectStore::new();

    assert_eq!(
        store.register(id(1), root("/Users/me/.Trash/app")),
        Err(ProjectError::InTrash("/Users/me/.Trash/app".into()))
    );
    assert_eq!(
        store.register(id(1), root("/Volumes/ext/.Trashes/501/app")),
        Err(ProjectError::InTrash(
            "/Volumes/ext/.Trashes/501/app".into()
        ))
    );
}

#[test]
fn 同じ_id_は二度登録できない() {
    let mut store = store_with(&["/Users/me/dev/a"]);

    assert_eq!(
        store.register(id(1), root("/Users/me/dev/b")),
        Err(ProjectError::DuplicateId(id(1)))
    );
    assert_eq!(store.projects().len(), 1);
}

// --- 二重登録の防止と入れ子の許可(FR-010) ---

#[test]
fn 正規化したパスが同じなら増えず既存が選択される() {
    let mut store = store_with(&["/Users/me/dev/app", "/Users/me/dev/other"]);
    assert_eq!(store.selected(), Some(&id(2)));

    for same in [
        "/Users/me/dev/app",
        "/Users/me/dev/app/",
        "/Users/me/dev/app//",
        "/Users/me//dev/app",
        "/Users/me/./dev/app",
    ] {
        assert_eq!(
            store.register(id(9), root(same)),
            Err(ProjectError::AlreadyRegistered(id(1))),
            "{same} は登録済みとみなす"
        );
    }

    assert_eq!(store.projects().len(), 2, "カードが増えない");
    assert_eq!(store.selected(), Some(&id(1)), "既存のカードが選択される");
}

#[test]
fn 入れ子のフォルダは別のプロジェクトとして登録できる() {
    let mut store = store_with(&["/Users/me/dev/app"]);

    store
        .register(id(2), root("/Users/me/dev/app/frontend"))
        .expect("子のフォルダは登録できる");
    store
        .register(id(3), root("/Users/me/dev"))
        .expect("親のフォルダも登録できる");

    assert_eq!(names(&store), ["app", "frontend", "dev"]);
}

#[test]
fn 名前の先頭が同じだけの別のフォルダは別のプロジェクト() {
    let mut store = store_with(&["/Users/me/dev/app"]);

    store
        .register(id(2), root("/Users/me/dev/app2"))
        .expect("別のフォルダ");

    assert_eq!(store.projects().len(), 2);
}

#[test]
fn パスで登録済みのプロジェクトを探せる() {
    let store = store_with(&["/Users/me/dev/app"]);

    assert_eq!(
        store.find_by_path(Path::new("/Users/me/dev/app/")),
        Some(&id(1))
    );
    assert_eq!(store.find_by_path(Path::new("/Users/me/dev")), None);
    assert_eq!(store.find_by_path(Path::new("dev/app")), None);
}

// --- 削除(FR-003) ---

#[test]
fn 削除すると一覧から消え選択は次に移る() {
    let mut store = store_with(&["/a", "/b", "/c"]);
    store.select(&id(2)).expect("選べる");

    let removed = store.remove(&id(2)).expect("削除できる");

    assert_eq!(removed.id(), &id(2));
    assert_eq!(names(&store), ["a", "c"]);
    assert_eq!(store.selected(), Some(&id(3)), "次のカード");
}

#[test]
fn 最後のカードを削除すると選択は前に移り_0_件なら無し() {
    let mut store = store_with(&["/a", "/b"]);
    assert_eq!(store.selected(), Some(&id(2)));

    store.remove(&id(2)).expect("削除できる");
    assert_eq!(store.selected(), Some(&id(1)), "前のカード");

    store.remove(&id(1)).expect("削除できる");
    assert_eq!(store.selected(), None);
    assert!(store.projects().is_empty());
}

#[test]
fn 選択中でないカードを削除しても選択は変わらない() {
    let mut store = store_with(&["/a", "/b", "/c"]);
    store.select(&id(1)).expect("選べる");

    store.remove(&id(3)).expect("削除できる");

    assert_eq!(store.selected(), Some(&id(1)));
}

#[test]
fn 無い_id_の削除と選択はエラーで何も変えない() {
    let mut store = store_with(&["/a"]);

    assert_eq!(store.remove(&id(9)), Err(ProjectError::NotFound(id(9))));
    assert_eq!(store.select(&id(9)), Err(ProjectError::NotFound(id(9))));
    assert_eq!(store.projects().len(), 1);
    assert_eq!(store.selected(), Some(&id(1)));
}

#[test]
fn 削除したフォルダは登録し直せる() {
    let mut store = store_with(&["/Users/me/dev/app"]);
    store.remove(&id(1)).expect("削除できる");

    store
        .register(id(2), root("/Users/me/dev/app"))
        .expect("登録し直せる");

    assert_eq!(store.projects().len(), 1);
}

// --- 色(FR-059) ---

#[test]
fn パレットが_8_色で_3_件登録すると色がすべて違う() {
    let store = store_with(&["/a", "/b", "/c"]);

    let got = colors(&store);
    assert_eq!(ProjectColor::PALETTE.len(), 8);
    assert_ne!(got[0], got[1]);
    assert_ne!(got[1], got[2]);
    assert_ne!(got[0], got[2]);
    assert_eq!(
        got,
        [
            ProjectColor::Red,
            ProjectColor::Orange,
            ProjectColor::Yellow
        ]
    );
}

#[test]
fn 九件目は使われている数が最も少ない色_同数ならパレットの順() {
    let mut store = store_with(&["/1", "/2", "/3", "/4", "/5", "/6", "/7", "/8"]);
    assert_eq!(
        colors(&store),
        ProjectColor::PALETTE,
        "8 件で 8 色を 1 回ずつ"
    );

    store.register(id(9), root("/9")).expect("登録できる");
    assert_eq!(
        store.get(&id(9)).map(Project::color),
        Some(ProjectColor::Red)
    );

    store.register(id(10), root("/10")).expect("登録できる");
    assert_eq!(
        store.get(&id(10)).map(Project::color),
        Some(ProjectColor::Orange)
    );
}

#[test]
fn 削除で空いた色が次の登録で使われる() {
    let mut store = store_with(&["/a", "/b", "/c"]);
    store.remove(&id(2)).expect("削除できる"); // orange が空く

    store.register(id(4), root("/d")).expect("登録できる");

    assert_eq!(
        store.get(&id(4)).map(Project::color),
        Some(ProjectColor::Orange)
    );
}

#[test]
fn 色をあとから変えられ_変えた色も使用数に数える() {
    let mut store = store_with(&["/a", "/b"]);

    store
        .set_color(&id(1), ProjectColor::Yellow)
        .expect("変えられる");
    assert_eq!(
        store.get(&id(1)).map(Project::color),
        Some(ProjectColor::Yellow)
    );

    // 使われているのは yellow と orange。最少(0)の色のうちパレットで最初の red になる
    store.register(id(3), root("/c")).expect("登録できる");
    assert_eq!(
        store.get(&id(3)).map(Project::color),
        Some(ProjectColor::Red)
    );
    // 次は red・orange・yellow が 1。0 の green
    store.register(id(4), root("/d")).expect("登録できる");
    assert_eq!(
        store.get(&id(4)).map(Project::color),
        Some(ProjectColor::Green)
    );

    assert_eq!(
        store.set_color(&id(9), ProjectColor::Red),
        Err(ProjectError::NotFound(id(9)))
    );
}

// --- 表示名と表示用のパス(§15.2 #5、FR-064) ---

#[test]
fn 表示名はいまのパスのフォルダ名_ルートそのものは_slash() {
    let store = store_with(&["/Users/me/dev/my-app", "/"]);

    assert_eq!(names(&store), ["my-app", "/"]);
}

#[test]
fn 変更した表示名があればそれを使い_空なら使わない() {
    let named = Project::restore(
        id(1),
        root("/Users/me/dev/app"),
        Some("お仕事".into()),
        ProjectColor::Blue,
    );
    let blank = Project::restore(
        id(2),
        root("/Users/me/dev/lib"),
        Some("  ".into()),
        ProjectColor::Blue,
    );

    assert_eq!(named.name(), "お仕事");
    assert_eq!(named.custom_name(), Some("お仕事"));
    assert_eq!(blank.name(), "lib");
}

#[test]
fn 表示用のパスはホームを_チルダにする() {
    let store = store_with(&["/Users/me/dev/app", "/Users/me", "/Users/meme/x", "/opt/x"]);

    let snapshot = store.snapshot(Some(Path::new(HOME)));
    let display: Vec<&str> = snapshot
        .projects
        .iter()
        .map(|p| p.display_path.as_str())
        .collect();
    assert_eq!(display, ["~/dev/app", "~", "/Users/meme/x", "/opt/x"]);

    // ホームが分からない・正規化できないときは絶対パスのまま
    let unknown = store.snapshot(None);
    assert_eq!(unknown.projects[0].display_path, "/Users/me/dev/app");
    let relative_home = store.snapshot(Some(Path::new("me")));
    assert_eq!(relative_home.projects[0].display_path, "/Users/me/dev/app");
    let slash_home = store.snapshot(Some(Path::new("/Users/me/")));
    assert_eq!(slash_home.projects[0].display_path, "~/dev/app");
}

#[test]
fn 写しには選択中と色と絶対パスが載る() {
    let mut store = store_with(&["/Users/me/dev/app", "/Users/me/dev/lib"]);
    store.select(&id(1)).expect("選べる");
    store.mark_missing(&id(2)).expect("印を付けられる");

    let snapshot = store.snapshot(Some(Path::new(HOME)));

    assert_eq!(snapshot.selected_project_id, Some(id(1)));
    assert_eq!(
        snapshot.projects[0],
        ProjectSnapshot {
            id: id(1),
            name: "app".into(),
            root_path: "/Users/me/dev/app".into(),
            display_path: "~/dev/app".into(),
            color: ProjectColor::Red,
            missing: false,
        }
    );
    assert!(snapshot.projects[1].missing);

    let json = serde_json::to_value(&snapshot).expect("JSON にできる");
    assert_eq!(json["selectedProjectId"], serde_json::json!(id(1).as_str()));
    assert_eq!(json["projects"][0]["rootPath"], "/Users/me/dev/app");
    assert_eq!(json["projects"][0]["displayPath"], "~/dev/app");
    assert_eq!(json["projects"][0]["color"], "red");
    assert_eq!(json["projects"][1]["missing"], true);
}

// --- ルートへの追従と「見つからない」(FR-007、§15.2 #7) ---

#[test]
fn パスが変わると表示名と表示用のパスが変わり_id_と色は変わらない() {
    let mut store = store_with(&["/Users/me/dev/app", "/Users/me/dev/lib"]);

    store
        .update_root(&id(1), root("/Users/me/dev/app2/"))
        .expect("追従できる");

    let project = store.get(&id(1)).expect("ある");
    assert_eq!(project.id(), &id(1));
    assert_eq!(project.color(), ProjectColor::Red);
    assert_eq!(project.name(), "app2");
    assert_eq!(project.path(), Path::new("/Users/me/dev/app2"));
    assert_eq!(project.root().bookmark(), Some("bm:/Users/me/dev/app2/"));
    assert_eq!(
        store.snapshot(Some(Path::new(HOME))).projects[0].display_path,
        "~/dev/app2"
    );
    assert_eq!(store.find_by_path(Path::new("/Users/me/dev/app")), None);
}

#[test]
fn ゴミ箱の中に移されたら_ルートは変えず見つからない扱い() {
    let mut store = store_with(&["/Users/me/dev/app"]);

    store
        .update_root(&id(1), root("/Users/me/.Trash/app"))
        .expect("受け付ける");

    let project = store.get(&id(1)).expect("カードは残る");
    assert!(project.is_missing());
    assert_eq!(project.path(), Path::new("/Users/me/dev/app"));
}

#[test]
fn 追従先が別のプロジェクトのフォルダなら追従しない() {
    let mut store = store_with(&["/a", "/b"]);

    assert_eq!(
        store.update_root(&id(1), root("/b")),
        Err(ProjectError::AlreadyRegistered(id(2)))
    );
    assert_eq!(store.get(&id(1)).map(Project::path), Some(Path::new("/a")));
}

#[test]
fn 追従と再リンクと見つからない印は無い_id_でエラー() {
    let mut store = store_with(&["/a"]);

    assert_eq!(
        store.update_root(&id(9), root("/x")),
        Err(ProjectError::NotFound(id(9)))
    );
    assert_eq!(
        store.relink(&id(9), root("/x")),
        Err(ProjectError::NotFound(id(9)))
    );
    assert_eq!(
        store.mark_missing(&id(9)),
        Err(ProjectError::NotFound(id(9)))
    );
    assert_eq!(
        store.update_root(&id(1), root("rel")),
        Err(ProjectError::RelativePath("rel".into()))
    );
}

#[test]
fn 見つからないプロジェクトを再リンクすると_id_と色を保つ() {
    let mut store = store_with(&["/Users/me/dev/app", "/Users/me/dev/lib"]);
    store.mark_missing(&id(1)).expect("印を付けられる");
    assert!(store.get(&id(1)).is_some_and(Project::is_missing));

    store
        .relink(&id(1), root("/Users/me/work/app"))
        .expect("再リンクできる");

    let project = store.get(&id(1)).expect("ある");
    assert!(!project.is_missing());
    assert_eq!(project.color(), ProjectColor::Red);
    assert_eq!(project.path(), Path::new("/Users/me/work/app"));
    assert_eq!(names(&store), ["app", "lib"], "並び順も保つ");
}

#[test]
fn 再リンク先が既に登録済みのフォルダなら再リンクしない() {
    let mut store = store_with(&["/Users/me/dev/app", "/Users/me/dev/lib"]);
    store.mark_missing(&id(1)).expect("印を付けられる");

    assert_eq!(
        store.relink(&id(1), root("/Users/me/dev/lib/")),
        Err(ProjectError::AlreadyRegistered(id(2)))
    );
    assert_eq!(
        store.relink(&id(1), root("/Users/me/.Trash/app")),
        Err(ProjectError::InTrash("/Users/me/.Trash/app".into()))
    );

    let project = store.get(&id(1)).expect("ある");
    assert!(project.is_missing());
    assert_eq!(project.path(), Path::new("/Users/me/dev/app"));
}

#[test]
fn 同じフォルダへの再リンクと追従は自分自身とぶつからない() {
    let mut store = store_with(&["/Users/me/dev/app"]);
    store.mark_missing(&id(1)).expect("印を付けられる");

    store
        .relink(&id(1), root("/Users/me/dev/app"))
        .expect("同じ場所に作り直したフォルダへ再リンクできる");
    store
        .update_root(&id(1), root("/Users/me/dev/app/"))
        .expect("同じパスへの追従はできる");

    assert!(!store.get(&id(1)).is_some_and(Project::is_missing));
}

// --- 復元とシリアライズ(FR-004) ---

#[test]
fn プロジェクトの_json_の往復で同じものに戻る() {
    let store = store_with(&["/Users/me/dev/app"]);
    let project = &store.projects()[0];

    let json = serde_json::to_value(project).expect("JSON にできる");
    assert_eq!(
        json,
        serde_json::json!({
            "id": id(1).as_str(),
            "bookmark": "bm:/Users/me/dev/app",
            "lastKnownPath": "/Users/me/dev/app",
            "customName": null,
            "colorId": "red",
        })
    );

    let back: Project = serde_json::from_value(json).expect("読み戻せる");
    assert_eq!(&back, project);
}

#[test]
fn ブックマークが_null_のプロジェクトも読める() {
    let json = serde_json::json!({
        "id": id(1).as_str(),
        "bookmark": null,
        "lastKnownPath": "/Users/me/dev/app",
        "customName": "x",
        "colorId": "teal",
    });

    let project: Project = serde_json::from_value(json).expect("読める");

    assert_eq!(project.root().bookmark(), None);
    assert_eq!(project.color(), ProjectColor::Teal);
    assert_eq!(project.name(), "x");
}

#[test]
fn 復元すると並び順と選択を保つ() {
    let original = store_with(&["/a", "/b", "/c"]);
    let mut selected_b = original.clone();
    selected_b.select(&id(2)).expect("選べる");

    let restored = ProjectStore::restore(
        selected_b.projects().to_vec(),
        selected_b.selected().cloned(),
    );

    assert_eq!(restored, selected_b);
    assert_eq!(names(&restored), ["a", "b", "c"]);
    assert_eq!(restored.selected(), Some(&id(2)));
}

#[test]
fn 復元では壊れた項目と重複を捨て_無い選択は無しにする() {
    let projects = vec![
        Project::restore(id(1), root("/a/"), None, ProjectColor::Red),
        Project::restore(id(1), root("/x"), None, ProjectColor::Red), // 同じ ID
        Project::restore(id(2), root("/a"), None, ProjectColor::Blue), // 同じフォルダ
        Project::restore(id(3), root("rel"), None, ProjectColor::Blue), // 相対パス
        Project::restore(id(4), root("/b"), None, ProjectColor::Pink),
    ];

    let restored = ProjectStore::restore(projects, Some(id(2)));

    let ids: Vec<&ProjectId> = restored.projects().iter().map(Project::id).collect();
    assert_eq!(ids, [&id(1), &id(4)]);
    assert_eq!(restored.projects()[0].path(), Path::new("/a"), "正規化する");
    assert_eq!(restored.selected(), None, "捨てた ID は選ばない");
}

#[test]
fn 失敗の文言に対象が入る() {
    assert_eq!(
        ProjectError::NotFound(id(1)).to_string(),
        format!("プロジェクト {} が見つからない", id(1))
    );
    for (error, needle) in [
        (ProjectError::AlreadyRegistered(id(1)), id(1).to_string()),
        (ProjectError::DuplicateId(id(1)), id(1).to_string()),
        (ProjectError::RelativePath("rel".into()), "rel".into()),
        (
            ProjectError::ParentComponent("/a/..".into()),
            "/a/..".into(),
        ),
        (ProjectError::InTrash("/t".into()), "/t".into()),
        (ProjectError::NotUtf8Path, "UTF-8".into()),
    ] {
        assert!(error.to_string().contains(&needle), "{error}");
    }
}
