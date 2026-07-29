# Tempo Plugin Host API

本文描述 Host API `1.5.0` 已实现的调用方式。接口实现以 `src-tauri/src/plugins/bridge.rs`、`plugin-ui/bridge-client.js` 和 `plugin-runtime/bootstrap.mjs` 为准。

- [插件开发指南](./plugin-development.md)
- [Manifest Schema](./schemas/plugin-manifest.schema.json)
- 单次消息上限：1 MiB
- 每插件并发请求上限：32
- 普通调用超时：30 秒
- 窗口和面板交互调用超时：5 秒

插件与宿主之间有三条互不混用的通道：

| 通道 | 用途 | 注册 / 声明 |
|---|---|---|
| Host API | UI/Runtime 调用宿主能力（`notify`、`storage`、面板等） | 固定方法表 |
| 对外 Command | Action / Hook / MCP | `contributes.commands` + `commands.register` |
| 对内 IPC | 仅本插件 UI ↔ Runtime（Electron 风格） | **不进 manifest**；`ipc.handle` / `ipc.invoke` |

## 1. UI 入口

Tempo 在每个插件 UI 中注入：

```ts
interface TempoPluginUiApi {
  ready(): Promise<PluginContext>;
  readonly context: PluginContext | null;
  readonly ipc: {
    invoke(channel: string, ...args: unknown[]): Promise<unknown>;
    send(channel: string, ...args: unknown[]): void;
    on(channel: string, listener: (event: { sender: string }, ...args: unknown[]) => void): () => void;
  };
  host<TResult = unknown>(method: string, params?: unknown): Promise<TResult>;
  on(event: string, handler: (payload: unknown) => void): () => void;
}

interface PluginContext {
  apiVersion: string;
  theme: string;
  params: unknown;
  session: unknown | null;
}

type ActionInvocation = {
  actionId: string;
  query: string;
  input:
    | { kind: "none" }
    | { kind: "text"; text: string }
    | {
        kind: "image";
        entryId: number;
        imageUrl: string;
        /** Runtime command targets only. */
        filePath?: string;
        width?: number | null;
        height?: number | null;
      }
    | {
        kind: "file";
        entryId: number;
        /** Absolute local paths from a clipboard file entry. */
        paths: string[];
      };
};

declare global {
  interface Window {
    plugin: TempoPluginUiApi;
  }
}
```

通过 `actions[].app` 打开 UI 时，`PluginContext.params` 是 `ActionInvocation`；通过
`actions[].command` 执行 Runtime 时，注册的 command handler 收到同样的结构。

UI ↔ Runtime 私有 IPC（Electron 风格；不进 `contributes.commands`）：

```js
const result = await window.plugin.ipc.invoke("greet", { who: "Tempo" });
window.plugin.ipc.on("greeted", (_event, payload) => console.log(payload));
window.plugin.ipc.send("ping", { n: 1 });
```

`ipc.invoke` / `ipc.send` / `ipc.on` 的参数与返回值（及事件 args）均使用 HTML Structured Clone 语义（Date / Map / Set / TypedArray / 循环引用等；Function / Promise / DOM 会抛错）。UI **不能**调用对外 command。

调用 Host API：

```js
await window.plugin.host("notify.show", {
  title: "完成",
  body: "任务已处理",
});
```

`host` 只接受下表中的 Host 方法。经 UI bridge 调用 `runtime.*` 会被拒绝。

## 2. Runtime 入口

含 `main` 的插件在 `activate(ctx)` 中使用 `ctx.host`：

```js
export async function activate(ctx) {
  await ctx.host.notify.show({ title: "Plugin", body: "Runtime activated" });
  const theme = await ctx.host.theme.get();
  const value = await ctx.host.storage.plugin.get("key");
}
```

Runtime 当前提供以下便捷接口：

```ts
interface RuntimeHostApi {
  mainPanel: {
    hide(): Promise<void>;
  };
  app: {
    open(appId: string, params?: Record<string, unknown>): Promise<void>;
  };
  external: {
    open(url: string): Promise<void>;
  };
  notify: {
    show(options: { title?: string; body?: string }): Promise<void>;
  };
  theme: {
    get(): Promise<{ theme: string }>;
  };
  storage: {
    plugin: {
      get<T = unknown>(key: string): Promise<T | null>;
      set(key: string, value: unknown): Promise<void>;
      delete(key: string): Promise<void>;
      list(): Promise<string[]>;
    };
  };
}
```

