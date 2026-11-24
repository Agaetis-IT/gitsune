import type { RangeCommit } from "./api";
import type { HostedRepo } from "./remoteUrl";
import { splitTickets, ticketFromBranch, ticketPrefix } from "./tickets";

/**
 * A readable title from a branch name: the type (`feat/`) and the ticket number are dropped,
 * dashes become spaces. `feat/login-page/5310` gives `Login page`.
 */
export function humanizeBranch(branch: string): string {
  const ticket = ticketFromBranch(branch);
  let segments = branch.split("/").filter(Boolean);
  if (segments.length > 1) segments = segments.slice(1);
  let words = segments
    .join(" ")
    .replace(/[-_]+/g, " ")
    .split(" ")
    .filter((word) => word !== "" && word !== ticket)
    .join(" ");
  if (words === "") words = branch;
  return words.charAt(0).toUpperCase() + words.slice(1);
}

const hasTicket = (text: string): boolean => splitTickets(text).some((part) => part.ticket);

/**
 * The title: the message of the only commit, or the branch name made readable when there are
 * several. It starts with the ticket of the branch (`AB#5310`) unless it already names one.
 */
export function pullRequestTitle(
  branch: string,
  commits: readonly RangeCommit[],
  repo: HostedRepo,
): string {
  const base = commits.length === 1 ? commits[0].summary : humanizeBranch(branch);
  const ticket = ticketFromBranch(branch);
  return ticket && !hasTicket(base) ? `${ticketPrefix(repo)}${ticket} ${base}` : base;
}

/**
 * The description: the commits as a list, oldest first. On Azure DevOps it starts with the
 * ticket reference, which links the work item to the pull request.
 */
export function pullRequestDescription(
  branch: string,
  commits: readonly RangeCommit[],
  repo: HostedRepo,
): string {
  const ticket = ticketFromBranch(branch);
  const parts: string[] = [];
  if (ticket && repo.kind === "azure") parts.push(`${ticketPrefix(repo)}${ticket}`);
  if (commits.length > 1) parts.push(commits.map((commit) => `- ${commit.summary}`).join("\n"));
  return parts.join("\n\n");
}
