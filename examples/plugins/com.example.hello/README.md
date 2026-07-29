# Hello 示例插件（com.example.hello）

混合示例：对外 `hello` command（Action / MCP）+ 对内 `tempo.ipc`（含 Structured Clone 探测）。

需要 Host API `^1.5.0`。

## 手动验证 SCA

1. 导入并信任、启用本插件（含 `main` 时需已安装插件 Node Runtime）。
2. 打开「Hello 示例插件」面板。
3. 点 **打招呼（Runtime）**  
   - `invoke greet` 返回值里的 `at` 应为 `Date`  
   - `on greeted` 里的 `at` 应为 `Date`
4. 点 **测试 SCA**  
   - UI → Runtime：`ipc.invoke("sca-probe")` 携带 Date / Map / Set / Uint8Array / 循环引用  
   - Runtime → UI：`ipc.send("sca-echo")` 同结构  
   - UI → Runtime：`ipc.send("sca-ping")` → Runtime `ipc.on` → `ipc.send("sca-pong")`  
   - 日志应出现 `✓ invoke/handle SCA 通过`、`✓ sca-echo SCA 通过`、`✓ sca-pong SCA 通过`

```js
// main.mjs
tempo.ipc.handle("greet", ...);
tempo.ipc.handle("sca-probe", ...);
tempo.ipc.on("sca-ping", ...);
tempo.commands.register("hello", ...); // Action/MCP only

// index.js
await tempo.ipc.invoke("greet", { who: "Tempo" });
await tempo.ipc.invoke("sca-probe", fixture);
tempo.ipc.send("sca-ping", fixture);
tempo.ipc.on("sca-echo" | "sca-pong" | "greeted", ...);
```
