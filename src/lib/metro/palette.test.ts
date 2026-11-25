import { describe, expect, it } from "vitest";
import { contrastText, DARK_PALETTE, laneColor, LIGHT_PALETTE, PALETTE_SIZE } from "./palette";

describe("palette", () => {
  it("has one distinct color per index in both themes", () => {
    for (const palette of [LIGHT_PALETTE, DARK_PALETTE]) {
      expect(palette.lanes).toHaveLength(PALETTE_SIZE);
      expect(new Set(palette.lanes).size).toBe(PALETTE_SIZE);
      for (const color of [...palette.lanes, palette.anonymous]) {
        expect(color).toMatch(/^#[0-9a-f]{6}$/);
      }
    }
  });

  it("maps lane colors, neutral for anonymous lanes", () => {
    expect(laneColor(LIGHT_PALETTE, 3)).toBe(LIGHT_PALETTE.lanes[3]);
    expect(laneColor(LIGHT_PALETTE, null)).toBe(LIGHT_PALETTE.anonymous);
    expect(laneColor(LIGHT_PALETTE, PALETTE_SIZE + 1)).toBe(LIGHT_PALETTE.lanes[1]);
  });

  it("picks readable text on light and dark backgrounds", () => {
    expect(contrastText("#ffffff")).toBe("#1a1a1a");
    expect(contrastText("#f9a825")).toBe("#1a1a1a");
    expect(contrastText("#3949ab")).toBe("#ffffff");
    expect(contrastText("#000000")).toBe("#ffffff");
  });
});
