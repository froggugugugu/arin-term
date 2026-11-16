//! コマンドの ID と既定のキーと受け口の表(計画書 M3 の前提 (e)、T38d、【仮定】21。M4.5 の T64c で 23 行、M4.7 の ADR-014 の 4 で 32 行)。
//!
//! キーの文字列をコードのあちこちに直書きせず、この 1 つの表から引く。メニューバー(`src-tauri/src/menu.rs`。T42e)は
//! 受け口が「メニューの項目」の行からアクセラレータを作り、画面(`src/logic/keys`。T41b)は表を受け取って振り分ける。
//! M4.5(T64)では、既定の表の行のキーを設定ファイルの値に替えた行で [`ShortcutTable::from_bindings`] を呼び、
//! 同じ検査(足りない行・同じコマンドの二重の行・同じキーの二重の割り当て)を通す。
//!
//! 表は決定的: 行は常にコマンドの ID の順([`ShortcutCommandId::ALL`] の順)、JSON の項目は
//! `command`・`accelerator`・`receiver` の順。

use std::fmt;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use super::key::KeyChord;

/// ショートカットで実行するコマンドの ID。
///
/// 文字列は M2 のメニューの項目の ID(`menu.rs`。`on_menu_event` に届く)をそのまま使い、M3 の分を足す。
/// 設定ファイル(M4.5)でもこの文字列でコマンドを指す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
pub enum ShortcutCommandId {
    /// 新規タブ。
    #[serde(rename = "terminal.new-tab")]
    TerminalNewTab,
    /// タブを閉じる。
    #[serde(rename = "terminal.close-tab")]
    TerminalCloseTab,
    /// 前のタブ。
    #[serde(rename = "terminal.previous-tab")]
    TerminalPreviousTab,
    /// 次のタブ。
    #[serde(rename = "terminal.next-tab")]
    TerminalNextTab,
    /// すべてを選択。
    #[serde(rename = "edit.select-all")]
    EditSelectAll,
    /// 終了。
    #[serde(rename = "app.quit")]
    AppQuit,
    /// 設定…(設定画面を開く。M4.6 の ADR-013 の 6)。M4.5(ADR-012 の 6)では「設定ファイルを開く」だった行で、ID・キー・受け口は
    /// そのまま意味だけを替えた。「設定ファイルを開く…」は表の外のキーの無いメニューの項目(`src-tauri/src/menu.rs`)。
    #[serde(rename = "app.open-settings")]
    AppOpenSettings,
    /// コピー(ターミナルとツリーで共有)。
    #[serde(rename = "edit.copy")]
    EditCopy,
    /// 切り取り(ターミナルとツリーで共有)。
    #[serde(rename = "edit.cut")]
    EditCut,
    /// ペースト(ターミナルとツリーで共有)。
    #[serde(rename = "edit.paste")]
    EditPaste,
    /// ツリー: 開く。
    #[serde(rename = "tree.open")]
    TreeOpen,
    /// ツリー: Finder で表示。
    #[serde(rename = "tree.reveal-in-finder")]
    TreeRevealInFinder,
    /// ツリー: パスをターミナルに挿入。
    #[serde(rename = "tree.insert-path")]
    TreeInsertPath,
    /// ツリー: 相対パスをコピー。
    #[serde(rename = "tree.copy-relative-path")]
    TreeCopyRelativePath,
    /// ツリー: 絶対パスをコピー。
    #[serde(rename = "tree.copy-absolute-path")]
    TreeCopyAbsolutePath,
    /// ツリー: 名称変更。
    #[serde(rename = "tree.rename")]
    TreeRename,
    /// ツリー: ゴミ箱に入れる。
    #[serde(rename = "tree.trash")]
    TreeTrash,
    /// ツリー: 新規ファイル。
    #[serde(rename = "tree.new-file")]
    TreeNewFile,
    /// ツリー: 新規フォルダ。
    #[serde(rename = "tree.new-folder")]
    TreeNewFolder,
    /// 表示: タイル表示とタブ表示を切り替える(M4。計画書 M4 の前提 (f))。
    #[serde(rename = "view.toggle-display-mode")]
    ViewToggleDisplayMode,
    /// 表示: タイル表示の前のページ。
    #[serde(rename = "view.previous-page")]
    ViewPreviousPage,
    /// 表示: タイル表示の次のページ。
    #[serde(rename = "view.next-page")]
    ViewNextPage,
    /// 表示: タイルを拡大する・元に戻す。
    #[serde(rename = "view.toggle-maximize")]
    ViewToggleMaximize,
    /// 表示: タイル表示で、表示中のページの 1 番目のタイルにフォーカスする(M4.7。ADR-014 の 4)。
    #[serde(rename = "view.focus-tile-1")]
    ViewFocusTile1,
    /// 表示: タイル表示で、表示中のページの 2 番目のタイルにフォーカスする(M4.7。ADR-014 の 4)。
    #[serde(rename = "view.focus-tile-2")]
    ViewFocusTile2,
    /// 表示: タイル表示で、表示中のページの 3 番目のタイルにフォーカスする(M4.7。ADR-014 の 4)。
    #[serde(rename = "view.focus-tile-3")]
    ViewFocusTile3,
    /// 表示: タイル表示で、表示中のページの 4 番目のタイルにフォーカスする(M4.7。ADR-014 の 4)。
    #[serde(rename = "view.focus-tile-4")]
    ViewFocusTile4,
    /// 表示: タイル表示で、表示中のページの 5 番目のタイルにフォーカスする(M4.7。ADR-014 の 4)。
    #[serde(rename = "view.focus-tile-5")]
    ViewFocusTile5,
    /// 表示: タイル表示で、表示中のページの 6 番目のタイルにフォーカスする(M4.7。ADR-014 の 4)。
    #[serde(rename = "view.focus-tile-6")]
    ViewFocusTile6,
    /// 表示: タイル表示で、表示中のページの 7 番目のタイルにフォーカスする(M4.7。ADR-014 の 4)。
    #[serde(rename = "view.focus-tile-7")]
    ViewFocusTile7,
    /// 表示: タイル表示で、表示中のページの 8 番目のタイルにフォーカスする(M4.7。ADR-014 の 4)。
    #[serde(rename = "view.focus-tile-8")]
    ViewFocusTile8,
    /// 表示: タイル表示で、表示中のページの 9 番目のタイルにフォーカスする(M4.7。ADR-014 の 4)。
    #[serde(rename = "view.focus-tile-9")]
    ViewFocusTile9,
}

