import { describe, expect, it } from "vitest";
import { escapeHtml, highlighterFor, languageForPath } from "./highlight";

describe("languageForPath", () => {
  it("recognises common extensions, in any case and in nested paths", () => {
    expect(languageForPath("src/app/main.ts")).toBe("typescript");
    expect(languageForPath("App.VUE")).toBe("xml");
    expect(languageForPath("Cargo.toml")).toBe("ini");
    expect(languageForPath("a/b/c.rs")).toBe("rust");
  });

  it("returns null for unknown files, dotfiles and files without extension", () => {
    expect(languageForPath("LICENSE")).toBeNull();
    expect(languageForPath(".gitignore")).toBeNull();
    expect(languageForPath("data.unknownext")).toBeNull();
  });
});

describe("highlighterFor", () => {
  it("wraps tokens in spans and escapes the code", async () => {
    const highlight = await highlighterFor("a.ts");
    expect(highlight).not.toBeNull();
    const html = highlight?.('const x = "<b>";') ?? "";
    expect(html).toContain('<span class="hljs-keyword">const</span>');
    expect(html).toContain("&lt;b&gt;");
    expect(html).not.toContain("<b>");
  });

  it("returns null when the language is not known", async () => {
    expect(await highlighterFor("README")).toBeNull();
  });

  it("escapes plain text", () => {
    expect(escapeHtml("a < b && c > d")).toBe("a &lt; b &amp;&amp; c &gt; d");
  });
});
