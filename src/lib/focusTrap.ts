const FOCUSABLE_SELECTOR = [
  "a[href]",
  "area[href]",
  "button:not([disabled])",
  "input:not([disabled]):not([type='hidden'])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  // Plugin hosts: keep in the cycle so Tab can move onto the frame, then into it.
  "iframe",
  "object",
  "embed",
  "[contenteditable]:not([contenteditable='false'])",
  '[tabindex]:not([tabindex="-1"])',
].join(",");

/** Portaled overlays manage their own Tab cycle — leave them alone. */
const PORTAL_FOCUS_ROOT_SELECTOR = [
  '[data-slot="alert-dialog-content"]',
  '[data-slot="dialog-panel"]',
  '[data-slot="dialog-content"]',
  '[data-slot="select-content"]',
  '[data-slot="popover-content"]',
  '[data-slot="dropdown-menu-content"]',
  '[role="alertdialog"]',
  '[role="dialog"]',
].join(",");

function isVisibleFocusable(element: HTMLElement): boolean {
  if (element.closest('[aria-hidden="true"], [hidden], [inert]')) return false;
  if (element.getAttribute("tabindex") === "-1") return false;
  const style = window.getComputedStyle(element);
  if (style.display === "none" || style.visibility === "hidden") return false;
  // Allow zero-size native inputs that still accept focus (rare); skip fully collapsed.
  const rects = element.getClientRects();
  return rects.length > 0;
}

export function listFocusableElements(root: HTMLElement): HTMLElement[] {
  return Array.from(root.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
    isVisibleFocusable,
  );
}

/**
 * Keep Tab / Shift+Tab cycling inside `root` so focus cannot leave the WebView
 * (which would fire window blur → main-panel hide).
 * Returns true when the event was handled.
 */
export function trapTabKey(
  event: KeyboardEvent,
  root: HTMLElement | null | undefined,
): boolean {
  if (event.key !== "Tab" || event.defaultPrevented || !root) return false;

  const active =
    document.activeElement instanceof HTMLElement ? document.activeElement : null;
  if (active?.closest(PORTAL_FOCUS_ROOT_SELECTOR)) return false;
  // Plugin UIs live in an iframe — let the browser move focus into the frame.
  // Key events inside the iframe never reach this parent listener, so Tab works there.
  if (active?.tagName === "IFRAME") return false;

  const focusables = listFocusableElements(root);
  if (focusables.length === 0) return false;

  const first = focusables[0];
  const last = focusables[focusables.length - 1];
  const index = active ? focusables.indexOf(active) : -1;
  const outside = !active || !root.contains(active);

  if (event.shiftKey) {
    if (index === 0 || outside) {
      event.preventDefault();
      last.focus({ preventScroll: true });
      return true;
    }
    return false;
  }

  if (index === focusables.length - 1 || outside) {
    event.preventDefault();
    first.focus({ preventScroll: true });
    return true;
  }

  return false;
}