impl ShortcutCommandId {
    /// すべてのコマンドの ID(表の順)。
    pub const ALL: [Self; 32] = [
        Self::TerminalNewTab,
        Self::TerminalCloseTab,
        Self::TerminalPreviousTab,
        Self::TerminalNextTab,
        Self::EditSelectAll,
        Self::AppQuit,
        Self::AppOpenSettings,
        Self::EditCopy,
        Self::EditCut,
        Self::EditPaste,
        Self::TreeOpen,
        Self::TreeRevealInFinder,
        Self::TreeInsertPath,
        Self::TreeCopyRelativePath,
        Self::TreeCopyAbsolutePath,
        Self::TreeRename,
        Self::TreeTrash,
        Self::TreeNewFile,
        Self::TreeNewFolder,
        Self::ViewToggleDisplayMode,
        Self::ViewPreviousPage,
        Self::ViewNextPage,
        Self::ViewToggleMaximize,
        Self::ViewFocusTile1,
        Self::ViewFocusTile2,
        Self::ViewFocusTile3,
        Self::ViewFocusTile4,
        Self::ViewFocusTile5,
        Self::ViewFocusTile6,
        Self::ViewFocusTile7,
        Self::ViewFocusTile8,
        Self::ViewFocusTile9,
    ];

    /// ID の文字列(JSON・メニューの項目の ID・設定ファイルで使う)。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TerminalNewTab => "terminal.new-tab",
            Self::TerminalCloseTab => "terminal.close-tab",
            Self::TerminalPreviousTab => "terminal.previous-tab",
            Self::TerminalNextTab => "terminal.next-tab",
            Self::EditSelectAll => "edit.select-all",
            Self::AppQuit => "app.quit",
            Self::AppOpenSettings => "app.open-settings",
            Self::EditCopy => "edit.copy",
            Self::EditCut => "edit.cut",
            Self::EditPaste => "edit.paste",
            Self::TreeOpen => "tree.open",
            Self::TreeRevealInFinder => "tree.reveal-in-finder",
            Self::TreeInsertPath => "tree.insert-path",
            Self::TreeCopyRelativePath => "tree.copy-relative-path",
            Self::TreeCopyAbsolutePath => "tree.copy-absolute-path",
            Self::TreeRename => "tree.rename",
            Self::TreeTrash => "tree.trash",
            Self::TreeNewFile => "tree.new-file",
            Self::TreeNewFolder => "tree.new-folder",
            Self::ViewToggleDisplayMode => "view.toggle-display-mode",
            Self::ViewPreviousPage => "view.previous-page",
            Self::ViewNextPage => "view.next-page",
            Self::ViewToggleMaximize => "view.toggle-maximize",
            Self::ViewFocusTile1 => "view.focus-tile-1",
            Self::ViewFocusTile2 => "view.focus-tile-2",
            Self::ViewFocusTile3 => "view.focus-tile-3",
            Self::ViewFocusTile4 => "view.focus-tile-4",
            Self::ViewFocusTile5 => "view.focus-tile-5",
            Self::ViewFocusTile6 => "view.focus-tile-6",
            Self::ViewFocusTile7 => "view.focus-tile-7",
            Self::ViewFocusTile8 => "view.focus-tile-8",
            Self::ViewFocusTile9 => "view.focus-tile-9",
        }
    }

    /// タイルの番号キーなら、その番号(1〜9。表示中のページの中の並び順)。ほかのコマンドは `None`。
    #[must_use]
    pub const fn tile_position(self) -> Option<usize> {
        match self {
            Self::ViewFocusTile1 => Some(1),
            Self::ViewFocusTile2 => Some(2),
            Self::ViewFocusTile3 => Some(3),
            Self::ViewFocusTile4 => Some(4),
            Self::ViewFocusTile5 => Some(5),
            Self::ViewFocusTile6 => Some(6),
            Self::ViewFocusTile7 => Some(7),
            Self::ViewFocusTile8 => Some(8),
            Self::ViewFocusTile9 => Some(9),
            _ => None,
        }
    }

    /// ID の文字列からコマンドの ID を引く。大文字・小文字を区別する。未知の ID は `None`。
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == id)
    }
}

