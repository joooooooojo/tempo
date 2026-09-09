---
title: 插件 API 入口
description: UI 与 Runtime 的 SDK 入口、宿主全局、生命周期和 IPC。
---

# 插件 API 入口

Tempo 在插件入口执行前注入底层 API，`@tempo/sdk` 将它们封装为类型安全的模块导出。官方模板已经声明 SDK 依赖。

## 注入位置

| 能力 | SDK 入口 | 底层宿主全局 |
| --- | --- | --- |
| UI | `@tempo/sdk/ui` | `window.tempo`、`window.ipcRenderer` |
| Runtime | `@tempo/sdk/runtime` | `globalThis.tempo`、`globalThis.ipcMain`、生命周期函数 |

```js
import { tempo as uiTempo } from "@tempo/sdk/ui";

await uiTempo.ready();
void uiTempo.notify.show({ title: "UI ready" });

// Runtime
import { onMounted, tempo } from "@tempo/sdk/runtime";

onMounted(() => {
  tempo.commands.register("run", async () => ({ ok: true }));
});
```

业务依赖和 SDK 由 Vite 打进最终产物。底层全局继续保留，以兼容不使用 SDK 的旧插件。

## tempo 的职责

`tempo` 只表示插件调用 Tempo 平台：

| API | UI | Runtime | 作用 |
| --- | :---: | :---: | --- |
| `context` / `ready()` | 是 | 否 | 当前页面参数、主题和 Session |
| `events.on()` / `once()` / `off()` | 是 | 是 | 管理平台广播监听 |
| `storage` | 是 | 是 | 插件私有持久化存储 |
| `files` | 是 | 是 | 读写插件私有数据目录 |
| `settings` | 是 | 是 | 读取宿主渲染的插件设置 |
| `notify.show()` | 是 | 是 | 系统通知 |
| `theme.get()` | 是 | 是 | 当前主题 |
| `theme.subscribe()` | 是 | 否 | 页面跟随主题变化 |
| `mainPanel.hide()` | 是 | 是 | 隐藏主面板 |
| `mainPanel.back()` / `setSize()` | 是 | 否 | 页面导航与面板高度 |
| `window` | 是 | 否 | 控制插件独立窗口 |
| `app.open()` | 是 | 是 | 打开一个 App |
| `external.open()` | 是 | 是 | 打开网页或邮件链接 |
| `session.push()` | 是 | 否 | 更新页面 Session 快照 |
| `commands.register()` | 否 | 是 | 注册 Action 可执行的 Command |
| `mcpTools.register()` | 否 | 是 | 注册 Manifest 声明的 MCP Tool |
| `paths` / `runtime` | 否 | 是 | Runtime 数据目录和 Deno 信息 |

完整参数见 [平台 API](/reference/plugin-host-api)。

## IPC 的职责

IPC 只用于同一个插件内部的 UI 与 Runtime 通信。它与平台事件、Action Command、MCP Tool 是彼此独立的通道。

### invoke / handle

```js
// Runtime
import { ipcMain } from "@tempo/sdk/runtime";

ipcMain.handle("load-user", async (_event, userId) => {
  return { id: userId, name: "Ada" };
});

// UI
import { ipcRenderer } from "@tempo/sdk/ui";

const user = await ipcRenderer.invoke("load-user", "42");
```

`invoke` 返回 Promise。没有对应 handler 时会返回 `NOT_FOUND`。

### send / on

```js
// UI -> Runtime
ipcRenderer.send("editor-changed", { dirty: true });
ipcMain.on("editor-changed", (event, payload) => {
  event.sender.send("save-state", { saving: payload.dirty });
});

// Runtime -> UI
const off = ipcRenderer.on("save-state", (_event, payload) => {
  console.log(payload.saving);
});
```

`ipcMain.send(channel, ...args)` 可以向当前打开的插件页面广播消息。`on` 返回取消监听函数。

IPC 使用 Structured Clone。支持普通对象、数组、`Date`、`Map`、`Set`、`ArrayBuffer` 和 TypedArray；不支持函数、Promise、Symbol、WeakMap、WeakSet 与 DOM 节点。单次消息上限约 1 MiB。

## 为什么不会混淆事件

名字相同也不会互相触发：

```js
tempo.events.on("status.changed", onPlatformStatus);
ipcRenderer.on("status.changed", onRuntimeStatus);
```

Tempo 在内部标记事件来源。平台广播只进入 `tempo.events`，Runtime 消息只进入 `ipcRenderer`。不要用 IPC 频道转发平台广播，除非你的业务确实需要 Runtime 处理后再把结果送给 UI。

## 生命周期

UI 没有 Tempo 生命周期钩子。页面按标准 WebView 规则加载和销毁：

```js
const context = await tempo.ready();
tempo.events.on("clipboard.changed", console.log);
```

使用 React、Vue 等框架时，在框架自己的组件生命周期中订阅和释放即可。整个页面 document 被销毁后，WebView 与 Host 会清理页面监听和订阅。

Runtime 才使用宿主生命周期钩子：

```js
let off;

onMounted(() => {
  off = tempo.events.on("clipboard.changed", console.log);
  tempo.mcpTools.register("status", async () => ({ running: true }));
});

onUnmounted(() => {
  off?.();
});
```

- Runtime 的 `onMounted` 在入口模块加载后执行，全部完成后 Runtime 才进入 ready。
- `onUnmounted` 用于释放监听、定时器和文件句柄，不应作为唯一的数据保存时机。
- 在已经 mounted 后注册 `onMounted`，回调会排入微任务执行。

## TypeScript 类型

`@tempo/sdk/ui` 与 `@tempo/sdk/runtime` 分别导出各自环境的值和类型。Hybrid 的两个 TypeScript 子项目使用不同入口，因此 UI 不会误用 Runtime 生命周期，Runtime 也不会获得 DOM API。公共类型可从任一入口使用 `import type` 导入。
