/** Must match `PALETTE_SIZE` in `src-tauri/src/layout/naming.rs`. */
export const PALETTE_SIZE = 12;

export interface Palette {
  lanes: readonly string[];
  /** Color of anonymous lanes. */
  anonymous: string;
}

export const LIGHT_PALETTE: Palette = {
  lanes: [
    "#e53935", // red
    "#fb8c00", // orange
    "#f9a825", // amber
    "#43a047", // green
    "#00897b", // teal
    "#00acc1", // cyan
    "#1e88e5", // blue
    "#3949ab", // indigo
    "#8e24aa", // purple
    "#d81b60", // pink
    "#6d4c41", // brown
    "#7cb342", // lime
  ],
  anonymous: "#9e9e9e",
};

export const DARK_PALETTE: Palette = {
  lanes: [
    "#ff6b68",
    "#ffa63d",
    "#ffd24d",
    "#66d06a",
    "#3cc7b4",
    "#3fd0e0",
    "#5aa9ff",
    "#8c9eff",
    "#c178e0",
    "#ff6fa5",
    "#b08968",
    "#aedb62",
  ],
  anonymous: "#8a8a8a",
};

/** Color of a lane given its palette index (`null` for anonymous lanes). */
export function laneColor(palette: Palette, color: number | null): string {
  if (color === null) return palette.anonymous;
  return palette.lanes[color % palette.lanes.length];
}

/** Black or white, whichever reads better on the given `#rrggbb` background. */
export function contrastText(hex: string): string {
  const value = Number.parseInt(hex.slice(1), 16);
  const [r, g, b] = [(value >> 16) & 255, (value >> 8) & 255, value & 255];
  // Perceived luminance (ITU-R BT.601).
  return (0.299 * r + 0.587 * g + 0.114 * b) / 255 > 0.62 ? "#1a1a1a" : "#ffffff";
}
