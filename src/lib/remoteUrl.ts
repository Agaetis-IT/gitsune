/** A repository on a hosting service the app knows how to link to. */
export type HostedRepo =
  | { kind: "github"; owner: string; repo: string }
  | { kind: "azure"; org: string; project: string; repo: string };

/** The host a token belongs to (one token per host). */
export const tokenHost = (repo: HostedRepo): string =>
  repo.kind === "github" ? "github.com" : "dev.azure.com";

const decode = (value: string): string => {
  try {
    return decodeURIComponent(value);
  } catch {
    return value;
  }
};

const stripGit = (name: string): string => name.replace(/\.git$/i, "");

const clean = (parts: string[]): string[] => parts.map(decode).filter((p) => p.length > 0);

/**
 * Reads the URL of a remote, in any of the shapes git accepts (https, `git@host:path`,
 * `ssh://`). Returns `null` for a host the app does not know.
 */
export function parseRemoteUrl(url: string): HostedRepo | null {
  const text = url.trim();
  let host: string;
  let path: string;

  const scp = /^(?:[^@/\s]+@)?([^:/\s]+):(?!\/\/)(.+)$/.exec(text);
  if (scp) {
    host = scp[1].toLowerCase();
    path = scp[2];
  } else {
    let parsed: URL;
    try {
      parsed = new URL(text);
    } catch {
      return null;
    }
    host = parsed.hostname.toLowerCase();
    path = parsed.pathname;
  }
  const parts = clean(path.split("/"));

  if (host === "github.com" || host === "www.github.com") {
    if (parts.length !== 2) return null;
    return { kind: "github", owner: parts[0], repo: stripGit(parts[1]) };
  }

  // ssh.dev.azure.com:v3/<org>/<project>/<repo>
  if (host === "ssh.dev.azure.com" || host === "vs-ssh.visualstudio.com") {
    if (parts.length !== 4 || parts[0] !== "v3") return null;
    return { kind: "azure", org: parts[1], project: parts[2], repo: parts[3] };
  }

  // https://dev.azure.com/<org>/<project>/_git/<repo>
  if (host === "dev.azure.com") {
    const git = parts.indexOf("_git");
    if (parts.length !== 4 || git !== 2) return null;
    return { kind: "azure", org: parts[0], project: parts[1], repo: parts[3] };
  }

  // https://<org>.visualstudio.com/[DefaultCollection/]<project>/_git/<repo>
  if (host.endsWith(".visualstudio.com")) {
    const org = host.slice(0, -".visualstudio.com".length);
    const rest = parts[0]?.toLowerCase() === "defaultcollection" ? parts.slice(1) : parts;
    if (!org || rest.length !== 3 || rest[1] !== "_git") return null;
    return { kind: "azure", org, project: rest[0], repo: rest[2] };
  }
  return null;
}

const enc = encodeURIComponent;

/** The web page that starts a pull request from `branch` (the service picks the target). */
export function createPullRequestUrl(repo: HostedRepo, branch: string): string {
  if (repo.kind === "github") {
    const branchPath = branch.split("/").map(enc).join("/");
    return `https://github.com/${enc(repo.owner)}/${enc(repo.repo)}/compare/${branchPath}?expand=1`;
  }
  const base = `https://dev.azure.com/${enc(repo.org)}/${enc(repo.project)}/_git/${enc(repo.repo)}`;
  return `${base}/pullrequestcreate?sourceRef=${enc(branch)}`;
}

/** The address of a pull request page, from what the API answered. */
export function pullRequestUrl(repo: HostedRepo, id: number): string {
  if (repo.kind === "github") {
    return `https://github.com/${enc(repo.owner)}/${enc(repo.repo)}/pull/${id}`;
  }
  return `https://dev.azure.com/${enc(repo.org)}/${enc(repo.project)}/_git/${enc(repo.repo)}/pullrequest/${id}`;
}

/** The repository of the first remote with a known host, preferring `origin`. */
export function pickHostedRepo(
  remotes: ReadonlyArray<{ name: string; url: string }>,
): { remote: string; repo: HostedRepo } | null {
  const ordered = [...remotes].sort(
    (a, b) => Number(b.name === "origin") - Number(a.name === "origin"),
  );
  for (const remote of ordered) {
    const repo = parseRemoteUrl(remote.url);
    if (repo) return { remote: remote.name, repo };
  }
  return null;
}