UI 专用的 `mainPanel.back`、`mainPanel.setSize`、`window.*`、`session.push` 和主题订阅不会暴露到 Runtime 便捷接口。

Bootstrap 还提供与 Host 方法表分离的 Runtime 注册面：

```js
export async function activate(ctx) {
  // 对内：仅 UI 通过 window.plugin.ipc / tempo.ipc 调用
  ctx.ipc.handle("greet", async (event, params) => {
    return { who: params?.who ?? "world" };
  });
  ctx.ipc.on("ping", (event, payload) => {
    console.log("ui ping", payload);
  });

  // 对外：应与 contributes.commands 对齐；Action / Hook / MCP
  ctx.registerCommand("hello", async (params, signal) => {
    return { who: params?.who ?? "world" };
  });

  ctx.ipc.send("ready", {});
  // ctx.ui.emit 为 ipc.send 的薄别名（deprecated）
}
```

## 3. 方法总览

| 方法 | UI | Runtime | 返回值 |
|---|---:|---:|---|
| `mainPanel.hide` | 是 | 是 | `null` / `void` |
| `mainPanel.back` | 是 | 否 | `null` |
| `mainPanel.setSize` | 是 | 否 | `null` |
| `window.setRect` | 仅独立窗口 UI | 否 | `null` |
| `window.close` | 仅独立窗口 UI | 否 | `null` |
| `app.open` | 是 | 是 | `null` / `void` |
| `external.open` | 是 | 是 | `null` / `void` |
| `notify.show` | 是 | 是 | `null` / `void` |
| `theme.get` | 是 | 是 | `{ theme }` |
| `theme.onChange` | 是 | 否 | `{ subscriptionId }` |
| `subscription.release` | 是 | 否 | `null` |
| `storage.plugin.get` | 是 | 是 | UI：`{ value }`；Runtime：`value` |
| `storage.plugin.set` | 是 | 是 | `null` / `void` |
| `storage.plugin.delete` | 是 | 是 | `null` / `void` |
| `storage.plugin.list` | 是 | 是 | UI：`{ keys }`；Runtime：`keys` |
| `session.push` | 是 | 否 | `null` |

## 4. 面板 API

### mainPanel.hide

隐藏主面板，不退出当前应用页。下次打开仍停留在原页面。

```js
await window.plugin.host("mainPanel.hide");
await ctx.host.mainPanel.hide();
```

### mainPanel.back

退出当前应用页并回到主面板搜索，仅 UI 可用。

```js
await window.plugin.host("mainPanel.back");
```

Tempo 注入的 UI Bridge 会在捕获阶段处理 Esc，并自动调用 `mainPanel.back`。

### mainPanel.setSize

调整主面板高度，仅 UI 可用。主面板宽度保持由宿主控制，高度最终会限制在宿主允许范围内。

```js
await window.plugin.host("mainPanel.setSize", { height: 640 });
```

参数：

```ts
{ height: number }
```

## 5. 独立窗口 API

### window.setRect

调整当前插件独立窗口。普通面板插件调用会返回 `FORBIDDEN`。

```js
await window.plugin.host("window.setRect", {
  width: "80%",
  height: 560,
  x: "center",
  y: "10%",
});
```

规则与 manifest 的 `apps[].rect` 一致：

- `width`：`320..4096` 像素或 `1%..100%`
- `height`：`240..2160` 像素或 `1%..100%`
- `x/y`：像素、`0%..100%` 或 `center`
- 未提供字段使用默认矩形，不表示保留当前字段

### window.close

关闭当前插件独立窗口：

```js
await window.plugin.host("window.close");
```

## 6. 应用与系统 API

### app.open

打开已注册的 Tempo 应用。内置应用使用本地 ID，插件应用使用运行时 ID `{pluginId}/{appId}`。

UI：

```js
await window.plugin.host("app.open", {
  appId: "translate",
  params: { initialTranslateText: "hello" },
});
```

Runtime：

```js
await ctx.host.app.open("translate", {
  initialTranslateText: "hello",
});
```

### external.open

使用系统默认应用打开外部 URL。仅接受 `https://`、`http://` 和 `mailto:`。

```js
await window.plugin.host("external.open", { url: "https://example.com" });
await ctx.host.external.open("https://example.com");
```

其它 scheme 返回 `FORBIDDEN`。

### notify.show

显示系统通知：

```js
await window.plugin.host("notify.show", {
  title: "Tempo Plugin",
  body: "操作完成",
});

await ctx.host.notify.show({ title: "Tempo Plugin", body: "操作完成" });
```

`title` 省略时为 `Tempo Plugin`，`body` 省略时为空字符串。

