# Tempo 插件开发指南

本文面向希望为 Tempo 开发第三方插件的开发者，描述当前已经实现的插件格式、运行方式和调试流程。

- Host API 版本：`1.2.0`
- Manifest 版本：`1`
- [Host API 参考](./plugin-host-api.md)
- [manifest.json JSON Schema](./schemas/plugin-manifest.schema.json)
- [Hello 示例插件](../examples/plugins/com.example.hello/)

## 1. 插件由什么组成

Tempo 插件可以只包含 UI、只包含 Runtime，也可以同时包含两者。

| 类型 | 必要文件 | 能力 |
|---|---|---|
| UI | `manifest.json`、`index.html` | 在主面板或独立窗口中显示页面，调用受限 Host API |
| Headless | `manifest.json`、`main.mjs` 或 `main.js` | 在独立 Node 进程中注册命令、处理 Hook 或 MCP 调用 |
| Hybrid | UI 文件和 Runtime 文件 | UI 调用 Runtime 命令，适合完整工具 |

含 `main` 的插件需要用户在“设置 → 插件”中安装 Tempo 插件 Node Runtime。纯 UI 插件不需要 Node Runtime。

插件包的推荐结构：

```text
com.example.my-plugin/
  manifest.json
  index.html
  index.js
  main.mjs
  icons/
    app.svg
```

入口约束：

- `manifest.json` 必须位于包根目录。
- 贡献 `apps` 时，`index.html` 必须位于包根目录，`apps[].entry` 固定为 `index.html`。
- `main` 只能是包根目录的 `main.mjs` 或 `main.js`。
- 路径必须使用 `/`，不能包含绝对路径、URL、盘符、`..`、反斜杠或连续 `/`。

## 2. 创建最小 UI 插件

`manifest.json`：

```json
{
  "$schema": "../../../docs/schemas/plugin-manifest.schema.json",
  "manifestVersion": 1,
  "id": "com.example.notes",
  "name": "Notes",
  "version": "1.0.0",
  "engines": {
    "tempo": ">=1.2.0",
    "pluginApi": "^1.2.0"
  },
  "kind": "ui",
  "contributes": {
    "apps": [
      {
        "id": "main",
        "name": "Notes",
        "keywords": ["notes", "笔记"],
        "entry": "index.html",
        "windowMode": "normal",
        "rect": {
          "width": 920,
          "height": 580,
          "x": "center"
        }
      }
    ]
  }
}
```

`index.html`：

```html
<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Notes</title>
  </head>
  <body>
    <textarea id="notes"></textarea>
    <script src="./index.js"></script>
  </body>
</html>
```

Tempo 会在插件页面加载前注入 `window.plugin`。插件不应自行引入 Tauri API，也不需要单独安装 UI SDK。

`index.js`：

```js
(async () => {
  const context = await window.plugin.ready();
  const notes = document.querySelector("#notes");

  const stored = await window.plugin.host("storage.plugin.get", { key: "notes" });
  notes.value = typeof stored.value === "string" ? stored.value : "";

  notes.addEventListener("input", () => {
    void window.plugin.host("storage.plugin.set", {
      key: "notes",
      value: notes.value,
    });
    void window.plugin.host("session.push", {
      payload: { selectionStart: notes.selectionStart },
    });
  });

  if (context.session?.selectionStart) {
    notes.selectionStart = context.session.selectionStart;
  }
})();
```

## 3. 添加 Runtime 命令

先在 manifest 中声明命令和调用入口：

```json
{
  "main": "main.mjs",
  "contributes": {
    "commands": [
      {
        "id": "format-note",
        "title": "Format note",
        "visibility": "private"
      }
    ],
    "actions": [
      {
        "id": "format-current-note",
        "name": "格式化笔记",
        "keywords": ["format", "格式化"],
        "accepts": ["text"],
        "command": "format-note"
      }
    ]
  }
}
```

然后在 `main.mjs` 的 `activate` 中注册同名命令：

