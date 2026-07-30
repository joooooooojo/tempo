---
title: 插件全局 API
description: UI 与 Runtime 中由 Tempo 注入的全局变量、生命周期和 IPC。
---

# 插件全局 API

Tempo 在插件入口执行前直接注入 API。项目不需要安装或导入额外的 Tempo 包。

## 注入位置

| 能力 | UI 页面 | Runtime |
| --- | --- | --- |
| 平台 API | `window.tempo` | `globalThis.tempo` |
| UI ↔ Runtime | `window.ipcRenderer` | `globalThis.ipcMain` |
| 生命周期 | WebView / 浏览器标准生命周期 | `onMounted`、`onUnmounted` |

```js
// UI
await window.tempo.ready();
void window.tempo.notify.show({ title: "UI ready" });

// Runtime
onMounted(() => {
  tempo.commands.register("run", async () => ({ ok: true }));
});
```

不需要插件包装对象、激活函数或任何模块导出。业务依赖仍然可以正常使用 npm，并由 Vite 打进最终产物。

## tempo 的职责

`tempo` 只表示插件调用 Tempo 平台：

| API | UI | Runtime | 作用 |
| --- | :---: | :---: | --- |
| `context` / `ready()` | 是 | 否 | 当前页面参数、主题和 Session |
| `events.on()` / `once()` / `off()` | 是 | 是 | 管理平台广播监听 |
| `storage` | 是 | 是 | 插件私有持久化存储 |
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
| `paths` / `runtime` | 否 | 是 | Runtime 数据目录和 Node 信息 |

完整参数见 [平台 API](/reference/plugin-host-api)。

## IPC 的职责

IPC 只用于同一个插件内部的 UI 与 Runtime 通信。它与平台事件、Action Command、MCP Tool 是彼此独立的通道。

### invoke / handle

```js
// Runtime
ipcMain.handle("load-user", async (_event, userId) => {
  return { id: userId, name: "Ada" };
});

// UI
const user = await window.ipcRenderer.invoke("load-user", "42");
```

`invoke` 返回 Promise。没有对应 handler 时会返回 `NOT_FOUND`。

### send / on

```js
// UI -> Runtime
window.ipcRenderer.send("editor-changed", { dirty: true });
ipcMain.on("editor-changed", (event, payload) => {
  event.sender.send("save-state", { saving: payload.dirty });
});

// Runtime -> UI
const off = window.ipcRenderer.on("save-state", (_event, payload) => {
  console.log(payload.saving);
});
```

`ipcMain.send(channel, ...args)` 可以向当前打开的插件页面广播消息。`on` 返回取消监听函数。

IPC 使用 Structured Clone。支持普通对象、数组、`Date`、`Map`、`Set`、`ArrayBuffer` 和 TypedArray；不支持函数、Promise、Symbol、WeakMap、WeakSet 与 DOM 节点。单次消息上限约 1 MiB。

## 为什么不会混淆事件

名字相同也不会互相触发：

```js
window.tempo.events.on("status.changed", onPlatformStatus);
window.ipcRenderer.on("status.changed", onRuntimeStatus);
```

Tempo 在内部标记事件来源。平台广播只进入 `tempo.events`，Runtime 消息只进入 `ipcRenderer`。不要用 IPC 频道转发平台广播，除非你的业务确实需要 Runtime 处理后再把结果送给 UI。

## 生命周期

UI 没有 Tempo 生命周期钩子。页面按标准 WebView 规则加载和销毁：

```js
const context = await window.tempo.ready();
window.tempo.events.on("clipboard.changed", console.log);
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

插件开发助手会生成对应环境的本地声明文件。Hybrid 模板分别使用 `src/ui/tempo.d.ts` 与 `src/runtime/tempo.d.ts`，避免 UI 与 Runtime 的全局类型互相泄漏；UI 和 Headless 模板仍使用各自的 `src/tempo.d.ts`。这些声明文件不会进入运行时代码，也没有独立版本需要维护。

UI 模板不会声明 Runtime 生命周期钩子。Hybrid 的 Runtime 入口单独声明 Runtime 全局变量，避免 UI 源码误用这些钩子。插件助手会从远端选择当前 Host API 能使用的最新模板，新建项目时即可获得对应类型，不需要等待 Tempo 应用发版。
