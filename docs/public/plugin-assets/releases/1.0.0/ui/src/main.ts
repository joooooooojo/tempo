import "./style.css";

await window.tempo.ready();

const status = document.querySelector<HTMLParagraphElement>("#status");
const button = document.querySelector<HTMLButtonElement>("#notify");

if (status) {
  status.textContent = `已连接：${window.tempo.context?.apiVersion ?? "unknown"}`;
}
button?.addEventListener("click", () => {
  void window.tempo.notify.show({
    title: "__PLUGIN_NAME__",
    body: "UI 已连接到 Tempo Host",
  });
});
