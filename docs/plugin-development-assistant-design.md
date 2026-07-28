# Tempo 内置插件开发助手详细设计

> 状态：Draft
>
> 目标版本：Manifest v1、Host API 1.3.x、Tempo 2.x
>
> 产品形态：Tempo 内置应用 `plugin-dev-assistant`

## 1. 产品定位

插件开发助手是 Tempo 与开发项目之间的轻量连接器，不是插件 IDE、构建器或发布工具。

它只负责四件事：

1. 将一个根目录登记为 Tempo 插件开发项目。
2. 可视化创建、编辑和校验根目录下的 `manifest.json`。
3. 将 UI 服务 URL、UI 静态目录或 Headless Runtime JavaScript 入口临时连接到插件平台。
4. 为没有 UI 的贡献点提供最小测试入口和运行日志。

项目的源代码、依赖、dev server、编译、打包和发布仍由用户自己的 Vite、Rollup、TypeScript、npm
脚本或其它外部工具处理。开发助手不接管这些工具，也不要求项目采用 Tempo 专用结构。

## 2. 职责边界

### 2.1 助手负责

- 创建或打开一个项目目录，并记住最近项目。
- 保证项目 Manifest 固定为 `<project-root>/manifest.json`。
- 提供 Manifest 表单、原始 JSON 和宿主真实校验结果。
- 保存本机开发连接设置，但不向项目增加额外配置文件。
- 将开发入口注册为当前 plugin ID 的临时开发提供者。
- UI/Hybrid 插件连接一个本地服务 URL 或静态目录，使平台能够按 App 地址打开页面。
- Headless/Hybrid 插件连接一个本地 JavaScript Runtime 入口。
- 显示连接状态、Runtime stdout/stderr 和最小调用结果。
- 断开开发连接后恢复同 ID 正式插件的原状态。

### 2.2 助手不负责

- 不编辑插件业务源代码，不提供完整文件树或代码编辑器。
- 不启动或管理 Vite、TypeScript compiler、npm script 等外部开发进程。
- 不安装依赖，不生成 lockfile。
- 不执行 build，不复制构建文件，不生成 ZIP，不决定包内容。
- 不提供独立的发布、签名或上传流程；正式导入继续使用 Tempo 现有插件管理功能。
- 不代理 Vite 资源或 WebSocket，不实现自己的 HMR。
- 不直接执行 TypeScript Runtime 源码。
- 不建立第二套 Plugin Host、Runtime 协议或 MCP Server。
- 不增加 Tempo 专用的项目配置、构建或忽略规则文件。

### 2.3 为什么必须是内置应用

普通第三方插件没有管理其它 plugin ID、贡献点、UI view 和 Runtime Supervisor 的 Host API。开发连接
需要临时注册插件身份、复用真实 Manifest 解析、建立 Host Bridge，并在断开后恢复正式插件，因此
只能由宿主内置能力完成。

## 3. 项目模型

### 3.1 一个目录就是一个项目

创建或打开项目后，用户选择的目录就是项目根目录：

```text
my-tempo-plugin/
  manifest.json         # 固定位置，必需
  ...                   # 其它内容完全由用户和外部工具链决定
```

开发助手不支持为同一项目另选 Manifest 路径，也不递归搜索子目录中的 Manifest。这保证项目身份、
相对路径和正式包根目录的语义一致。

项目身份由 canonical root path 确定，plugin ID 从根目录 Manifest 读取。项目记录和插件身份必须
分开：用户在尚未填写合法 plugin ID 时，项目仍然存在，但不能建立开发连接。

### 3.2 创建项目

创建流程只生成根目录和 `manifest.json`：

1. 选择或新建一个空目录。
2. 选择 UI、Headless 或 Hybrid 类型。
3. 输入 plugin ID、名称、版本和兼容范围。
4. 生成最小合法 Manifest 并进入可视化编辑页。

助手不生成 Vite 项目、不安装 npm 依赖，也不强制创建 `src`、`dist` 或 Runtime 文件。用户可以在
外部脚手架中继续使用这个根目录。

若目标目录已存在且不为空，只创建缺失的 `manifest.json`，并在写入前要求确认；绝不覆盖现有
Manifest 或业务文件。

### 3.3 打开项目

