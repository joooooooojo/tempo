/** Fade out the in-page boot splash. Does not show the main window. */
export function dismissBootSplash() {
  const splash = document.getElementById("boot-splash");
  if (!splash || splash.classList.contains("is-leaving")) return;
  splash.classList.add("is-leaving");
  window.setTimeout(() => splash.remove(), 240);
}

/** @deprecated Use dismissBootSplash — main window is no longer revealed on boot. */
export async function revealAppShell() {
  dismissBootSplash();
}
