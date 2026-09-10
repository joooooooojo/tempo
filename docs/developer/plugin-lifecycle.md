---
title: 插件类型与生命周期
description: UI、Hybrid、Headless 的入口格式、启动时机与清理方式。
---

# 插件类型与生命周期

插件类型由是否存在页面和 Runtime 决定。Manifest 的 `kind` 用于分类，真正的运行方式取决于 `contributes.apps` 和 `main`。

| 类型 | `apps` | `main` | 适合 |
| --- | :---: | :---: | --- |
| UI | 有 | 无 | 页面、表单、展示、本地插件存储 |
| Hybrid | 有 | 有 | 页面加 Deno 后台、文件或复杂计算 |
| Headless | 无 | 有 | Action、MCP Tool、后台监听 |

## UI 生命周期

每次打开 App，Tempo 都会创建页面实例，并在插件脚本执行前注入宿主桥接。UI 入口通过 SDK 连接：

```ts
import { connect } from "tempo-plugin-sdk/ui";

const app = await connect();
console.log(app.context.params, app.context.session);

const stopTheme = await app.theme.subscribe((theme) => {
  document.documentElement.dataset.theme = theme;
});
```

页面顺序如下：

1. Tempo 创建 iframe 并注入 Host Bridge。
2. HTML 与模块脚本按浏览器规则加载。
3. `connect()` 等待宿主上下文并返回 `TempoUiClient`。
4. 页面关闭或刷新时，WebView 销毁 document 和页面监听。

`connect()` 不代表 DOM 已经加载。React、Vue 等框架继续使用自己的组件生命周期；模块脚本在 `head` 中时仍需等待 `DOMContentLoaded`。

页面关闭时浏览器不会等待异步清理。数据应在业务发生时写入 `app.storage` 或 `app.session.push()`，不要依赖卸载事件保存。

## Runtime 入口

Runtime 使用一个声明式入口：

```ts
import { defineRuntime } from "tempo-plugin-sdk/runtime";

defineRuntime(({ commands, events, ipc, onDispose }) => {
  commands.register("status", async () => ({ running: true }));
  ipc.handle("get-status", async () => ({ running: true }));

  const timer = setInterval(() => console.log("tick"), 60_000);
  onDispose(() => clearInterval(timer));
  onDispose(events.on("clipboard.changed", console.log));
});
```

入口不需要导出激活函数或默认对象。模板把 TypeScript 构建为 ESM `main.mjs`；Manifest 的 `main` 不能直接指向 TypeScript 文件。

原生 JavaScript 插件仍可直接使用 Tempo 注入的底层全局。`tempo-plugin-sdk` 1.x 不再导出这些全局对象，新 TypeScript 项目应使用 `connect()` / `defineRuntime()`。

## Runtime 启动时机

Runtime 在确实需要时启动：

- Action 执行 Command。
- 外部 MCP 客户端调用插件 Tool。
- UI 第一次调用 `app.ipc.invoke()` 或 `send()`。
- Manifest 包含 `activationEvents: ["onStartup"]`。
- 开发助手连接 Hybrid 或 Headless 项目。

平台广播不会启动已停止的 Runtime，只会送到已经运行的 Runtime 和当前打开的页面。

## Runtime 生命周期

```text
加载 main.mjs
    ↓
执行顶层代码并调用 defineRuntime(setup)
    ↓
宿主挂载，SDK 执行 setup
    ↓
Runtime ready，处理 Command、IPC 和平台事件
    ↓
宿主停止，SDK 逆序执行 disposer
```

setup 可以返回 Promise；完成后 Runtime 才进入 ready。setup 抛错会使启动失败。

清理函数可通过 `onDispose()` 注册，也可由 setup 返回。SDK 会逆序执行全部清理项，并汇总错误。进程崩溃、强制退出或系统终止时仍可能来不及清理，因此持久化数据应在业务发生时写入 `storage` 或 `files`。

## 三种类型的事件

| 类型 | 平台事件 | UI ↔ Runtime |
| --- | --- | --- |
| UI | 页面打开期间用 `app.events.on` | 没有 Runtime，不使用 IPC |
| Hybrid | 页面和运行中的 Runtime 都可监听 | 两侧使用 `ipc` |
| Headless | setup 中用 `events.on`；常驻监听需 `onStartup` | 没有 UI，通常不使用 IPC |

同一个平台事件如果在 Hybrid 两侧都监听，会执行两次业务代码。通常让 Runtime 负责后台工作，UI 只监听影响当前画面的事件。
