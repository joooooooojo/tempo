# Tempo Plugin SDK（`@tempo/plugin-sdk`）

官方插件 SDK，覆盖 **Runtime** 与 **UI**。统一从 `@tempo/plugin-sdk` 导入，用方法名区分：

- Runtime：`definePlugin` / `wrapRuntimeContext`
- UI：`createPluginClient` / `createPluginClientSync`

源码：`packages/plugin-sdk`。示例：`examples/plugins/com.example.hello`。需要 Host API `>= 1.5.0` 才能使用完整的 `tempo.ipc`。

## 安装

```bash
npm install @tempo/plugin-sdk
```

**必须把 SDK 打进你的 `main.mjs` / UI 产物。** Tempo 不会在用户机器上再下载一份 SDK。

### 开发服务 UI

插件开发助手使用本地服务 URL 时，页面不是由 `tempo-plugin://` 提供，宿主无法自动修改 HTML。
应用入口需要安装开发 Bridge：

```ts
import "@tempo/plugin-sdk/dev";
```

Vite 项目也可以在现有 `vite.config.ts` 中使用仅 `serve` 阶段生效的适配器：

```ts
import { defineConfig } from "vite";
import { tempoPluginDev } from "@tempo/plugin-sdk/vite";

export default defineConfig({
  plugins: [tempoPluginDev()],
  server: { host: "127.0.0.1" },
});
```

该适配器只注入与正式页面相同的 `window.plugin` Bridge，不启动 Vite、不执行 build，也不修改
`dist`。开发助手选择静态目录时仍由 `tempo-plugin://` 自动注入 Bridge，无需以上配置。

## Runtime

```ts
import { definePlugin } from "@tempo/plugin-sdk";

export default definePlugin({
  async activate(tempo) {
    const loud = await tempo.settings.get("loud", false);

    tempo.settings.subscribe((values) => {
      console.log("settings changed", values);
    });

    // Private UI ↔ Runtime (Electron-style; not in contributes.commands)
    tempo.ipc.handle("greet", async (_event, params) => {
      await tempo.notify.show({ title: "Hi", body: String(params?.who ?? "") });
      tempo.ipc.send("greeted", { who: params?.who });
      return { ok: true };
    });

    // Declared external command for Action / MCP only
    tempo.commands.register("hello", async (params) => {
      return { ok: true, who: params?.who };
    });
  },
});
```

### Runtime `tempo` 一览

| API | 说明 |
|---|---|
| `ipc.handle` / `ipc.on` / `ipc.send` | Electron 风格对内 IPC（UI 专用） |
| `commands.register` | 对外 command（Action / Hook / MCP） |
| `settings.*` / `storage.*` | 配置与私有 KV |
| `notify` / `theme` / `app` / `external` / `mainPanel.hide` | Host 能力 |
| `ui.emit` | **deprecated**，委托 `ipc.send` |
| `on(event, fn)` | 宿主推送（含 `settings.changed`） |
| `raw` | 原始 bootstrap `ctx` |

## UI

```ts
import { createPluginClient } from "@tempo/plugin-sdk";

const tempo = await createPluginClient();

const result = await tempo.ipc.invoke("greet", { who: "Tempo" });
tempo.ipc.on("greeted", (_event, payload) => console.log(payload));
tempo.ipc.send("ping", { n: 1 });

await tempo.notify.show({ title: "Ready" });
tempo.settings.subscribe((values) => console.log(values));
```

`ipc.invoke` / `ipc.send` / `ipc.on` 的参数与返回值（及事件 args）均使用 HTML Structured Clone 语义序列化（Date / Map / Set / TypedArray / 循环引用等）。Function / Promise / DOM 等会抛错。

### UI `tempo` 一览

| API | 说明 |
|---|---|
| `ipc.invoke` / `ipc.send` / `ipc.on` | 对内 Runtime IPC（**不能**调对外 command） |
| `settings.*` / `storage.*` / `notify` / `theme` / … | Host 能力 |
| `on` | 宿主推送；Runtime 事件优先 `ipc.on` |
| `host(method, params)` | 逃生舱 |
| `raw` | `window.plugin` |

## 中心配置约定

- Manifest：`contributes.settings`
- 存储键：`__tempo/settings`（SDK 常量 `TEMPO_SETTINGS_KEY`）
- 事件：`settings.changed`（payload：`{ values }`）
- 插件应只读；写入由「插件配置」Dialog 完成

## 与裸 API 的关系

底层仍是 Host Bridge。SDK 不替换 `window.plugin` / bootstrap `ctx`，只是在其上提供完整封装。