impl fmt::Display for ShortcutCommandId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// キーの受け口(【仮定】21)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum ShortcutReceiver {
    /// メニューバーの自前の項目のアクセラレータで OS が受ける(フォーカスによらない)。
    MenuItem,
    /// 「編集」の標準の項目が受け、画面のコピー・切り取り・ペーストのイベントになる(フォーカスによらない)。
    EditItem,
    /// ツリーにフォーカスがあるときに画面の keydown で受ける。
    Screen,
}

/// 表の 1 行(コマンドの ID・キー・受け口)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub struct ShortcutBinding {
    /// コマンドの ID。
    command: ShortcutCommandId,
    /// キーの文字列(Tauri のアクセラレータの書式。`Cmd+Shift+N` の形の正規の書式)。
    #[ts(type = "string")]
    accelerator: KeyChord,
    /// 受け口。
    receiver: ShortcutReceiver,
}

impl ShortcutBinding {
    /// 行を作る。表の検査は [`ShortcutTable::from_bindings`] で行う。
    #[must_use]
    pub const fn new(
        command: ShortcutCommandId,
        accelerator: KeyChord,
        receiver: ShortcutReceiver,
    ) -> Self {
        Self {
            command,
            accelerator,
            receiver,
        }
    }

    /// コマンドの ID。
    #[must_use]
    pub const fn command(&self) -> ShortcutCommandId {
        self.command
    }

