---
title: 平台 API
description: "@tempo/sdk 导出的 tempo 方法、参数和运行位置。"
---

# 平台 API

Tempo 把平台 API 注入插件运行环境。UI 从 `@tempo/sdk/ui` 导入 `tempo`，Runtime 从 `@tempo/sdk/runtime` 导入 `tempo`。这里的 API 不负责 UI 与 Runtime 通信；私有通信请使用 SDK 对应入口的 `ipcRenderer` 和 `ipcMain`。

下面的片段省略重复的 SDK import。

## 可用位置

| 分组 | UI | Runtime |
| --- | :---: | :---: |
| 页面上下文 `context`、`ready`、`session` | 是 | 否 |
| `storage`、`files`、`settings`、`notify`、`events` | 是 | 是 |
| `theme.get` | 是 | 是 |
| `theme.subscribe` | 是 | 否 |
| `mainPanel.hide`、`app.open`、`external.open` | 是 | 是 |
| `mainPanel.back`、`mainPanel.setSize`、`window` | 是 | 否 |
| `commands`、`paths`、`runtime` | 否 | 是 |

## 页面上下文

```js
const context = await tempo.ready();
console.log(context.apiVersion, context.theme, context.params, context.session);
```

调用 `ready()` 后可以直接读取 `tempo.context`。通过 Action 打开 App 时，`context.params` 包含：

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

直接调用 `tempo.app.open(appId, params)` 时，`params` 保持调用方传入的结构。

## 私有存储

UI 和 Runtime 共享当前插件的命名空间：

```js
await tempo.storage.set("preferences", { compact: true });
const value = await tempo.storage.get("preferences");
const keys = await tempo.storage.list();
await tempo.storage.delete("preferences");
```

不存在的 key 返回 `null`。

- key 长度：1 到 256 个字符。
- 单个值上限：256 KiB。
- 每个插件总计上限：5 MiB。
- 超出配额返回 `RESOURCE_EXHAUSTED`。

## 私有文件

`tempo.files` 由宿主执行，只能访问当前插件的数据目录。UI 和 Runtime 都能使用，且不需要在 Deno `permissions` 数组中声明 `read` 或 `write`：

```js
await tempo.files.mkdir("notes", { recursive: true });
await tempo.files.writeText("notes/today.md", "# Today");
const text = await tempo.files.readText("notes/today.md");

await tempo.files.writeBytes("avatar.bin", new Uint8Array([0, 1, 255]));
const bytes = await tempo.files.readBytes("avatar.bin");

const entries = await tempo.files.list("notes");
const info = await tempo.files.stat("notes/today.md");
await tempo.files.rename("notes/today.md", "notes/archive.md");
await tempo.files.remove("notes", { recursive: true });
```

路径必须使用正斜杠分隔的相对路径。绝对路径、`..`、盘符、UNC、反斜杠、NUL、符号链接和 Windows junction 都会被拒绝。`list()` 可用空路径读取数据目录根层；其它方法不接受空路径。

- 单个文本或二进制文件上限 512 KiB。
- 单次目录列表最多 256 项。
- `stat()` 在路径不存在时返回 `null`。
- `mkdir` 和 `remove` 默认不递归；递归操作需传入 `{ recursive: true }`。
- `rename` 的目标必须尚不存在。

Runtime 直接使用 `Deno.readFile`、`Deno.writeFile` 或 Node 兼容文件 API 时，仍受 Manifest 中的 Deno 文件权限控制。`tempo.files` 不会返回宿主绝对路径。

## 插件设置

`contributes.settings` 由 Tempo 渲染。插件只负责读取：

```js
const all = await tempo.settings.getAll();
const compact = await tempo.settings.get("compact", false);

const off = tempo.settings.subscribe((values) => {
  console.log(values.compact);
});
```

`subscribe` 返回取消监听函数。不要直接写保留存储键 `__tempo/settings`。

## 平台事件

```js
const off = tempo.events.on("clipboard.changed", (payload) => {
  console.log(payload.at);
});
```

