import { invoke } from "@tauri-apps/api/core";

let revealed = false;

/** Show the main window; Rust closes the splashscreen window. */
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
