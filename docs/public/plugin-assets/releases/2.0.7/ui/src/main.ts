import "./style.css";
import { connect } from "@tempo/sdk/ui";

const app = await connect();

const status = document.querySelector<HTMLParagraphElement>("#status");
const button = document.querySelector<HTMLButtonElement>("#notify");

if (status) {
  status.textContent = `已连接：${app.context.apiVersion}`;
}
button?.addEventListener("click", () => {
  void app.notify.show({
    title: "__PLUGIN_NAME__",
    body: "UI 已连接到 Tempo Host",
  });
});
