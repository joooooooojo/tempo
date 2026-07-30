---
title: 加入后台能力
description: 使用 ipcMain、ipcRenderer 和 Commands 为插件增加 Runtime。
---

# 加入后台能力

Runtime 是独立的 Node 进程。读写任意文件、耗时计算、Action 命令和 MCP Tool 应放在 Runtime；普通页面展示和交互留在 UI。

## 先分清四条通道

| 通道 | 用途 | Manifest 声明 |
| --- | --- | --- |
| `tempo.*` | UI 或 Runtime 调用 Tempo 平台能力 | 不需要 |
| `window.ipcRenderer` ↔ `globalThis.ipcMain` | 本插件 UI 与 Runtime 私有通信 | 不需要 |
| `tempo.commands.register` | 让 Action 执行 Runtime Command | 需要 `commands` |
| `tempo.mcpTools.register` | 注册 MCP Tool 的 Runtime 实现 | 需要 `mcpTools` |

平台广播是第四类输入：使用 `tempo.events.on(...)` 监听，不经过 IPC 或 Command，也不写进 Manifest。

## Runtime 入口

Hybrid 和 Headless 模板会把 TypeScript Runtime 构建成 `dist/main.mjs`。发布包的 Manifest 写：

```json
{
  "main": "main.mjs"
}
```

Hybrid 模板把两侧代码与类型环境完全分开：

```text
src/
  ui/
    main.ts
    style.css
    tempo.d.ts
    tsconfig.json       # DOM 与 window.tempo / ipcRenderer
  runtime/
    main.ts
    tempo.d.ts          # Runtime-only 宿主全局类型
    tsconfig.json       # Node.js 与 tempo / ipcMain / 生命周期
```

两套 `tsconfig.json` 不共享全局类型。UI 侧不会获得 Node.js 类型，Runtime 侧也不会获得 DOM 类型；`pnpm build` 会分别检查两边再生成插件包。

开发助手新建的项目默认监听项目内的 `dist/main.mjs`。Hybrid 开发时通常同时运行：

```bash
pnpm dev
pnpm dev:runtime
```

第一个命令启动 UI，第二个命令持续重建 Runtime。Headless 模板只需运行 `pnpm dev`。

## 页面调用 Runtime

Runtime 直接在入口文件注册处理器：

```ts
function formatNote(input: { text?: string } = {}) {
  return { text: String(input.text ?? "").trim() };
}

onMounted(() => {
  ipcMain.handle("format-note", async (event, input) => {
    const result = formatNote(input);
    event.sender.send("note-formatted", result);
    return result;
  });
});
```

UI 调用它：

```ts
await window.tempo.ready();
const result = await window.ipcRenderer.invoke("format-note", {
  text: "  hello  ",
});
console.log(result.text);
```

单向消息使用 `send/on`：

```ts
// UI
window.ipcRenderer.send("editor-changed", { dirty: true });
const off = window.ipcRenderer.on("saved", (_event, payload) => {
  console.log(payload);
});

// Runtime
ipcMain.on("editor-changed", (event, payload) => {
  console.log(payload.dirty);
  event.sender.send("saved", { at: Date.now() });
});
```

IPC 参数与返回值使用 Structured Clone，可以包含 `Date`、`Map`、`Set`、TypedArray 等值，不能包含函数、Promise 或 DOM 节点。

## 让 Action 执行 Command

先在 Manifest 声明 Command：

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

再在 Runtime 注册同名实现：

```ts
onMounted(() => {
  tempo.commands.register("format-note", async (params, signal) => {
    if (signal.aborted) throw new Error("cancelled");
    return formatNote(params);
  });
});
```

`ipcMain.handle("format-note")` 和 `tempo.commands.register("format-note")` 即使同名也属于不同通道。前者只给插件 UI 使用，后者只由 Tempo 的 Action 调用。

## 注册 MCP Tool

Manifest 只声明工具名称、说明和 Schema，不引用 Command：

```json
{
  "mcpTools": [
    {
      "name": "format-note",
      "description": "去除文字首尾空白",
      "inputSchema": {
        "type": "object",
        "properties": {
          "text": { "type": "string" }
        }
      }
    }
  ]
}
```

Runtime 使用同名 `tempo.mcpTools.register()` 注册实现：

```ts
onMounted(() => {
  tempo.mcpTools.register("format-note", async (params, signal) => {
    if (signal.aborted) throw new Error("cancelled");
    return formatNote(params);
  });
});
```

`tempo.mcpTools.register` 只存在于 Runtime。UI 的 `window.tempo` 没有 `mcpTools`。Tool 名称必须与 `mcpTools[].name` 一致；只声明不注册时，调用返回 `NOT_FOUND`。

## 监听平台事件

```ts
onMounted(() => {
  const off = tempo.events.on("clipboard.changed", (payload) => {
    console.log(payload.at);
  });
  onUnmounted(off);
});
```

广播只发给已经运行的 Runtime 和当前打开的页面，不会为了事件启动已停止的 Runtime。需要常驻监听时，在 Manifest 根字段添加：

```json
{ "activationEvents": ["onStartup"] }
```

一次性监听、按处理器移除和批量清理见 [宿主事件](/reference/host-events#监听方法)。

## 发布与信任

Tempo 不会替插件安装依赖或编译 TypeScript。模板的 Vite 配置会把依赖打进 `main.mjs`，并把 Manifest 复制到 `dist`。

带 `main` 的插件需要用户安装插件 Node 运行时、信任插件并启用。Runtime 权限接近本机 Node 进程，`capabilities` 是用途说明，不是安全沙箱。
