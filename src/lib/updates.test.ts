import { describe, expect, it } from "vitest";
import { UPDATE_CHECK_INTERVAL, updateCheckDue } from "./updates";

describe("updateCheckDue", () => {
  const now = 1_700_000_000_000;

  it("is due when there was no check yet", () => {
    expect(updateCheckDue(0, now)).toBe(true);
  });

  it("is not due before a day has passed", () => {
    expect(updateCheckDue(now - 1000, now)).toBe(false);
    expect(updateCheckDue(now - UPDATE_CHECK_INTERVAL + 1, now)).toBe(false);
  });

  it("is due a day after the last check", () => {
    expect(updateCheckDue(now - UPDATE_CHECK_INTERVAL, now)).toBe(true);
    expect(updateCheckDue(now - 3 * UPDATE_CHECK_INTERVAL, now)).toBe(true);
  });

  it("is due when the clock was set back", () => {
    expect(updateCheckDue(now + 60_000, now)).toBe(true);
  });
});
