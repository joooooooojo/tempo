---
title: 插件类型与生命周期
description: UI、Hybrid、Headless 的入口格式、启动时机与清理方式。
---

# 插件类型与生命周期

插件类型由是否存在页面和 Runtime 决定。Manifest 的 `kind` 用于分类，真正的运行方式取决于 `contributes.apps` 和 `main`。

| 类型 | `apps` | `main` | 适合 |
| --- | :---: | :---: | --- |
| UI | 有 | 无 | 页面、表单、展示、本地插件存储 |
| Hybrid | 有 | 有 | 页面加 Node 后台、文件或复杂计算 |
| Headless | 无 | 有 | Action、MCP Tool、后台监听 |

## UI 生命周期

每次打开 App，Tempo 会创建一个页面实例，并在插件脚本执行前注入：

```js
window.tempo
window.ipcRenderer
```

UI 不使用 Tempo 生命周期钩子。页面的创建、加载、刷新和销毁都由 WebView 管理，React、Vue 等框架继续使用自己的组件生命周期，原生页面使用标准浏览器事件。

页面顺序如下：

1. Tempo 创建 iframe 并注入 Host Bridge。
2. HTML 加载，模块脚本按浏览器规则执行。
3. `window.tempo.ready()` 在宿主上下文到达后 resolve。
4. 页面关闭或刷新时，由 WebView 销毁 document 和其中的监听器。

```js
const context = await window.tempo.ready();
console.log(context.params, context.session);

const stopTheme = await window.tempo.theme.subscribe((theme) => {
  document.documentElement.dataset.theme = theme;
});
```

模块脚本位于 `body` 末尾时可以直接访问 DOM；脚本位于 `head` 时，按标准浏览器方式等待 `DOMContentLoaded`。`window.tempo.ready()` 只等待 Tempo 上下文，不代表 DOM 生命周期。整个页面被销毁时，WebView 和 Host 会清理页面监听与订阅；如果只是 SPA 中的某个组件不再使用主题订阅，则由组件主动调用 `stopTheme()`。

页面关闭时浏览器不会等待异步清理。需要保存的数据应在用户操作发生时立即写入 `tempo.storage` 或 `tempo.session.push`，不要依赖卸载事件进行异步保存。

## Runtime 入口格式

Runtime 不需要导出对象或函数。入口被加载后，直接使用宿主注入的全局变量：

```js
globalThis.tempo
globalThis.ipcMain
onMounted(fn)
onUnmounted(fn)
```

推荐使用 `main.mjs`：

```js
let timer;

onMounted(() => {
  tempo.commands.register("status", async () => ({ running: true }));
  ipcMain.handle("get-status", async () => ({ running: true }));
  timer = setInterval(() => console.log("tick"), 60_000);
});

onUnmounted(() => {
  clearInterval(timer);
});
```

这里不需要插件包装对象、激活函数或默认导出。

### main.mjs

`.mjs` 始终按 ESM 处理，可以使用静态 `import`：

```js
import { readFile } from "node:fs/promises";

onMounted(async () => {
  const text = await readFile(new URL("./data.txt", import.meta.url), "utf8");
  console.log(text);
});
```

### main.js

`.js` 也可以直接写全局 API：

```js
onMounted(() => {
  tempo.commands.register("run", async (params) => ({ ok: true, params }));
});
```

如果要在 `main.js` 中使用 ESM `import`，包根目录需要 `package.json` 并声明 `"type": "module"`。没有这个声明时，Node 会把 `.js` 当作 CommonJS。为避免发布环境歧义，模板统一输出 `main.mjs`。

TypeScript 不能直接写进 Manifest 的 `main`。必须先用 Vite 等工具构建成 `.js` 或 `.mjs`。

## Runtime 启动时机

Runtime 在确实需要时启动：

- Action 执行 Command。
- 外部 MCP 客户端调用已注册的 MCP Tool。
- UI 第一次调用 `ipcRenderer.invoke` 或 `send`。
- Manifest 包含 `activationEvents: ["onStartup"]`，Tempo 启动后主动加载。
- 开发助手连接 Hybrid 或 Headless 项目。

平台广播不会启动已停止的 Runtime。它只送到已经运行的 Runtime 和当前打开的页面。

## Runtime 生命周期

```text
加载 main.mjs
    ↓
执行顶层代码，收集 onMounted/onUnmounted
    ↓
依次执行 onMounted
    ↓
Runtime ready，开始处理 Command、IPC 和平台事件
    ↓
正常停止时执行 onUnmounted
```

任意入口加载错误或 `onMounted` 抛错都会使启动失败。`onUnmounted` 是尽力执行：进程崩溃、强制退出或系统终止时可能来不及运行，因此持久化数据应在业务发生时写入 `tempo.storage` 或 `tempo.paths.data`。

## 三种类型如何处理事件

| 类型 | 平台事件 | UI ↔ Runtime |
| --- | --- | --- |
| UI | 页面打开期间用 `window.tempo.events.on` | 没有 Runtime，不使用 IPC |
| Hybrid | 页面和运行中的 Runtime 都可监听；按职责选择一侧 | `window.ipcRenderer` ↔ `globalThis.ipcMain` |
| Headless | 在 Runtime 用 `tempo.events.on`；常驻监听需 `onStartup` | 没有 UI，通常不使用 IPC |

同一个平台事件如果在 Hybrid 两侧都监听，会执行两次业务代码。通常让 Runtime 负责后台工作，UI 只监听确实影响当前画面的事件。
