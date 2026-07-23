# Tempo

<div align="center">

<img src="./public/favicon.png" alt="Tempo" width="120">

**高性能、可扩展的桌面快捷面板与插件平台**

_以 Rust + Tauri 为宿主，统一搜索启动本机应用、官方能力与第三方插件_

[![GitHub release](https://img.shields.io/github/v/release/joooooooojo/tempo)](https://github.com/joooooooojo/tempo/releases)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS-blue)](https://github.com/joooooooojo/tempo)

</div>

---

## ✨ 特性

- 🧩 **插件平台** — UI 插件与无界面插件共用 `manifest.json` 与 Host Bridge；本地目录/压缩包导入、显式信任、可选独立 Node Runtime，进程隔离负责生命周期与稳定性
- ⚡ **高性能宿主** — Tauri 2 + Rust 承担 IPC、剪贴板监听、前台应用统计与系统调用；主进程后台驻留，快捷面板与选择器按需弹出，内存与体积显著低于典型 Electron 启动器
- 🚀 **快速启动** — 全局快捷键唤起面板，拼音/关键词搜索内置应用、已启用插件、本机程序与快捷操作；支持固定、最近使用与会话恢复
- 📋 **效率扩展** — 自带剪贴板历史、快捷短语、待办与番茄、屏幕时间等官方扩展，能力边界由插件生态扩展
- 🤖 **MCP** — 本机 `127.0.0.1` HTTP 服务，Bearer 鉴权，供 Cursor 等客户端操作待办、短语、剪贴板、番茄钟与日报
- 🎨 **主题与体验** — 亮/暗/跟随系统，托盘驻留、快捷键可配置、GitHub Releases 自动更新
- 🔒 **本地优先** — SQLite 离线存储，不上传使用数据；第三方插件仅在用户确认信任后执行本机代码

> 安装包内的待办、番茄钟、屏幕时间、Hosts、端口管理、翻译等属于**官方内置扩展**，用于演示面板与宿主 API；长期新能力以插件为主。

## 🚀 快速开始

### 安装

**方式 1：预编译包（推荐）**

从 [Releases](https://github.com/joooooooojo/tempo/releases) 下载对应平台的安装包或压缩包。

**方式 2：源码构建**

```bash
git clone https://github.com/joooooooojo/tempo.git
cd tempo

pnpm install   # 或 npm install
pnpm dev       # 或 npm run dev
```

构建安装包：

```bash
npm run build
# 输出：src-tauri/target/release/bundle/
```

### 使用

1. 启动后驻留系统托盘；默认 **Alt + Space**（macOS：**⌥ + Space**）打开快捷面板  
2. 输入关键词搜索，方向键选择，**Enter** 打开，**Esc** 关闭（面板可见时）  
3. 默认可配置的全局快捷键（设置 → 快捷键）：  
   - **Ctrl + Shift + V** — 剪贴板选择器  
   - **Ctrl + Shift + S** — 快捷短语选择器  

### 环境要求（仅源码构建）

- Node.js 18+、Rust toolchain  
- Windows 10/11 或 macOS 12+  
- Windows 另需 [VS Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)（C++）与 [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)

## 🧩 插件平台

Tempo 的定位是**可扩展宿主**：内置应用与第三方插件在快捷面板中使用同一套「应用 / 快捷操作」模型（`builtin` 与 `plugin` 仅来源不同）。

### 架构要点

| 概念 | 说明 |
|------|------|
| **声明式清单** | 导入时只解析 `manifest.json`，注册面板入口与快捷操作，不执行插件代码 |
| **信任** | 用户确认后才视为信任包；含 `main` 的插件会提示其权限接近 Tempo 本体（读写文件、网络、起进程等） |
| **启用** | 开关控制是否向面板注册贡献；Runtime **懒启动**，首次 `invoke` 或需要时才拉起 Node 进程 |
| **Host Bridge** | 插件 UI 不直连 Tauri IPC；通过 `window.plugin.host(...)` 走宿主鉴权路由 |
| **Runtime** | 声明 `main.mjs` / `main.js` 的插件在独立 Node 进程中运行；Supervisor 负责启停与清理 |
| **安全模型** | 信任模型而非沙箱：恶意插件无法被能力列表完全拦住，请只安装可信来源 |

### 包结构与校验

插件包根目录（与 `manifest.json` 同级）约定：

```text
com.example.myplugin/
  manifest.json
  index.html          # 有 contributes.apps 时必填；apps[].entry 必须为 index.html
  index.js            # UI 脚本，名称自定，由 index.html 引用
  main.mjs            # 可选；无 UI 的纯后台插件则必填 main.mjs 或 main.js
  icons/...
```

- **纯 UI**：无 `main`，不需安装插件 Node 运行时  
- **混合 / 无界面**：有 `main` 时须在设置 → 插件中安装 **插件 Node 运行时**  

`manifest.json` 常用字段：`id`（如 `com.example.hello`）、`name`、`version`、`engines.tempo` / `engines.pluginApi`、`main`（可选）、`contributes.apps` / `actions` / `commands` / `mcpTools`（可选）等。完整示例见仓库 `examples/plugins/com.example.hello/manifest.json`。

### 安装与试用（Hello 示例）

1. 设置 → 插件 → 安装 **插件 Node 运行时**（仅含 `main` 的插件需要）  
2. **导入目录** → 选择 `examples/plugins/com.example.hello`  
3. 导入后为**未信任、已禁用**；点击 **信任** → 打开 **启用**  
4. 面板搜索「Hello 示例」或快捷操作「Hello 一下」  

面板内 UI 自动注入 `window.plugin`（无需单独 SDK 即可起步）：

```js
await window.plugin.invoke("hello", { who: "Tempo" });      // 调用 Runtime 命令
await window.plugin.host("notify.show", { title: "Hi" });    // 调用宿主 API
window.plugin.on("greeted", (payload) => console.log(payload));
```

宿主侧常用 Bridge 方法（节选）：`palette.hide` / `palette.back` / `palette.setSize`、`app.open`、`external.open`（仅 http(s)/mailto）、`notify.show`、`theme.get` / `theme.onChange`、`storage.plugin.get|set|delete|list`、`session.push` 等。

卸载插件会停止 Runtime、移除面板贡献，安装包可移入回收目录（可选删除插件私有数据）。

### 插件 SDK

`packages/plugin-sdk` 提供类型与封装（持续演进），用于在 TypeScript 中更稳妥地调用 Bridge 与 Runtime；入门可直接使用 `window.plugin`。

## 🤖 MCP

服务默认监听 `http://127.0.0.1:17832`（仅本机）。在 **设置 → MCP** 复制令牌与 Cursor 片段，或手动配置：

```json
{
  "mcpServers": {
    "tempo": {
      "url": "http://127.0.0.1:17832/mcp",
      "headers": {
        "Authorization": "Bearer <设置页令牌>"
      }
    }
  }
}
```

- 健康检查：`GET http://127.0.0.1:17832/health`（无需鉴权）  
- Tempo 未运行或 MCP 关闭时客户端无法连接  

**内置工具（节选）**：待办列表/详情/增删改、子任务与备注、短语与分组、剪贴板搜索、番茄钟状态与控制、按日屏幕使用报告等。插件可通过 `contributes.mcpTools` 声明工具，但**默认不向 AI 暴露**；用户须在插件设置中逐项开启后，客户端先调用 `tempo_list_exposed_plugin_tools` 再 `tempo_call_plugin_tool`。

## 🛠️ 技术栈

| 层级 | 选型 |
|------|------|
| 壳层 | Tauri 2、Rust |
| 前端 | React 19、TypeScript、Vite 7、Tailwind CSS 4、shadcn/ui |
| 数据 | SQLite（rusqlite），存储路径可在设置中修改 |
| 系统能力 | 全局快捷键、托盘、前台窗口检测（active-win）、剪贴板（arboard） |
| 插件 | 嵌入式 Node Runtime、Supervisor、Host Bridge、MCP 桥接 |

## 📁 项目结构

```
├── src/
│   ├── apps/                 # 应用/插件注册、快捷操作、面板宿主
│   ├── pages/                # 官方内置扩展 UI
│   └── lib/
├── src-tauri/src/
│   ├── commands/             # IPC 模块
│   ├── plugins/              # 清单解析、Runtime、Bridge、信任与安装
│   └── mcp/                  # MCP HTTP 服务
├── packages/plugin-sdk/
└── examples/plugins/         # 示例插件（含 com.example.hello）
```

## 💻 开发

```bash
pnpm dev              # Tauri 开发模式
pnpm run sync:version # 同步 package.json → Cargo/tauri 版本
pnpm run build        # 类型检查 + 发布构建
```

版本号以 `package.json` 为唯一来源：`npm version patch|minor|major` 会自动同步 `src-tauri`。

调试：开发模式下可通过 Tauri/WebView 开发者工具查看面板前端；插件 UI 可在对应面板内调试。

## ⚠️ 说明

- 关闭主窗口默认最小化到托盘，托盘菜单可退出  
- 部分全屏或受保护窗口可能无法参与屏幕时间统计  
- 待办列表与剪贴板等采用分页/按需加载，减轻面板唤起时的 IO  
- 启用含 `main` 的插件前请完成信任确认；未签名包的 `publisher` 仅作展示，不能代替验签身份  

---

<div align="center">

**Tempo — 轻量宿主，能力交给插件**

</div>