    /// キー。
    #[must_use]
    pub const fn accelerator(&self) -> KeyChord {
        self.accelerator
    }

    /// 受け口。
    #[must_use]
    pub const fn receiver(&self) -> ShortcutReceiver {
        self.receiver
    }
}

/// 表を作れない理由。
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ShortcutTableProblem {
    /// コマンドの行が無い。
    #[error("コマンド {0} のキーが無い")]
    Missing(ShortcutCommandId),
    /// 同じコマンドの行が 2 つ以上ある。
    #[error("コマンド {0} の行が 2 つ以上ある")]
    DuplicateCommand(ShortcutCommandId),
    /// 2 つのコマンドに同じキーを割り当てた(受け口によらない。メニューと編集の項目は OS がフォーカスによらず受けるため)。
    #[error("{first} と {second} に同じキー {accelerator} を割り当てている")]
    Conflict {
        /// 表の順で先の行のコマンド。
        first: ShortcutCommandId,
        /// 表の順で後の行のコマンド。
        second: ShortcutCommandId,
        /// 重なったキー。
        accelerator: KeyChord,
    },
}

/// ショートカットの表。すべてのコマンドの行を 1 つずつ、コマンドの ID の順に持ち、どの 2 行も同じキーを持たない。
///
/// 画面へは配列として渡す(`get_shortcuts`。T42e)。既定の表は `src/ipc/generated/defaultShortcuts.json` にも生成する。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct ShortcutTable(Vec<ShortcutBinding>);

impl ShortcutTable {
    /// 既定の表(PRD §6.4 と M2 のメニューのキー。受け口は【仮定】21。表示の 4 行は M4 の【仮定】9・ADR-011 の 6。
    /// 設定 ⌘, は M4.5 の ADR-012 の 6 で足し、M4.6 の ADR-013 の 6 で設定画面を開く意味に替えた。
    /// タイルの番号キー ⌘1〜⌘9 は M4.7 の ADR-014 の 4)。
    ///
    /// # Panics
    ///
    /// 既定の値が検査を通らないとき(テストで確かめているので起きない)。
    #[must_use]
    pub fn default_table() -> Self {
        use ShortcutCommandId as C;
        use ShortcutReceiver::{EditItem, MenuItem, Screen};

        let rows: [(C, &str, ShortcutReceiver); 32] = [
            (C::TerminalNewTab, "Cmd+T", MenuItem),
            (C::TerminalCloseTab, "Cmd+W", MenuItem),
            (C::TerminalPreviousTab, "Cmd+Shift+[", MenuItem),
            (C::TerminalNextTab, "Cmd+Shift+]", MenuItem),
            (C::EditSelectAll, "Cmd+A", MenuItem),
            (C::AppQuit, "Cmd+Q", MenuItem),
            (C::AppOpenSettings, "Cmd+,", MenuItem),
            (C::EditCopy, "Cmd+C", EditItem),
            (C::EditCut, "Cmd+X", EditItem),
            (C::EditPaste, "Cmd+V", EditItem),
            (C::TreeOpen, "Cmd+O", Screen),
            (C::TreeRevealInFinder, "Cmd+Alt+R", Screen),
            (C::TreeInsertPath, "Cmd+Enter", Screen),
            (C::TreeCopyRelativePath, "Cmd+Shift+Alt+C", Screen),
            (C::TreeCopyAbsolutePath, "Cmd+Alt+C", Screen),
            (C::TreeRename, "Enter", Screen),
            (C::TreeTrash, "Cmd+Backspace", Screen),
            (C::TreeNewFile, "Cmd+N", Screen),
            (C::TreeNewFolder, "Cmd+Shift+N", Screen),
            (C::ViewToggleDisplayMode, "Cmd+Shift+G", MenuItem),
            (C::ViewPreviousPage, "Cmd+Alt+ArrowLeft", MenuItem),
            (C::ViewNextPage, "Cmd+Alt+ArrowRight", MenuItem),
            (C::ViewToggleMaximize, "Cmd+Shift+Enter", MenuItem),
            (C::ViewFocusTile1, "Cmd+1", MenuItem),
            (C::ViewFocusTile2, "Cmd+2", MenuItem),
            (C::ViewFocusTile3, "Cmd+3", MenuItem),
            (C::ViewFocusTile4, "Cmd+4", MenuItem),
            (C::ViewFocusTile5, "Cmd+5", MenuItem),
            (C::ViewFocusTile6, "Cmd+6", MenuItem),
            (C::ViewFocusTile7, "Cmd+7", MenuItem),
            (C::ViewFocusTile8, "Cmd+8", MenuItem),
            (C::ViewFocusTile9, "Cmd+9", MenuItem),
        ];
        let bindings = rows
            .into_iter()
            .map(|(command, key, receiver)| {
                let chord = KeyChord::parse(key)
                    .unwrap_or_else(|e| panic!("既定のキー {key:?} を読めない: {e}"));
                ShortcutBinding::new(command, chord, receiver)
            })
            .collect();
        Self::from_bindings(bindings)
            .unwrap_or_else(|problems| panic!("既定の表が検査を通らない: {problems:?}"))
    }

