import { describe, expect, it } from "vitest";
import type { RefInfo } from "@/lib/api";
import { buildRows, folderPaths } from "./refTree";

const ref = (name: string, isHead = false, kind: RefInfo["kind"] = "local"): RefInfo => ({
  name,
  fullName: `refs/${kind}/${name}`,
  kind,
  target: "0".repeat(40),
  isHead,
});

const describeRows = (rows: ReturnType<typeof buildRows>) =>
  rows.map((r) =>
    r.kind === "folder"
      ? `${" ".repeat(r.depth)}[${r.name}${r.collapsed ? " +" : ""}] ${r.refs.length}`
      : `${" ".repeat(r.depth)}${r.label}`,
  );

describe("the branch list in folders", () => {
  const refs = [
    ref("main", true),
    ref("feature/auth-18"),
    ref("feature/auth-6"),
    ref("feature/api/v2"),
    ref("feature/api/v10"),
    ref("hotfix/x"),
    ref("develop"),
  ];

  it("makes a folder of each part before a slash, nested, with folders first", () => {
    expect(describeRows(buildRows(refs, new Set()))).toEqual([
      "[feature] 4",
      " [api] 2",
      "  v2",
      "  v10",
      " auth-6",
      " auth-18",
      "[hotfix] 1",
      " x",
      "develop",
      "main",
    ]);
  });

  it("sorts numbers as numbers", () => {
    const rows = buildRows([ref("a/10"), ref("a/9"), ref("a/100")], new Set());
    expect(describeRows(rows)).toEqual(["[a] 3", " 9", " 10", " 100"]);
  });

  it("a closed folder hides what is inside and says so", () => {
    expect(describeRows(buildRows(refs, new Set(["feature"])))).toEqual([
      "[feature +] 4",
      "[hotfix] 1",
      " x",
      "develop",
      "main",
    ]);
    // Closing a folder inside another keeps the outer one open.
    expect(describeRows(buildRows(refs, new Set(["feature/api"])))).toContain(" [api +] 2");
  });

  it("tells that a folder holds HEAD, so it can be marked while closed", () => {
    const rows = buildRows([ref("feature/x", true), ref("feature/y"), ref("other/z")], new Set());
    const folders = rows.filter((r) => r.kind === "folder");
    expect(folders.map((f) => f.kind === "folder" && [f.name, f.hasHead])).toEqual([
      ["feature", true],
      ["other", false],
    ]);
  });

  it("keeps the remote name as the first folder", () => {
    const remotes = [
      ref("origin/main", false, "remote"),
      ref("origin/feature/x", false, "remote"),
      ref("upstream/main", false, "remote"),
    ];
    expect(describeRows(buildRows(remotes, new Set()))).toEqual([
      "[origin] 2",
      " [feature] 1",
      "  x",
      " main",
      "[upstream] 1",
      " main",
    ]);
  });

  it("ignores empty parts of a name and handles a flat list and an empty one", () => {
    expect(describeRows(buildRows([ref("a//b"), ref("/c")], new Set()))).toEqual([
      "[a] 1",
      " b",
      "c",
    ]);
    expect(buildRows([], new Set())).toEqual([]);
    expect(describeRows(buildRows([ref("v1"), ref("v2")], new Set()))).toEqual(["v1", "v2"]);
  });

  it("lists every folder path, for open all and close all", () => {
    expect(folderPaths(refs).sort()).toEqual(["feature", "feature/api", "hotfix"]);
  });

  it("every ref of a folder is reachable for hiding it as a whole", () => {
    const rows = buildRows(refs, new Set(["feature"]));
    const feature = rows[0];
    expect(feature.kind === "folder" && feature.refs.map((r) => r.name).sort()).toEqual([
      "feature/api/v10",
      "feature/api/v2",
      "feature/auth-18",
      "feature/auth-6",
    ]);
  });
});
