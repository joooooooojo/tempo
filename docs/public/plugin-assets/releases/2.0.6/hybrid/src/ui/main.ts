import "./style.css";
import { ipcRenderer, tempo } from "@tempo/sdk/ui";

await tempo.ready();

const input = document.querySelector<HTMLInputElement>("#name");
const button = document.querySelector<HTMLButtonElement>("#greet");
const result = document.querySelector<HTMLElement>("#result");

button?.addEventListener("click", async () => {
  const response = await ipcRenderer.invoke("greet", {
    name: input?.value || "Tempo",
  });
  if (result) result.textContent = JSON.stringify(response, null, 2);
});

ipcRenderer.on("greeted", (_event, payload) => {
  if (result) result.textContent = JSON.stringify(payload, null, 2);
});
