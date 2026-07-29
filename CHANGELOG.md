# Changelog

本文件记录 Tempo 面向用户的版本变更。GitHub Release 正文由 `scripts/changelog-for-version.mjs` 按版本章节生成。

## [2.0.1] - 2026-07-29

### Feat

- **插件开发助手**：新增内置应用与 Rust `plugin_dev_*` 命令（项目列表/创建/打开、写 Manifest、探测 UI URL、连接/断开/重载 Runtime、模拟 Hook、运行 MCP Tool 等）；支持本地目录与开发态 UI 调试（含 `@tempo/plugin-sdk/dev`、Vite `tempoPluginDev`）。
- **开发助手体验迭代**：Windows 原生路径规范化；开发 UI `Cache-Control: no-store` 与 Runtime 开发日志转发；连接与测试合并为「运行」页；顶栏 Manifest/连接双入口、操作区沉底、Workspace KeepAlive；贡献点验证 Dialog、MCP Schema 表单编辑、Manifest 根字段编辑。
- **UI↔Runtime 私有 IPC（Host API 1.5.0）**：`tempo.ipc.invoke` / `handle` / `send` / `on`（Electron 风格），与对外 `commands`（Action / Hook / MCP）分表；UI bridge 拒绝 `runtime.*`。
- **Structured Clone 载荷**：`invoke`/`handle`/`send`/`on` 经可移植 SCA 编解码（`Date` / `Map` / `Set` / `TypedArray` / 循环引用等）；Host 透传 `{ $sca }` 信封。
- **主面板推荐操作**：有剪贴板操作上下文时查询只过滤推荐操作并隐藏应用搜索；`listVisibleQuickActions` 按名称/关键词分词过滤。
- **插件 MCP 状态展示**：`InstalledPlugin.mcpEnabledToolCount`；设置页插件列表展示 MCP 已启用 / 已停用 / 部分启用。
- **Hello 示例**：对内 `ipc` + SCA 探测；对外保留 `hello` command；`engines.pluginApi` 升至 `^1.5.0`。

### Fix

- **主面板拖动与位置**：自定义拖动位移（非系统 `startDragging`），输入区可选中；位置读写与落点钳制（`get/set/save_main_panel_position`）；`set_main_panel_rect` / `set_plugin_window_rect` 在省略坐标时保留当前轴位置。
- **主面板搜索框布局**：输入测宽与自定义 placeholder（CSS wrapper/measure），避免占位与内容宽度抖动。

### Refactor

- **内置插件目录化**：前端迁入 `src/builtin-plugins/*`，Rust 迁入 `src-tauri/src/builtin_plugins/*`；拆分过大的 todo / hosts / translate / reports / snippets 等模块。

### Docs / Chore

- Host API / SDK / 开发指南同步 1.5.0 三通道（Host / Command / IPC）；新增开发助手设计文档。
- 应用与 `@tempo/plugin-sdk` 版本升至 **2.0.1**；Release 工作流从本文件生成发布说明。

## [2.0.0] - 2026-07-28

### Feat

- Tempo 2.0 正式版基线（应用与 `@tempo/plugin-sdk` 同版本）。
