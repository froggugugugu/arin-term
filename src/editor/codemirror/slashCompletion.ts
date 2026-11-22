// 「/」コマンドを CodeMirror の補完として出す(ADR-017)。候補の表と組み立ては src/logic/editor/slashCommands.ts。
// - 行の先頭か空白のあとの「/」だけ。コードの塊・行内のコード・生の HTML の中では出さない。読み取り専用でも出さない
// - 絞り込みは自前(filter: false)。打つたびに問い合わせ直す(validFor を付けない)
// - 選んだら「/問い合わせ」を書式に置き換え、決めた位置にカーソルを置く

import type { Completion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { syntaxTree } from "@codemirror/language";
import { EditorSelection } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import type { SyntaxNode } from "@lezer/common";

import { filterSlashCommands, findSlashQuery, planSlashInsertion, type SlashCommand } from "../../logic/editor/slashCommands";

/** この中では「/」を書式の候補にしない構文の名前(lezer の Markdown)。 */
const NO_SLASH_NODES = new Set(["FencedCode", "CodeBlock", "InlineCode", "CodeText", "HTMLBlock", "HTMLTag", "Comment", "CommentBlock", "ProcessingInstruction", "ProcessingInstructionBlock"]);

function insideCode(context: CompletionContext): boolean {
  for (let node: SyntaxNode | null = syntaxTree(context.state).resolveInner(context.pos, -1); node; node = node.parent) {
    if (NO_SLASH_NODES.has(node.name)) return true;
  }
  return false;
}

function toCompletion(command: SlashCommand, linePrefix: string, previousLine: string): Completion {
  return {
    label: command.label,
    detail: command.aliases[0] ?? "",
    apply: (view: EditorView, _completion: Completion, from: number, to: number) => {
      const { text, cursor } = planSlashInsertion(command, linePrefix, previousLine);
      view.dispatch({
        changes: { from, to, insert: text },
        selection: EditorSelection.cursor(from + cursor),
        scrollIntoView: true,
        userEvent: "input.complete",
      });
    },
  };
}

/** 補完の出し元。`markdownLanguage.data.of({ autocomplete })` で取り付ける。 */
export function slashCompletion(context: CompletionContext): CompletionResult | null {
  if (context.state.readOnly) return null;
  const line = context.state.doc.lineAt(context.pos);
  const before = line.text.slice(0, context.pos - line.from);
  const found = findSlashQuery(before);
  if (found === null) return null;
  const from = line.from + found.slash;
  if (insideCode(context)) return null;
  const linePrefix = before.slice(0, found.slash);
  const previousLine = line.number > 1 ? context.state.doc.line(line.number - 1).text : "";
  const options = filterSlashCommands(found.query).map((command) => toCompletion(command, linePrefix, previousLine));
  if (options.length === 0) return null;
  return { from, options, filter: false };
}
