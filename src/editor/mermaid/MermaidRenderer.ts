// Mermaid で図を描く(ADR-017)。mermaid を import してよいのは src/editor/mermaid/ だけ(境界検査)。
// src/ipc は import しない。EditorManager が動的 import で読み、mermaid 本体は最初に描くときに読む。
//
// - securityLevel は strict(図の中の HTML を文字にし、クリックの処理を付けない)
// - htmlLabels は使わない(文字は SVG の <text>。画面は SVG を <img> で出すので foreignObject に頼らない)
// - ライト(neutral。白黒の落ち着いた色)とダーク(dark)の 2 つを描く。どちらを見せるかは画面の <picture> の media が決める
// - mermaid の設定は全体で 1 つなので、描画は 1 つずつ順に行う
// - 同じ文の結果は使い回す(打つたびに描き直さない)

import type { DiagramRenderer, DiagramResult } from "../diagram";

type Mermaid = (typeof import("mermaid"))["default"];

/** 使い回す結果の数。 */
const CACHE_LIMIT = 32;

const FONT_FAMILY = '-apple-system, BlinkMacSystemFont, "Hiragino Sans", sans-serif';

/** 図の種類ごとの useMaxWidth を切る(mermaid は種類ごとの設定にしかこの項目を持たない)。 */
const NO_MAX_WIDTH = Object.fromEntries(
  [
    "flowchart",
    "sequence",
    "gantt",
    "journey",
    "timeline",
    "class",
    "state",
    "er",
    "pie",
    "quadrantChart",
    "xyChart",
    "requirement",
    "mindmap",
    "kanban",
    "gitGraph",
    "c4",
    "sankey",
    "packet",
    "block",
    "radar",
    "architecture",
  ].map((key) => [key, { useMaxWidth: false }]),
);

let nextId = 0;

class MermaidRenderer implements DiagramRenderer {
  #mermaid: Promise<Mermaid> | null = null;
  /** 描画を 1 つずつ順に行うための鎖。 */
  #queue: Promise<unknown> = Promise.resolve();
  readonly #cache = new Map<string, DiagramResult>();

  render(source: string): Promise<DiagramResult> {
    const cached = this.#cache.get(source);
    if (cached !== undefined) {
      // 使った順に並べ直す(古いものから捨てる)
      this.#cache.delete(source);
      this.#cache.set(source, cached);
      return Promise.resolve(cached);
    }
    const run = this.#queue.then(() => this.#renderNow(source));
    this.#queue = run.catch(() => undefined);
    return run;
  }

  async #renderNow(source: string): Promise<DiagramResult> {
    const again = this.#cache.get(source);
    if (again !== undefined) return again;
    // 読み込みの失敗は使い回さない(次の描画でもう一度読む)。使い回すのは描いた結果(文法の誤りを含む)だけ
    let mermaid: Mermaid;
    try {
      mermaid = await this.#load();
    } catch (error) {
      return { ok: false, message: `図の部品を読み込めませんでした: ${describe(error)}` };
    }
    let result: DiagramResult;
    try {
      const light = await renderWith(mermaid, source, "neutral");
      const dark = await renderWith(mermaid, source, "dark");
      result = { ok: true, light, dark };
    } catch (error) {
      result = { ok: false, message: describe(error) };
    }
    this.#cache.set(source, result);
    if (this.#cache.size > CACHE_LIMIT) {
      const oldest = this.#cache.keys().next();
      if (!oldest.done) this.#cache.delete(oldest.value);
    }
    return result;
  }

  #load(): Promise<Mermaid> {
    if (this.#mermaid === null) {
      this.#mermaid = import("mermaid").then(
        (module) => module.default,
        (error: unknown) => {
          // 次の描画でもう一度読む
          this.#mermaid = null;
          throw error;
        },
      );
    }
    return this.#mermaid;
  }
}

async function renderWith(mermaid: Mermaid, source: string, theme: "neutral" | "dark"): Promise<string> {
  mermaid.initialize({
    startOnLoad: false,
    securityLevel: "strict",
    htmlLabels: false,
    suppressErrorRendering: true,
    theme,
    fontFamily: FONT_FAMILY,
    // 幅を 100% にせず、図の大きさを SVG に書かせる(<img> で元の大きさに出し、広すぎるときだけ CSS で縮める)
    ...NO_MAX_WIDTH,
  });
  nextId += 1;
  const { svg } = await mermaid.render(`arin-mermaid-${nextId}`, source);
  return svg;
}

function describe(error: unknown): string {
  if (error instanceof Error && error.message !== "") return error.message;
  if (typeof error === "string" && error !== "") return error;
  return "原因が分かりません";
}

/** Mermaid の描画。EditorManager の既定の読み込みが使う。 */
export function createMermaidRenderer(): DiagramRenderer {
  return new MermaidRenderer();
}