```js
export async function activate(ctx) {
  ctx.registerCommand("format-note", async (params, signal) => {
    if (signal.aborted) throw new Error("cancelled");
    const text = params?.input?.kind === "text" ? params.input.text : "";
    await ctx.host.notify.show({ title: "Notes", body: "格式化完成" });
    return { text: text.trim() };
  });
}

export async function deactivate() {
  // 可选：关闭 socket、文件句柄等资源。
}
```

UI 调用 Runtime：

```js
const result = await window.plugin.invoke("format-note", { text: "  hello  " });
console.log(result.text);
```

Runtime 运行在独立 Node 进程中，拥有与本机 Node 相近的文件、网络和进程能力。它不是安全沙箱，因此用户启用含 `main` 的插件前必须明确授予信任。

## 4. Manifest 贡献点

### apps

向主面板注册可打开的应用页面。

```json
{
  "id": "main",
  "name": "My App",
  "keywords": ["tool"],
  "icon": "icons/app.svg",
  "entry": "index.html",
  "windowMode": "normal",
  "rect": { "width": 920, "height": 580 },
  "sessionVersion": 1
}
```

- `windowMode: "normal"`：嵌入主面板，默认值。
- `windowMode: "standalone"`：使用带系统标题栏、任务栏或 Dock 图标的独立窗口。
- `sessionVersion`：插件 session payload 的兼容版本，可省略，默认 `1`。

应用页默认保留。失焦、再次按全局快捷键或调用 `mainPanel.hide` 只隐藏主面板；重新打开仍停留在原应用页。只有返回按钮或 Esc 会退出应用页并回到搜索。

### actions 与 commands

`commands` 声明 Runtime 能力；`actions` 是主面板搜索时的上下文推荐入口。有文本或图片
输入时，主面板先展示匹配的应用/插件，再按 `accepts` 展示匹配的推荐 action。插件开发者
可为每个 action 选择打开 UI `app`，或执行无界面的 Runtime `command`。
每个 action 必须在 `app` 与 `command` 中二选一。

打开图片裁剪 UI：

```json
{
  "id": "crop-image",
  "name": "裁剪图片",
  "keywords": ["crop", "裁剪"],
  "icon": "icons/crop.svg",
  "accepts": ["image"],
  "app": "crop"
}
```

当主面板输入中存在图片时，这个 action 会出现在搜索结果的「推荐操作」中。选择后，目标 app 的
`window.plugin.ready()` context 会收到：

```js
{
  actionId: "crop-image",
  query: "",
  input: {
    kind: "image",
    entryId: 42,
    imageUrl: "tempo-clipboard-image://localhost/...png",
    width: 1920,
    height: 1080
  }
}
```

UI 可以将 `imageUrl` 用于 `<img>`、Canvas 或图片编辑器。文字输入的结构为
`{ kind: "text", text: "..." }`；无输入为 `{ kind: "none" }`。

直接执行 Runtime command：

```json
{
  "id": "compress-image",
  "name": "压缩图片",
  "accepts": ["image"],
  "command": "compress"
}
```

当 action 目标是 Runtime command 时，宿主会校验图片记录并额外提供
`input.filePath`，Node 可以直接读取该 PNG。UI app 不会收到本机路径。

```text
用户选择 action
  ├─ action.app     → 打开插件 UI，invocation 作为 context.params
  └─ action.command → Runtime 执行 handler，invocation 作为 params
```

`accepts` 支持 `text`、`image`，默认是 `["text"]`。它既表示 action 能消费的输入，
也决定推荐时机。兼容字段 `requiresQuery` 仍可读取，但一律映射为 `["text"]`；新插件应使用
`accepts`。

一个 command 可以被多个 action、hook 或 MCP tool 复用。仅声明 command 不会自动产生主面板入口。
`query` 保留用于兼容旧 action 和标题模板，实际输入以 `input` 为准。`visibility` 默认是
`private`；Host API `1.2.0` 的 UI、Action、Hook 和 MCP 调用都只路由到当前插件。
`public` 为后续跨插件调用保留，当前没有向第三方插件开放跨插件 command 入口。

### hooks

Hook 将宿主事件映射到 Runtime command。目前已实现的事件：

