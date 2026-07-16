import { invoke } from "@tauri-apps/api/core";

let revealed = false;

/** Fade out the in-page boot splash once settings are ready (window already shown after first paint). */
export async function revealAppShell() {
  if (revealed) return;
  revealed = true;

  const splash = document.getElementById("boot-splash");
  if (splash) {
    splash.classList.add("is-leaving");
    window.setTimeout(() => splash.remove(), 240);
  }

  try {
    await invoke("show_window");
  } catch (error) {
    console.error("Failed to reveal main window", error);
  }
}
