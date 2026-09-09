# Hello 示例插件（com.example.hello）

混合示例：Action 通过 `hello` Command 调用 Runtime，MCP Tool 通过 `tempo.mcpTools.register("say-hello", ...)` 独立注册，页面使用 `ipcRenderer` / `ipcMain` 与 Runtime 私下通信，并可直接检查插件权限。

需要 Tempo >=2.2.6、Host API `^2.1.0`、Manifest v2 和 Deno 2.9.6。

该目录已是可直接导入的插件包，无需 npm install。Manifest 使用空的 `permissions: {}`，Deno 的数据目录读写、网络、环境变量、系统信息、子进程、FFI 和远程导入默认关闭。问候日志通过 `tempo.files` 写入 `hello.log`，因此不需要 Deno 文件权限；Host API 也不需要逐项授权。

示例图标位于 `icons/app.svg`。插件图标也可使用 PNG、JPEG（`.jpg` / `.jpeg`）、WebP 或 GIF，路径写在 App / Action 的 `icon` 字段中。

从旧版升级时保持插件 ID，导入 2.1.0 即可沿用数据目录。

## 手动验证权限

1. 在设置中安装插件 Deno Runtime，再导入、信任并启用本插件。
2. 打开「Hello 示例插件」面板，点 **运行检查**。
3. `tempo.files（UI）` 与 `tempo.files（Runtime）` 应显示读写成功；其余 Deno 权限应显示已阻止。
4. 点 **打招呼（Runtime）** 后再次运行检查，`hello.log` 仍可由 Host 文件 API 正常读写。

要演示细粒度权限，将 Manifest 改为以下内容，提高插件版本后重新导入。再次检查时，Deno 对 `$DATA` 的直接读写会成功，匹配的网络和环境变量查询也会变为允许；`sys`、`run`、`ffi` 和远程 `import` 仍保持关闭。

```json
"permissions": {
  "read": ["$DATA"],
  "write": ["$DATA"],
  "net": ["example.com:443"],
  "env": ["PERMISSION_DEMO_VALUE"]
}
```

要演示 Deno 完全访问，改为独占的 `all` 声明。它不能与上述细粒度权限同时使用：

```json
"permissions": {
  "all": true
}
```

完全访问使用 Deno `-A`。权限检查中的 Deno 项会全部显示允许，托管 UI 的 HTTP(S)、WebSocket、图片、媒体和字体网络访问也会开放；远程脚本、样式和 iframe 仍受宿主 CSP 限制。

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
ipcMain.handle("permission-probe", ...);
ipcMain.handle("sca-probe", ...);
ipcMain.on("sca-ping", ...);
tempo.commands.register("hello", ...); // 仅供 Action 调用
tempo.mcpTools.register("say-hello", ...); // 仅供 MCP 调用

// index.js
await window.ipcRenderer.invoke("greet", { who: "Tempo" });
await window.ipcRenderer.invoke("permission-probe");
await window.ipcRenderer.invoke("sca-probe", fixture);
window.ipcRenderer.send("sca-ping", fixture);
window.ipcRenderer.on("sca-echo" | "sca-pong" | "greeted", ...);
```
