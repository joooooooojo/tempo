---
title: 加入后台能力
description: 使用 ipcMain、ipcRenderer 和 Commands 为插件增加 Runtime。
---

# 加入后台能力

Runtime 是独立的受限 Deno 2.9.6 进程。已授权的数据文件操作、耗时计算、Action 命令和 MCP Tool 放在 Runtime；普通页面展示和交互留在 UI。

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
tsconfig.json           # references 统一组织两个子项目
tsconfig.ui.json        # DOM 与 window.tempo / ipcRenderer
tsconfig.runtime.json   # Node 兼容类型与 tempo / ipcMain / 生命周期
src/
  ui/
    main.ts
    style.css
    tempo.d.ts
  runtime/
    main.ts
    tempo.d.ts          # Runtime-only 宿主全局类型
```

根 `tsconfig.json` 通过 `references` 引用 `tsconfig.ui.json` 和 `tsconfig.runtime.json`，`pnpm typecheck` 使用 `tsc -b` 统一检查。两侧不共享全局类型：UI 不会获得 Node.js 类型，Runtime 也不会获得 DOM 类型。`pnpm build` 先完成统一类型检查，再由 Vite 生成插件包；TypeScript 增量缓存位于 `node_modules/.cache`。

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

带 `main` 的插件需要安装 Deno 运行时、信任插件并启用。Manifest 必须为 v2，Deno 权限默认拒绝。

```json
{
  "permissions": {
    "read": ["$DATA"],
    "write": ["$DATA"],
    "net": ["api.example.com:443"],
    "env": ["PLUGIN_API_KEY"]
  }
}
```

`$DATA` 对应 `tempo.paths.data`；包文件读取和专用 IPC 端点由宿主授予。使用 `tempo.storage` 或 `tempo.files` 不需要 Deno 磁盘权限，其中 `tempo.files` 始终把路径限定在当前插件的数据目录。网络只接受明确的 `host:port`，同一白名单同时用于 Deno Runtime 和 Tempo 托管的插件 UI；环境变量按声明从宿主启动环境中传入，保留的运行时配置变量不可声明。Tempo Host API 不需要 Manifest 授权，仍会执行参数、调用位置、私有目录边界和 URL scheme 等接口校验。

Deno 2.9 提供 `read`、`write`、`net`、`env`、`sys`、`run`、`ffi` 和 `import` 权限开关。细粒度模式只开放声明的 `read`、`write`、`net`、`env`，`sys`、子进程、FFI、远程导入和运行时 npm 下载保持关闭。

确实需要全部 Deno 能力时，可以单独声明：

```json
{
  "permissions": {
    "all": true
  }
}
```

`all: true` 使用 Deno `-A`，允许读取和写入任意文件、访问任意网络和环境变量、系统信息、子进程、FFI、远程模块及运行时 npm 包，并让 Runtime 继承宿主环境。它不能与 `read`、`write`、`net` 或 `env` 同时声明；托管 UI 的网络访问也会完全开放，但远程脚本、样式和 iframe 仍由 CSP 禁止。

Node/npm/TypeScript/Vite 仍用于构建。内联纯 JS npm 依赖的 ESM `main.mjs` 可以直接在 Deno 执行；动态 require、外置 node_modules、Electron API、`.node` 和 Node 打包的原生 exe 不在支持范围。对最终产物进行测试，不以打包成功作为兼容证明。

Tempo 托管的插件 UI 通过 CSP 使用同一网络策略；连接到外部开发服务器的 UI 由开发服务器自身策略负责。Deno 的权限不是 OS 级隔离，`all: true` 等同于信任插件代码以当前用户权限执行。只安装信任来源的插件。
