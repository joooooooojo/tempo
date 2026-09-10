# Changelog

本文件记录 Tempo 面向用户的版本变更。GitHub Release 正文由 `scripts/changelog-for-version.mjs` 按版本章节生成。

## [Unreleased]

### Changed

- 插件运行时（破坏性变更）：后台统一迁移到 Deno 2.9.6，不再回退 Node；插件需使用 Manifest v2 / Host API 2.x，新增文件 API 的模板要求 2.1.0。旧插件须提升版本、重新打包并确认信任，Node/npm/Vite 继续用于构建。
- 插件 SDK（破坏性变更）：`tempo-plugin-sdk` 升级到 1.0，UI 改用 `connect()` 获取上下文非空的 Client，Runtime 改用 `defineRuntime()` 统一 setup 与逆序清理；Hybrid 可用一份 IPC 契约约束两侧频道、参数和返回值。
- 插件权限：Manifest 改为八项可独立选择的权限数组，分别控制全局文件读取、全局文件写入、网络、环境变量、系统信息、子进程、动态库和远程导入；默认全部关闭，八项全选等价于 Deno 完全访问，`net` 同时控制 Runtime 与托管 UI。Tempo Host API 不再要求逐项授权，并移除重复的 `capabilities` 声明。
- 插件文件 API：UI 与 Runtime 新增始终可用的 `tempo.files`，无需 Deno 文件权限即可读写当前插件的私有数据目录；拒绝绝对路径、路径穿越和符号链接，并限制单文件与目录列表大小。
- 插件模板：同步 UI / Hybrid / Headless 模板至 2.0.8 和 Host API 2.1.0，并提供内置模板回退；Hybrid 的 UI、Runtime tsconfig 位于根目录，由 references 和 `tsc -b` 统一检查，同时通过共享 IPC 契约保持两侧类型一致。
- 插件图标：统一支持 SVG、PNG、JPEG、WebP 与 GIF，并在 Manifest、包导入、主界面、插件管理、插件仓库及独立窗口使用同一格式契约；模板 Schema 更新至 2.0.5。
- 演示插件：Hello 2.1.0 新增权限检查面板，对比零 Deno 权限下的敏感操作拦截与 UI / Runtime 始终可用的 `tempo.files` 私有目录读写，并提供八项权限的选择示例。
- 主面板：自动收起改为按「原生应用激活」判断——只有切换到其他应用时才收起；右键菜单、插件窗口、开发者工具、系统文件对话框等 Tempo 自身窗口获得焦点不再误关面板。
- 插件仓库：凭证不再写入系统凭证库，Token 和密码只留在当前应用会话。

### Fix

- 主面板：打开时优先显示在鼠标所在屏幕；无法读取鼠标屏幕时再回退到前台应用所在屏幕或主屏。
- 插件：主面板可正确加载 `icons/app.svg` 等包含多级路径的插件图标，不再回退为默认应用图标。
- 插件：停用不再清空私有存储；开发连接期间修改身份、入口或权限须先断开，避免新配置与旧进程不一致。
- 插件权限：Runtime 进入停用流程后拒绝新的 Host API 请求；UI Bridge 必须绑定已注册视图，插件 UI 静态资源改用视图凭证隔离，并校验插件数据目录的 ID 与安装记录。
- 插件启动：IPC 统一为预绑定本机 TCP，收到明确握手确认后再加载插件；补充启动超时、无效连接及消息队列限制，并校验 Deno 下载包与运行二进制。
- 演示插件：Welcome 0.2.0 更新 v2 Schema 并移除多余权限。
- 快捷键（Windows）：移除低级键盘钩子及前台抢占逻辑，统一使用系统全局快捷键注册；冲突时让系统占用方优先，避免 Tempo 干预其他应用的按键事件。
- 主面板：右键打开应用菜单时不再误关面板。
- 主面板：剪贴板多文件匹配时，文件图标上的数量角标不再被裁切。
- 插件仓库：HTTPS 源不再钉死 TLS 证书指纹，改由系统证书校验；GitHub 等站点证书轮换不会再误拦。SSH 主机密钥仍需首次确认。

### Docs / Chore

