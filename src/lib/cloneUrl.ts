/**
 * The folder name git would pick for a repository address: the last part of the path without
 * `.git` (`https://host/team/tool.git` and `git@host:team/tool.git` both give `tool`).
 */
export function repoNameFromUrl(url: string): string {
  const text = url.trim().replace(/[/\\]+$/, "");
  const last = text.split(/[/:\\]/).pop() ?? "";
  return last.replace(/\.git$/i, "");
}