## 7. 主题 API

### theme.get

```js
const { theme } = await window.plugin.host("theme.get");
const runtimeTheme = await ctx.host.theme.get();
```

当前值通常为 `light`、`dark` 或 `system`。

### theme.onChange

UI 订阅主题变化：

```js
const { subscriptionId } = await window.plugin.host("theme.onChange");
const off = window.plugin.on("theme.changed", ({ theme }) => {
  document.documentElement.dataset.theme = theme;
});

// 不再需要时：
off();
await window.plugin.host("subscription.release", { subscriptionId });
```

`window.plugin.on` 只移除当前页面的 JavaScript handler；`subscription.release` 同时释放宿主订阅记录。

## 8. 插件私有存储

数据按插件 ID 隔离。默认限制：每个值最大 256 KiB，每个插件总计最大 5 MiB，key 写入时必须为 1–256 个字符。

UI：

```js
await window.plugin.host("storage.plugin.set", {
  key: "preferences",
  value: { compact: true },
});

const { value } = await window.plugin.host("storage.plugin.get", {
  key: "preferences",
});

const { keys } = await window.plugin.host("storage.plugin.list");
await window.plugin.host("storage.plugin.delete", { key: "preferences" });
```

Runtime：

```js
await ctx.host.storage.plugin.set("preferences", { compact: true });
const value = await ctx.host.storage.plugin.get("preferences");
const keys = await ctx.host.storage.plugin.list();
await ctx.host.storage.plugin.delete("preferences");
```

未找到的 key 返回 `null`。超过配额时返回 `RESOURCE_EXHAUSTED`。

宿主集中配置（`contributes.settings`）写入保留键 `__tempo/settings`。推荐用 SDK：`tempo.settings.getAll()` / `get(id)` / `subscribe(fn)`。裸 API 仍可用 `storage.plugin.get("__tempo/settings")`。用户在「插件配置」中修改后，宿主会向该插件打开的 UI 与运行中的 Runtime 推送 `settings.changed`。

## 9. Session API

### session.push

UI 主动提交用于下次创建页面时恢复的轻量状态：

```js
await window.plugin.host("session.push", {
  payload: {
    route: "/editor/42",
    cursor: 120,
  },
});
```

宿主按 `{pluginId, appId, pluginVersion, sessionVersion}` 保存最新 payload，并在下次 `window.plugin.ready()` 的 `context.session` 中返回。最终持久化上限为 64 KiB。

Session 不用于敏感数据或大型文档。长期数据使用 `storage.plugin.*` 或 Runtime 的 `ctx.paths.data`。

## 10. Runtime ↔ UI 事件（`ipc.send`）

Runtime 可向当前打开的同插件 UI 广播事件：

```js
// main.mjs
ctx.ipc.send("sync.completed", { count: 3 });
// deprecated alias:
ctx.ui.emit("sync.completed", { count: 3 });

// index.js
window.plugin.ipc.on("sync.completed", (_event, payload) => {
  console.log(`synced ${payload.count}`);
});
```

事件 args 与 `ipc.invoke` 一样使用 Structured Clone 序列化。页面未打开时发送的事件会丢失。宿主推送（如 `settings.changed`）仍走 `plugin.on`（JSON）；SDK 优先用 `settings.subscribe` / `theme.subscribe`。

## 11. 错误处理

UI 和 Runtime Host API 都以 rejected Promise 返回结构化错误：

```ts
interface RpcError {
  code:
    | "INVALID_REQUEST"
    | "PAYLOAD_TOO_LARGE"
    | "RESOURCE_EXHAUSTED"
    | "NOT_FOUND"
    | "FORBIDDEN"
    | "TIMEOUT"
    | "CANCELLED"
    | "ACTIVATION_FAILED"
    | "RUNTIME_UNAVAILABLE"
    | "COMMAND_FAILED"
    | "INTERNAL";
  message: string;
  data?: unknown;
}
```

```js
try {
  await window.plugin.ipc.invoke("save", { value });
} catch (error) {
  if (error.code === "RESOURCE_EXHAUSTED") {
    // 提示用户清理插件数据。
  }
}
```

- `COMMAND_FAILED`：插件 command 抛出的业务错误。
- `INTERNAL`：宿主基础设施错误，内部细节会被隐藏。
- `TIMEOUT`：调用超过时间限制；Runtime command 的 `AbortSignal` 会被触发。
- `RUNTIME_UNAVAILABLE`：插件没有可用 Runtime 或 Runtime 已退出。
