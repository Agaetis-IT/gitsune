import { ref } from "vue";
import { defaultRemoteIcon, type IconName, isIconName } from "@/lib/metro/icons";
import type { Density } from "@/lib/metro/metrics";

const INITIALS_KEY = "git-hydra:graph-initials";
const DENSITY_KEY = "git-hydra:graph-density";
const COLUMNS_KEY = "git-hydra:graph-columns";
const REFS_WIDTH_KEY = "git-hydra:graph-refs-width";
const GRAPH_WIDTH_KEY = "git-hydra:graph-width";
const REMOTE_ICONS_KEY = "git-hydra:remote-icons";

/** Width limits of the column of branch labels, in pixels. */
export const REFS_WIDTH = { min: 120, max: 480, initial: 220 } as const;
/** Smallest width the user can give the lane area. */
export const GRAPH_WIDTH_MIN = 60;

/** The columns shown to the right of the commit message. */
export interface GraphColumns {
  author: boolean;
  date: boolean;
  sha: boolean;
}

const DEFAULT_COLUMNS: GraphColumns = { author: true, date: true, sha: false };

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null; // Storage unavailable: the defaults.
  }
}

function write(key: string, value: string | null): void {
  try {
    if (value === null) localStorage.removeItem(key);
    else localStorage.setItem(key, value);
  } catch {
    // Not kept for the next session.
  }
}

function readDensity(): Density {
  return read(DENSITY_KEY) === "detailed" ? "detailed" : "compact";
}

function readColumns(): GraphColumns {
  try {
    const parsed = JSON.parse(read(COLUMNS_KEY) ?? "null") as Partial<GraphColumns> | null;
    return {
      author: typeof parsed?.author === "boolean" ? parsed.author : DEFAULT_COLUMNS.author,
      date: typeof parsed?.date === "boolean" ? parsed.date : DEFAULT_COLUMNS.date,
      sha: typeof parsed?.sha === "boolean" ? parsed.sha : DEFAULT_COLUMNS.sha,
    };
  } catch {
    return { ...DEFAULT_COLUMNS };
  }
}

export const clampRefsWidth = (value: number): number =>
  Math.round(Math.min(REFS_WIDTH.max, Math.max(REFS_WIDTH.min, value)));

function readRefsWidth(): number {
  const value = Number(read(REFS_WIDTH_KEY));
  return Number.isFinite(value) && value > 0 ? clampRefsWidth(value) : REFS_WIDTH.initial;
}

function readGraphWidth(): number | null {
  const value = Number(read(GRAPH_WIDTH_KEY));
  return Number.isFinite(value) && value >= GRAPH_WIDTH_MIN ? Math.round(value) : null;
}

function readRemoteIcons(): Record<string, IconName> {
  try {
    const parsed = JSON.parse(read(REMOTE_ICONS_KEY) ?? "{}") as Record<string, unknown>;
    const icons: Record<string, IconName> = {};
    for (const [remote, icon] of Object.entries(parsed)) {
      if (isIconName(icon)) icons[remote] = icon;
    }
    return icons;
  } catch {
    return {};
  }
}

/** How tall the rows of the graph are: compact, or detailed (author and date under each line). */
const density = ref<Density>(readDensity());

/** Show the initials of the author in the stations of the graph (on unless switched off). */
const showInitials = ref(read(INITIALS_KEY) !== "off");

const columns = ref<GraphColumns>(readColumns());

/** Width of the column of branch labels. */
const refsWidth = ref(readRefsWidth());

/** Width given to the lane area by the user; `null` to fit it to the graph. */
const graphWidth = ref<number | null>(readGraphWidth());

/** The icon chosen for each remote (by name); the others get their default icon. */
const remoteIcons = ref<Record<string, IconName>>(readRemoteIcons());

export function useGraphSettings() {
  function setDensity(value: Density) {
    density.value = value;
    write(DENSITY_KEY, value);
  }
  function setShowInitials(value: boolean) {
    showInitials.value = value;
    write(INITIALS_KEY, value ? "on" : "off");
  }
  function setColumn(column: keyof GraphColumns, value: boolean) {
    columns.value = { ...columns.value, [column]: value };
    write(COLUMNS_KEY, JSON.stringify(columns.value));
  }
  function setRefsWidth(value: number) {
    refsWidth.value = clampRefsWidth(value);
    write(REFS_WIDTH_KEY, String(refsWidth.value));
  }
  function setGraphWidth(value: number | null) {
    graphWidth.value = value === null ? null : Math.max(GRAPH_WIDTH_MIN, Math.round(value));
    write(GRAPH_WIDTH_KEY, graphWidth.value === null ? null : String(graphWidth.value));
  }
  /** `null` goes back to the default icon of the remote. */
  function setRemoteIcon(remote: string, icon: IconName | null) {
    const next = { ...remoteIcons.value };
    if (icon === null) delete next[remote];
    else next[remote] = icon;
    remoteIcons.value = next;
    write(REMOTE_ICONS_KEY, JSON.stringify(next));
  }
  const remoteIcon = (remote: string): IconName =>
    remoteIcons.value[remote] ?? defaultRemoteIcon(remote);

  return {
    showInitials,
    setShowInitials,
    density,
    setDensity,
    columns,
    setColumn,
    refsWidth,
    setRefsWidth,
    graphWidth,
    setGraphWidth,
    remoteIcons,
    setRemoteIcon,
    remoteIcon,
  };
}
