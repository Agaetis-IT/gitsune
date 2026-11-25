/**
 * Generates a synthetic git repository with an invented history (fake authors,
 * fake files) using `git fast-import`. Used for benchmarks and layout tests.
 *
 * Usage: bun scripts/gen-synthetic-repo.ts <target-dir> [commits=100000] [seed=1]
 *
 * The history contains a long-lived `main`, feature branches that fork from
 * random earlier points, merges back into main (some branches are deleted after
 * merging), a few kept branches, fake remote refs and tags.
 */
import { existsSync, mkdirSync, readdirSync } from "node:fs";

const [, , targetArg, countArg, seedArg] = process.argv;
if (!targetArg) {
  console.error("Usage: bun scripts/gen-synthetic-repo.ts <target-dir> [commits] [seed]");
  process.exit(1);
}
const target = targetArg;
const total = Number(countArg ?? 100_000);
let seed = Number(seedArg ?? 1) >>> 0;

if (existsSync(target) && readdirSync(target).length > 0) {
  console.error(`Refusing to write into non-empty directory: ${target}`);
  process.exit(1);
}
mkdirSync(target, { recursive: true });

// Small deterministic PRNG (mulberry32) so the same seed gives the same repo.
function rand(): number {
  seed = (seed + 0x6d2b79f5) >>> 0;
  let t = seed;
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
}
const pick = <T>(items: T[]): T => items[Math.floor(rand() * items.length)];

const authors = [
  ["Alice Fakename", "alice@example.invalid"],
  ["Bob Placeholder", "bob@example.invalid"],
  ["Chloe Sample", "chloe@example.invalid"],
  ["Dan Testuser", "dan@example.invalid"],
  ["Eve Mockson", "eve@example.invalid"],
];
const topics = ["parser", "cache", "ui", "api", "docs", "build", "auth", "report", "sync", "perf"];
const files = Array.from(
  { length: 200 },
  (_, i) => `src/${topics[i % topics.length]}/file${i}.txt`,
);

type Branch = { name: string; tip: number; commits: number };
const chunks: string[] = [];
const out = (s: string) => chunks.push(s);

const start = 1_577_836_800; // 2020-01-01T00:00:00Z
let mark = 0;
let featureCounter = 0;
const branches: Branch[] = [];
const marks: number[] = [];

function commit(ref: string, message: string, from?: number, mergeFrom?: number): number {
  mark += 1;
  const [name, email] = pick(authors);
  const ts = start + mark * 60;
  const file = pick(files);
  const body = `${message}\nrevision ${mark}\n`;
  out(`commit refs/heads/${ref}\nmark :${mark}\n`);
  out(`author ${name} <${email}> ${ts} +0000\n`);
  out(`committer ${name} <${email}> ${ts} +0000\n`);
  out(`data ${Buffer.byteLength(message)}\n${message}\n`);
  if (from !== undefined) out(`from :${from}\n`);
  if (mergeFrom !== undefined) out(`merge :${mergeFrom}\n`);
  out(`M 100644 inline ${file}\ndata ${Buffer.byteLength(body)}\n${body}\n`);
  marks.push(mark);
  return mark;
}

const main: Branch = { name: "main", tip: commit("main", "Initial commit"), commits: 1 };
branches.push(main);
const deleted: string[] = [];
const kept: Branch[] = [];

while (mark < total) {
  const roll = rand();
  const live = branches.filter((b) => b.name !== "main");
  if (roll < 0.05 && live.length < 12) {
    // Fork a new feature branch from a random earlier commit.
    featureCounter += 1;
    const name = `feature/${pick(topics)}-${featureCounter}`;
    const base = marks[Math.floor(rand() * marks.length)];
    const tip = commit(name, `Start ${name}`, base);
    branches.push({ name, tip, commits: 1 });
  } else if (roll < 0.08 && live.length > 0) {
    // Merge a feature branch into main, then keep or delete it.
    const feature = pick(live);
    main.tip = commit("main", `Merge branch '${feature.name}'`, undefined, feature.tip);
    branches.splice(branches.indexOf(feature), 1);
    if (rand() < 0.7) deleted.push(feature.name);
    else kept.push(feature);
  } else if (roll < 0.55 || live.length === 0) {
    main.tip = commit("main", `Update ${pick(topics)}`);
  } else {
    const branch = pick(live);
    branch.tip = commit(branch.name, `Work on ${branch.name}`);
    branch.commits += 1;
  }
}

// Remove refs of merged-and-deleted branches (null sha deletes a ref).
const NULL_SHA = "0".repeat(40);
for (const name of deleted) out(`reset refs/heads/${name}\nfrom ${NULL_SHA}\n\n`);
// Fake remote-tracking refs and tags.
out(`reset refs/remotes/origin/main\nfrom :${main.tip}\n\n`);
for (const b of [...branches, ...kept].slice(0, 5)) {
  out(`reset refs/remotes/origin/${b.name}\nfrom :${b.tip}\n\n`);
}
for (let i = 1; i <= 10; i++) {
  const at = marks[Math.floor((marks.length * i) / 11)];
  out(`reset refs/tags/v${i}.0.0\nfrom :${at}\n\n`);
}
out("done\n");

async function run(cmd: string[], stdin?: string) {
  const proc = Bun.spawn(cmd, { cwd: target, stdin: stdin ? "pipe" : "ignore", stderr: "inherit" });
  if (stdin) {
    proc.stdin!.write(stdin);
    await proc.stdin!.end();
  }
  const code = await proc.exited;
  if (code !== 0) throw new Error(`${cmd.join(" ")} exited with ${code}`);
}

await run(["git", "init", "-q", "-b", "main"]);
await run(["git", "fast-import", "--quiet", "--done"], chunks.join(""));
await run(["git", "reset", "-q", "--hard", "main"]);
console.log(`Generated ${mark} commits in ${target}`);
