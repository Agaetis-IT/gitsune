import type { BranchPr } from "./api";
import type { IconName } from "./metro/icons";

/** What a branch shows for its pull request: one icon, in one color, with a sentence. */
export interface PrBadge {
  icon: IconName;
  color: string;
  title: string;
  id: number;
}

const RED = "#e5484d";
const AMBER = "#d9930d";
const GREEN = "#2f9e5b";
const BLUE = "#3b82f6";
const GRAY = "#8b8d98";

/**
 * The badge of a pull request. A failing or running build comes first, then a review asking for
 * changes, then success or approval, then the plain state (draft or open). A `stale` badge
 * (the last refresh failed) is grey.
 */
export function prBadge(pr: BranchPr, stale = false): PrBadge {
  let icon: IconName = "git-pull-request";
  let color = pr.draft ? GRAY : BLUE;
  if (pr.checks === "failure") {
    icon = "x";
    color = RED;
  } else if (pr.review === "changesRequested") {
    icon = "x";
    color = AMBER;
  } else if (pr.checks === "pending") {
    icon = "clock";
    color = AMBER;
  } else if (pr.checks === "success" || pr.review === "approved") {
    icon = "check";
    color = GREEN;
  }
  const facts = [
    pr.draft ? "draft" : "open",
    ...(pr.review === "approved" ? ["approved"] : []),
    ...(pr.review === "changesRequested" ? ["changes requested"] : []),
    ...(pr.review === "pending" ? ["review required"] : []),
    ...(pr.checks === "failure" ? ["build failing"] : []),
    ...(pr.checks === "pending" ? ["build running"] : []),
    ...(pr.checks === "success" ? ["build passing"] : []),
  ];
  const note = stale ? " (could not be refreshed)" : "";
  return {
    icon,
    color: stale ? GRAY : color,
    title: `Pull request ${pr.id}: ${pr.title} · ${facts.join(", ")}${note}`,
    id: pr.id,
  };
}

/** The badges of a list of pull requests, by branch name. */
export function prBadges(items: readonly BranchPr[], stale = false): Map<string, PrBadge> {
  return new Map(items.map((pr) => [pr.branch, prBadge(pr, stale)]));
}
