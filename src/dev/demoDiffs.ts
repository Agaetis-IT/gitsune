import type {
  ChangeStatus,
  CommitDetail,
  DiffHunk,
  DiffLine,
  FileChange,
  FileDiff,
} from "@/lib/api";

/** Invented commit contents so every state of the diff view can be seen without a backend. */

/** Parses a snippet of unified diff (lines starting with ` `, `+` or `-`) into one hunk. */
let hunkCount = 0;

export function hunk(oldStart: number, newStart: number, snippet: string): DiffHunk {
  const lines: DiffLine[] = [];
  let oldNo = oldStart;
  let newNo = newStart;
  for (const raw of snippet.split("\n")) {
    const text = raw.slice(1);
    if (raw.startsWith("+")) {
      lines.push({ kind: "add", oldNo: null, newNo: newNo++, text, noEol: false });
    } else if (raw.startsWith("-")) {
      lines.push({ kind: "delete", oldNo: oldNo++, newNo: null, text, noEol: false });
    } else {
      lines.push({ kind: "context", oldNo: oldNo++, newNo: newNo++, text, noEol: false });
    }
  }
  const oldLines = lines.filter((l) => l.oldNo !== null).length;
  const newLines = lines.filter((l) => l.newNo !== null).length;
  return {
    id: `demo-hunk-${++hunkCount}`,
    oldStart,
    oldLines,
    newStart,
    newLines,
    header: `@@ -${oldStart},${oldLines} +${newStart},${newLines} @@`,
    lines,
  };
}

const LEXER_HUNKS = [
  hunk(
    10,
    10,
    [
      " export class Lexer {",
      "   private position = 0;",
      "-  private line = 1;",
      "+  private line = 1;",
      "+  private column = 0;",
      " ",
      "   constructor(private readonly source: string) {}",
      " ",
    ].join("\n"),
  ),
  hunk(
    42,
    43,
    [
      "   next(): Token | null {",
      "-    const char = this.source[this.position++];",
      "-    if (char === undefined) return null;",
      "-    return { kind: classify(char), text: char, line: this.line };",
      "+    const char = this.source[this.position];",
      "+    if (char === undefined) return null;",
      '+    this.position += 1;',
      '+    this.column = char === "\\n" ? 0 : this.column + 1;',
      '+    if (char === "\\n") this.line += 1;',
      "+    // Keep the position of every token for error messages.",
      "+    return { kind: classify(char), text: char, line: this.line, column: this.column };",
      "   }",
      " }",
    ].join("\n"),
  ),
];

const TOKENS_HUNK = hunk(
  0,
  1,
  [
    "+export type TokenKind = 'word' | 'number' | 'symbol' | 'space';",
    "+",
    "+export interface Token {",
    "+  kind: TokenKind;",
    "+  text: string;",
    "+  line: number;",
    "+  column: number;",
    "+}",
    "+",
    "+export const isWord = (token: Token): boolean => token.kind === 'word';",
  ].join("\n"),
);

const NOTES_HUNK = hunk(
  1,
  0,
  ["-# Old notes", "-", "-Things we tried before the rewrite:", "-", "-- a hand-written tokenizer", "-- regular expressions everywhere"].join(
    "\n",
  ),
);

const BUTTON_HUNK = hunk(
  3,
  3,
  [
    " <template>",
    '-  <button class="btn"><slot /></button>',
    '+  <button class="btn" type="button"><slot /></button>',
    " </template>",
  ].join("\n"),
);

const README_HUNK = hunk(
  1,
  1,
  ["-# Parser demo", "+# Parser demo (invented data)", " ", " A small project used to try out the diff view."].join("\n"),
);

function generatedJson(count: number): DiffHunk {
  const lines = ["+["];
  for (let i = 0; i < count; i++) {
    lines.push(`+  { "id": ${i}, "name": "athlete-${i}", "score": ${(i * 37) % 100} },`);
  }
  return hunk(0, 1, lines.join("\n"));
}

interface DemoFile extends FileChange {
  size?: [number, number];
  hunks?: DiffHunk[];
  large?: boolean;
  forceable?: boolean;
}

const file = (
  path: string,
  status: ChangeStatus,
  extra: Partial<DemoFile> = {},
): DemoFile => ({
  path,
  oldPath: null,
  status,
  additions: null,
  deletions: null,
  binary: false,
  ...extra,
});

const count = (hunks: DiffHunk[], kind: "add" | "delete") =>
  hunks.reduce((n, h) => n + h.lines.filter((l) => l.kind === kind).length, 0);

const DEMO_FILES: DemoFile[] = [
  file("README.md", "modified", {
    hunks: [README_HUNK],
    additions: count([README_HUNK], "add"),
    deletions: count([README_HUNK], "delete"),
  }),
  file("assets/logo.png", "modified", { binary: true, size: [48213, 51876] }),
  file("data/fixtures.json", "added", { large: true, forceable: true, size: [0, 2_400_000] }),
  file("docs/old-notes.md", "deleted", {
    hunks: [NOTES_HUNK],
    additions: 0,
    deletions: count([NOTES_HUNK], "delete"),
  }),
  file("src/parser/lexer.ts", "modified", {
    hunks: LEXER_HUNKS,
    additions: count(LEXER_HUNKS, "add"),
    deletions: count(LEXER_HUNKS, "delete"),
  }),
  file("src/parser/tokens.ts", "added", {
    hunks: [TOKENS_HUNK],
    additions: count([TOKENS_HUNK], "add"),
    deletions: 0,
  }),
  file("src/ui/ActionButton.vue", "renamed", {
    oldPath: "src/ui/Button.vue",
    hunks: [BUTTON_HUNK],
    additions: 1,
    deletions: 1,
  }),
  file("vendor/huge.dat", "added", { large: true, forceable: false, size: [0, 12_000_000] }),
];

const toChange = (demo: DemoFile): FileChange => ({
  path: demo.path,
  oldPath: demo.oldPath,
  status: demo.status,
  additions: demo.additions,
  deletions: demo.deletions,
  binary: demo.binary,
});

export function demoCommitDetail(id: string): CommitDetail {
  return {
    id,
    parents: [],
    authorName: "Alice Fakename",
    authorEmail: "alice@example.invalid",
    authorTime: 1_577_836_800,
    message:
      "Track token columns in the lexer\n\nThe lexer now remembers the column of every token so that\nerror messages can point at the right place.\n\nAlso renames Button to ActionButton and drops the old notes.",
    files: DEMO_FILES.map(toChange),
    filesTruncated: false,
  };
}

export function demoFileDiff(change: FileChange, force: boolean): FileDiff {
  const demo = DEMO_FILES.find((f) => f.path === change.path);
  const base: FileDiff = {
    path: change.path,
    oldPath: change.oldPath,
    status: change.status,
    binary: false,
    submodule: false,
    tooLarge: false,
    canForce: false,
    truncated: false,
    oldSize: demo?.size?.[0] ?? null,
    newSize: demo?.size?.[1] ?? null,
    additions: 0,
    deletions: 0,
    hunks: [],
  };
  if (!demo) return base;
  if (demo.binary) return { ...base, binary: true };
  if (demo.large && !(force && demo.forceable)) {
    return { ...base, tooLarge: true, canForce: demo.forceable ?? false };
  }
  if (demo.large) {
    const hunks = [generatedJson(1200)];
    return { ...base, hunks, additions: 20_000, truncated: true };
  }
  const hunks = demo.hunks ?? [];
  return { ...base, hunks, additions: count(hunks, "add"), deletions: count(hunks, "delete") };
}
