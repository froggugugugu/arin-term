import "./MarkdownPreview.css";

import { type ReactNode, createElement, memo } from "react";

import { isMermaidInfo } from "../../editor/diagram";
import type { MdAlign, MdBlock, MdInline, MdListItem } from "../../editor/markdownTypes";
import { MermaidBlock, type RenderDiagram } from "./MermaidBlock";

// Markdown のプレビュー(ADR-015 の 2・7、AC-E03・E10)。自前の木(src/editor/markdownTypes)を React の要素にする。
// HTML の文字列は使わない(innerHTML・dangerouslySetInnerHTML を使わない。境界検査が見る)。
//   - 生の HTML は文字として出す(淡い等幅)
//   - 画像は読み込まず `[画像: 代わりの文字]`(<img> を作らない)
//   - 例外は Mermaid の塊だけ(ADR-017)。自前の描画が作った SVG を blob: の URL にして <img> で出す(MermaidBlock)。
//     描画を渡されていなければ、ふつうのコードの塊として出す
//   - リンクは開かない。<a> を作らず <span class="md-link"> に下線だけ付け、title に URL の文字を出す
//   - タスクの印は読み取り専用の文字 ☑ / ☐(<input> を作らない)
//   - 見出しに id を付けない(文書の中身で DOM の名前を作らない)
// 揃え・字の組みはクラスで当てる。style 属性は外側の本文の大きさ(ターミナルの文字の大きさ)の 1 つだけ。

export interface MarkdownPreviewProps {
  /** 文書の木(`parseMarkdown` の結果)。 */
  blocks: readonly MdBlock[];
  /** 本文の文字の大きさ(px)。ターミナルの文字の大きさ(`fontSizes.terminal.value`)を渡す。見出しは em で相対。 */
  fontSize: number;
  /** Mermaid の図を描く(ADR-017)。無ければ Mermaid の塊もコードの塊として出す。 */
  renderDiagram?: RenderDiagram;
}

/** プレビューの部品。木が同じなら描き直さない。 */
export const MarkdownPreview = memo(function MarkdownPreview({ blocks, fontSize, renderDiagram }: MarkdownPreviewProps) {
  return (
    <div className="md-preview" style={{ fontSize: `${fontSize}px` }}>
      {renderBlocks(blocks, renderDiagram)}
    </div>
  );
});

// 中身は文書から作り直すだけで並べ替えないので、キーは位置で足りる(行・列・項目も同じ)。
// Mermaid の図だけは「何番目の図か」をキーにする(上に段落を足しても図を作り直さず、描いた図を出したままにする)。
// 図の描画は木を下りながら渡す(引用やリストの中の Mermaid の塊も図にする)。
function renderBlocks(blocks: readonly MdBlock[], diagram: RenderDiagram | undefined): ReactNode[] {
  let diagrams = 0;
  return blocks.map((block, index) => {
    if (diagram !== undefined && block.kind === "code" && isMermaidInfo(block.info)) {
      const key = `diagram-${diagrams}`;
      diagrams += 1;
      return <MermaidBlock key={key} source={block.text} renderDiagram={diagram} />;
    }
    return renderBlock(block, index, diagram);
  });
}

function renderInlines(inlines: readonly MdInline[]): ReactNode[] {
  return inlines.map((inline, index) => renderInline(inline, index));
}

const ALIGN_CLASS: Readonly<Record<Exclude<MdAlign, null>, string>> = {
  left: "md-align-left",
  center: "md-align-center",
  right: "md-align-right",
};

const alignClass = (align: MdAlign | undefined): string | undefined =>
  align === null || align === undefined ? undefined : ALIGN_CLASS[align];

function renderBlock(block: MdBlock, key: number, diagram: RenderDiagram | undefined): ReactNode {
  switch (block.kind) {
    case "heading":
      return createElement(`h${block.level}`, { key }, renderInlines(block.children));
    case "paragraph":
      return <p key={key}>{renderInlines(block.children)}</p>;
    case "code":
      return (
        <pre key={key} className="md-code">
          <code>{block.text}</code>
        </pre>
      );
    case "quote":
      return <blockquote key={key}>{renderBlocks(block.children, diagram)}</blockquote>;
    case "list": {
      const items = block.items.map((item, index) => renderItem(item, index, diagram));
      if (!block.ordered) return <ul key={key}>{items}</ul>;
      return (
        <ol key={key} start={block.start === null || block.start === 1 ? undefined : block.start}>
          {items}
        </ol>
      );
    }
    case "table":
      return (
        <table key={key} className="md-table">
          <thead>
            <tr>
              {block.head.map((cell, index) => (
                <th key={index} className={alignClass(block.align[index])}>
                  {renderInlines(cell)}
                </th>
              ))}
            </tr>
          </thead>
          {block.rows.length > 0 && (
            <tbody>
              {block.rows.map((row, rowIndex) => (
                <tr key={rowIndex}>
                  {row.map((cell, index) => (
                    <td key={index} className={alignClass(block.align[index])}>
                      {renderInlines(cell)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          )}
        </table>
      );
    case "rule":
      return <hr key={key} />;
    case "raw":
      return (
        <pre key={key} className="md-raw">
          {block.text}
        </pre>
      );
    case "truncated":
      return (
        <pre key={key} className="md-truncated">
          {block.text}
        </pre>
      );
  }
}

function renderItem(item: MdListItem, key: number, diagram: RenderDiagram | undefined): ReactNode {
  if (item.checked === null) return <li key={key}>{renderBlocks(item.children, diagram)}</li>;
  return (
    <li key={key} className="md-task">
      <span className="md-task-mark" role="img" aria-label={item.checked ? "済み" : "未完了"}>
        {item.checked ? "☑" : "☐"}
      </span>
      {renderBlocks(item.children, diagram)}
    </li>
  );
}

function renderInline(inline: MdInline, key: number): ReactNode {
  switch (inline.kind) {
    case "text":
      return inline.text;
    case "emphasis":
      return <em key={key}>{renderInlines(inline.children)}</em>;
    case "strong":
      return <strong key={key}>{renderInlines(inline.children)}</strong>;
    case "strikethrough":
      return <del key={key}>{renderInlines(inline.children)}</del>;
    case "code":
      return (
        <code key={key} className="md-inline-code">
          {inline.text}
        </code>
      );
    case "link":
      return (
        <span key={key} className="md-link" title={inline.url === "" ? "リンクは開けません" : `リンクは開けません: ${inline.url}`}>
          {renderInlines(inline.children)}
        </span>
      );
    case "image":
      return (
        <span key={key} className="md-image">
          {inline.alt === "" ? "[画像]" : `[画像: ${inline.alt}]`}
        </span>
      );
    case "raw":
      return (
        <span key={key} className="md-raw">
          {inline.text}
        </span>
      );
    case "break":
      return <br key={key} />;
    case "truncated":
      return (
        <span key={key} className="md-truncated">
          {inline.text}
        </span>
      );
  }
}
