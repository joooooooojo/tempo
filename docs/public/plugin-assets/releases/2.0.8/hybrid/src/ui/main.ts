import "./style.css";
import { connect } from "tempo-plugin-sdk/ui";
import type { PluginIpc } from "../ipc.js";

const app = await connect<PluginIpc>();

const input = document.querySelector<HTMLInputElement>("#name");
const button = document.querySelector<HTMLButtonElement>("#greet");
const result = document.querySelector<HTMLElement>("#result");

button?.addEventListener("click", async () => {
  const response = await app.ipc.invoke("greet", {
    name: input?.value || "Tempo",
  });
  if (result) result.textContent = JSON.stringify(response, null, 2);
});

app.ipc.on("greeted", (_event, payload) => {
  if (result) result.textContent = JSON.stringify(payload, null, 2);
});
