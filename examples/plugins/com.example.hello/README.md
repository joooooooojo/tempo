# Hello 示例插件（com.example.hello）

混合示例：Action 通过 `hello` Command 调用 Runtime，MCP Tool 通过 `tempo.mcpTools.register("say-hello", ...)` 独立注册，页面使用 `ipcRenderer` / `ipcMain` 与 Runtime 私下通信。

需要 Host API `^1.0.0`。

## 手动验证 SCA

1. 导入并信任、启用本插件（含 `main` 时需已安装插件 Node Runtime）。
2. 打开「Hello 示例插件」面板。
3. 点 **打招呼（Runtime）**  
   - `invoke greet` 返回值里的 `at` 应为 `Date`  
   - `on greeted` 里的 `at` 应为 `Date`
4. 点 **测试 SCA**  
   - UI → Runtime：`ipcRenderer.invoke("sca-probe")` 携带 Date / Map / Set / Uint8Array / 循环引用
   - Runtime → UI：`ipcMain.send("sca-echo")` 同结构
   - UI → Runtime：`ipcRenderer.send("sca-ping")` → Runtime `ipcMain.on` → `ipcMain.send("sca-pong")`
   - 日志应出现 `✓ invoke/handle SCA 通过`、`✓ sca-echo SCA 通过`、`✓ sca-pong SCA 通过`

```js
// main.mjs
ipcMain.handle("greet", ...);
ipcMain.handle("sca-probe", ...);
ipcMain.on("sca-ping", ...);
tempo.commands.register("hello", ...); // 仅供 Action 调用
tempo.mcpTools.register("say-hello", ...); // 仅供 MCP 调用

// index.js
await window.ipcRenderer.invoke("greet", { who: "Tempo" });
await window.ipcRenderer.invoke("sca-probe", fixture);
window.ipcRenderer.send("sca-ping", fixture);
window.ipcRenderer.on("sca-echo" | "sca-pong" | "greeted", ...);
```
