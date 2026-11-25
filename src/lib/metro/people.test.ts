import { describe, expect, it } from "vitest";
import { contrastRatio } from "@/lib/theme/types";
import { initialsOf, isDarkBackground, personColor, personKey } from "./people";

describe("initials", () => {
  it("takes the first letter of the first and the last word", () => {
    expect(initialsOf("Ghassan Jabbour")).toBe("GJ");
    expect(initialsOf("Simon Dujardin")).toBe("SD");
    expect(initialsOf("Jean-Pierre de la Fontaine")).toBe("JF");
    expect(initialsOf("  Ada   Lovelace  ")).toBe("AL");
  });

  it("uses the first two letters of a single word", () => {
    expect(initialsOf("AlexandreFIALE")).toBe("AL");
    expect(initialsOf("x")).toBe("X");
  });

  it("reads an e-mail address by what is before the @, and drops accents", () => {
    expect(initialsOf("ada.lovelace@example.invalid")).toBe("AL");
    expect(initialsOf("Étienne Ørsted")).toBe("ER");
  });

  it("gives a question mark when there is nothing to use", () => {
    expect(initialsOf("")).toBe("?");
    expect(initialsOf("   ")).toBe("?");
    expect(initialsOf("--")).toBe("?");
  });
});

describe("a color per person", () => {
  it("is the same for the same person, whatever the case, the accents and the spaces", () => {
    expect(personKey("Simon  DUJARDIN")).toBe(personKey("simon dujardin"));
    expect(personKey("Zoë")).toBe("zoe");
    expect(personColor("Simon DUJARDIN", true)).toEqual(personColor("Simon Dujardin", true));
  });

  it("differs between people, and between a light and a dark page", () => {
    const names = [
      "Alice Fakename",
      "Bob Fakename",
      "Carol Fakename",
      "Dan Testuser",
      "Eve Sample",
    ];
    const fills = names.map((name) => personColor(name, true).fill);
    expect(new Set(fills).size).toBe(names.length);
    expect(personColor("Alice Fakename", true).fill).not.toBe(
      personColor("Alice Fakename", false).fill,
    );
  });

  it("keeps the initials readable on the fill, for many people on both pages", () => {
    for (let i = 0; i < 200; i++) {
      for (const dark of [true, false]) {
        const { fill, text } = personColor(`Person ${i} Name${i * 7}`, dark);
        expect(contrastRatio(fill, text), `${i} ${dark}`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it("produces #rrggbb colors", () => {
    expect(personColor("Alice Fakename", true).fill).toMatch(/^#[0-9a-f]{6}$/);
  });
});

describe("dark or light page", () => {
  it("tells from the background color, and treats anything unknown as light", () => {
    expect(isDarkBackground("#0a0a0a")).toBe(true);
    expect(isDarkBackground("#2e3440")).toBe(true);
    expect(isDarkBackground("#ffffff")).toBe(false);
    expect(isDarkBackground("oklch(1 0 0)")).toBe(false);
    expect(isDarkBackground("")).toBe(false);
  });
});