1. 用户选择一个目录。
2. 目录存在 `manifest.json` 时直接登记并校验。
3. 目录没有 Manifest 时提示“在此目录创建 Manifest”，不自动搜索或移动文件。
4. 本机曾保存连接设置时恢复表单值，但不自动连接或启动 Runtime。
5. plugin ID 与已安装插件冲突时只提示；真正连接前再确认临时覆盖。

### 3.4 本机设置

以下信息保存在 Tempo SQLite，不写入项目：

- UI 连接类型：服务 URL 或静态目录。
- UI 服务 URL。
- UI 静态根目录。
- Runtime 开发入口路径。
- Runtime 入口变化后是否自动重连。
- 是否允许真实 Hook 事件。

这些都是当前机器的开发偏好，不属于发布 Manifest。

## 4. Manifest 可视化编辑

### 4.1 页面结构

Manifest 页面提供两个视图：

- `可视化`：按字段和贡献类型编辑。
- `JSON`：编辑完整原始文件，显示行列号和诊断。

可视化视图包含：

| 分组 | 字段 |
|---|---|
| 基础信息 | id、name、version、description、author、icon |
| 兼容性 | manifestVersion、engines.tempo、Plugin API |
| 插件类型 | kind、main、capabilities、activationEvents |
| Apps | id、title、entry、keywords、windowMode、rect、sessionVersion |
| Actions | id、title、accepts、app/command、keywords |
| Commands | id、title、visibility |
| Hooks | event、command |
| Settings | switch、select、multiselect、input、default |
| MCP Tools | command、description、inputSchema、outputSchema、annotations |

字段组件按数据类型选择：布尔值使用 Switch，多选使用 Checkbox/Select，枚举使用 Select，尺寸和
坐标使用数字输入，Schema 使用专门的 JSON 编辑区。引用字段优先以下拉选择已有 App 或 Command，
同时允许先输入尚未创建的 ID。

### 4.2 类型联动

- `ui`：显示 Apps/Actions UI 配置，隐藏 Runtime 必填项。
- `headless`：要求 `main`，隐藏 Apps 的 UI 连接配置。
- `hybrid`：同时显示 UI 和 Runtime 配置。
- Action 选择 app target 后不能同时选择 command target。
- Hook 和 MCP Tool 只能引用已声明 Command。
- 删除被引用项前显示引用关系，不静默产生无效 Manifest。

### 4.3 写入与冲突

- 可视化修改最终写回根目录 `manifest.json`，使用两空格缩进。
- 后端使用同目录临时文件加原子 rename。
- 每次写入携带读取时的文件 hash；外部编辑器已修改时拒绝覆盖并要求重新载入。
- JSON 语法错误时保留原文，可视化视图进入只读，避免表单覆盖用户草稿。
- 可视化视图不丢弃自己不认识但合法的 Manifest 字段。

### 4.4 校验

最终结果必须调用 Rust `PluginManifest::parse_str`，JSON Schema 只用于即时输入提示。诊断包含：

```ts
type ManifestDiagnostic = {
  severity: "error" | "warning" | "info";
  code: string;
  jsonPath?: string;
  line?: number;
  column?: number;
  message: string;
  suggestion?: string;
};
```

开发校验区分“Manifest 合法”和“当前开发连接可用”：使用服务 URL 时，不因项目中没有 `dist` 或
`apps[].entry` 对应的静态文件而阻止连接；使用静态目录时才检查其中的正式 UI 入口。Manifest 的
字段格式、引用关系和兼容范围在两种模式下都必须校验。

## 5. 开发连接模型

### 5.1 连接而不是安装

点击“连接到 Tempo”后，助手创建一个内存中的开发连接：

```ts
type DevPluginConnection = {
  projectId: string;
  pluginId: string;
  manifest: PluginManifest;
  ui?:
    | { kind: "url"; url: string }
    | { kind: "static"; rootPath: string };
  runtime?: { kind: "node-entry"; entryPath: string };
  state: "connecting" | "connected" | "partial" | "failed";
};
```

连接只存在于当前 Tempo 运行环境，不复制项目、不生成开发包、不写入正式插件表。平台解析 plugin ID
时，先查询活动开发连接，再查询正式安装插件。断开连接后移除覆盖并恢复正式插件。

### 5.2 类型对应关系

