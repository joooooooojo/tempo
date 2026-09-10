---
title: 加入后台能力
description: 使用 defineRuntime、IPC 和 Commands 为插件增加 Runtime。
---

# 加入后台能力

Runtime 是独立的受限 Deno 2.9.6 进程。全局文件操作、耗时计算、Action Command 和 MCP Tool 放在 Runtime；普通页面展示和交互留在 UI。

## 四条通道

| 通道 | 用途 | Manifest 声明 |
| --- | --- | --- |
| UI Client / Runtime Context | 调用 Tempo 平台能力 | 不需要 |
| `ipc` | 本插件 UI 与 Runtime 私有通信 | 不需要 |
| `commands.register` | 让 Action 执行 Runtime Command | 需要 `commands` |
| `mcpTools.register` | 注册 MCP Tool 的实现 | 需要 `mcpTools` |

平台广播使用 `events.on(...)` 监听，不经过 IPC 或 Command，也不写进 Manifest。

## 项目结构

Hybrid 模板把 UI 和 Runtime 的类型环境分开，并共享 IPC 类型：

```text
tsconfig.json
tsconfig.ui.json
tsconfig.runtime.json
src/
  ipc.ts
  ui/main.ts
  runtime/main.ts
```

根 `tsconfig.json` 通过 references 组织两个子项目。`pnpm typecheck` 使用 `tsc -b` 检查，UI 不会获得 Node 类型，Runtime 也不会获得 DOM 类型。

开发时通常同时运行：

```bash
pnpm dev
pnpm dev:runtime
```

第一个命令启动 UI，第二个持续构建 `dist/main.mjs`。Headless 模板只需运行 `pnpm dev`。

## 定义 Runtime

```ts
import { defineRuntime } from "tempo-plugin-sdk/runtime";

defineRuntime(({ commands, events, onDispose }) => {
  commands.register("status", async () => ({ running: true }));

  const timer = setInterval(() => console.log("tick"), 60_000);
  onDispose(() => clearInterval(timer));
  onDispose(events.on("clipboard.changed", console.log));
});
```

setup 在宿主挂载 Runtime 时执行，可以是异步函数。每次 `onDispose()` 注册一个清理项；setup 还可直接返回清理函数。停止时 SDK 按注册的相反顺序执行，并在一个清理项失败后继续处理其余项。

## 页面调用 Runtime

先声明共享契约：

```ts
// src/ipc.ts
export type PluginIpc = {
  invokes: {
    "format-note": (input: { text?: string }) => { text: string };
  };
  messages: {
    "editor-changed": [payload: { dirty: boolean }];
    saved: [payload: { at: number }];
  };
};
```

Runtime 实现频道：

```ts
import { defineRuntime } from "tempo-plugin-sdk/runtime";
import type { PluginIpc } from "../ipc.js";

defineRuntime<PluginIpc>(({ ipc }) => {
  ipc.handle("format-note", async (_event, input) => ({
    text: String(input.text ?? "").trim(),
  }));

  ipc.on("editor-changed", (event, payload) => {
    if (payload.dirty) event.sender.send("saved", { at: Date.now() });
  });
});
```

UI 调用同一个契约：

```ts
import { connect } from "tempo-plugin-sdk/ui";
import type { PluginIpc } from "../ipc.js";

const app = await connect<PluginIpc>();
const result = await app.ipc.invoke("format-note", { text: "  hello  " });

app.ipc.send("editor-changed", { dirty: true });
const offSaved = app.ipc.on("saved", (_event, payload) => {
  console.log(payload.at);
});
```

IPC 参数与返回值使用 Structured Clone，可以包含 `Date`、`Map`、`Set` 和 TypedArray，不能包含函数、Promise 或 DOM 节点。省略 IPC 泛型时可以使用任意字符串频道。

## Action Command

先在 Manifest 声明：

```json
{
  "commands": [
    { "id": "format-note", "title": "Format note" }
  ],
  "actions": [
    {
      "id": "format-current-note",
      "name": "格式化文字",
      "accepts": ["text"],
      "command": "format-note"
    }
  ]
}
```

再注册实现：

```ts
defineRuntime(({ commands }) => {
  commands.register("format-note", async (params, signal) => {
    if (signal.aborted) throw new Error("cancelled");
    return { text: String(params.input.text).trim() };
  });
});
```

`ipc.handle("format-note")` 和 `commands.register("format-note")` 即使同名也属于不同通道。前者只给插件 UI 使用，后者由 Tempo Action 调用。

## MCP Tool

Manifest 声明工具名称、说明和 JSON Schema，Runtime 注册同名实现：

```ts
defineRuntime(({ mcpTools }) => {
  mcpTools.register("format-note", async (params, signal) => {
    if (signal.aborted) throw new Error("cancelled");
    return { text: String(params.text ?? "").trim() };
  });
});
```

MCP Tool 使用独立注册表，不会调用同名 Command。只声明但没有注册时，调用返回 `NOT_FOUND`。

## 平台事件

```ts
defineRuntime(({ events, onDispose }) => {
  onDispose(events.on("clipboard.changed", (payload) => {
    console.log(payload.at);
  }));
});
```

广播只发给已经运行的 Runtime 和当前打开的页面。需要常驻监听时，在 Manifest 根字段添加：

```json
{ "activationEvents": ["onStartup"] }
```

## 权限

带 `main` 的插件需要安装 Deno Runtime、信任插件并启用。Deno 敏感权限默认全部关闭：

```json
{
  "permissions": ["read", "write", "net", "env"]
}
```

八项权限是 `read`、`write`、`net`、`env`、`sys`、`run`、`ffi` 和 `import`。每项都允许对应能力的全局访问；八项全选等价于 Deno `-A`。`net` 同时控制 Runtime 与 Tempo 托管插件 UI 的网络。

Host API 无需 Manifest 授权。`storage` 和 `files` 始终可用，其中 `files` 限定在当前插件数据目录。只有直接调用 Deno 或 Node 兼容文件 API 时才需要 `read` / `write`。

## 构建与发布

Tempo 不会为插件安装依赖或编译 TypeScript。模板的 Vite 配置会把 SDK 和其它依赖内联到 `main.mjs`，并复制 Manifest 到 `dist`。

Node/npm/TypeScript/Vite 负责构建；最终后台代码由 Deno 执行。动态 require、外置 `node_modules`、Electron API、`.node` 和 Node 原生可执行模块不受支持。