| 事件 | Payload |
|---|---|
| `clipboard.changed` | `{ schemaVersion: 1, at: string }`，不包含剪贴板正文 |

```json
{
  "hooks": [
    { "event": "clipboard.changed", "command": "on-clipboard-changed" }
  ]
}
```

Hook 是异步触发，不等待插件结果；失败只写入宿主日志。

需要在 Tempo 启动时提前激活 Runtime，可在根级声明 `"activationEvents": ["onStartup"]`。没有 `main` 的纯 UI 插件不能使用该字段。其它 Runtime 默认在第一次 command、Hook 或 MCP 调用时懒启动。

### mcpTools

将 Runtime command 映射成 MCP 工具：

```json
{
  "mcpTools": [
    {
      "name": "summarize-note",
      "description": "Summarize a stored note",
      "command": "summarize-note",
      "inputSchema": {
        "type": "object",
        "properties": {
          "id": { "type": "string" }
        },
        "required": ["id"]
      }
    }
  ]
}
```

MCP 工具不会默认暴露。用户必须在 Tempo 插件设置中单独开启该插件的 MCP 暴露开关。

## 5. 窗口尺寸和位置

`rect` 的所有字段都可省略：

| 字段 | 类型 | 默认值 |
|---|---|---|
| `width` | `320..4096` 像素或 `1%..100%` | `800`（快捷搜索面板宽度） |
| `height` | `240..2160` 像素或 `1%..100%` | `580` |
| `x` | 像素、`0%..100%` 或 `center` | 横向居中 |
| `y` | 像素、`0%..100%` 或 `center` | 与主面板顶部位置一致 |

百分比尺寸相对当前显示器工作区。百分比位置相对“工作区尺寸减去最终窗口尺寸”的可移动范围，因此 `0%` 靠左或上，`100%` 靠右或下。`center` 会使用最终解析后的宽高计算。

独立窗口可在运行时调整：

```js
await window.plugin.host("window.setRect", {
  width: "80%",
  height: 560,
  x: "center",
  y: "10%",
});
```

## 6. 页面上下文与会话

```js
const context = await window.plugin.ready();
```

`context` 包含：

```ts
interface PluginContext {
  apiVersion: string;
  theme: string;
  params: unknown;
  session: unknown | null;
}
```

- `params` 来自打开应用时传入的参数。
- `session` 是同插件、同 app、同插件版本和同 `sessionVersion` 保存的 payload。
- 插件通过 `session.push` 主动提交最新 payload。
- payload 最大 64 KiB，不适合存储密码、令牌或大型数据。
- 长期数据应使用 `storage.plugin.*` 或 Runtime 的 `ctx.paths.data`。

## 7. 导入、更新与调试

1. 打开“设置 → 插件”。
2. 含 `main` 时先安装插件 Node Runtime。
3. 点击“导入目录”并选择插件根目录，或导入 `.zip`。
4. 检查包 hash 后点击“信任”。
5. 打开启用开关，在主面板搜索贡献的 app 或 action。

更新插件时必须增加 `version`。Tempo 将 `{id, version}` 视为不可变包；同版本但内容 hash 不同会拒绝导入。

常见排查顺序：

- manifest 是否通过 JSON Schema 检查。
- `action/hook/mcpTool.command` 是否引用了同包 `commands[].id`。
- `main` 和 `index.html` 是否位于包根目录。
- manifest 声明的 command 是否在 `activate` 中注册。
- `engines.pluginApi` 是否包含当前 Host API `1.2.0`。
- 插件是否已信任、启用，含 `main` 时 Node Runtime 是否已安装。

## 8. 发布前检查

- 使用反向域名插件 ID，避免 `builtin` 和 `tempo` 保留命名空间。
- 不在包中放置密钥、访问令牌或用户数据。
- 不依赖安装目录可写；持久数据写入 `ctx.paths.data`。
- command handler 响应 `AbortSignal`，避免超过默认 30 秒超时。
- UI 捕获并展示 `RpcError.code` 与 `message`。
- 更新包内容时同步增加插件版本。
- 在 Windows 和 macOS 上分别检查百分比窗口、DPI 和任务栏或 Dock 行为。