| Manifest kind | UI 连接 | Runtime 连接 | 成功状态 |
|---|---|---|---|
| `ui` | 必需：URL 或静态目录 | 无 | UI 可由平台打开 |
| `headless` | 无 | 必需：JavaScript 入口 | Runtime ready，贡献点可调用 |
| `hybrid` | 必需：URL 或静态目录 | 必需：JavaScript 入口 | 两端都连接；单端失败显示 partial |

Hybrid 的 UI 和 Runtime 是两个独立端点。UI dev server 重启不应终止 Runtime；Runtime 重连也不应
reload UI。`partial` 状态允许开发者继续调试已连接的一端，但调用不可用的一端时返回明确错误。

### 5.3 同 ID 正式插件

- 同一 plugin ID 同时只能有一个活动提供者。
- 连接前显示将被临时覆盖的正式版本和启用状态。
- 用户确认后暂停正式插件，保存其启用、信任和 MCP 状态。
- 断开连接或 Tempo 退出时恢复正式插件。
- 恢复失败必须显示修复操作，不能静默留下半连接状态。

## 6. UI 与 Hybrid 连接

### 6.1 服务 URL 模式

适用于 Vite、Webpack dev server 或开发者自己的本地 Web 服务：

1. 用户输入完整 URL，例如 `http://127.0.0.1:5173/`。
2. 助手校验 URL 并做短超时连通性探测。
3. 建立开发连接后，`plugin_ui_prepare` 为 Manifest 中的 App 返回该开发 URL。
4. 平台继续创建真实 `viewInstanceId`，在 `PluginAppHost` iframe 中打开 URL。
5. Host Bridge 调用仍绑定父页面持有的 plugin ID 和 view，不由 URL 页面自行声明身份。

URL 只允许 `http`/`https` loopback 地址，包括 `127.0.0.1`、`[::1]` 和解析后仅指向 loopback 的
`localhost`。开发者可以让服务监听 `0.0.0.0`，但填写给 Tempo 的地址必须是 loopback 地址。

服务未启动时连接状态为“等待服务”，不要求存在 `dist`，也不自动回退到旧静态文件。服务恢复后
可以重新加载当前 App。

### 6.2 URL 页面的 Host Bridge

正式静态插件由 `tempo-plugin://` 协议向 HTML 注入 `__tempo__/client.js`。服务 URL 来自独立 origin，
宿主不能修改其 HTML，因此页面必须主动安装同一份 Bridge client。

通用开发服务器可以在应用入口导入 SDK 的开发 Bridge：

```ts
import "@tempo/plugin-sdk/dev";
```

Vite 可使用只在 `serve` 阶段注入该入口的便捷插件：

```ts
import { defineConfig } from "vite";
import { tempoPluginDev } from "@tempo/plugin-sdk/vite";

export default defineConfig({
  plugins: [tempoPluginDev()],
  server: { host: "127.0.0.1" },
});
```

这两个入口只是连接适配器：

- 复用正式 `window.plugin` 客户端和现有 `postMessage` 协议。
- Bridge 安装后发送 `tempo-plugin-ready`，宿主返回 context。
- 不启动 dev server，不执行 build，不修改输出目录。
- Vite 插件只在 `serve` 生效，正式 build 不包含开发 Bridge。
- Bridge 使用页面级 singleton，避免 HMR 重复注册 listener。

如果超时未收到 ready，助手显示 Bridge 未连接和接入示例；不能把 HTTP 200 等同于插件已连接。

### 6.3 静态目录模式

适用于 Vanilla 项目或已经由外部工具生成的静态目录：

1. 默认静态根为项目根，也可选择项目内的其它目录或单独授权的外部构建目录。
2. 助手根据根目录 Manifest 的 `apps[].entry` 解析入口。
3. 入口存在且未逃逸静态根后，将目录登记为开发资源根。
4. `plugin_ui_prepare` 返回 `tempo-plugin://` 开发地址。
5. 现有资源处理器负责内容类型、CSP 和正式 Bridge 注入。

静态目录可以是外部工具已经生成的 `dist`，但开发助手只是连接它，不生成、复制或修复它。静态
文件变化时可以 reload iframe；这是页面重载，不宣传为模块 HMR。

### 6.4 页面生命周期

