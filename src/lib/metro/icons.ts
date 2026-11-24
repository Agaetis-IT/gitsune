import { ICON_PATHS, type IconName } from "./iconPaths";

export type { IconName };

/** The icons a remote can be given, with their names in the settings. */
export const REMOTE_ICON_CHOICES: ReadonlyArray<{ name: IconName; label: string }> = [
  { name: "users", label: "People" },
  { name: "git-fork", label: "Fork" },
  { name: "server", label: "Server" },
  { name: "cloud", label: "Cloud" },
  { name: "globe", label: "Globe" },
  { name: "database", label: "Database" },
  { name: "building", label: "Company" },
  { name: "rocket", label: "Rocket" },
  { name: "star", label: "Star" },
  { name: "box", label: "Box" },
  { name: "cpu", label: "Chip" },
  { name: "hard-drive", label: "Disk" },
  { name: "house", label: "Home" },
];

/** The icon of a remote nobody configured: `origin` and `upstream` are told apart at a glance. */
export function defaultRemoteIcon(remote: string): IconName {
  if (remote === "origin") return "users";
  if (remote === "upstream") return "git-fork";
  return "server";
}

export function isIconName(value: unknown): value is IconName {
  return typeof value === "string" && Object.prototype.hasOwnProperty.call(ICON_PATHS, value);
}

const paths = new Map<IconName, Path2D>();

/** Draws an icon of the set, centered on (`x`, `y`), `size` pixels wide. */
export function drawIcon(
  ctx: CanvasRenderingContext2D,
  name: IconName,
  x: number,
  y: number,
  size: number,
  color: string,
): void {
  let path = paths.get(name);
  if (!path) {
    path = new Path2D(ICON_PATHS[name]);
    paths.set(name, path);
  }
  const scale = size / 24;
  ctx.save();
  ctx.translate(x - size / 2, y - size / 2);
  ctx.scale(scale, scale);
  ctx.strokeStyle = color;
  ctx.lineWidth = 2;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  ctx.stroke(path);
  ctx.restore();
}