    /// 行から表を作る。行はコマンドの ID の順に並べ直す(渡した順によらない)。
    ///
    /// # Errors
    ///
    /// 同じコマンドの二重の行・足りない行・同じキーの二重の割り当てがあるとき、見つけた問題をすべて返す
    /// (二重の行 → 足りない行 → 重なり の順。重なりは表の順で先の行どうしから)。
    pub fn from_bindings(
        mut bindings: Vec<ShortcutBinding>,
    ) -> Result<Self, Vec<ShortcutTableProblem>> {
        // 安定な並べ替えなので、同じコマンドの行は渡した順のまま並ぶ。
        bindings.sort_by_key(ShortcutBinding::command);

        let mut problems = Vec::new();
        for pair in bindings.windows(2) {
            if pair[0].command == pair[1].command
                && problems.last() != Some(&ShortcutTableProblem::DuplicateCommand(pair[0].command))
            {
                problems.push(ShortcutTableProblem::DuplicateCommand(pair[0].command));
            }
        }
        for command in ShortcutCommandId::ALL {
            if !bindings.iter().any(|b| b.command == command) {
                problems.push(ShortcutTableProblem::Missing(command));
            }
        }
        for (i, first) in bindings.iter().enumerate() {
            for second in &bindings[i + 1..] {
                if first.command != second.command && first.accelerator == second.accelerator {
                    problems.push(ShortcutTableProblem::Conflict {
                        first: first.command,
                        second: second.command,
                        accelerator: first.accelerator,
                    });
                }
            }
        }

        if problems.is_empty() {
            Ok(Self(bindings))
        } else {
            Err(problems)
        }
    }

    /// すべての行(コマンドの ID の順)。
    #[must_use]
    pub fn bindings(&self) -> &[ShortcutBinding] {
        &self.0
    }

    /// コマンドの行。表はすべてのコマンドの行を持つので必ずある。
    ///
    /// # Panics
    ///
    /// 起きない(表は作るときに足りない行を拒否している)。
    #[must_use]
    pub fn get(&self, command: ShortcutCommandId) -> &ShortcutBinding {
        self.0
            .iter()
            .find(|b| b.command == command)
            .unwrap_or_else(|| panic!("表に {command} の行が無い"))
    }

    /// 受け口が `receiver` の行(コマンドの ID の順)。
    pub fn by_receiver(
        &self,
        receiver: ShortcutReceiver,
    ) -> impl Iterator<Item = &ShortcutBinding> + '_ {
        self.0.iter().filter(move |b| b.receiver == receiver)
    }

    /// 表の JSON(2 文字の字下げ、末尾に改行 1 つ)。`src/ipc/generated/defaultShortcuts.json` の中身。
    ///
    /// # Panics
    ///
    /// 起きない(文字列と列挙だけの値なので JSON にできる)。
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut text = serde_json::to_string_pretty(self)
            .unwrap_or_else(|e| panic!("ショートカットの表を JSON にできない: {e}"));
        text.push('\n');
        text
    }
}
