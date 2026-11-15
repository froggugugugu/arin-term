// 画面の小さなアイコン(設計書 §15.2 #12: 自作の少数の SVG。ツリーのほか、上部の帯とサイドバーの道具のボタンも使う)。
// どれも 16×16 の単色の線画で、色は currentColor(CSS で控えめな色を付ける)。
// 飾りなので読み上げない(aria-hidden。意味はボタンの aria-label や行の役割と名前が伝える)。

interface IconProps {
  className?: string;
}

const common = {
  width: 16,
  height: 16,
  viewBox: "0 0 16 16",
  fill: "none",
  "aria-hidden": true,
  focusable: false,
} as const;

/** 開閉の印(右向きの山形。開くと CSS で 90 度回す)。 */
export function ChevronIcon({ className }: IconProps) {
  return (
    <svg {...common} className={className}>
      <path d="M6 4.5 9.5 8 6 11.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

/** フォルダ(薄く塗った面と輪郭)。 */
export function FolderIcon({ className }: IconProps) {
  return (
    <svg {...common} className={className}>
      <path
        d="M1.75 4.25c0-.69.56-1.25 1.25-1.25h3.1l1.4 1.5H13c.69 0 1.25.56 1.25 1.25v6.5c0 .69-.56 1.25-1.25 1.25H3c-.69 0-1.25-.56-1.25-1.25v-8Z"
        fill="currentColor"
        fillOpacity="0.22"
        stroke="currentColor"
        strokeWidth="1.1"
        strokeLinejoin="round"
      />
      <path d="M1.75 6.25h12.5" stroke="currentColor" strokeWidth="1.1" />
    </svg>
  );
}

/** ファイル(右上を折った紙)。 */
export function FileIcon({ className }: IconProps) {
  return (
    <svg {...common} className={className}>
      <path
        d="M3.75 2.25c0-.28.22-.5.5-.5h4.6l3.4 3.4v8.6c0 .28-.22.5-.5.5h-7.5a.5.5 0 0 1-.5-.5v-11.5Z"
        stroke="currentColor"
        strokeWidth="1.1"
        strokeLinejoin="round"
      />
      <path d="M8.75 1.9v3.35h3.35" stroke="currentColor" strokeWidth="1.1" strokeLinejoin="round" />
    </svg>
  );
}

/** リンクの印(アイコンの左下に重ねる小さな曲がった矢印。Finder のエイリアスの印にならう)。 */
export function LinkBadgeIcon({ className }: IconProps) {
  return (
    <svg {...common} className={className}>
      <rect x="0.5" y="8.5" width="7" height="7" rx="1.75" className="tree-icon__badge-bg" />
      <path d="M2.4 13.6c0-1.9 1-2.8 2.8-2.8" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
      <path d="M4.1 9.6 5.4 10.8 4.1 12" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

/** 隠しファイルを表示(目)。 */
export function EyeIcon({ className }: IconProps) {
  return (
    <svg {...common} className={className}>
      <path
        d="M1.5 8s2.4-4.25 6.5-4.25S14.5 8 14.5 8s-2.4 4.25-6.5 4.25S1.5 8 1.5 8Z"
        stroke="currentColor"
        strokeWidth="1.1"
        strokeLinejoin="round"
      />
      <circle cx="8" cy="8" r="2" stroke="currentColor" strokeWidth="1.1" />
    </svg>
  );
}

/** すべて折りたたむ(上下から内へ向かう山形)。 */
export function CollapseAllIcon({ className }: IconProps) {
  return (
    <svg {...common} className={className}>
      <path d="M5 2.75 8 5.5l3-2.75M5 13.25 8 10.5l3 2.75" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round" />
      <path d="M3 8h10" stroke="currentColor" strokeWidth="1.1" strokeLinecap="round" />
    </svg>
  );
}

/** 閉じる(×。知らせを閉じる)。 */
export function CloseIcon({ className }: IconProps) {
  return (
    <svg {...common} className={className}>
      <path d="m4.5 4.5 7 7m0-7-7 7" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
    </svg>
  );
}

/** サイドバーの開閉(macOS のツールバーのサイドバーの印: 角の丸い枠と、左に寄せた仕切り)。 */
export function SidebarIcon({ className }: IconProps) {
  return (
    <svg {...common} className={className}>
      <rect x="1.75" y="2.75" width="12.5" height="10.5" rx="2.25" stroke="currentColor" strokeWidth="1.1" />
      <path d="M6.25 2.75v10.5" stroke="currentColor" strokeWidth="1.1" />
      <path d="M3.5 5.25h1.25M3.5 7h1.25M3.5 8.75h1.25" stroke="currentColor" strokeWidth="1" strokeLinecap="round" />
    </svg>
  );
}

/** 追加(+。フォルダを追加)。 */
export function PlusIcon({ className }: IconProps) {
  return (
    <svg {...common} className={className}>
      <path d="M8 3.25v9.5M3.25 8h9.5" stroke="currentColor" strokeWidth="1.25" strokeLinecap="round" />
    </svg>
  );
}
