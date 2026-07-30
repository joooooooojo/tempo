---
title: API 参考
description: 按职责查找插件全局 API、平台 API、宿主事件和 Manifest。
---

# API 参考

第一次开发插件，请先完成 [做出第一个插件](/developer/first-plugin)。本节用于按方法和字段查阅。

## 按问题查

| 你在找什么 | 页面 |
| --- | --- |
| UI 和 Runtime 注入了哪些全局变量 | [插件全局 API](/reference/plugin-api) |
| `tempo.storage`、通知、主题、窗口等参数 | [平台 API](/reference/plugin-host-api) |
| `tempo.events` 支持哪些平台广播 | [宿主事件](/reference/host-events) |
| Apps、Commands、Actions、MCP Tools、Settings | [Manifest](/reference/manifest-schema) |
| UI、Hybrid、Headless 与入口格式 | [插件类型与生命周期](/developer/plugin-lifecycle) |

## 不要混用四条通道

| 通道 | API | 调用方向 |
| --- | --- | --- |
| 平台能力与广播 | `tempo.*` | 插件 ↔ Tempo |
| 插件私有 IPC | `ipcRenderer` / `ipcMain` | 插件 UI ↔ 插件 Runtime |
| Action Command | `tempo.commands.register` | Action → Runtime |
| MCP Tool | `tempo.mcpTools.register` | MCP → Runtime |

`tempo.events.on()` / `once()` 不会收到 `ipcMain.send()` 的消息，`ipcRenderer.on()` 也不会收到平台广播。

## 当前协议版本

- Manifest 格式：`1`
- Host API：`1.0.0`
- 常规调用超时：30 秒
- 面板与窗口调用超时：5 秒
- 单次消息上限：约 1 MiB
- 每个插件最多并发 Host 请求：32
