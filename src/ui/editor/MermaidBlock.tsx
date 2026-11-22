import { memo, useEffect, useRef, useState } from "react";

import type { DiagramResult } from "../../editor/diagram";

// プレビューの Mermaid の図(ADR-017)。描画(src/editor/mermaid)が作った SVG を blob: の URL にして <img> で出す。
// SVG を画面に直接は入れない(<img> の中の SVG はスクリプト・リンク・外の読み込みが動かない)。
//   - ライト・ダークは <picture> の media で選ぶ(テーマの切替を JS に書かない。tokens.css と同じ prefers-color-scheme)
//   - 描けるまで・描けなかったときは、書かれた文をコードの塊のまま出す(失敗の理由は文字だけ)
//   - 文が変わったら少し待ってから描く(作った直後はすぐ)。前の図は新しい図が届くまで出したまま

/** 文が変わってから描き始めるまでの待ち(ms)。 */
export const DIAGRAM_DEBOUNCE_MS = 300;

export type RenderDiagram = (source: string) => Promise<DiagramResult>;

/** 図の表示の状態。 */
export type DiagramView =
  | { kind: "pending" }
  | { kind: "ok"; lightUrl: string; darkUrl: string }
  | { kind: "error"; message: string };

interface MermaidBlockProps {
  source: string;
  renderDiagram: RenderDiagram;
}

/** Mermaid の塊 1 つ。描画を呼び、結果を MermaidFigure に渡す。 */
export const MermaidBlock = memo(function MermaidBlock({ source, renderDiagram }: MermaidBlockProps) {
  const [view, setView] = useState<DiagramView>({ kind: "pending" });
  // いま出している URL。差し替えたときと外したときに手放す
  const urls = useRef<readonly string[]>([]);

  // 作った直後(タブを開いた・塊が増えた)はすぐ描く。打って文が変わったときだけ待つ
  const first = useRef(true);

  useEffect(() => {
    let cancelled = false;
    const delay = first.current ? 0 : DIAGRAM_DEBOUNCE_MS;
    first.current = false;
    const timer = setTimeout(() => {
      void renderDiagram(source).then((result) => {
        if (cancelled) return;
        const previous = urls.current;
        if (result.ok) {
          const lightUrl = svgUrl(result.light);
          const darkUrl = svgUrl(result.dark);
          urls.current = [lightUrl, darkUrl];
          setView({ kind: "ok", lightUrl, darkUrl });
        } else {
          urls.current = [];
          setView({ kind: "error", message: result.message });
        }
        for (const url of previous) URL.revokeObjectURL(url);
      });
    }, delay);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [source, renderDiagram]);

  useEffect(
    () => () => {
      // 外したとき、最後に出していた URL を手放す(DOM の参照ではなく、自分で作った URL の一覧なので最新を読んでよい)
      for (const url of urls.current) URL.revokeObjectURL(url);
      urls.current = [];
    },
    [],
  );

  return <MermaidFigure source={source} view={view} />;
});

function svgUrl(svg: string): string {
  return URL.createObjectURL(new Blob([svg], { type: "image/svg+xml" }));
}

/** 図の見た目だけ(状態を持たない。テストは静的な HTML で確かめる)。 */
export function MermaidFigure({ source, view }: { source: string; view: DiagramView }) {
  if (view.kind === "ok") {
    return (
      <figure className="md-diagram" data-testid="md-diagram">
        <picture>
          <source media="(prefers-color-scheme: dark)" srcSet={view.darkUrl} />
          <img src={view.lightUrl} alt="Mermaid の図" />
        </picture>
      </figure>
    );
  }
  return (
    <div className="md-diagram-source">
      <pre className="md-code">
        <code>{source}</code>
      </pre>
      {view.kind === "error" && (
        <p className="md-diagram-error" role="note" data-testid="md-diagram-error">
          図を描けませんでした: {view.message}
        </p>
      )}
    </div>
  );
}
