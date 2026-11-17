// xterm.js の CSS の読み込み(XtermEngine.ts)を型検査に通すための宣言。
// TypeScript 6 は副作用だけの import も解決先の型を求める(noUncheckedSideEffectImports)。中身は Vite が扱う。
declare module "@xterm/xterm/css/xterm.css";
