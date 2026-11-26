import { beforeEach, describe, expect, it, vi } from "vitest";
import type { UpdateCheck } from "@/lib/api";

const DAY = 24 * 60 * 60 * 1000;

const release = (available: boolean): UpdateCheck => ({
  current: "0.1.0",
  latest: available ? "0.2.0" : "0.1.0",
  available,
  url: "https://github.com/Agaetis-IT/gitsune/releases/tag/v0.2.0",
  notes: "Invented notes.",
});

/** A fresh copy of the composable (its state is shared by the module) and a fake backend. */
async function load(answer: () => Promise<UpdateCheck>) {
  vi.resetModules();
  const check = vi.fn(answer);
  vi.doMock("@/lib/api", () => ({ checkForUpdates: check }));
  const { useUpdates } = await import("./useUpdates");
  return { updates: useUpdates(), check };
}

beforeEach(() => localStorage.clear());

describe("the automatic check", () => {
  it("does nothing when it is switched off", async () => {
    const { updates, check } = await load(async () => release(true));
    await updates.checkIfDue(false);
    expect(check).not.toHaveBeenCalled();
    expect(updates.open.value).toBe(false);
  });

  it("opens the window only when a newer version exists", async () => {
    const newer = await load(async () => release(true));
    await newer.updates.checkIfDue(true);
    expect(newer.updates.open.value).toBe(true);
    expect(newer.updates.result.value?.latest).toBe("0.2.0");

    localStorage.clear();
    const same = await load(async () => release(false));
    await same.updates.checkIfDue(true);
    expect(same.check).toHaveBeenCalledOnce();
    expect(same.updates.open.value).toBe(false);
  });

  it("asks once a day, not at every start", async () => {
    const { updates, check } = await load(async () => release(false));
    const now = Date.now();
    await updates.checkIfDue(true, now);
    await updates.checkIfDue(true, now + DAY - 1000);
    expect(check).toHaveBeenCalledTimes(1);
    await updates.checkIfDue(true, now + DAY + 1000);
    expect(check).toHaveBeenCalledTimes(2);
  });

  it("stays silent on a failure and tries again at the next start", async () => {
    const failing = await load(async () => {
      throw new Error("Could not reach GitHub");
    });
    await failing.updates.checkIfDue(true);
    expect(failing.updates.open.value).toBe(false);

    const working = await load(async () => release(true));
    await working.updates.checkIfDue(true);
    expect(working.check).toHaveBeenCalledOnce();
  });
});

describe("the check asked by the user", () => {
  it("opens the window and says the failure", async () => {
    const { updates } = await load(async () => {
      throw new Error("Could not reach GitHub");
    });
    await updates.checkNow();
    expect(updates.open.value).toBe(true);
    expect(updates.error.value).toBe("Could not reach GitHub");
    expect(updates.result.value).toBeNull();
  });

  it("says when the version is the latest one", async () => {
    const { updates } = await load(async () => release(false));
    await updates.checkNow();
    expect(updates.open.value).toBe(true);
    expect(updates.result.value?.available).toBe(false);
    expect(updates.error.value).toBe("");
  });
});
