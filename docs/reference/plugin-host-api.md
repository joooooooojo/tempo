---
title: 平台 API
description: "tempo-plugin-sdk 的 UI Client 与 Runtime Context 方法。"
---

# 平台 API

Tempo 把平台 API 注入插件运行环境。UI 使用 `await connect()` 获得 Client，Runtime 使用 `defineRuntime(setup)` 获得 Context。两者按运行位置提供相同或不同的平台能力，并都用 `ipc` 处理插件内部通信。

下面的片段统一用 `app` 表示 UI Client 或 Runtime Context，并省略重复的 SDK 入口代码。

## 可用位置

| 分组 | UI | Runtime |
| --- | :---: | :---: |
| 页面上下文 `context`、`session` | 是 | 否 |
| `storage`、`files`、`settings`、`notify`、`events` | 是 | 是 |
| `theme.get` | 是 | 是 |
| `theme.subscribe` | 是 | 否 |
| `mainPanel.hide`、`apps.open`、`external.open` | 是 | 是 |
| `mainPanel.back`、`mainPanel.setSize`、`window` | 是 | 否 |
| `commands`、`mcpTools`、`paths`、`runtime`、`onDispose` | 否 | 是 |

## 页面上下文

```js
console.log(
  app.context.apiVersion,
  app.context.theme,
  app.context.params,
  app.context.session,
);
```

`connect()` 返回后 `app.context` 保证非空。通过 Action 打开 App 时，`context.params` 包含：

```ts
interface ActionInvocation {
  actionId: string;
  query: string;
  input:
    | { kind: "text"; text: string }
    | {
        kind: "image";
        entryId: number;
        imageUrl: string;
        filePath?: string;
        width?: number | null;
        height?: number | null;
      }
    | { kind: "file"; entryId: number; paths: string[] };
}
```

直接调用 `app.apps.open(appId, params)` 时，`params` 保持调用方传入的结构。

## 私有存储

UI 和 Runtime 共享当前插件的命名空间：

```js
await app.storage.set("preferences", { compact: true });
const value = await app.storage.get("preferences");
const keys = await app.storage.list();
await app.storage.delete("preferences");
```

不存在的 key 返回 `null`。

- key 长度：1 到 256 个字符。
- 单个值上限：256 KiB。
- 每个插件总计上限：5 MiB。
- 超出配额返回 `RESOURCE_EXHAUSTED`。

## 私有文件

`app.files` 由宿主执行，只能访问当前插件的数据目录。UI 和 Runtime 都能使用，且不需要在 Deno `permissions` 数组中声明 `read` 或 `write`：

```js
await app.files.mkdir("notes", { recursive: true });
await app.files.writeText("notes/today.md", "# Today");
const text = await app.files.readText("notes/today.md");

await app.files.writeBytes("avatar.bin", new Uint8Array([0, 1, 255]));
const bytes = await app.files.readBytes("avatar.bin");

const entries = await app.files.list("notes");
const info = await app.files.stat("notes/today.md");
await app.files.rename("notes/today.md", "notes/archive.md");
await app.files.remove("notes", { recursive: true });
```

路径必须使用正斜杠分隔的相对路径。绝对路径、`..`、盘符、UNC、反斜杠、NUL、符号链接和 Windows junction 都会被拒绝。`list()` 可用空路径读取数据目录根层；其它方法不接受空路径。

- 单个文本或二进制文件上限 512 KiB。
- 单次目录列表最多 256 项。
- `stat()` 在路径不存在时返回 `null`。
- `mkdir` 和 `remove` 默认不递归；递归操作需传入 `{ recursive: true }`。
- `rename` 的目标必须尚不存在。

Runtime 直接使用 `Deno.readFile`、`Deno.writeFile` 或 Node 兼容文件 API 时，仍受 Manifest 中的 Deno 文件权限控制。`app.files` 不会返回宿主绝对路径。

## 插件设置

`contributes.settings` 由 Tempo 渲染。插件只负责读取：

```js
const all = await app.settings.getAll();
const compact = await app.settings.get("compact", false);

const off = app.settings.subscribe((values) => {
  console.log(values.compact);
});
```

`subscribe` 返回取消监听函数。不要直接写保留存储键 `__tempo/settings`。

## 平台事件

```js
const off = app.events.on("clipboard.changed", (payload) => {
  console.log(payload.at);
});
```

