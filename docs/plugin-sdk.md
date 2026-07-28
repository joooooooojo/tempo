# Tempo Plugin SDK（`@tempo/plugin-sdk`）

官方插件 SDK，覆盖 **Runtime** 与 **UI**。统一从 `@tempo/plugin-sdk` 导入，用方法名区分：

- Runtime：`definePlugin` / `wrapRuntimeContext`
- UI：`createPluginClient` / `createPluginClientSync`

源码：`packages/plugin-sdk`。示例：`examples/plugins/com.example.hello`。

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
    const all = await tempo.settings.getAll();
    const loud = await tempo.settings.get("loud", false);

    tempo.settings.subscribe((values) => {
      console.log("settings changed", values);
    });

    tempo.commands.register("hello", async (params, signal) => {
      await tempo.notify.show({ title: "Hi", body: String(params?.who ?? "") });
      tempo.ui.emit("greeted", { who: params?.who });
      return { ok: true };
    });

    await tempo.mainPanel.hide();
    await tempo.app.open("translate", { initialTranslateText: "hi" });
    await tempo.external.open("https://example.com");
    const theme = await tempo.theme.get();

    await tempo.storage.set("preferences", { compact: true });
    const prefs = await tempo.storage.get("preferences");
  },
});
```

也支持具名导出：

```ts
export const { activate, deactivate } = definePlugin({ ... });
```

Bootstrap 同时支持 `export default definePlugin(...)` 与 `export async function activate(ctx)`。

### Runtime `tempo` 一览

| API | 说明 |
|---|---|
| `settings.getAll()` / `get(id, default?)` / `subscribe(fn)` | 中心配置 |
| `storage.get/set/delete/list/update` | 插件私有 KV |
| `notify.show` | 系统通知 |
| `theme.get` | 当前主题 |
| `mainPanel.hide` | 隐藏主面板 |
| `app.open` / `external.open` | 打开应用 / 外链 |
| `commands.register` | 注册 command |
| `ui.emit` | 广播给本插件 UI |
| `paths.data` / `runtime.nodeVersion` / `pluginId` | 上下文 |
| `on(event, fn)` | 宿主推送事件（含 `settings.changed`） |
| `raw` | 原始 bootstrap `ctx` |

## UI

```ts
import { createPluginClient } from "@tempo/plugin-sdk";

const tempo = await createPluginClient();

const loud = await tempo.settings.get("loud", false);
await tempo.notify.show({ title: "Ready" });
const result = await tempo.invoke("hello", { who: "Tempo" });

tempo.settings.subscribe((values) => {
  console.log(values);
});

const stopTheme = await tempo.theme.subscribe((theme) => {
  document.documentElement.dataset.theme = theme;
});

await tempo.session.push({ route: "/home" });
await tempo.mainPanel.setSize(640);
```

### UI `tempo` 一览

| API | 说明 |
|---|---|
| `settings.*` | 同 Runtime；变更由宿主推送到打开的页面 |
| `storage.*` | 私有 KV（自动适配 UI `{ value }` / `{ keys }` 信封） |
| `notify` / `theme` / `app` / `external` | Host 能力 |
| `theme.subscribe` | 自动 `theme.onChange` + `subscription.release` |
| `mainPanel.hide/back/setSize` | 面板 |
| `window.setRect/close` | 独立窗口 |
| `session.push` | 会话恢复 |
| `invoke` / `on` | Runtime command / 事件 |
| `ready` / `context` | 与注入 bridge 一致 |
| `host(method, params)` | 逃生舱 |
| `raw` | `window.plugin` |

## 中心配置约定

- Manifest：`contributes.settings`
- 存储键：`__tempo/settings`（SDK 常量 `TEMPO_SETTINGS_KEY`）
- 事件：`settings.changed`（payload：`{ values }`）
- 插件应只读；写入由「插件配置」Dialog 完成

## 与裸 API 的关系

底层仍是 Host Bridge。SDK 不替换 `window.plugin` / bootstrap `ctx`，只是在其上提供完整封装。需要时仍可用 `tempo.raw` 或 `tempo.host(...)`。
