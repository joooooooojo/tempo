---
title: 插件开发
description: 从插件类型、项目模板和运行边界开始开发 Tempo 插件。
---

# 插件开发

Tempo 已经把插件 API 注入运行环境。插件不需要安装额外的 Tempo 包或创建包装对象，在入口文件里直接使用全局 API 即可。

## 先选择插件类型

| 需求 | 类型 | 组成 |
| --- | --- | --- |
| 在 Tempo 中显示页面 | UI | `index.html` |
| 搜索后直接执行任务，或提供 MCP Tool | Headless | `main.mjs` |
| 页面还需要后台计算、文件或网络能力 | Hybrid | `index.html` + `main.mjs` |

不确定时先选 UI。只有页面确实需要 Node 能力或后台任务时，再增加 Runtime。

插件开发助手可以直接创建三套 Vite 模板。创建时会从远端目录选择当前 Host API 能使用的最新版本，并缓存校验通过的文件；模板更新不要求更新 Tempo。模板源代码位于 `src`，`pnpm build` 生成的 `dist` 可以直接导入 Tempo：

| 模板 | `dist` 产物 |
| --- | --- |
| UI · Vite | `index.html`、静态资源、`manifest.json` |
| Hybrid · Vite | UI 产物、`main.mjs`、`manifest.json` |
| Headless · Vite | `main.mjs`、`manifest.json` |

新项目的 Manifest 会自动带上该模板版本协商得到的远端 `$schema`。第一次创建项目需要联网；之后网络不可用时可以使用已成功缓存的版本。

## 四个概念

```text
manifest.json
  apps --------> 可打开的 UI 页面
  commands ----> Runtime 中注册的能力
  actions -----> 打开一个 app，或执行一个 command
  mcpTools ----> MCP 工具的公开契约
  settings ----> 由 Tempo 渲染的插件设置

UI ------ window.ipcRenderer ------ globalThis.ipcMain ------ Runtime
 |                                                          |
 +---------------- window.tempo / globalThis.tempo ----------+
                         Tempo 平台 API
```

- `tempo`：调用通知、存储、设置、主题等平台能力。UI 和 Runtime 都有，各自只暴露适合当前位置的方法。
- `ipcRenderer` / `ipcMain`：只在本插件的 UI 与 Runtime 之间传消息，不写进 Manifest。
- `commands`：Runtime 对 Action 提供的可执行能力，使用 `tempo.commands.register()` 注册。
- `mcpTools`：Manifest 声明工具契约，Runtime 使用同名 `tempo.mcpTools.register()` 注册实现，不经过 Commands。
- `tempo.events`：监听平台广播，不写进 Manifest，也不经过 Command。

## 推荐阅读顺序

1. [做出第一个插件](/developer/first-plugin)：从 UI · Vite 模板开始。
2. [插件类型与生命周期](/developer/plugin-lifecycle)：理解三种插件、入口格式和启停时机。
3. [加入后台能力](/developer/runtime)：实现 Runtime、IPC 和 Command。
4. [插件全局 API](/reference/plugin-api)：查 UI 与 Runtime 注入了什么。
5. [平台 API](/reference/plugin-host-api)：查 `tempo` 的方法和参数。
6. [Manifest](/reference/manifest-schema)：查 Apps、Commands、Actions、MCP Tools 和 Settings。

## 版本字段

| 字段 | 当前值 | 作用 |
| --- | --- | --- |
| Tempo | `2.0.1` | 宿主应用版本 |
| `manifestVersion` | `1` | Manifest 文件格式 |
| `engines.pluginApi` | `^1.0.0` | Host 注入 API 的兼容范围 |
| `version` | 由插件维护 | 当前插件包版本 |

插件 API 不再作为单独 npm 包发布，也不跟随 Tempo 应用版本同步。插件只需在 `engines.pluginApi` 中声明自己依赖的 Host API 范围。

::: info 内部设计文档
`docs/design` 记录 Tempo 平台实现决策，面向仓库贡献者，不是插件入门教程。
:::
