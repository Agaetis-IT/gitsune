import hljs from "highlight.js/lib/core";
import type { LanguageFn } from "highlight.js";

type Loader = () => Promise<{ default: LanguageFn }>;

/** Languages are loaded the first time a file needs them. */
const LANGUAGES: Record<string, Loader> = {
  typescript: () => import("highlight.js/lib/languages/typescript"),
  javascript: () => import("highlight.js/lib/languages/javascript"),
  json: () => import("highlight.js/lib/languages/json"),
  xml: () => import("highlight.js/lib/languages/xml"),
  css: () => import("highlight.js/lib/languages/css"),
  scss: () => import("highlight.js/lib/languages/scss"),
  rust: () => import("highlight.js/lib/languages/rust"),
  markdown: () => import("highlight.js/lib/languages/markdown"),
  yaml: () => import("highlight.js/lib/languages/yaml"),
  bash: () => import("highlight.js/lib/languages/bash"),
  python: () => import("highlight.js/lib/languages/python"),
  java: () => import("highlight.js/lib/languages/java"),
  csharp: () => import("highlight.js/lib/languages/csharp"),
  sql: () => import("highlight.js/lib/languages/sql"),
  ini: () => import("highlight.js/lib/languages/ini"),
  go: () => import("highlight.js/lib/languages/go"),
  c: () => import("highlight.js/lib/languages/c"),
  cpp: () => import("highlight.js/lib/languages/cpp"),
};

const EXTENSIONS: Record<string, string> = {
  ts: "typescript",
  tsx: "typescript",
  mts: "typescript",
  cts: "typescript",
  js: "javascript",
  jsx: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  json: "json",
  html: "xml",
  htm: "xml",
  xml: "xml",
  svg: "xml",
  vue: "xml",
  css: "css",
  scss: "scss",
  rs: "rust",
  md: "markdown",
  yml: "yaml",
  yaml: "yaml",
  sh: "bash",
  bash: "bash",
  zsh: "bash",
  py: "python",
  java: "java",
  cs: "csharp",
  sql: "sql",
  toml: "ini",
  ini: "ini",
  go: "go",
  c: "c",
  h: "c",
  cpp: "cpp",
  cc: "cpp",
  hpp: "cpp",
};

/** Language name for a file path, `null` when we do not highlight it. */
export function languageForPath(path: string): string | null {
  const name = path.split("/").pop() ?? path;
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return null;
  return EXTENSIONS[name.slice(dot + 1).toLowerCase()] ?? null;
}

export function escapeHtml(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/** Turns one line of code into HTML (every character of the input is escaped). */
export type LineHighlighter = (line: string) => string;

/**
 * Highlighter for the language of `path`, or `null` if it is unknown or fails to load.
 * Lines are highlighted one by one, so tokens spanning several lines (block comments,
 * template strings) are not recognised: this is deliberately light.
 */
export async function highlighterFor(path: string): Promise<LineHighlighter | null> {
  const language = languageForPath(path);
  if (!language) return null;
  try {
    if (!hljs.getLanguage(language)) {
      hljs.registerLanguage(language, (await LANGUAGES[language]()).default);
    }
  } catch {
    return null;
  }
  return (line) => hljs.highlight(line, { language, ignoreIllegals: true }).value;
}
