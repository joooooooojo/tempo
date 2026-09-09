# Hello 示例插件（com.example.hello）

混合示例：Action 通过 `hello` Command 调用 Runtime，MCP Tool 通过 `tempo.mcpTools.register("say-hello", ...)` 独立注册，页面使用 `ipcRenderer` / `ipcMain` 与 Runtime 私下通信。

需要 Tempo >=2.2.6、Host API `^2.0.0`、Manifest v2 和 Deno 2.9.6。

该目录已是可直接导入的插件包，无需 npm install。`main.mjs` 使用 Deno 的 Node 兼容层执行 `node:fs/promises` 和 `node:path`。Manifest 仅授予 `$DATA` 写入（追加 `hello.log`）和系统通知；设置通过 Host storage 读取，无需文件读取权限。不申请网络、环境变量或子进程权限。

从旧版升级时保持插件 ID，导入 2.0.1 并确认新权限即可沿用数据目录。2.0.1 补齐日志写权限，不能用它覆盖同版本的不可变安装包。

## 手动验证 SCA

1. 在设置中安装插件 Deno Runtime，再导入、信任并启用本插件。
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