- 补充 Deno 迁移详设与审核记录，同步插件开发、权限及升级文档；增加受限运行时、模板构建、类型隔离和演示插件回归测试。
- 仓库调整为 pnpm monorepo：主应用迁入 `core`，文档、插件模板与 `tempo-plugin-sdk` 独立为 workspace；持续发布内容的官方插件仓库由 `plugin-repository/` Git Submodule 引入，可复制的仓库模板保留在 `templates/plugin-repository/` 并独立打包。插件项目模板改用 SDK 的 UI / Runtime 模块入口。
- 新增 `tempo-plugin-sdk` npm 发布工作流：`sdk-v*` 标签或手动版本输入触发，发布前校验版本、SDK、插件模板、Deno Runtime 和 npm 包内容，并生成 provenance。

## [2.2.6] - 2026-08-31

### Feat

- 设置「插件仓库」：可添加 Git 源（HTTPS / SSH）、同步目录并从已提交的 `dist/` 安装插件；支持 TLS 证书与 SSH 主机密钥确认、凭证档案，以及从官方模板在本机创建仓库后推送到远程再添加源。
- 端口管理：打开后焦点在搜索框；方向键选择进程，回车结束进程（受保护进程会提示），确认框回车执行。
- 主面板：支持在通用设置中自定义搜索栏右侧图标；点击当前图标即可选择本地图片，保存后立即更新并持久化。
- 设置：按钮、输入框、下拉框与图标操作统一视觉高度和方形控件宽度，Switch 保持原有尺寸与样式。
- 主面板：搜索框支持快速计算数学表达式，点击“复制计算结果”或按回车即可复制答案并关闭主面板。

### Fix

- 主面板：修复失焦关闭的一瞬间按 `Alt+空格` 重新打开后，旧的关闭请求仍会把新面板自动关闭的问题。
- 插件仓库：安装完成只弹出一条成功提示，避免历史操作与重复监听叠出多条 toast。
- 插件仓库：确认 TLS 证书对话框中主机、指纹等字段与只读输入框对齐。

### Docs / Chore

- Tempo 应用版本升至 **2.2.6**。
- 文档：Git 插件仓库约定、JSON Schema 与官方仓库模板。

## [2.2.5] - 2026-08-24

### Fix

- 主面板：Tab 在面板内循环焦点，避免焦点落到窗口外触发失焦关闭。
- 主面板：Esc 关闭改为窗口级处理，不再绑在搜索框上；点「展开 / 收起」后 Esc 仍可关闭，且输入框保持焦点。
- Hosts：确认激活 / 取消激活后失焦开关，避免 Esc 无法返回上一级。
- Hosts：Windows 上写入系统 hosts 后刷新 DNS 不再闪出控制台窗口，点「确认激活」不会再抢走焦点导致主面板关闭；确认后焦点不再回到开关，再次打开面板也不会弹出方向相反的二次确认框。

### Docs / Chore

- Tempo 应用版本升至 **2.2.5**。

## [2.2.4] - 2026-08-10

### Fix

- 主面板搜索：应用结果与「推荐操作」按同一查询一起更新，并去掉应用搜索防抖，避免先只出现推荐操作再插入应用导致闪一下。
- 文件搜索：首次打开若需拉起 Everything，先提示「正在启动」并等待 `IS_DB_LOADED`，不再误显示「搜索中…」；数据库未就绪时统一提示「数据库准备中…」。
- Hosts：激活 / 取消激活改用应用内确认框（不再用 `window.confirm`），避免抢焦点导致主面板失焦关闭，并消除再打开后开关被反复拨动的循环；一键授权期间抑制 blur-hide；读取工作区时以 Tempo 状态为准，避免系统 hosts 旧标记把已取消的配置又标回激活。

### Docs / Chore

- Tempo 应用版本升至 **2.2.4**。

## [2.2.3] - 2026-08-06

### Feat

- 主面板：上次打开的插件若超过 1 分钟未再次打开，下次唤起回到搜索主界面。
- 文件搜索：Everything 索引进度展示、分页加载与分类切换体验完善；便携 Everything 随 Tempo 退出并持久化索引。

### Fix

