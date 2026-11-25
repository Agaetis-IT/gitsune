import type { HostedRepo } from "./remoteUrl";

const enc = encodeURIComponent;

/**
 * The ticket number a branch is named after: the last segment of `type/title/5310`, in the
 * forms `5310`, `5310-title` and `title-5310`. Null when there is none.
 */
export function ticketFromBranch(branch: string): string | null {
  const last = branch.split("/").filter(Boolean).pop() ?? "";
  const match = /^(\d{2,})(?:[-_].*)?$/.exec(last) ?? /[-_](\d{2,})$/.exec(last);
  return match ? match[1] : null;
}

/** What goes before the number in a commit message: `AB#` links the commit on Azure Boards. */
export const ticketPrefix = (repo: HostedRepo): string => (repo.kind === "azure" ? "AB#" : "#");

/** The page of a ticket on the hosting service of the repository. */
export function ticketUrl(repo: HostedRepo, id: string): string {
  if (repo.kind === "azure") {
    return `https://dev.azure.com/${enc(repo.org)}/${enc(repo.project)}/_workitems/edit/${enc(id)}`;
  }
  if (repo.kind === "github") {
    return `https://github.com/${enc(repo.owner)}/${enc(repo.repo)}/issues/${enc(id)}`;
  }
  return `https://${repo.host}/${repo.path.split("/").map(enc).join("/")}/-/issues/${enc(id)}`;
}

export interface TextPart {
  text: string;
  /** The ticket number when this part is a reference such as `AB#5310` or `#5310`. */
  ticket?: string;
}

/** Cuts a text into plain parts and ticket references (`AB#5310`, `#5310`). */
export function splitTickets(text: string): TextPart[] {
  const parts: TextPart[] = [];
  const pattern = /(?<![\w&])(?:AB)?#(\d+)\b/g;
  let last = 0;
  for (const match of text.matchAll(pattern)) {
    if (match.index > last) parts.push({ text: text.slice(last, match.index) });
    parts.push({ text: match[0], ticket: match[1] });
    last = match.index + match[0].length;
  }
  if (last < text.length) parts.push({ text: text.slice(last) });
  return parts;
}
