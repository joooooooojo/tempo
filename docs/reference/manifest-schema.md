---
title: Manifest
description: manifest.json 的入口、贡献点关系和字段规则。
---

# Manifest

`manifest.json` 位于插件包根目录。它只描述插件身份、入口和宿主能发现的能力，不负责 UI 与 Runtime 的私有 IPC，也不声明平台事件监听。

## 关系总览

```text
apps[].id <---------------- actions[].app
    |
    +----> index.html

commands[].id <------------ actions[].command
      |
      +----> tempo.commands.register(id, handler) in main.mjs

mcpTools[].name -----------> tempo.mcpTools.register(name, handler) in main.mjs

settings[] ----------------> Tempo 渲染设置界面
                              UI / Runtime 用 tempo.settings 读取
```

- **App** 是可打开的页面。
- **Command** 是 Runtime 可执行能力的名字。
- **Action** 是用户在主面板触发的操作，只能打开一个 App 或执行一个 Command。
- **MCP Tool** 声明公开工具契约，由 Runtime 的 `tempo.mcpTools.register()` 提供实现。
- **Settings** 放在这些入口之后，由宿主管理。
- 平台事件使用 `tempo.events.on()` 监听，不存在 `contributes.hooks`。

## 完整示例

```json
{
  "$schema": "https://joooooooojo.github.io/tempo/plugin-assets/releases/1.0.0/plugin-manifest.schema.json",
  "manifestVersion": 1,
  "id": "com.example.notes",
  "name": "Notes",
  "version": "1.0.0",
  "description": "管理本地笔记",
  "engines": {
    "tempo": ">=2",
    "pluginApi": "^1.0.0"
  },
  "kind": "hybrid",
  "main": "main.mjs",
  "contributes": {
    "apps": [
      {
        "id": "main",
        "name": "Notes",
        "entry": "index.html"
      }
    ],
    "commands": [
      {
        "id": "search",
        "title": "Search notes"
      }
    ],
    "actions": [
      {
        "id": "open",
        "name": "打开 Notes",
        "app": "main"
      },
      {
        "id": "search",
        "name": "搜索笔记",
        "accepts": ["text"],
        "command": "search"
      }
    ],
    "mcpTools": [
      {
        "name": "search-notes",
        "description": "按关键词搜索本地笔记",
        "inputSchema": {
          "type": "object",
          "properties": {
            "query": { "type": "string" }
          },
          "required": ["query"]
        }
      }
    ],
    "settings": [
      {
        "id": "compact",
        "type": "switch",
        "title": "紧凑模式",
        "default": false
      }
    ]
  }
}
```

## 根字段

| 字段 | 必需 | 说明 |
| --- | :---: | --- |
| `manifestVersion` | 是 | 当前固定为 `1` |
| `id` | 是 | 全局唯一的小写反向域名，如 `com.example.notes` |
| `name` | 是 | 用户看到的插件名称 |
| `version` | 是 | 插件包版本 |
| `engines.tempo` | 是 | 兼容的 Tempo 应用版本范围 |
| `engines.pluginApi` | 是 | 兼容的 Host 注入 API 版本范围 |
| `kind` | 否 | `ui`、`hybrid`、`headless`，仅用于分类 |
| `main` | 条件 | Runtime 的包内 `.js` 或 `.mjs` 相对路径 |
| `activationEvents` | 否 | 当前只支持 `onStartup`；需要 `main` |
| `capabilities` | 否 | 插件能力用途说明 |
| `contributes` | 否 | Apps、Commands、Actions、MCP Tools、Settings |

`author`、`publisher`、`description`、`homepage`、`repository`、`license`、`categories` 是可选展示信息。

插件至少需要一个 App 或一个 Runtime `main`。`kind` 不决定运行方式：有 Apps 无 main 是 UI，两者都有是 Hybrid，只有 main 是 Headless。

::: warning capabilities 不是沙箱
`filesystem`、`network`、`process`、`clipboard`、`system` 用于向用户说明用途，不会限制 Runtime 的真实 Node 权限。带 `main` 的插件仍需用户信任。
:::

## Apps

Apps 声明插件可以打开的页面。每个 App 都会成为 Tempo 可注册的页面入口。

```json
{
  "apps": [
    {
      "id": "main",
      "name": "Notes",
      "keywords": ["notes", "笔记"],
      "entry": "index.html",
      "windowMode": "normal",
      "rect": { "width": 920, "height": 580, "x": "center" },
      "sessionVersion": 1
    }
  ]
}
```

| 字段 | 必需 | 说明 |
| --- | :---: | --- |
| `id` | 是 | 插件内唯一 ID |
| `name` | 是 | 页面名称 |
| `entry` | 是 | 当前必须是包根目录的 `index.html` |
| `keywords` | 否 | 搜索关键词 |
| `icon` | 否 | 包内图标相对路径 |
| `windowMode` | 否 | `normal` 主面板或 `standalone` 独立窗口 |
| `rect` | 否 | 窗口宽、高与位置 |
| `sessionVersion` | 否 | Session 结构变化时递增，必须为正整数 |

`rect.width` 支持 `320..4096` 像素或百分比，`height` 支持 `240..2160` 像素或百分比；`x`、`y` 还支持 `center`。

## Commands

Command 先在 Manifest 声明，再由 Runtime 注册同名 handler：

```json
{
  "commands": [
    { "id": "format", "title": "Format text", "visibility": "private" }
  ]
}
```