- 内置插件数据更新时的界面闪烁：插件开关不再因全局 busy 集体半透明；剪贴板 / 快捷短语跳过自触发事件重载；待办完成 / 置顶乐观更新；Hosts 外部刷新改为 soft load；自定义打开按行 busy。
- Hosts：切换配置时内容区滚回顶部；激活 / 取消激活时不再整表开关抖动。
- 插件配置 Dialog：ScrollArea 与 `height` / `maxHeight` 正常工作；配置内 Switch 恢复滑动过渡（兼容 Dialog 居中 `translate` 与 Tailwind v4）。
- 主面板失焦关闭：打开宽限期内点别处丢失的失焦关闭会在 arm 到期后补关。
- 主面板 Alt+Space：缩短快捷键双通道防抖；逻辑可见状态与前端 blur-hide 同步，避免失焦后第一次快捷键被当成关闭；隐藏后重装键盘钩子降低被其他启动器抢走。

### Docs / Chore

- Tempo 应用版本升至 **2.2.3**。

## [2.2.2] - 2026-08-05

### Feat

- 内置「文件搜索」：Windows 优先复用或下载便携 Everything（ES IPC），macOS 按需下载 fd 并从 `/` 实时全盘扫描；顶栏搜索与滚动分页；预览支持图片、音视频、文本 / Markdown、Office（Word 样式化、Excel）与压缩包目录列表，引擎下载显示进度。
- 设置「自定义打开」：可将本地文件 / 文件夹 / 快捷方式加入主面板索引，支持搜索打开、最近使用与固定。
- 设置「快捷键」：检测 Tempo 内部冲突与系统级占用；冲突行显示「与其他快捷键冲突」，注册失败显示「已被占用」/「注册失败」。
- 待办日历模式：图例增加「完成时间」（按 `completed_at` 标记日期）；日格标记过多时改为单元格右侧竖排。
- 剪贴板快捷操作：复制内容为链接时显示「打开链接」及系统已登记的全部 http 浏览器（Windows `StartMenuInternet` / macOS NSWorkspace），链接类操作优先于翻译 / 待办等。
- 主面板：应用与插件均可固定且图钉即时刷新，「已固定」按固定时间与上次使用的较新者排序；第三方插件图标与应用同风格（保留「插件」角标）；右键改为独立浮动菜单（打开 / 打开位置 / 固定 / 移出列表等）。
- 开发环境下打开 WebView 控制台时主面板不因失焦关闭（Windows 按 `msedgewebview2` 检测）；系统外观由 Rust 推送 `os:appearance-changed`，避免前端定时轮询。
- 插件独立窗口在 Dock / 任务栏使用 Tempo 平台图标，右下角叠加插件角标。
- 插件开发助手：验证 MCP Tool 时可在大编辑器中编辑输入 JSON（按 `inputSchema` 预填）；验证 Command 可输入字符串 / 图片 / 文件参数；验证对话框可滚动；MCP 参数列表项增加卡片分割样式。
- Dialog 内容区默认使用 ScrollArea（隐藏原生滚动条），面板按可用高度滚动；需要自管滚动时可设 `scrollable={false}`。

### Fix

- Windows：增加底层键盘钩子（`WH_KEYBOARD_LL`）守护全局快捷键，并定期重装以压过 uTools 等同类钩子；匹配到的组合会被吞掉，避免被抢走。此前增量注册（检查状态时不松手）仍然保留。
- 全局快捷键改为增量注册：已成功占用的组合在检查状态 / 改其他设置时不再先 unregister，避免松开后被其他程序抢走；仅变更或清空的项才会释放，并仍会重试此前「已被占用」的绑定。
- 文件搜索：去掉输入防抖；Windows 仅在 Everything `IS_DB_LOADED=false` 时提示索引进度（不再误把 `IS_DB_BUSY` 当成索引）；查询改用 FindWindow 探活；Windows 搜索改为 Everything WM IPC；分类筛选改用 `ext:` 语法；结果列表对关键词做高亮。
- 修复剪贴板长文本 chip 把「修改」按钮挤出并被搜索框遮挡的问题。
- 端口管理器：切换分页时列表滚回顶部；收窄端口 / 程序路径列、放宽协议与状态列，避免表格横向滚动。

### Docs / Chore

- Tempo 应用版本升至 **2.2.2**。

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
