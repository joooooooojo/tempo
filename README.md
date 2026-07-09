# Tempo

基于 **Tauri 2 + Vite + React + shadcn/ui** 的轻量化桌面屏幕使用时间监控工具。

## 功能特性

- **实时统计**：屏幕亮屏时长、前台应用使用时长（每秒更新）
- **系统托盘**：最小化驻留托盘，悬浮显示当日累计时长
- **数据报表**：日报折线图、周报柱状图，支持 CSV 导出（Excel 可直接打开）
- **提醒管控**：久坐护眼（45 分钟可配）、应用时长限额、夜间作息提醒
- **本地存储**：SQLite 离线数据库，保留 30 天历史
- **隐私安全**：全程离线，不上传任何数据

## 环境要求

- [Node.js](https://nodejs.org/) 18+
- [Rust](https://www.rust-lang.org/tools/install)（Tauri 后端）
- Windows 10/11 或 macOS 12+

### Windows 额外依赖

安装 [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)（含 C++ 工作负载），以及 [WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)。

## 快速开始

```bash
cd tempo
npm install
npm run tauri dev
```

## 构建发布

```bash
npm run tauri build
```

安装包输出在 `src-tauri/target/release/bundle/`。

## 版本管理

`package.json` 是唯一需要手动更新的版本源。执行下面命令会自动同步 `src-tauri/Cargo.toml` 和 `src-tauri/tauri.conf.json`：

```bash
npm version patch
```

也可以使用 `npm version minor` 或 `npm version major`。只想同步当前版本时运行：

```bash
npm run sync:version
```

`package-lock.json` 由 npm 维护，`src-tauri/Cargo.lock` 由 Cargo 维护，不在同步脚本中直接写入。

## 项目结构

```
tempo/
├── src/                    # React 前端
│   ├── components/ui/      # shadcn/ui 组件
│   ├── pages/              # 首页、报表、设置、关于
│   └── lib/                # API 封装与工具函数
└── src-tauri/              # Rust 后端
    └── src/
        ├── db.rs           # SQLite 数据层
        ├── platform.rs     # 前台窗口/锁屏检测
        ├── commands.rs     # Tauri IPC 命令
        └── tracker.rs      # 后台统计线程
```

## 页面导航


| 页面   | 说明                  |
| ---- | ------------------- |
| 首页   | 今日/7日/30日时长、TOP5 应用 |
| 时长报表 | 日报、周报图表与导出          |
| 管控设置 | 护眼提醒、限额、屏蔽、主题       |
| 关于我们 | 版本与隐私说明             |


## 技术栈

- **前端**：React 19、TypeScript、Vite 7、Tailwind CSS 4、shadcn/ui、Recharts
- **后端**：Rust、Tauri 2、rusqlite、active-win-pos-rs
- **存储**：SQLite（本地 AppData 目录）

## 注意事项

1. 首次启动会弹出权限与隐私引导
2. 关闭窗口默认最小化到托盘，托盘右键可退出
3. 部分全屏游戏/加密软件可能无法识别，会归类为未知应用
4. 需安装 Rust 才能运行 `npm run tauri dev`