```js
onMounted(() => {
  tempo.commands.register("format", async (params, signal) => {
    if (signal.aborted) throw new Error("cancelled");
    return { text: String(params?.text ?? "").trim() };
  });
});
```

| 字段 | 必需 | 说明 |
| --- | :---: | --- |
| `id` | 是 | 插件内唯一 Command ID |
| `title` | 是 | 宿主和开发工具显示的名称 |
| `visibility` | 否 | `private` 或 `public`，默认 `private` |

只声明不注册时，Action 调用会返回 `NOT_FOUND`。Command 本身不会自动出现在主面板，需要由 Action 引用。

## Actions

Action 是主面板中的用户操作。每个 Action 必须在 `app` 和 `command` 中二选一。

```json
{
  "actions": [
    {
      "id": "open-notes",
      "name": "打开笔记",
      "app": "main"
    },
    {
      "id": "format-text",
      "name": "格式化文字",
      "accepts": ["text"],
      "command": "format"
    }
  ]
}
```

| 字段 | 必需 | 说明 |
| --- | :---: | --- |
| `id` | 是 | 插件内唯一 Action ID |
| `name` | 是 | 主面板显示名称 |
| `keywords` | 否 | 搜索关键词 |
| `icon` | 否 | 包内图标相对路径 |
| `accepts` | 否 | `text`、`image`、`file`，默认 `text` |
| `titleTemplate` | 否 | 根据输入生成操作标题 |
| `app` | 二选一 | 引用 `apps[].id` |
| `command` | 二选一 | 引用 `commands[].id`，插件必须有 `main` |

- `app`：Tempo 打开页面，把 `{ actionId, query, input }` 放入 `window.tempo.context.params`。
- `command`：Tempo 启动 Runtime，把相同结构传给 Command handler，并等待返回结果。

Action 不会通过 `ipcMain` 执行。IPC 只服务于已经打开的插件 UI。

## MCP Tools

MCP Tool 向外部 AI 客户端声明名称、说明和参数契约。它不引用 Command。

```json
{
  "mcpTools": [
    {
      "name": "search-notes",
      "description": "按关键词搜索本地笔记",
      "inputSchema": {
        "type": "object",
        "properties": {
          "query": { "type": "string", "description": "搜索关键词" }
        },
        "required": ["query"]
      },
      "annotations": {
        "readOnlyHint": true
      }
    }
  ]
}
```

Runtime 使用 `tempo.mcpTools.register()` 注册同名实现：

```js
onMounted(() => {
  tempo.mcpTools.register("search-notes", async (params, signal) => {
    if (signal.aborted) throw new Error("cancelled");
    return { items: [] };
  });
});
```

`name` 和 `description` 必需。`inputSchema` 和可选的 `outputSchema` 必须是 JSON Schema object。每个插件最多声明 64 个 MCP Tools。

`tempo.mcpTools.register()` 只在 Runtime 中可用。名称必须与 `mcpTools[].name` 一致；它使用独立注册表，不会调用同名 Command。

从早期设计升级时，删除原来的 `mcpTools[].command`，并把对应实现改为 `tempo.mcpTools.register(tool.name, handler)`。插件开发助手会在可视化编辑后自动移除旧字段。

插件 MCP 默认关闭。用户必须在插件详情中启用插件 MCP 和具体工具，外部客户端才能调用已注册的 Tool handler。

## Settings

Settings 放在其它入口之后，由 Tempo 统一渲染配置界面：

```json
{
  "settings": [
    {
      "id": "compact",
      "type": "switch",
      "title": "紧凑模式",
      "description": "减少列表间距",
      "default": false
    }
  ]
}
```

| 类型 | 额外字段 | `default` |
| --- | --- | --- |
| `switch` | 无 | 布尔值 |
| `input` | `placeholder` 可选 | 字符串 |
| `select` | `options` | 一个选项值 |
| `multiselect` | `options` | 选项值数组 |

每个 option 需要 `value`，可选 `label`。UI 和 Runtime 使用 `tempo.settings.get()`、`getAll()` 和 `subscribe()` 读取结果，不要直接修改保留键 `__tempo/settings`。

## 平台事件不写 Manifest

Runtime 中直接监听即可，不要增加 `hooks` 配置：

```js
onMounted(() => {
  const off = tempo.events.on("clipboard.changed", console.log);
  onUnmounted(off);
});
```

广播接收规则见 [宿主事件](/reference/host-events)。

## 路径规则

包内路径必须：

- 使用 `/`，不能使用反斜杠。
- 使用相对路径，不能包含盘符、URL 或开头的 `/`。
- 不能包含 `..` 或连续的 `//`。

## JSON Schema

给 Manifest 添加 `$schema` 可以获得编辑器补全。插件开发助手不会写死一个永久的“最新版”地址，而是从远端模板目录选择兼容 release，并把该 release 对应的版本化 Schema URL 写入新项目：

- [远端模板目录](https://joooooooojo.github.io/tempo/plugin-assets/catalog.json)
- [Manifest Schema 1.0.0](https://joooooooojo.github.io/tempo/plugin-assets/releases/1.0.0/plugin-manifest.schema.json)
- [仓库中的 Schema 源文件](https://github.com/joooooooojo/tempo/blob/master/docs/schemas/plugin-manifest.schema.json)

Schema 与模板一起独立发布。现有项目保留创建时的版本化地址，不会因为远端更新突然改变校验规则。
