import { contrastRatio } from "@/lib/theme/types";
import { contrastText } from "./palette";

/** Lowercase, without accents or extra spaces: "Simon DUJARDIN" and "Simon Dujardin" match. */
export function personKey(name: string): string {
  return name.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase().replace(/\s+/g, " ").trim();
}

/**
 * Two letters for a name: the first letters of the first and the last words ("Ghassan Jabbour"
 * gives GJ), or the first two letters of a single word. An e-mail address counts by what comes
 * before the @. `?` when there is nothing to use.
 */
export function initialsOf(name: string): string {
  const base = name.includes("@") ? name.slice(0, name.indexOf("@")) : name;
  const words = base
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .split(/[^A-Za-z0-9]+/)
    .filter((word) => word !== "");
  if (words.length === 0) return "?";
  if (words.length === 1) return words[0].slice(0, 2).toUpperCase();
  return (words[0][0] + words[words.length - 1][0]).toUpperCase();
}

/** A stable number for a text (FNV-1a), so a person keeps the same color in every session. */
function hash(text: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

function hslToHex(h: number, s: number, l: number): string {
  const a = s * Math.min(l, 1 - l);
  const channel = (n: number) => {
    const k = (n + h / 30) % 12;
    const value = l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1));
    return Math.round(value * 255)
      .toString(16)
      .padStart(2, "0");
  };
  return `#${channel(0)}${channel(8)}${channel(4)}`;
}

export interface PersonColor {
  /** Fill of the station. */
  fill: string;
  /** Color of the initials on that fill. */
  text: string;
}

const cache = new Map<string, PersonColor>();

/**
 * The color of a person: the same name (whatever the case or the accents) always gives the same
 * hue, lighter on a dark background and deeper on a light one so the initials stay readable.
 */
export function personColor(name: string, dark: boolean): PersonColor {
  const key = `${dark ? "d" : "l"}:${personKey(name)}`;
  let color = cache.get(key);
  if (!color) {
    const hue = hash(personKey(name)) % 360;
    // Start from a lightness that suits the page, then move it until the initials reach a
    // contrast of 4.5 (some hues, yellow and green, are bright even when "deep").
    let lightness = dark ? 0.64 : 0.4;
    let fill = hslToHex(hue, dark ? 0.62 : 0.58, lightness);
    for (let step = 0; step < 14; step++) {
      if (contrastRatio(fill, contrastText(fill)) >= 4.5) break;
      lightness += dark ? 0.02 : -0.02;
      fill = hslToHex(hue, dark ? 0.62 : 0.58, lightness);
    }
    color = { fill, text: contrastText(fill) };
    cache.set(key, color);
  }
  return color;
}

/** Whether a page background (`#rrggbb`) is dark; anything else counts as light. */
export function isDarkBackground(color: string): boolean {
  if (!/^#[0-9a-f]{6}/i.test(color)) return false;
  const value = Number.parseInt(color.slice(1, 7), 16);
  const [r, g, b] = [(value >> 16) & 255, (value >> 8) & 255, value & 255];
  return (0.299 * r + 0.587 * g + 0.114 * b) / 255 < 0.5;
}
