import "./style.css";
import { tempo } from "@tempo/sdk/ui";

await tempo.ready();

const status = document.querySelector<HTMLParagraphElement>("#status");
const button = document.querySelector<HTMLButtonElement>("#notify");

if (status) {
  status.textContent = `已连接：${tempo.context?.apiVersion ?? "unknown"}`;
}
button?.addEventListener("click", () => {
  void tempo.notify.show({
    title: "__PLUGIN_NAME__",
    body: "UI 已连接到 Tempo Host",
  });
});
