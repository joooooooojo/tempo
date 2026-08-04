# Changelog

本文件记录 Tempo 面向用户的版本变更。GitHub Release 正文由 `scripts/changelog-for-version.mjs` 按版本章节生成。

## [Unreleased]

### Feat

- 内置「文件搜索」：Windows 优先复用或下载便携 Everything（ES IPC），macOS 按需下载 fd 并从 `/` 实时全盘扫描；顶栏搜索与滚动分页；预览支持图片、音视频、文本 / Markdown、Office（Word 样式化、Excel）与压缩包目录列表，引擎下载显示进度。
- 设置「自定义打开」：可将本地文件 / 文件夹 / 快捷方式加入主面板索引，支持搜索打开、最近使用与固定。
- 设置「快捷键」：检测 Tempo 内部冲突与系统级占用；冲突行显示「与其他快捷键冲突」，注册失败显示「已被占用」/「注册失败」。
- 待办日历模式：图例增加「完成时间」（按 `completed_at` 标记日期）；日格标记过多时改为单元格右侧竖排。
- 剪贴板快捷操作：复制内容为链接时显示「打开链接」及已安装的 Chrome / Edge / Firefox（真实图标），链接类操作优先于翻译 / 待办等。
- 主面板：应用与插件均可固定且图钉即时刷新，「已固定」按固定时间与上次使用的较新者排序；第三方插件图标与应用同风格（保留「插件」角标）；右键改为独立浮动菜单（打开 / 打开位置 / 固定 / 移出列表等）。
- 开发环境下打开 WebView 控制台时主面板不因失焦关闭（Windows 按 `msedgewebview2` 检测）；系统外观由 Rust 推送 `os:appearance-changed`，避免前端定时轮询。
- 插件独立窗口在 Dock / 任务栏使用 Tempo 平台图标，右下角叠加插件角标。
- 插件开发助手：验证 MCP Tool 时可在大编辑器中编辑输入 JSON（按 `inputSchema` 预填）；验证 Command 可输入字符串 / 图片 / 文件参数；验证对话框可滚动；MCP 参数列表项增加卡片分割样式。
- Dialog 内容区默认使用 ScrollArea（隐藏原生滚动条），面板按可用高度滚动；需要自管滚动时可设 `scrollable={false}`。

### Fix

- Windows：增加底层键盘钩子（`WH_KEYBOARD_LL`）守护全局快捷键，并定期重装以压过 uTools 等同类钩子；匹配到的组合会被吞掉，避免被抢走。此前增量注册（检查状态时不松手）仍然保留。
- 全局快捷键改为增量注册：已成功占用的组合在检查状态 / 改其他设置时不再先 unregister，避免松开后被其他程序抢走；仅变更或清空的项才会释放，并仍会重试此前「已被占用」的绑定。
- 修复剪贴板长文本 chip 把「修改」按钮挤出并被搜索框遮挡的问题。
- 端口管理器：切换分页时列表滚回顶部；收窄端口 / 程序路径列、放宽协议与状态列，避免表格横向滚动。

## [2.2.1] - 2026-08-01

### Feat

- 插件 Manifest 新增 `platforms` 字段，可声明适用宿主（macOS / Windows）；Linux 已预留但暂未支持，开发助手中置灰不可选。未声明时视为支持当前已发布平台。
- 内置插件编辑页支持 ⌘S / Ctrl+S 快捷保存（Hosts、短语、翻译配置、待办编辑、插件开发助手）。
- Hosts 重构：移除公共配置与内置环境种子；配置支持本地（空白/导入文件）与远程（URL + 自动刷新）；可同时激活多个配置写入系统 hosts。
- 腾讯翻译升级为混元翻译（`ChatTranslations` / `hunyuan-translation`），沿用原 SecretId / SecretKey 配置；单引擎时 SSE 流式打字机显示，多引擎对比仍为非流式。
- 聚合翻译：外部注入 / 面板重开时剪贴板首条文本可自动填充并翻译；手动输入需点击「翻译」或按 Enter（Shift+Enter 换行）。

### Fix

- 修复 Hosts 编辑内容时输入区自动滚回顶部的问题。
- 修复 Windows 下插件开发助手连接或重连 Runtime 时，进程树清理命令短暂弹出控制台窗口的问题。

### Docs / Chore

- Tempo 应用版本升至 **2.2.1**。
- 远程插件模板发布版本升至 **1.0.1**（含 Manifest `platforms` 字段）。

## [2.2.0] - 2026-07-30

### Feat

- 插件 API 改为 Host 直接注入：UI 使用 `window.tempo` / `window.ipcRenderer` 并遵循 WebView 生命周期；Runtime 使用 `globalThis.tempo` / `globalThis.ipcMain` 与 `onMounted` / `onUnmounted`。
- MCP Tool 与 Commands 解耦：Manifest 不再声明 `mcpTools[].command`，Runtime 使用专用 `tempo.mcpTools.register()` 注册工具实现；Host API 基线设为 `1.0.0`。
- `tempo.events` 在 UI 与 Runtime 同时支持 `on`、`once`、`off`、批量清理和监听状态查询；通用事件监听与设置、主题订阅相互隔离。
- 插件开发助手新增 UI、Hybrid、Headless 三套 Vite 模板，构建后的 `dist` 可直接导入 Tempo。
- 插件模板与 Manifest Schema 改为独立远端发布：创建项目时选择最新兼容版本、校验 SHA-256 并缓存，模板更新不再要求升级 Tempo。
- 插件开发助手的项目切换菜单支持二次确认后移除项目记录，并明确不会删除本地文件；当前项目仅使用背景色标识。

### Fix

- 修复跟随系统主题时界面没有及时同步的问题，并分离 macOS 货架窗口与主面板的外观状态。
- 修复 macOS 首次请求通知权限导致主面板失焦的问题，并支持通过本地 `.env` 配置应用签名。
- 隐藏 Windows 下插件 Runtime 启动时短暂出现的 Node.js 控制台窗口。

### Breaking Changes

- 移除 `@tempo/plugin-sdk`、`definePlugin`、`createPluginClient` 和旧版 SDK 包装层；插件入口改为直接使用宿主注入的全局 API。
- 移除 Manifest `hooks` 配置和 Hook 到 Command 的旧路由；平台广播统一由 UI 或 Runtime 的 `tempo.events` 监听。
- UI 与 Runtime 私有通信统一为 Electron 风格的 `ipcRenderer` / `ipcMain`，不再与平台事件或 Commands 共用命名空间。

### Docs / Chore

- 文档站点重组为用户指南、插件开发指南和 API 参考，补充插件生命周期、三类插件差异、Commands / Actions / MCP Tools 关系与完整 Host API 说明。
- 三套模板声明文件移除所有显式 `any`，为上下文、Action、IPC、事件、设置、Commands 和 MCP Tools 提供明确类型；模板构建会拒绝包含 `any` 的声明文件。
- Tempo 应用版本升至 **2.2.0**；插件 Host API 与远程模板版本继续独立维护为 **1.0.0**。

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
