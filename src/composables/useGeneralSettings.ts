import { ref } from "vue";
import { isAutoFetchChoice } from "@/lib/autoFetch";

const RESTORE_KEY = "gitsune:restore-tabs";
const TICKET_KEY = "gitsune:ticket-prefill";
const FETCH_KEY = "gitsune:auto-fetch";
const UPDATE_KEY = "gitsune:update-check";

function readFlag(key: string): boolean {
  try {
    return localStorage.getItem(key) !== "off";
  } catch {
    return true; // Storage unavailable: the default.
  }
}

function writeFlag(key: string, value: boolean): void {
  try {
    localStorage.setItem(key, value ? "on" : "off");
  } catch {
    // Not kept for the next session.
  }
}

/** Reopen the tabs of the last session at start (on unless switched off). */
const restoreTabs = ref(readFlag(RESTORE_KEY));
function readMinutes(): number {
  try {
    const value = Number(localStorage.getItem(FETCH_KEY));
    return isAutoFetchChoice(value) ? value : 0;
  } catch {
    return 0;
  }
}

/** Minutes between the automatic fetches; 0 (the default) switches them off. */
const autoFetchMinutes = ref(readMinutes());
/** An empty commit box starts with the ticket of the branch (on unless switched off). */
const ticketPrefill = ref(readFlag(TICKET_KEY));
/** Look for a new version of the application once a day (on unless switched off). */
const updateCheck = ref(readFlag(UPDATE_KEY));

export function useGeneralSettings() {
  function setRestoreTabs(value: boolean) {
    restoreTabs.value = value;
    writeFlag(RESTORE_KEY, value);
  }
  function setTicketPrefill(value: boolean) {
    ticketPrefill.value = value;
    writeFlag(TICKET_KEY, value);
  }
  function setUpdateCheck(value: boolean) {
    updateCheck.value = value;
    writeFlag(UPDATE_KEY, value);
  }
  function setAutoFetchMinutes(value: number) {
    if (!isAutoFetchChoice(value)) return;
    autoFetchMinutes.value = value;
    try {
      localStorage.setItem(FETCH_KEY, String(value));
    } catch {
      // Not kept for the next session.
    }
  }
  return {
    restoreTabs,
    setRestoreTabs,
    ticketPrefill,
    setTicketPrefill,
    autoFetchMinutes,
    setAutoFetchMinutes,
    updateCheck,
    setUpdateCheck,
  };
}