- Vite HMR 期间保留 iframe、`viewInstanceId` 和插件 session。
- Vite full reload 或静态文件 reload 后保留 view 身份，增加 `documentEpoch`。
- 新 document ready 时清理旧 epoch 的 UI subscriptions 和 pending RPC，再发送 theme、params、
  session 与 API version。
- `PluginAppHost` 同时校验 `event.source === iframe.contentWindow` 和已登记 URL origin。
- 切换 URL、静态根或 plugin ID 时销毁旧 view，再创建新连接。

### 6.5 平台如何打开

连接成功后，开发插件的 Apps 和 Actions 进入现有 Loader。用户从开发助手点击“打开 App”，或从
Tempo 搜索结果触发 Action，都会走现有 App 路由。区别只在 `plugin_ui_prepare` 将正式包地址替换为
当前 URL 或静态开发地址；主面板、独立窗口、params、session 和 Host Bridge 不建立第二套实现。

## 7. Headless 连接与测试

### 7.1 为什么 Headless 不使用服务 URL

Tempo 当前正式 Runtime 由 Runtime Supervisor 启动本地 Node.js 入口，并通过现有 bootstrap 和
stdio RPC 调用 Command。为了让开发测试与正式运行一致，Headless 开发应复用同一条链路。

V1 不设计远程 HTTP/WebSocket Runtime 服务协议。那会产生第二套激活、取消、事件、崩溃恢复和
权限模型，明显超出开发助手的职责。

### 7.2 Runtime 开发入口

Headless 和 Hybrid 项目在连接页选择一个 JavaScript 入口：

- 如果项目根的 Manifest `main` 文件存在，默认选择它。
- 如果使用 TypeScript 或其它编译工具，选择外部 watcher 持续生成的 `.mjs` 或 `.js` 文件。
- 入口可位于项目根内；位于其它构建目录时需要单独授权该目录。
- `.ts`、`.tsx` 和未知扩展名不能作为 Runtime 入口。

Manifest 的 `main` 继续描述正式包内入口；`runtimeDevEntry` 只是当前机器的连接覆盖，不写回
Manifest。开发时没有正式 `dist/main.mjs` 不影响项目存在，只要用户为连接提供了一个可运行的
JavaScript 入口。

### 7.3 连接流程

```text
读取根目录 manifest.json
  -> 校验 headless/hybrid、main 和贡献关系
  -> 校验并授权 runtimeDevEntry
  -> 注册临时开发插件提供者
  -> Runtime Supervisor 使用现有 bootstrap 启动该入口
  -> 等待 Runtime ready
  -> 将 Commands/Actions/Hooks/MCP Tools 接入现有平台
```

助手不启动 TypeScript compiler。若入口尚不存在，状态显示“等待 Runtime 产物”，用户启动自己的
watch 命令后可以重试连接。

### 7.4 Runtime 重连

连接页提供“入口变化后自动重连”开关。开启后，Tempo 只监听已选择的最终 JavaScript 入口：

1. 合并 rename/write 事件并 debounce。
2. 等待文件大小和修改时间短暂稳定，避免读取写到一半的文件。
3. 取消或 drain 旧 Runtime 请求。
4. 停止旧 Node 进程树，再通过同一 Supervisor 启动新入口。
5. 成功后增加 runtime generation；失败时保留日志并等待下一次变化。

这只是重新连接最终入口，不是编译、打包或 Node 模块 HMR。关闭开关后，用户通过“重新连接
Runtime”按钮显式触发同一流程。

### 7.5 Headless 测试面板

Headless 没有 App 页面，因此连接成功后默认进入“测试”页。测试页不复制平台业务逻辑，只提供
真实调用链的入口：

| 贡献 | 测试方式 |
|---|---|
| Command | 选择已声明 Command，输入 JSON object，经 Supervisor 调用并显示结果、耗时和错误 |
| Action | 在 Tempo 主搜索中以开发标记显示，使用真实 text/image/file 输入触发 |
| Hook | 默认只允许手动模拟指定事件；显式开启后才接收真实 Hook |
| MCP Tool | 在助手内调用真实 MCP Bridge 的 schema 校验与 Command 路由，不加入外部 `tools/list` |
| Settings | 使用现有插件设置组件和开发存储命名空间预览 |

Command 测试必须使用与正式调用相同的参数校验、timeout、AbortSignal 和错误码。测试面板不能直接
导入 Runtime 模块或绕过 Supervisor，否则测试结果没有代表性。

