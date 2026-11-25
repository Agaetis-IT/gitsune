import { describe, expect, it } from "vitest";
import { GitCommandError, toGitError } from "./api";

describe("toGitError", () => {
  it("reads the object a command rejects with", () => {
    const error = toGitError({ kind: "hookFailed", message: "lint failed" });
    expect(error).toBeInstanceOf(GitCommandError);
    expect(error.kind).toBe("hookFailed");
    expect(error.message).toBe("lint failed");
  });

  it("falls back to `other` for strings, unknown shapes and a missing kind", () => {
    expect(toGitError("plain text").kind).toBe("other");
    expect(toGitError("plain text").message).toBe("plain text");
    expect(toGitError({ message: "no kind" }).kind).toBe("other");
    expect(toGitError(42).message).toBe("42");
  });

  it("keeps an error that already is one", () => {
    const original = new GitCommandError("locked", "index.lock exists");
    expect(toGitError(original)).toBe(original);
  });
});