也可以使用 `once`、`off`、`removeAllListeners`、`listenerCount` 和 `eventNames` 管理监听。平台事件不需要 Manifest 声明，也不会进入 IPC。完整方法与事件列表见 [宿主事件](/reference/host-events)。

## 通知

```js
await app.notify.show({
  title: "Notes",
  body: "保存完成",
});
```

`title` 默认是 `Tempo Plugin`，`body` 默认为空字符串。系统通知权限不可用时会返回 `FORBIDDEN`。

## 主题

UI 和 Runtime 都可以读取当前主题：

```js
const theme = await app.theme.get();
```

返回 `light`、`dark` 或 `system`。只有 UI 可以订阅变化：

```js
const off = await app.theme.subscribe((theme) => {
  document.documentElement.dataset.theme = theme;
});
```

返回的 `off` 会同时释放页面监听和宿主订阅。

## 主面板

### hide

UI 和 Runtime 可用。隐藏主面板但保留当前页面状态：

```js
await app.mainPanel.hide();
```

### back

仅 UI 可用。退出当前插件页面并回到搜索：

```js
await app.mainPanel.back();
```

Tempo 已在捕获阶段把 `Esc` 映射到此操作。

### setSize

仅 UI 可用，只调整主面板高度：

```js
await app.mainPanel.setSize(640);
```

## 独立窗口

只有 `windowMode: "standalone"` 的 UI 页面可以调用：

```js
await app.window.setRect({
  width: "80%",
  height: 560,
  x: "center",
  y: "10%",
});

await app.window.close();
```

- `width`：`320..4096` 像素或 `1%..100%`。
- `height`：`240..2160` 像素或 `1%..100%`。
- `x`、`y`：像素、`0%..100%` 或 `center`。

普通主面板页面调用窗口 API 会返回 `FORBIDDEN`。

## 打开 App

```js
await app.apps.open("translate", {
  initialTranslateText: "hello",
});
```

内置 App 使用本地 ID。插件 App 使用 `{pluginId}/{appId}`，例如 `com.example.notes/main`。

## 打开外部链接

```js
await app.external.open("https://example.com");
```

只允许 `https://`、`http://` 和 `mailto:`，其它 scheme 返回 `FORBIDDEN`。

## 页面 Session

页面 Session 用于恢复轻量 UI 状态：

```js
await app.session.push({
  route: "/editor/42",
  cursor: 120,
});
```

下次打开时从 `app.context.session` 读取。宿主按插件、App、插件版本和 `sessionVersion` 保存最新值，上限 64 KiB。长期数据使用 `app.storage`，不要把敏感信息放进 Session。

## Runtime Commands

Command 是 Runtime Context 暴露给 Tempo 宿主的能力：

```js
app.commands.register("search", async (params, signal) => {
  if (signal.aborted) throw new Error("cancelled");
  return { items: [] };
});
```

ID 必须与 Manifest 的 `contributes.commands[].id` 一致。只有 Action 通过 Manifest 引用 Command；插件 UI 不能直接调用 Command，应使用 IPC。

## Runtime MCP Tools

`app.mcpTools.register` 只在 Runtime 中存在，用于实现 Manifest 声明的 MCP Tool：

```js
app.mcpTools.register("search-notes", async (params, signal) => {
  if (signal.aborted) throw new Error("cancelled");
  return { items: [] };
});
```

名称必须与 `contributes.mcpTools[].name` 一致。MCP Tool 使用自己的注册表和调用帧，不会进入 Commands；即使 Tool 与 Command 同名，也会分别调用各自的 handler。

## Runtime 信息

```js
console.log(app.pluginId);
console.log(app.paths.data);
console.log(app.runtime.version);
```

`paths.data` 是 Runtime 的插件数据目录绝对路径；只有直接调用 Deno 文件 API 时才需要它。一般文件读写优先使用 `app.files`。入口文件所在的安装目录应视为只读。

## 错误

Host API 失败时 Promise 会 reject，并带有 `code` 与 `message`。常见 code：

| code | 含义 |
| --- | --- |
| `INVALID_REQUEST` | 参数缺失或格式错误 |
| `FORBIDDEN` | 当前位置或协议不允许 |
| `NOT_FOUND` | App、方法、Command 或 IPC channel 不存在 |
| `TIMEOUT` | 调用超时 |
| `PAYLOAD_TOO_LARGE` | 消息超过约 1 MiB |
| `RESOURCE_EXHAUSTED` | 存储或并发配额耗尽 |
| `INTERNAL` | 宿主内部错误 |