### 7.6 Hybrid 测试

Hybrid 连接成功后同时提供“打开 App”和“测试 Runtime”：

- App 内的 `window.plugin.invoke` 调用当前开发 Runtime。
- Headless 测试面板也可以单独调用同一 Command。
- UI URL/HMR 不改变 runtime generation。
- Runtime 重连时 UI 保持打开；调用期间返回 `DEV_RUNTIME_RECONNECTING`，重连完成后继续使用。

## 8. 界面信息架构

```text
┌ 项目选择器 ─ Manifest 状态 ─ 连接/断开 ─ 打开 App ┐
├──────────────┬─────────────────────────────────────┤
│ 概览         │ 当前页面                             │
│ Manifest     │                                     │
│ 开发连接     │ 表单 / 诊断 / 状态 / 测试 / 日志     │
│ 测试         │                                     │
└──────────────┴─────────────────────────────────────┘
```

- UI 项目连接后显示“打开 App”，没有 Runtime 测试页。
- Headless 项目连接后直接显示“测试”，没有 App 按钮和 UI 设置。
- Hybrid 同时显示两者，并分别显示 UI、Runtime 连接状态。
- 不提供文件树、终端、依赖安装、打包、产物检查和发布页面。

### 8.1 概览

- 项目根目录和根 Manifest 状态。
- plugin ID、名称、版本、kind 和兼容结果。
- 当前 UI 地址或静态根。
- 当前 Runtime 开发入口。
- 开发连接状态和被临时覆盖的正式插件版本。
- 最近一次错误和 Runtime generation。

### 8.2 开发连接状态

UI 状态：`未配置`、`等待服务`、`Bridge 未连接`、`已连接`、`已断开`。

Runtime 状态：`未配置`、`等待产物`、`启动中`、`Ready`、`重连中`、`崩溃`。

Hybrid 汇总状态：两端均成功为 `connected`，只有一端成功为 `partial`。状态文本必须指出具体端点，
不能只显示一个模糊的“插件运行失败”。

## 9. 平台接入设计

### 9.1 统一活动插件解析

当前 Loader、Supervisor、Hooks、MCP Bridge、Windows 和 UI Prepare 多处直接通过安装目录定位插件。
需收敛为一个轻量的活动提供者解析：

```rust
resolve_active_plugin(app, conn, plugin_id) -> ActivePlugin
```

```text
ActivePlugin
  source: installed | development
  manifest
  uiSource: package | url | static-root | none
  runtimeSource: package | node-entry | none
  trusted, enabled
  hostStorageNamespace, recommendedDataPath
  devSessionId, runtimeGeneration
```

开发助手只负责向 registry 添加或移除 `development` provider。Loader、UI、Supervisor、Hooks 和 MCP
继续使用原有实现，只把文件来源改为 `ActivePlugin`。这样开发连接不是平行插件平台。

### 9.2 UI 地址解析

`plugin_ui_prepare` 的返回值仍是 `entryUrl`、`viewInstanceId`、theme、params 和 session：

- `package`：现有 `tempo-plugin://` 安装地址。
- `static-root`：开发资源根对应的 `tempo-plugin://` 地址。
- `url`：登记的 loopback URL。

`PluginAppHost` 不需要知道项目目录或构建工具，只按返回地址创建 iframe，并根据当前 URL origin 校验
消息。独立窗口也必须走同一个 prepare 入口，避免主面板能开发而独立窗口仍加载正式包。

### 9.3 Runtime 入口解析

Supervisor 启动 Runtime 时不再自行拼安装包路径，而是读取 `ActivePlugin.runtimeSource`：

- 正式插件使用包根 Manifest `main`。
- 开发插件使用已授权 `node-entry`。

其余 bootstrap、RPC、取消、并发和进程停止逻辑保持不变。

## 10. 日志、数据与安全边界

### 10.1 日志

- 开发 Runtime stdout/stderr 使用持续 drain 的 pipe，避免子进程阻塞。
- 每会话使用有上限的内存 ring buffer，不默认永久写盘。
- 显示 Host 连接事件、Runtime 输出和 UI console；不收集 Vite/TypeScript 外部进程日志。
- 每条事件携带 devSessionId、runtimeGeneration、source、level、timestamp 和 sequence。
- 导出日志前提示可能包含 token、路径和用户数据。