也可以使用 `once`、`off`、`removeAllListeners`、`listenerCount` 和 `eventNames` 管理监听。平台事件不需要 Manifest 声明，也不会进入 IPC。完整方法与事件列表见 [宿主事件](/reference/host-events)。

## 通知

```js
await tempo.notify.show({
  title: "Notes",
  body: "保存完成",
});
```

`title` 默认是 `Tempo Plugin`，`body` 默认为空字符串。系统通知权限不可用时会返回 `FORBIDDEN`。

## 主题

UI 和 Runtime 都可以读取当前主题：

```js
const theme = await tempo.theme.get();
```

返回 `light`、`dark` 或 `system`。只有 UI 可以订阅变化：

```js
const off = await tempo.theme.subscribe((theme) => {
  document.documentElement.dataset.theme = theme;
});
```

返回的 `off` 会同时释放页面监听和宿主订阅。

## 主面板

### hide

UI 和 Runtime 可用。隐藏主面板但保留当前页面状态：

```js
await tempo.mainPanel.hide();
```

### back

仅 UI 可用。退出当前插件页面并回到搜索：

```js
await tempo.mainPanel.back();
```

Tempo 已在捕获阶段把 `Esc` 映射到此操作。

### setSize

仅 UI 可用，只调整主面板高度：

```js
await tempo.mainPanel.setSize(640);
```

## 独立窗口

只有 `windowMode: "standalone"` 的 UI 页面可以调用：

```js
await tempo.window.setRect({
  width: "80%",
  height: 560,
  x: "center",
  y: "10%",
});

await tempo.window.close();
```

- `width`：`320..4096` 像素或 `1%..100%`。
- `height`：`240..2160` 像素或 `1%..100%`。
- `x`、`y`：像素、`0%..100%` 或 `center`。

普通主面板页面调用窗口 API 会返回 `FORBIDDEN`。

## 打开 App

```js
await tempo.app.open("translate", {
  initialTranslateText: "hello",
});
```

内置 App 使用本地 ID。插件 App 使用 `{pluginId}/{appId}`，例如 `com.example.notes/main`。

## 打开外部链接

```js
await tempo.external.open("https://example.com");
```

只允许 `https://`、`http://` 和 `mailto:`，其它 scheme 返回 `FORBIDDEN`。

## 页面 Session

页面 Session 用于恢复轻量 UI 状态：

```js
await tempo.session.push({
  route: "/editor/42",
  cursor: 120,
});
```

下次打开时从 `tempo.context.session` 读取。宿主按插件、App、插件版本和 `sessionVersion` 保存最新值，上限 64 KiB。长期数据使用 `tempo.storage`，不要把敏感信息放进 Session。

## Runtime Commands

Command 是 Runtime 暴露给 Tempo 宿主的能力：

```js
tempo.commands.register("search", async (params, signal) => {
  if (signal.aborted) throw new Error("cancelled");
  return { items: [] };
});
```

ID 必须与 Manifest 的 `contributes.commands[].id` 一致。只有 Action 通过 Manifest 引用 Command；插件 UI 不能直接调用 Command，应使用 IPC。

## Runtime MCP Tools

`tempo.mcpTools.register` 只在 Runtime 中存在，用于实现 Manifest 声明的 MCP Tool：

```js
tempo.mcpTools.register("search-notes", async (params, signal) => {
  if (signal.aborted) throw new Error("cancelled");
  return { items: [] };
});
```

名称必须与 `contributes.mcpTools[].name` 一致。MCP Tool 使用自己的注册表和调用帧，不会进入 Commands；即使 Tool 与 Command 同名，也会分别调用各自的 handler。

## Runtime 信息

```js
console.log(tempo.pluginId);
console.log(tempo.paths.data);
console.log(tempo.runtime.version);
```

`paths.data` 是 Runtime 的插件数据目录绝对路径；只有直接调用 Deno 文件 API 时才需要它。一般文件读写优先使用 `tempo.files`。入口文件所在的安装目录应视为只读。

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
