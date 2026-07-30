# Changelog

本文件记录 Tempo 面向用户的版本变更。GitHub Release 正文由 `scripts/changelog-for-version.mjs` 按版本章节生成。

## [Unreleased]

### Feat

- 插件 API 改为 Host 直接注入：UI 使用 `window.tempo` / `window.ipcRenderer` 并遵循 WebView 生命周期；Runtime 使用 `globalThis.tempo` / `globalThis.ipcMain` 与 `onMounted` / `onUnmounted`。
- MCP Tool 与 Commands 解耦：Manifest 不再声明 `mcpTools[].command`，Runtime 使用专用 `tempo.mcpTools.register()` 注册工具实现；Host API 基线设为 `1.0.0`。
- `tempo.events` 在 UI 与 Runtime 同时支持 `on`、`once`、`off`、批量清理和监听状态查询；通用事件监听与设置、主题订阅相互隔离。
- 插件开发助手新增 UI、Hybrid、Headless 三套 Vite 模板，构建后的 `dist` 可直接导入 Tempo。
- 插件模板与 Manifest Schema 改为独立远端发布：创建项目时选择最新兼容版本、校验 SHA-256 并缓存，模板更新不再要求升级 Tempo。
- 移除旧插件 API 包及其版本同步，插件只通过 `engines.pluginApi` 声明 Host API 兼容范围。

## [2.0.1] - 2026-07-29

### Feat

- **插件开发助手**：新增内置应用与 Rust `plugin_dev_*` 命令（项目列表/创建/打开、写 Manifest、探测 UI URL、连接/断开/重载 Runtime、运行 MCP Tool 等）；支持本地目录与开发态 UI 调试。
- **开发助手体验迭代**：Windows 原生路径规范化；开发 UI `Cache-Control: no-store` 与 Runtime 开发日志转发；连接与测试合并为「运行」页；顶栏 Manifest/连接双入口、操作区沉底、Workspace KeepAlive；贡献点验证 Dialog、MCP Schema 表单编辑、Manifest 根字段编辑。
- **UI↔Runtime 私有 IPC（Host API 1.0.0）**：Electron 风格的 `ipcRenderer` / `ipcMain` 与对外 Commands 分开路由。
- **Structured Clone 载荷**：`invoke`/`handle`/`send`/`on` 经可移植 SCA 编解码（`Date` / `Map` / `Set` / `TypedArray` / 循环引用等）；Host 透传 `{ $sca }` 信封。
- **主面板推荐操作**：有剪贴板操作上下文时查询只过滤推荐操作并隐藏应用搜索；`listVisibleQuickActions` 按名称/关键词分词过滤。
- **插件 MCP 状态展示**：`InstalledPlugin.mcpEnabledToolCount`；设置页插件列表展示 MCP 已启用 / 已停用 / 部分启用。
- **Hello 示例**：对内 `ipc` + SCA 探测；对外保留 `hello` command；`engines.pluginApi` 使用 `^1.0.0`。

### Fix

- **主面板拖动与位置**：自定义拖动位移（非系统 `startDragging`），输入区可选中；位置读写与落点钳制（`get/set/save_main_panel_position`）；`set_main_panel_rect` / `set_plugin_window_rect` 在省略坐标时保留当前轴位置。
- **主面板搜索框布局**：输入测宽与自定义 placeholder（CSS wrapper/measure），避免占位与内容宽度抖动。

### Refactor

- **内置插件目录化**：前端迁入 `src/builtin-plugins/*`，Rust 迁入 `src-tauri/src/builtin_plugins/*`；拆分过大的 todo / hosts / translate / reports / snippets 等模块。

### Docs / Chore

- Host API / 开发指南统一为 1.0.0 基线，并明确 Host / Command / IPC 三条通道；新增开发助手设计文档。
- Tempo 应用版本升至 **2.0.1**；Release 工作流从本文件生成发布说明。

## [2.0.0] - 2026-07-28

### Feat

- Tempo 2.0 正式版基线。
