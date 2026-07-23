# Tempo

基于 **Tauri 2 + Vite + React** 的桌面效率工具：待办事项、番茄专注与屏幕使用时间统计。

## 功能特性

### 待办事项
- 创建/编辑待办，支持 Markdown 正文与图片嵌入
- 子任务、标签、备注、置顶、重复周期与截止提醒
- 搜索、分页列表、备份导入/导出（ZIP）
- 与番茄钟联动，记录每条待办的专注时长

### 番茄时钟
- 工作 / 短休 / 长休循环，可绑定当前待办
- 系统通知与声音提醒

### 屏幕使用时间
- 实时统计亮屏时长与前台应用使用时长
- 日报（按小时）、周报（按天）图表
- 久坐护眼提醒、夜间作息提醒

### 快捷操作
- **Alt + Space** 全局快捷键（macOS 为 **⌥ + Space**）：打开快捷面板
- 系统托盘驻留，关闭窗口默认最小化到托盘
- 本地自动更新

### MCP
Tempo 运行时可在本机提供 MCP 服务，供 AI 客户端创建待办、管理快捷短语、查询剪贴板、控制番茄钟、读取今日报告。

1. 打开 Tempo（设置 → **MCP** 默认开启）
2. 在设置页复制 Cursor 配置片段，或手动写入：

```json
{
  "mcpServers": {
    "tempo": {
      "url": "http://127.0.0.1:17832/mcp",
      "headers": {
        "Authorization": "Bearer <在设置页查看的令牌>"
      }
    }
  }
}
```

3. 健康检查：`GET http://127.0.0.1:17832/health`（无需鉴权）

服务仅监听 `127.0.0.1`；Tempo 未运行时客户端无法连接。

## 隐私与安全

- 全程离线运行，数据存储在本地 SQLite
- 不上传任何使用数据

## 环境要求

- [Node.js](https://nodejs.org/) 18+
- [Rust](https://www.rust-lang.org/tools/install)
- Windows 10/11 或 macOS 12+

### Windows 额外依赖

[Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)（含 C++ 工作负载）及 [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)。

## 快速开始

```bash
cd tempo
npm install
npm run dev
```

开发模式使用 `npm run dev`（内部调用 Tauri dev）。

## 构建发布

```bash
npm run build
```

安装包输出在 `src-tauri/target/release/bundle/`。

## 版本管理

`package.json` 中的 `version` 为唯一版本源。执行以下命令会自动同步 `src-tauri/Cargo.toml` 与 `src-tauri/tauri.conf.json`：

```bash
npm version patch   # 或 minor / major
```

仅同步当前版本：

```bash
npm run sync:version
```

## 项目结构

```
tempo/
├── src/                          # React 前端
│   ├── components/
│   │   ├── layout/               # 应用布局与导航
│   │   ├── todos/                # 待办表单、标签、子任务等
│   │   └── ui/                   # shadcn/ui 基础组件
│   ├── pages/
│   │   ├── todos/                # 待办页主逻辑与工具函数
│   │   ├── TodoPage.tsx          # 待办页入口（re-export）
│   │   ├── PomodoroPage.tsx
│   │   ├── ReportsPage.tsx
│   │   ├── SettingsPage.tsx
│   │   └── CommandPalettePage.tsx # 应用搜索与快捷操作面板
│   └── lib/                      # API 封装、主题、通知等
└── src-tauri/                    # Rust 后端
    └── src/
        ├── commands/             # IPC 命令（按模块拆分）
        │   ├── todos.rs          # 待办 CRUD、备份
        │   ├── tracker.rs        # 后台统计与提醒线程
        │   ├── reports.rs        # 报表查询
        │   ├── settings.rs       # 设置与存储目录
        │   ├── markdown.rs       # Markdown 图片协议
        │   └── ...
        ├── db.rs                 # SQLite 数据层
        ├── platform.rs           # 前台窗口 / 锁屏检测
        └── pomodoro.rs           # 番茄钟状态机
```

## 页面导航

| 页面 | 路径 | 说明 |
|------|------|------|
| 待办事项 | `/` | 默认首页，待办列表与详情 |
| 番茄时钟 | `/pomodoro` | 专注计时，可关联待办 |
| 屏幕显示时间 | `/reports` | 日报、周报与应用排行 |
| 设置 | `/settings` | 主题、提醒、存储目录、自动启动等 |

## 技术栈

- **前端**：React 19、TypeScript、Vite 7、Tailwind CSS 4、shadcn/ui、Recharts
- **后端**：Rust、Tauri 2、rusqlite、active-win-pos-rs
- **存储**：SQLite（用户数据目录，可自定义存储路径）

## 注意事项

1. 首次启动会显示引导对话框
2. 关闭主窗口默认最小化到托盘，托盘右键可退出
3. 部分全屏游戏或受保护窗口可能无法识别前台应用
4. 待办列表默认轻量加载（不含图片二进制数据），打开详情或展开时按需加载完整数据
