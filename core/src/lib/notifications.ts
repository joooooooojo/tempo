import { invoke } from "@tauri-apps/api/core";

/** Show a system notification via the host (macOS uses UNUserNotificationCenter). */
export async function notifyUser(title: string, body: string) {
  try {
    await invoke("show_user_notification", { title, body });
  } catch (error) {
    console.error("Failed to send notification", error);
  }
}