### 10.2 存储语义

开发连接默认给 `storage.plugin.*`、session 和 `ctx.paths.data` 使用独立的宿主管理命名空间。用户可
显式选择正式命名空间，但必须提示数据兼容风险。

这不是文件系统沙箱。含 `main` 的第三方 Runtime 是受信任 Node 进程，可以使用 `fs` 读写进程有
权限访问的任意位置；Tempo 目前不能限制它实际存储的内容和位置。开发助手只能隔离宿主管理的
能力和推荐目录。

### 10.3 安全规则

1. 项目根、静态根和 Runtime 构建目录首次使用时分别授权并 canonicalize。
2. 静态资源相对路径解析后必须仍位于已授权根内。
3. URL 模式仅允许 loopback，并同时校验 iframe window 和 origin。
4. 开发 Runtime 与正式 Runtime 具有相同本机 Node 权限，连接前必须提示。
5. 真实 Hook 默认关闭。
6. 开发 MCP Tool 默认不对外暴露。
7. 前端文件命令只接受 project handle，不接受任意绝对写入路径。

## 11. 数据模型

```sql
CREATE TABLE plugin_dev_projects (
  id TEXT PRIMARY KEY,
  root_path TEXT NOT NULL UNIQUE,
  plugin_id TEXT,
  name TEXT,
  last_opened_at TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE plugin_dev_preferences (
  project_id TEXT PRIMARY KEY,
  ui_source_kind TEXT,
  ui_service_url TEXT,
  ui_static_root TEXT,
  runtime_dev_entry TEXT,
  auto_reconnect_runtime INTEGER NOT NULL DEFAULT 1,
  receive_real_hooks INTEGER NOT NULL DEFAULT 0,
  use_production_data INTEGER NOT NULL DEFAULT 0,
  FOREIGN KEY(project_id) REFERENCES plugin_dev_projects(id) ON DELETE CASCADE
);
```

活动连接、view、Runtime process、日志 buffer 和正式插件恢复描述主要保存在内存。Tempo 重启后项目
记录仍在，但不会自动连接、启动 Runtime 或接收 Hook。

## 12. 后端命令与事件

### 12.1 项目和 Manifest

| Command | 作用 |
|---|---|
| `plugin_dev_list_projects` | 列出最近项目 |
| `plugin_dev_create_project` | 创建根目录和最小 Manifest |
| `plugin_dev_open_project` | 登记包含根 Manifest 的目录 |
| `plugin_dev_forget_project` | 删除项目记录，不删除目录 |
| `plugin_dev_read_manifest` | 返回 raw、parsed、hash 和 diagnostics |
| `plugin_dev_write_manifest` | 带 expectedHash 写入根 Manifest |
| `plugin_dev_validate_manifest` | 调用 Rust Manifest 校验 |
| `plugin_dev_update_preferences` | 保存本机连接设置 |

### 12.2 开发连接和测试

| Command | 作用 |
|---|---|
| `plugin_dev_connect` | 建立 UI、Runtime 或 Hybrid 开发连接 |
| `plugin_dev_disconnect` | 移除开发提供者并恢复正式插件 |
| `plugin_dev_probe_ui_url` | 校验 loopback URL 和 HTTP 连通性 |
| `plugin_dev_reload_ui` | 重载静态页面或重新连接服务页面 |
| `plugin_dev_reconnect_runtime` | 使用当前 JS 入口重启 Runtime |
| `plugin_dev_run_command` | 经正式 Supervisor 路由测试 Command |
| `plugin_dev_simulate_hook` | 显式模拟当前插件 Hook |
| `plugin_dev_run_mcp_tool` | 经真实 MCP Bridge 在助手内测试工具 |
| `plugin_dev_read_logs` | 读取当前开发连接日志 |

没有 build、package、ZIP、依赖安装或通用 shell command。

### 12.3 事件

- `plugin-dev://project-changed`
- `plugin-dev://manifest-diagnostics`
- `plugin-dev://connection-state`
- `plugin-dev://ui-state`
- `plugin-dev://runtime-state`
- `plugin-dev://log`

连接事件包含 projectId、pluginId 和 devSessionId；Runtime 事件增加 generation，UI 文档事件增加
documentEpoch。前端丢弃旧 generation/epoch 的迟到结果。

