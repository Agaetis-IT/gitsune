import { ref } from "vue";
import { checkForUpdates, type UpdateCheck } from "@/lib/api";
import { updateCheckDue } from "@/lib/updates";

const CHECKED_KEY = "gitsune:update-checked-at";

function readCheckedAt(): number {
  try {
    const value = Number(localStorage.getItem(CHECKED_KEY));
    return Number.isFinite(value) ? value : 0;
  } catch {
    return 0; // Storage unavailable: checked at each start.
  }
}

function writeCheckedAt(time: number): void {
  try {
    localStorage.setItem(CHECKED_KEY, String(time));
  } catch {
    // Not kept for the next session.
  }
}

/** Whether the update window is open (one window for the whole app). */
const open = ref(false);
const checking = ref(false);
const result = ref<UpdateCheck | null>(null);
const error = ref("");

/**
 * Asks for the latest release. Only a successful answer counts as a check: after a failure
 * (no network) the next start tries again.
 */
async function run(): Promise<UpdateCheck | null> {
  if (checking.value) return null;
  checking.value = true;
  error.value = "";
  try {
    const answer = await checkForUpdates();
    result.value = answer;
    writeCheckedAt(Date.now());
    return answer;
  } catch (e) {
    result.value = null;
    error.value = e instanceof Error ? e.message : String(e);
    return null;
  } finally {
    checking.value = false;
  }
}

export function useUpdates() {
  return {
    open,
    checking,
    result,
    error,
    close: () => (open.value = false),
    /** The user asked: the window opens at once and says the answer, whatever it is. */
    async checkNow() {
      open.value = true;
      await run();
    },
    /**
     * The automatic check: at most once a day, and only when `enabled`. It says nothing unless
     * a newer version exists, and nothing at all when it fails.
     */
    async checkIfDue(enabled: boolean, now: number = Date.now()) {
      if (!enabled || !updateCheckDue(readCheckedAt(), now)) return;
      const answer = await run();
      if (answer?.available) open.value = true;
    },
  };
}
