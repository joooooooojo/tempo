---
title: 插件 API 入口
description: UI Client、Runtime Context、生命周期和强类型 IPC。
---

# 插件 API 入口

Tempo 在插件入口执行前注入底层 API。`@tempo/sdk` 在这些对象之上提供两个入口：UI 使用 `connect()`，Runtime 使用 `defineRuntime()`。官方模板已经声明 SDK 依赖并把它打进最终产物。

## 两个入口

| 环境 | SDK 入口 | 入口函数 | 结果 |
| --- | --- | --- | --- |
| UI | `@tempo/sdk/ui` | `await connect()` | 上下文非空的 `TempoUiClient` |
| Runtime | `@tempo/sdk/runtime` | `defineRuntime(setup)` | 挂载时传入 `TempoRuntimeContext` |

```ts
// UI
import { connect } from "@tempo/sdk/ui";

const app = await connect();
await app.notify.show({ title: `UI ready: ${app.context.apiVersion}` });

// Runtime
import { defineRuntime } from "@tempo/sdk/runtime";

defineRuntime(({ commands }) => {
  commands.register("run", async () => ({ ok: true }));
});
```

底层 `window.tempo`、`window.ipcRenderer` 与 Runtime 全局继续存在，以运行 SDK 和兼容旧插件。使用 SDK 的业务代码无需直接读取它们。

## 平台能力

| API | UI Client | Runtime Context | 作用 |
| --- | :---: | :---: | --- |
| `context` | 是 | 否 | 当前页面参数、主题和 Session |
| `events` | 是 | 是 | 管理平台广播监听 |
| `storage` / `files` | 是 | 是 | 插件私有数据 |
| `settings` / `notify` | 是 | 是 | 设置与系统通知 |
| `theme.get()` | 是 | 是 | 当前主题 |
| `theme.subscribe()` | 是 | 否 | 页面跟随主题变化 |
| `mainPanel.hide()` | 是 | 是 | 隐藏主面板 |
| `mainPanel.back()` / `setSize()` | 是 | 否 | 页面导航与面板高度 |
| `window` / `session` | 是 | 否 | 独立窗口与页面 Session |
| `apps.open()` / `external.open()` | 是 | 是 | 打开 App 或外部链接 |
| `commands` / `mcpTools` | 否 | 是 | 注册 Command 与 MCP Tool |
| `paths` / `runtime` | 否 | 是 | 数据目录和 Deno 信息 |
| `onDispose()` | 否 | 是 | 注册 Runtime 清理函数 |

完整参数见 [平台 API](/reference/plugin-host-api)。

## IPC

`ipc` 只用于同一个插件内部的 UI 与 Runtime 通信。它与平台事件、Action Command、MCP Tool 是相互独立的通道。

### invoke / handle

```ts
// Runtime Context
ipc.handle("load-user", async (_event, userId) => {
  return { id: userId, name: "Ada" };
});

// UI Client
const user = await app.ipc.invoke("load-user", "42");
```

`invoke` 返回 Promise。没有对应 handler 时返回 `NOT_FOUND`。

### send / on

```ts
// UI -> Runtime
app.ipc.send("editor-changed", { dirty: true });
ipc.on("editor-changed", (event, payload) => {
  event.sender.send("save-state", { saving: payload.dirty });
});

// Runtime -> UI
const off = app.ipc.on("save-state", (_event, payload) => {
  console.log(payload.saving);
});
```

Runtime 的 `ipc.send(channel, ...args)` 可向当前打开的插件页面广播消息。两侧的 `on` 都返回取消监听函数。

IPC 使用 Structured Clone，支持普通对象、数组、`Date`、`Map`、`Set`、`ArrayBuffer` 和 TypedArray；不支持函数、Promise、Symbol、WeakMap、WeakSet 与 DOM 节点。单次消息上限约 1 MiB。

## IPC 契约

TypeScript Hybrid 插件应在共享文件中描述频道：

```ts
export type PluginIpc = {
  invokes: {
    "load-user": (userId: string) => { id: string; name: string };
  };
  messages: {
    "editor-changed": [payload: { dirty: boolean }];
    "save-state": [payload: { saving: boolean }];
  };
};
```

把同一类型传给两侧：

```ts
const app = await connect<PluginIpc>();

defineRuntime<PluginIpc>(({ ipc }) => {
  // channel、参数、返回值均会检查
});
```

`invokes` 中函数的参数对应 `invoke/handle` 参数，返回值对应 `invoke` 的 Promise 结果。`messages` 的元组对应 `send/on` 参数。省略契约时保留动态字符串频道。

## 事件隔离

平台广播通过 `events` 接收，Runtime 消息通过 `ipc` 接收。即使名称相同也不会互相触发：

```ts
app.events.on("clipboard.changed", onPlatformEvent);
app.ipc.on("clipboard.changed", onRuntimeMessage);
```

## 生命周期

UI 没有 Tempo 生命周期钩子。`connect()` 只等待 Tempo 页面上下文；DOM 加载、框架组件挂载和页面销毁仍按 WebView 规则处理。

Runtime 的 setup 在宿主挂载阶段执行：

```ts
defineRuntime(({ events, mcpTools, onDispose }) => {
  onDispose(events.on("clipboard.changed", console.log));
  mcpTools.register("status", async () => ({ running: true }));

  return async () => {
    await flushPendingWork();
  };
});
```

setup 可以同步或异步执行。`onDispose()` 可调用多次，setup 也可返回一个清理函数。停止时 SDK 按相反顺序执行所有清理项；某项失败不会阻止其余项执行。进程崩溃或系统强制终止时清理可能来不及完成，因此它不应是保存数据的唯一时机。

## TypeScript 类型

`@tempo/sdk/ui` 与 `@tempo/sdk/runtime` 都重新导出公共类型。Hybrid 的两个 TypeScript 子项目使用不同入口和 lib，因此 UI 不会误用 Runtime 生命周期，Runtime 也不会获得 DOM API。