## 13. 错误与恢复

| 错误 | 行为 |
|---|---|
| 根目录没有 Manifest | 提供创建入口，不搜索子目录 |
| Manifest 无法解析 | 保留原始文件，禁用连接和可视化写入 |
| plugin ID 与正式插件冲突 | 连接前确认临时覆盖，断开后恢复 |
| UI 服务未启动 | 显示等待服务，不要求 `dist` |
| UI Bridge 未就绪 | 显示 SDK 接入方式，不误报连接成功 |
| 静态入口不存在 | 指向具体 `apps[].entry` 和静态根 |
| Runtime JS 入口不存在 | 显示等待产物，不启动编译器 |
| Runtime 启动失败 | 保留 stdout/stderr 和重试按钮 |
| Runtime 重连中被调用 | 返回 `DEV_RUNTIME_RECONNECTING` |
| Hybrid 单端失败 | 保持另一端连接，状态为 partial |
| 正式插件恢复失败 | 保存恢复描述并给出明确修复操作 |
| 项目外部修改冲突 | 不覆盖文件，要求重新载入 |

## 14. 实施阶段

### Phase 1：项目与 Manifest

- 项目创建、打开、最近项目。
- 根目录 Manifest 约束。
- 可视化/JSON 双视图、引用联动和 Rust 真实校验。
- 本机连接设置。

### Phase 2：UI 连接

- URL 和静态目录两种 UI source。
- `ActivePlugin` 开发 provider 与 `plugin_ui_prepare` 统一解析。
- 通用 SDK dev Bridge、Vite 便捷插件和 ready 握手。
- 保留 view 的 HMR/full reload 生命周期。

### Phase 3：Headless 与 Hybrid

- Runtime `node-entry` 开发 source。
- Supervisor 复用、入口自动重连和开发日志。
- Command、Hook、MCP 最小测试入口。
- Hybrid partial 状态与 UI/Runtime 独立生命周期。

不规划开发助手内的构建、打包和发布阶段。

## 15. 验收标准

1. 创建或打开项目后，根目录就是项目，Manifest 始终位于根目录。
2. Manifest 可通过可视化表单完整编辑，外部修改不会被静默覆盖。
3. UI 插件填写本地服务 URL 后，平台可以通过现有 App 路由打开该页面。
4. Vite `dev` 时无需 `dist`；HMR 不重建 iframe 和 `viewInstanceId`。
5. UI 插件选择静态目录后，平台通过 `tempo-plugin://` 打开并获得正式 Bridge。
6. Headless 插件选择 JavaScript 入口后，Runtime 通过现有 Supervisor ready。
7. TypeScript Headless 项目可连接外部 watcher 的 JS 产物，但助手不启动 compiler。
8. Runtime 产物变化只重连 Runtime，不 reload Hybrid UI。
9. Headless Command 测试经过正式参数校验、Supervisor RPC、timeout 和取消链路。
10. 开发 Hook 默认不接收真实事件，开发 MCP Tool 不进入外部 `tools/list`。
11. Hybrid 任一端失败时另一端仍可测试，并明确显示 partial 原因。
12. 同 ID 正式插件在开发连接断开后恢复原启用状态。
13. 项目中不出现助手新增的专用配置或忽略文件。
14. 助手中不存在依赖安装、外部命令执行、构建、打包或发布实现。

## 16. 依赖的现有模块

- `plugins::manifest`：Manifest v1 解析和语义校验。
- `plugins::loader`：Apps 与 Actions 贡献同步。
- `plugins::host`：view、subscription 和 UI event 生命周期。
- `plugins::ui`：`tempo-plugin://`、Bridge 注入和 UI 地址生成。
- `plugins::supervisor`：Runtime bootstrap、Command RPC、取消与 crash 状态。
- `plugins::hooks`：真实和模拟 Hook 路由。
- `plugins::windows`：独立窗口生命周期。
- `plugins::mcp_bridge`：MCP Schema 校验和调用链。
- `@tempo/plugin-sdk`：共享 UI Bridge client 和 Runtime SDK。

开发助手只提供项目、Manifest 和开发 source 的连接入口。插件运行仍由现有插件平台完成，外部工具链
继续负责产生可运行的 URL、静态文件和 Runtime JavaScript。
