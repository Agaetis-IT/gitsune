/**
 * Desktop notifications through the Tauri plugin. Outside the desktop app (the plain browser
 * used for development) every call quietly does nothing.
 */

/** Whether notifications are allowed; asks the system once when nobody has decided yet. */
export async function ensureNotificationPermission(): Promise<boolean> {
  try {
    const { isPermissionGranted, requestPermission } =
      await import("@tauri-apps/plugin-notification");
    if (await isPermissionGranted()) return true;
    return (await requestPermission()) === "granted";
  } catch {
    return false;
  }
}

/** Shows a notification when they are allowed (it never asks); returns whether it was sent. */
export async function sendDesktopNotification(title: string, body: string): Promise<boolean> {
  try {
    const { isPermissionGranted, sendNotification } =
      await import("@tauri-apps/plugin-notification");
    if (!(await isPermissionGranted())) return false;
    sendNotification({ title, body });
    return true;
  } catch {
    return false;
  }
}
