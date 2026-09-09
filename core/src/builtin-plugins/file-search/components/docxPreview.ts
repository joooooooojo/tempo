import { renderAsync } from "docx-preview";

/** Render a .docx ArrayBuffer into an HTML container with preserved styles. */
export async function renderDocxPreview(
  buf: ArrayBuffer,
  container: HTMLElement,
  styleContainer?: HTMLElement | null,
): Promise<void> {
  container.replaceChildren();
  await renderAsync(buf, container, styleContainer ?? container, {
    className: "docx",
    inWrapper: true,
    ignoreWidth: true,
    ignoreHeight: true,
    breakPages: false,
    renderHeaders: true,
    renderFooters: true,
    useBase64URL: true,
  });
}
