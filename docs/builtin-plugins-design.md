# Tempo 内置应用 → 内置插件 详细设计

> 版本：v0.3  
> 状态：详设（**仅设计，不实现**）  
> 前置文档：[插件系统设计](./plugin-system-design.md)（v0.4）  
> 范围：把现有 `BuiltinApp` / 内置快捷操作，收敛为**第一方（first-party）官方插件**，在来源解析后复用同一套贡献归一化 / Registry / Settings / Host Bridge，而不是另起一套「伪插件」机制。  
> 约束：产品仍在开发期，**允许破坏性变更**；不为旧 `builtin:` usage / 静态注册表保留长期兼容层。

---

## 0. 一句话结论

**插件化解决的是「平台与业务解耦 + 统一接入」；第一方业务后端继续用 Rust（拆成独立 crate / 契约），不是改写成 Node。**  
注册、启停、清单与贡献点走插件系统；UI 过渡期允许特权的「宿主进程内 React」；第三方扩展才用 Node Runtime（或自带二进制）。

这回答了 [插件系统设计 §16 开放问题 #2](./plugin-system-design.md)：内置应用**会**迁移为官方插件，与第三方共用 manifest 解析、贡献归一化、Registry 与设置中的生命周期控制面；bundled 与 user package 使用不同的来源解析器。差异在信任档位、**执行后端语言与进程模型**、特权 API、打包与是否允许卸载。

### 0.1 评审后收敛的硬约束

1. **共用的是接入管线，不是同一种可执行加载方式**：第一方 Rust crate 在编译期链接，第三方 Runtime 在运行期启动；两者从 `ResolvedPlugin` 之后共用贡献归一化与 Registry。
2. **manifest 是元数据唯一事实源**：Rust backend registry 与 React binding map 只绑定代码，不重复 name、version、contributes；启动和 CI 都校验所需 binding / backend 存在且没有孤儿项。
3. **来源先于信任**：`tempo.*` 只允许来自只读 bundled catalog；`install_source`、manifest 字段或调用参数都不能自行升级为 first-party。
4. **禁用必须作用于后端入口**：不只隐藏 App / Action；command、Bridge、MCP、旧页面和已缓存调用都必须经同一 enabled gate。
5. **seed 不覆盖用户选择**：升级可更新版本与元数据，但不得把用户禁用的官方插件重新启用。
6. **禁用不删持久数据**：只清理实例、订阅和可丢弃会话；`plugin_storage`、核心表与数据目录仅在明确的重置 / 卸载动作中删除。

---

## 1. Goals / Non-goals

### 1.1 Goals

| # | 目标 | 成功标准 |
|---|------|----------|
| G1 | **统一贡献模型** | 面板里的「应用 / 快捷操作」只来自 Registry；内置与第三方都经 `registerApp` / `registerQuickAction`（owner = pluginId） |
| G2 | **可禁用的官方能力** | 用户可在设置中关闭部分官方插件（如 Hosts、端口管理器），关闭后入口立即消失 |
| G3 | **官方插件验证平台** | 至少 1～2 个官方包使用相同 manifest schema、贡献归一化和 Registry；第一方专属 Rust / React binding 不伪装成第三方 Runtime 模板 |
| G4 | **特权边界清晰** | 待办 / 短语 / 剪贴板等 **Tempo 核心业务数据**仍不向第三方 Host Bridge 开放；仅第一方插件 ID 可调用特权 API |
| G5 | **与现有插件设计对齐** | 不发明第二套给第三方的 Runtime；引入统一 `ResolvedPlugin`，扩展来源、信任档位与 UI entry kind |
| G6 | **迁移可分阶段** | 每一阶段可独立合并：契约 crate → 清单化启停 → 业务迁出宿主 →（可选）sidecar / webview |
| G7 | **Rust 业务解耦** | 内置后端从「散落在 `src-tauri` 宿主」变为依赖 `tempo-plugin-api` 的独立 crate；宿主不再直接拥有业务细节 |

### 1.2 Non-goals

- **不**把已有 Rust 业务改写成 Node（Node 是第三方扩展通道，不是官方业务运行时）
- 本期**不**强制全部内置页改成隔离 WebView（成本高；可永久保留 react-in-host）
- **不**把「设置」做成可卸载 / 可禁用插件（设置是宿主壳）
- **不**把屏幕时间采集线程、全局热键、托盘、Launcher 索引等**宿主基础设施**插件化
- **不**以主进程 `cdylib`/`dlopen` 作为第一方默认加载方式（ABI 脆、共崩溃域；需要隔离时用 sidecar）
- **不**引入细粒度沙箱；第一方与第三方仍是信任模型（见插件设计 §0.4）
- **不**为旧 `source: "builtin"` + 短 id（`todo`）做永久别名层
- **不**在本详设中实现迁移代码或改 Registry

### 1.3 产品动机（Why）

1. **解耦**：内置能力与平台代码（面板、热键、托盘、DB 打开方式、MCP 壳）绑太紧；目标是依赖倒置，而不是换语言。  
2. 插件系统文档已声明官方能力应成为插件样板；现状是前端静态表 + 宿主巨型 `commands/*`，双轨会分叉。  
3. 用户期望「关掉不用的工具」；Hosts / 端口管理器等本就像可选扩展。  
4. 官方包与第三方共用 contributes，才能 dogfood 平台；执行后端可以不同。  
5. MCP 静态 tool 与内置业务强绑定；插件化后业务 tool 归官方包，宿主只留基础设施。

---

## 2. 现状盘点（Research summary）

### 2.1 前端内置注册

| 模块 | 路径 | 行为 |
|------|------|------|
| App 定义 | `src/apps/registry.tsx` | `BUILTIN_APP_DEFS` → `registerApp(BUILTIN_OWNER, …)`，`source: "builtin"`，`ui.type: "react"` |
| Owner 常量 | `src/apps/constants.ts` | `BUILTIN_OWNER = "builtin"` |
| 类型 | `src/apps/types.ts` | `AppSource = "builtin" \| "plugin"`；`TempoAppUi = react \| plugin-webview` |
| 快捷操作 | `src/apps/actions/builtin.ts` + `actions/registry.ts` | `create-todo`、`translate`；直接调 `@/lib/api` |
| 插件贡献同步 | `src/apps/plugins/syncContributions.ts` | 读 `listPluginContributions`，注册 `source: "plugin"` + webview |
| 插件 UI 宿主 | `src/apps/PluginAppHost.tsx` | iframe + `tempo-plugin://` + `postMessage` → `plugin_bridge_invoke` |
| 面板 | `src/pages/CommandPalettePage.tsx` | 统一 `listApps`；usage 仍区分 `builtin:` / `plugin:` |
| 设置 | `src/pages/settings/PluginSettingsSection.tsx` | 仅第三方安装列表；**不含**内置应用 |

### 2.2 Rust 插件宿主

| 模块 | 路径 | 与内置关系 |
|------|------|------------|
| Loader | `src-tauri/src/plugins/loader.rs` | 只扫 `plugins/packages` 中 enabled+trusted 包 |
| Trust / DB | `src-tauri/src/plugins/trust.rs` | `install_source`: local / marketplace / dev_directory（**无 builtin**） |
| Paths | `src-tauri/src/plugins/paths.rs` | `{Tempo}/plugins/{packages,data,…}` |
| Bridge | `src-tauri/src/plugins/bridge.rs` | MVP `host.*`；**无** todos/snippets 业务 API |
| Commands | `src-tauri/src/commands/plugins.rs` | 安装 / 启用 / 贡献列表 |
| MCP | `src-tauri/src/mcp/server.rs` | 待办 / 短语 / 剪贴板 / 番茄 / 报告为**静态** tool；插件走 meta-tool |

### 2.3 内置页对宿主的耦合方式

所有内置页均通过 `@/lib/api` → Tauri `invoke`，与插件 iframe **不同通道**：

```text
内置 React 页  ──invoke──►  Tauri commands  ──►  SQLite / OS
插件 WebView   ──postMessage──►  Host Bridge / Runtime  ──►  有限 host.* 或 Node
```

因此「立刻全部迁到 WebView」会迫使要么：(a) 把整套业务 command 暴露为特权 Host API，要么 (b) 把业务搬进每插件 Node + 自管 DB。本期推荐 (a) 的**受控子集** + 过渡期保留 React-in-host。

---

## 3. 架构总览

### 3.1 决策（Decision）：三档插件，而不是「内置 vs 插件」二元对立

| 档位 | `install_source` | 信任 | UI 形态 | 卸载 | 典型 |
|------|------------------|------|---------|------|------|
| **First-party（内置插件）** | `builtin` | 随 Tempo 安装即信任；无需用户确认对话框 | `react-in-host`（特权）和/或标准 webview | **不可卸载**；多数可禁用 | `tempo.todo`、`tempo.hosts` |
| **Third-party** | `local` / `marketplace` / `dev_directory` | 现有信任流程（hash / 签名） | 仅 `plugin-webview` + 可选 Node | 可卸载 | `com.example.hello` |
| **Host shell（非插件）** | — | — | 宿主 SPA 固定入口 | — | 设置、托盘、热键、采集器、Launcher、护眼 overlay、Shelf |

> **决策**：面板「应用」网格中的官方能力全部升为 First-party 插件；**设置页本身不是插件**，但设置里的「插件」分区同时列出 First-party（可禁用）与 Third-party（可卸载）。

### 3.2 决策：第一方执行后端 = Rust Plugin API（核心解耦）

**问题**：内置后台已是 Rust；改成 Node 不合适；但又必须从宿主里拆出去。

**答案**：**插件化 ≠ 必须用 Node。** 统一的是「接入与生命周期」；第一方的**实现语言与进程模型**可以不同。

| 角色 | 语言 / 形态 | 说明 |
|------|-------------|------|
| 第三方 `main` | 按需 Node Runtime | 见插件系统设计；外人扩展 Tempo |
| 第一方业务后端 | **Rust crate**（默认**同进程**注册） | 解耦 + 保留现有实现；依赖稳定契约 |
| 需要强隔离的官方工具（可选） | 同一契约，实现改为 **Rust sidecar 二进制 + IPC** | 崩溃域独立；非默认 |
| 主进程动态库 `cdylib` | **不做默认路径** | ABI / 卸载 / 共崩溃问题大于收益 |

#### 3.2.1 Crate 划分（目标形态）

```text
src-tauri/                         # 或后续拆 workspace
├── crates/
│   ├── tempo-plugin-api/          # 仅契约：trait、请求/响应、错误码、Host 能力句柄
│   ├── tempo-plugin-todo/         # 待办业务：只依赖 plugin-api（+ rusqlite 等）
│   ├── tempo-plugin-pomodoro/
│   ├── tempo-plugin-clipboard/
│   ├── tempo-plugin-snippets/
│   ├── tempo-plugin-reports/
│   ├── tempo-plugin-hosts/
│   ├── tempo-plugin-translate/
│   └── tempo-plugin-port-manager/
└── src/                           # tempo-host：面板、Bridge、Supervisor、打开 DB、seed
    ├── plugins/                   # Loader / trust / bridge（平台）
    └── …                          # 不再堆业务 SQL 细节
```

依赖方向（严格）：

```text
tempo-plugin-*  ──depends──►  tempo-plugin-api
tempo-host      ──depends──►  tempo-plugin-api  +  各 tempo-plugin-*（编译期链接）
tempo-plugin-*  ──禁止──►  tempo-host 内部模块（托盘 / 热键 / 页面）
```

#### 3.2.2 契约草图

下面是**语义草图，不要求按此签名逐字实现**。关键点是：注册可失败、启停可失败且可取消、调用携带宿主生成的身份，并且业务 crate 不拿原始 `AppHandle` 或全局 SQLite 连接。

```rust
pub trait FirstPartyPlugin: Send + Sync {
    fn id(&self) -> &'static str; // 必须与 bundled manifest 一致
    fn register(&self, registrar: &mut dyn CommandRegistrar)
        -> Result<RegistrationSet, PluginError>;
    fn on_enable(&self, ctx: &PluginContext)
        -> PluginFuture<Result<(), PluginError>>;
    fn on_disable(&self, ctx: &PluginContext)
        -> PluginFuture<Result<(), PluginError>>;
}

pub struct PluginContext {
    pub services: Arc<dyn HostServices>, // 窄接口：事务、事件、通知等
    pub data_dir: ReadOnlyOrScopedPath,
    pub cancellation: CancellationToken,
}
```

- manifest 从 bundled 资源读取并解析一次，是 version、name、contributes、engine range 的唯一事实源；trait 不再返回另一份 manifest。
- `HostServices` 按能力拆成小接口。不得直接暴露 `Arc<Mutex<rusqlite::Connection>>`，否则容易阻塞 async executor、扩大锁粒度并把 schema 所有权重新泄漏给每个 crate。
- 不直接注入 `AppHandle`；需要通知、事件、打开窗口时通过可审计的窄接口。确需 Tauri 能力的例外必须在依赖审查中逐项批准。
- `RegistrationSet` 由宿主持有并负责原子发布 / 回滚，禁止注册失败后留下半套 handler。
- `on_enable` / `on_disable` 必须幂等，并有超时与取消语义；同一 pluginId 的 enable、disable、upgrade 由单线程状态机串行化。

`HostApiHub` 是统一命令路由：Bridge、MCP、快捷操作和旧 Tauri command 适配层都调用 hub，**不知道**具体 SQL。现有 `commands/todos.rs` 等逐步搬进对应 crate，宿主侧只保留参数转换、调用身份和错误映射。

#### 3.2.3 与「特权 Host API」的关系

- 对外（WebView / 第三方）名称仍是 `host.tempo.todos.*` 等（§7）。  
- 对内实现是 `FirstPartyPlugin::register` 挂到 hub，**不是** Node command。  
- react-in-host 页可继续 `@/lib/api` → Tauri command，但 command 实现应只转发到同一 hub（单一业务入口）。  

#### 3.2.4 何时升级为 sidecar

仅当某官方包满足：**可独立发版**、**崩溃不能拖垮 Tempo**、或 **OS 工具极重**（例如极端端口扫描）时，把该 crate 的「可执行入口」打成 sidecar。业务 DTO、错误码和 `host.tempo.<domain>.*` 方法名保持稳定，但 Rust trait 不能直接跨进程；必须增加有版本握手、超时、取消、进程回收和 payload 上限的 IPC adapter。换 transport 不是零成本实现替换。

> **Decision（D9）**：第一方默认 = **同进程 Rust crate 插件**；Node 不用于重写官方业务；sidecar 按包可选；不做 in-process `dlopen`。

### 3.3 决策：ID 命名空间

- 第一方插件包 ID 使用保留前缀 **`tempo.<name>`**。注意当前 `is_valid_plugin_id` 会明确拒绝 `tempo.*`，因此不能直接复用现有单参数校验器。  
- manifest 校验必须接收宿主产生的 `PackageOrigin`：`Bundled` 仅接受编译期 catalog 中的 `tempo.*`；`UserPackage` / `DevDirectory` 一律拒绝 `tempo`、`tempo.*`、`builtin`、`builtin.*`。origin 由只读资源解析器或安装 API 决定，绝不读取 manifest 自报值。  
- App / Action 局部 ID 仍用短 kebab-case（如 `main`、`create`）。  
- 运行时 ID：`{pluginId}/{localId}`，例如 `tempo.todo/main`、`tempo.todo/create`。  
- 废弃全局短 id `todo`、`create-todo` 作为注册主键（破坏性 OK）。面板展示名仍为「待办事项」等中文名。  
- `BUILTIN_OWNER` 常量废弃；每个官方包用自己的 `pluginId` 作为 owner。

### 3.4 决策：UI 双模态（核心）

扩展 `TempoAppUi`（概念上，与插件设计 §2.2 对齐并增补）：

```ts
type TempoAppUi =
  | { type: "react"; component: ComponentType<TempoAppProps> }           // 仅 first-party
  | { type: "plugin-webview"; entryPath: string; localAppId: string }; // 通用
```

| UI 模式 | 谁可用 | 渲染 | 数据访问 |
|---------|--------|------|----------|
| **react-in-host** | 仅 `install_source=builtin` | 继续在宿主 SPA 内替换渲染（现状） | 可继续 `@/lib/api` **或** 逐步改为特权 `host.tempo.*`（同进程封装） |
| **plugin-webview** | 所有插件 | `PluginAppHost` | 仅 Bridge + 本插件 Runtime |

Manifest 侧在**每个 App** 的 `entry` 上显式声明，允许同一包混合 host React 与标准 WebView。现有字符串形式保持为第三方默认路径；对象形式仅限 bundled origin：

```json
"contributes": {
  "apps": [{
    "id": "main",
    "name": "待办事项",
    "entry": { "type": "host-react" },
    "defaultSize": { "width": 920, "height": 720 }
  }]
}
```

```text
entry: "index.html"                  -> 标准 WebView，沿用 manifest v1
entry: { "type": "host-react" }     -> 仅 PackageOrigin::Bundled
```

> **决策**：不使用包级 `uiHost`，也不放一个永远不会加载的占位 `index.html`。Loader 将 entry 归一化为 `webview | host-react` 后下发；第三方声明 `host-react` 必须校验失败。宿主维护 `FIRST_PARTY_REACT_MODULES: Record<runtimeAppId, Component>`，这个表只绑定代码，name / keywords / size 等仍全部来自 manifest。

启动时必须验证：每个 `host-react` contribution 恰有一个 React binding、每个 binding 都有 contribution、binding 对应 pluginId 存在于 bundled backend catalog。CI 使用同一验证器；校验失败时仅隔离该官方包并在设置中显示诊断，不让整个贡献集半注册。

长期（可选）：工具类官方包改为真正的 `index.html`，从白名单删除，dogfood 标准路径。

### 3.5 逻辑架构图

```text
BundledResolver ──> ResolvedPlugin(origin=builtin) ─┐
                                                     ├─> validate + normalize
UserResolver ─────> ResolvedPlugin(origin=user) ────┘          │
                                                               ▼
                                      Registry / Settings / MCP contributions
                                               │
                         ┌─────────────────────┴────────────────────┐
                         ▼                                          ▼
              host-react binding / WebView                    action / command
                         │                                          │
                         └────────────────┬─────────────────────────┘
                                          ▼
                  enabled gate + identity + method allowlist
                                          │
                         ┌────────────────┴────────────────┐
                         ▼                                 ▼
             HostApiHub -> linked Rust crates       Third-party Supervisor
             (optional sidecar adapter)             -> Node / package binary
```

### 3.6 与第三方的差异一览

| 维度 | First-party | Third-party |
|------|-------------|-------------|
| 分发 | 随 Tempo 安装包 / 资源目录 + **链进宿主的 Rust crate** | zip / 目录 / 市场 |
| 执行后端 | **Rust**（同进程 crate；可选 sidecar） | **Node `main`** 和/或自带二进制 |
| 更新 | 随 Tempo 版本；无独立市场更新 | 包级更新事务（插件设计 §8.4） |
| 信任确认 | 跳过；启动 seed 即 trusted | 必须用户确认 |
| Node Runtime | **默认不需要**；仅当某官方包主动带 Node main（不推荐） | 有 `main` 则需要 |
| 特权 API | 可调用 / 实现 `host.tempo.*`（按包白名单） | **禁止**；业务数据自管 |
| 禁用 | 允许（除下方「关键包」策略） | 允许 |
| 卸载 | 禁止（设置项隐藏或 disabled） | 允许 |
| 设置列表分组 | 「官方插件」 | 「已安装插件」 |
| 面板角标 | 「官方」或无角标（产品二选一，见 §9） | 「插件」 |
| 代码签名 | 随应用签名 | Phase 2 起发布者签名 |

---

## 4. Inventory：现有内置 → 建议包装

### 4.1 应用（Apps）

| 当前 id | 名称 | 建议 pluginId | kind | UI 初态 | 是否可禁用 | 依赖 / 备注 |
|---------|------|---------------|------|---------|------------|-------------|
| `todo` | 待办事项 | `tempo.todo` | hybrid | react-in-host | 是* | SQLite todos；MCP 静态 tools；与番茄联动 |
| `pomodoro` | 番茄时钟 | `tempo.pomodoro` | hybrid | react-in-host | 是* | 状态机在 Rust；另有浮动窗（**宿主窗口，非插件 UI**） |
| `reports` | 屏幕使用时间 | `tempo.reports` | ui | react-in-host | 是* | 读 tracker DB；采集线程仍属宿主，禁用只关闭查看 / 暴露 |
| `clipboard` | 剪贴板 | `tempo.clipboard` | hybrid | react-in-host | 是* | hook 源属宿主；禁用时 history consumer 停写，已有历史保留 |
| `snippets` | 快捷短语 | `tempo.snippets` | hybrid | react-in-host | 是* | SQLite；Shelf 选择器仍为宿主窗口 |
| `hosts` | Hosts | `tempo.hosts` | hybrid | react-in-host → 后期 webview | **是（优先）** | 改系统 hosts；适合第一批「像插件」的官方包 |
| `translate` | 聚合翻译 | `tempo.translate` | hybrid | react-in-host → 后期 webview | **是（优先）** | `persistSession`；HTTP；快捷操作入口 |
| `port-manager` | 端口管理器 | `tempo.port-manager` | hybrid | react-in-host → 后期 webview | **是（优先）** | 进程/端口；适合 **Rust crate 解耦**；强隔离时可改 sidecar |
| `settings` | 设置 | — | — | **保持宿主壳** | **否（不插件化）** | 含 `PluginSettingsSection`；必须始终可达 |

\*核心生产力包允许禁用，但设置中应有「恢复默认官方插件」一键；禁用 `tempo.todo` 时，依赖它的快捷操作 / MCP tool 一并消失。

### 4.2 快捷操作（Actions）

| 当前 id | 名称 | 迁入插件 | 局部 action id | command | 说明 |
|---------|------|----------|----------------|---------|------|
| `create-todo` | 创建待办 | `tempo.todo` | `create` | `create` | 现有 `validate`/`title` 函数 → Phase 1 仍可用 react 侧注册的增强 action；声明式路径用 `titleTemplate: "创建待办：{query}"` + command 内校验 |
| `translate` | 聚合翻译 | `tempo.translate` | `open` | — 或 `open-with-query` | 现逻辑是 `openApp("translate", …)`；插件化后 `openApp("tempo.translate/main", { initialTranslateText })` |

### 4.3 明确不插件化的宿主面

| 能力 | 原因 |
|------|------|
| 设置页 | 插件管理 / MCP / 存储根必须始终可进 |
| 全局热键、托盘、更新器 | 进程级基础设施 |
| 屏幕时间采集 / 锁屏检测 | `platform.rs` + tracker 线程 |
| OS 应用 Launcher 索引 | `commands/launcher.rs` |
| 护眼 overlay、番茄浮动窗、Shelf picker | 独立 Tauri 窗口，不是面板 App |
| MCP HTTP Server 本身 | 宿主服务；tool 可逐步由官方插件贡献 |

### 4.4 建议的仓库布局

```text
builtin-plugins/                    # 源码与 manifest（随 repo）
  tempo.todo/
    manifest.json
    README.md                       # 可选；说明特权 API
    # host-react entry 不需要 index.html
  tempo.hosts/
    manifest.json
    index.html                      # 后期迁移目标
    main.mjs
  …
src/apps/first-party/
  modules.ts                        # pluginId/localId → React 组件映射
  seed.ts                           # 前端侧仅类型/文档；真正 seed 在 Rust
src-tauri/resources/builtin-plugins/  # 构建时复制，供安装包嵌入
```

运行时**不**把 first-party 复制到 `{Tempo}/plugins/packages`。复制会制造用户可写副本、双份版本清理和卸载误删风险。统一 Loader 拆为两步：

```text
BundledResolver(read-only resources + linked backend catalog) ─┐
                                                               ├─> Vec<ResolvedPlugin>
UserPackageResolver({Tempo}/plugins/packages) ──────────────────┘
       -> validate by origin -> normalize contributes -> Registry / Settings / MCP
```

`ResolvedPlugin` 至少携带 `origin`、canonical manifest、resolved resource root、integrity metadata、enabled state 与 backend kind。后续管线不再自行拼接 `packages/{id}/{version}` 路径。

编译期 backend catalog 只保存 `{ pluginId, factory, defaultEnabled }` 这类宿主策略 / 代码绑定；不得复制 manifest 文案或贡献点。构建脚本可由该 catalog 生成 Rust inventory，并与 bundled manifests 做集合校验。

> **决策**：双来源解析、单一归一化管线。DB 仍有 `plugins` / `plugin_versions` 行保存启用状态与诊断；`install_source=builtin` 的资源解析永远指向只读 bundled root。第三方导入器在 publish 前拒绝保留命名空间，任何 user package 都不能遮蔽 bundled package。

---

## 5. Manifest / SDK 形态

### 5.1 First-party manifest 示例（react-in-host）

```json
{
  "manifestVersion": 1,
  "id": "tempo.todo",
  "name": "待办事项",
  "version": "1.2.0",
  "engines": { "tempo": ">=1.2.0", "pluginApi": "^1.0.0" },
  "kind": "hybrid",
  "publisher": "tempo",
  "description": "官方待办：列表、子任务、与番茄联动。",
  "capabilities": [],
  "contributes": {
    "apps": [
      {
        "id": "main",
        "name": "待办事项",
        "keywords": ["todo", "任务", "待办"],
        "icon": "icons/app.svg",
        "entry": { "type": "host-react" },
        "defaultSize": { "width": 920, "height": 720 }
      }
    ],
    "actions": [
      {
        "id": "create",
        "name": "创建待办",
        "keywords": ["todo", "待办"],
        "icon": "icons/app.svg",
        "requiresQuery": true,
        "titleTemplate": "创建待办：{query}",
        "command": "create"
      }
    ],
    "commands": [
      { "id": "create", "title": "创建待办", "visibility": "private" },
      { "id": "mcp-list-todos", "title": "列出待办", "visibility": "private" },
      { "id": "mcp-create-todo", "title": "通过 MCP 创建待办", "visibility": "private" }
    ],
    "mcpTools": [
      {
        "name": "list_todos",
        "description": "列出 Tempo 待办事项。",
        "command": "mcp-list-todos",
        "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
      },
      {
        "name": "create_todo",
        "description": "在 Tempo 中创建待办事项。",
        "command": "mcp-create-todo",
        "inputSchema": {
          "type": "object",
          "properties": { "title": { "type": "string" } },
          "required": ["title"],
          "additionalProperties": false
        }
      }
    ]
  }
}
```

说明：

- `entry: { "type": "host-react" }` 是现有 `ContributedApp.entry: String` 的显式扩展；package verifier 不要求 `index.html`，但只对 `PackageOrigin::Bundled` 放行。  
- 有 `mcpTools` / `actions→command` 但无 Node `main` 时，命令由第一方 Rust `FirstPartyPlugin::register` 挂到 `HostApiHub`，不启动 Node。第三方无 `main` 仍不得贡献需要 Runtime command 的 action / MCP tool。  
- 该示例必须作为测试 fixture 通过实际 Rust manifest parser 与 entry verifier，避免设计示例和实现 schema 再次漂移。

### 5.2 SDK

- 第三方：继续 `@tempo/plugin-sdk`（`packages/plugin-sdk`）。  
- 第一方 react-in-host：**不强制**走 `window.plugin`；可保留现有 React 代码，但 `@/lib/api` 只能调用转发到 hub 的薄 adapter，不能保留直达业务实现的旁路。  
- 若官方包改为 webview：同一 SDK + 特权方法仅在 Bridge 鉴权通过时可用（`ConnectionContext.trust_tier == FirstParty`）。

### 5.3 前端类型演进

```ts
type AppSource = "plugin";
type PluginOrigin = "builtin" | "local" | "marketplace" | "devDirectory";
type TrustTier = "firstParty" | "thirdParty";

interface TempoApp {
  id: string;              // tempo.todo/main
  source: AppSource;
  pluginId: string;        // tempo.todo（官方也必填）
  origin: PluginOrigin;
  trustTier: TrustTier;
  // ...
}
```

> **决策**：实现阶段删除 `source: "builtin"`，统一 `source: "plugin"`；`origin` 表示资源来源，`trustTier` 表示授权档位，两者由 Rust resolver 下发，前端不得按 pluginId 字符串自行推导。`PluginContributionBundle` 同步增加 `origin`、`trustTier` 与每个 App 的归一化 entry kind。

设置侧 `InstalledPlugin` 另增 `availabilityState`；`enabled=true + availabilityState=unavailable` 表示“用户希望启用，但当前构建无法提供”，不能渲染成普通关闭状态。

---

## 6. Lifecycle

### 6.1 Discovery（发现）

```text
Tempo 启动
  → migrate/ensure plugin tables
  → BundledResolver：解析 manifest + backend catalog + React bindings
  → validate_builtin_catalog()：ID、版本、entry、binding、backend 集合一致
  → reconcile_builtin_plugins()（单个 SQLite 事务）：
       新 ID：INSERT plugins(enabled=该包默认值)
       已有 ID：UPDATE current_version / metadata，保留 enabled
       UPSERT plugin_versions(version=manifest.version,
              install_source='builtin', trusted_at=now,
              signature_status='builtin', package_hash=build-time digest)
       已从 catalog 移除的旧 builtin：availability_state=unavailable，保留 enabled 偏好，不解析 user path
  → resolve_enabled_plugins(): bundled(enabled) ∪ user packages(enabled+trusted)
  → normalize_contributions()：全包校验成功后原子发布 bundle
  → 前端 startPluginContributionSync()
  → 前端按 runtimeAppId 绑定 host-react 组件
```

禁止 `UPSERT ... enabled=DEFAULT`。默认值只在首次发现 ID 时使用；升级、重启和 manifest 版本变化都必须保留用户选择。`current_version` 只取 manifest 的精确 `version`，不允许“Tempo 版本或包版本”二选一。

`enabled` 表示用户偏好，新增 `availability_state = available | unavailable` 表示当前构建能否提供该包，现有 `runtime_state` 继续只表示 `disabled | enabled | starting | active | draining | failed`。三个维度不能互相覆盖；包恢复可用时按保留的 enabled 偏好恢复生命周期。

### 6.2 Load order（加载顺序）

1. Host shell 就绪（DB、热键、tracker）  
2. 解析并校验 first-party catalog  
3. 事务 reconcile DB 状态  
4. 扫描贡献（声明式，不执行插件代码）  
5. 后端对每个 bundle 完成校验后原子替换 Registry  
6. 前端合并 Registry 并绑定 host-react component  
7. 第三方 Runtime 仍懒激活（现规则）

冲突：若用户包 ID 撞 `tempo.*` → **安装拒绝**（保留命名空间）。  
App 运行时 ID 冲突：整包注册失败（插件设计 §4.1）。

单个 bundled 包无效时进入 `unavailable` 并记录 `last_error`；其它包继续加载。禁止把损坏 bundled 包回退解析为同 ID 的 user package。

### 6.3 Activation

| 类型 | 激活 |
|------|------|
| react-in-host 官方 App | 打开即渲染组件；无 Node |
| 声明式 action → 特权 command（无 main） | **Rust crate** 在 hub 上执行（同进程） |
| 官方 hybrid 含真正 `main.mjs` | **不推荐**；仅当官方包刻意 dogfood 第三方路径时使用 |
| `onStartup` | 官方默认**禁止**（除非明确批准的包，如未来剪贴板增强）；避免拖慢启动 |

### 6.4 Enable / Disable

- 设置 → 官方插件列表 → Switch，复用 `set_plugin_enabled_command`。  
- 所有调用入口先执行统一 gate：`plugin exists && origin=builtin && enabled && availability_state=available && runtime_state∉{draining,failed}`。快捷操作、Bridge、MCP 和旧 Tauri command adapter 不得各自猜测状态。  
- Disable 流程按 pluginId 串行化：`mark draining → reject new calls → close host-react / webview UI → cancel or drain in-flight calls → on_disable(timeout) → unregister contributes → persist disabled`。失败时仍强制收口为 disabled，并记录可诊断错误。  
- Enable 流程：先持久化用户意图 `enabled=true, runtime_state=starting`，再执行 `validate catalog/integrity → on_enable → atomically publish handlers + contributes`；失败时回滚注册、保留 `enabled=true` 并置 `runtime_state=failed` 以便诊断 / 重试，不能留下“入口可见但后端不可用”的半启用状态。  
- `plugin_storage` 与数据目录在 disable 时保留；当前 `set_plugin_enabled_command` 中的 `storage::delete_all` 必须移出禁用路径。可丢弃的 UI session 按产品策略清理。  
- `on_disable` 必须注销该包拥有的 timer、hook、事件 consumer 和后台写入任务；宿主级事件源可继续运行，但不能继续替已禁用插件产生业务数据。  
- `tempo.clipboard` 的默认语义是禁用后停止新增剪贴板历史，已有历史保留；若底层 watcher 仍被其它宿主能力使用，只移除 history consumer。  
- `tempo.reports` 是明确例外：屏幕时间 tracker 属于宿主核心基础设施，禁用只隐藏报告 UI / API / MCP，不停止全局采集；设置文案必须直说。`tempo.pomodoro` 禁用时应关闭其宿主浮动窗并停止状态机。  
- restart 后以持久化 `enabled` 为准；seed 不得改变它。  
- **不可禁用候选（Open，见 §10）**：是否强制 `tempo.todo` 始终启用。详设默认：**全部可禁用**，但设置入口与「恢复默认」永远在宿主设置壳。

### 6.5 Uninstall

- First-party：`plugin_uninstall` 返回 `FORBIDDEN` / UI 不展示卸载。  
- 卸载 API 必须先读取并校验 origin，再停止 Runtime、清 session、删 DB 或移动目录；不能在 destructive steps 之后才判断 builtin。  
- 数据：禁用**不**删除核心 SQLite 表（todos 等仍在）；仅贡献入口消失。  
- 若未来官方包使用 `plugin_storage` / `plugins/data/tempo.*`，禁用可保留数据；「重置官方插件数据」为独立危险操作。

### 6.6 Update

- 随 Tempo 升级替换 bundled 资源；reconcile 以 manifest `version` 更新 `current_version`，保留 enabled。  
- 不走第三方 pending_version 确认流（已信任）。  
- `engines.pluginApi` 在 CI / 构建时是强约束，运行时仍复评以防打包错误；不兼容则置 `availability_state=unavailable` + 设置页诊断并保留 enabled 偏好，而不是伪装成用户主动 disabled。回滚随 Tempo 应用版本回滚，不做独立 first-party pending version。

---

## 7. 特权 Host API（First-party only）

### 7.1 原则

插件设计 §0.3 / §7.1 已规定：**不把待办等业务 API 暴露给第三方**。本详设保持该原则，并明确第一方例外通道：

```text
host.tempo.<domain>.<method>
```

鉴权：`ConnectionContext` 增加宿主构造的 `origin` / `trust_tier`；仅 `FirstParty`、目标插件 enabled 且 `plugin_id` 在方法级 allowlist 内可调用。第三方得到 `FORBIDDEN`。

身份来源必须满足：

- WebView UI：由已登记的 `view_instance_id` 反查 pluginId / origin，禁止无 view 的 UI 调用自报身份。
- Runtime / sidecar：由 Supervisor 完成握手后构造，禁止从 RPC payload 接受 origin / trust tier。
- react-in-host：它属于可信宿主 UI，不伪装成某个插件连接；通过 `CallOrigin::HostUi` 的内部 adapter 调 hub，同时仍按**目标 pluginId**执行 enabled gate。
- DB 中的 `install_source` 和 `signature_status` 用于状态与展示，不单独构成授权证据；first-party 身份还必须命中本次构建的 bundled catalog。

错误码固定区分：身份 / allowlist 不通过返回 `FORBIDDEN`；用户关闭返回 `PLUGIN_DISABLED`；catalog、engine 或 migration 不可用返回 `PLUGIN_UNAVAILABLE`；启动失败沿用 `ACTIVATION_FAILED`。先做身份授权再返回状态，避免第三方借错误差异探测特权方法。

### 7.2 建议的方法域（按包拆分 allowlist）

| 域 | 允许的 pluginId | 示例方法 | 对应现状 |
|----|-----------------|----------|----------|
| `host.tempo.todos.*` | `tempo.todo`（只读摘要可给 `tempo.pomodoro`） | `list`, `get`, `create`, `update`, `delete`, … | `commands/todos.rs` |
| `host.tempo.snippets.*` | `tempo.snippets` | CRUD、copy | `commands/snippets.rs` |
| `host.tempo.clipboard.*` | `tempo.clipboard` | `listHistory`, … | `commands/clipboard.rs` |
| `host.tempo.pomodoro.*` | `tempo.pomodoro` | `getState`, `start`, `pause`, … | `pomodoro` commands |
| `host.tempo.reports.*` | `tempo.reports` | `daily`, `weekly` | `commands/reports.rs` |
| `host.tempo.hosts.*` | `tempo.hosts` | workspace / apply | `hosts` commands |
| `host.tempo.ports.*` | `tempo.port-manager` | list / terminate | port-manager commands |
| `host.tempo.translate.*` | `tempo.translate` | `translate`, config | translate commands |

react-in-host 阶段：页面可继续调用薄 Tauri adapter；adapter 必须转到同一 hub 并指定目标 pluginId，不能直接访问旧业务实现或绕过 enabled gate。同时 Bridge 实现同一套，供未来 webview 复用。  
**禁止**把整表 `invoke` 反射给插件。

### 7.3 MCP 迁移策略

| 阶段 | 行为 |
|------|------|
| Phase A | MCP 静态 tools 保持；与官方插件 enable 状态**联动**（禁用 `tempo.todo` → tool list 不再返回；已缓存 tool 的调用也返回结构化 `PLUGIN_DISABLED`） |
| Phase B | 静态实现改为「调用与 `tempo.todo` 相同的 `HostApiHub` 方法」 |
| Phase C | 改为官方包 `contributes.mcpTools`；可见条件固定为 `MCP 总开关 && plugin enabled`，本期不复用第三方逐插件 exposure 开关 |

第三方 MCP 仍默认不暴露，需用户开关（现状）。

### 7.4 跨官方包调用

- 禁止业务 crate 直接查询另一个插件拥有的表；这会绕过 enabled gate、数据所有权和未来迁移边界。`tempo.pomodoro` 读取待办摘要时，经 `HostApiHub` 的 typed service / command 调用，并显式列入 allowlist。进程内实现可以是函数调用，不要求真的序列化 RPC。  
- 跨包依赖默认是 optional：`tempo.todo` 禁用后，番茄仍可使用，但隐藏待办绑定并对旧关联显示“待办功能已停用”。若未来出现 hard dependency，必须进入 manifest schema，并由启停状态机阻止非法组合；本期不暗含 hard dependency。  
- 面板 `app.open`：任何插件可打开**已注册且 enabled**的 App（含官方）；不因此获得数据 API。

---

## 8. Storage / Permissions / Security

### 8.1 信任模型

| 项 | First-party |
|----|-------------|
| 安装确认文案 | 不显示第三方「将在本机执行代码」对话框 |
| `signature_status` | `builtin`（新枚举值） |
| first-party 判定 | bundled resolver + 编译期 catalog；不能只信 DB / manifest / pluginId 前缀 |
| 包篡改 | bundled 资源随应用签名，并记录构建期 digest 用于损坏诊断；user 不可替换 `tempo.*` ID |
| 权限沙箱 | 无；与插件设计一致 |

### 8.2 数据归属

| 数据 | 位置 | 说明 |
|------|------|------|
| 待办 / 短语 / 剪贴板历史 / 报告 / 设置 | Tempo 核心 `tempo.db` | **不**迁入 `plugin_storage` |
| 插件私有 KV | `plugin_storage` | 官方包一般不用；工具类可选用；禁用时保留 |
| 插件数据目录 | `plugins/data/{id}` | sidecar / Node 或需要独立文件的官方包使用；禁用时保留 |
| 会话 | 现有 session / `plugin_sessions` | react-in-host 可继续用 `src/apps/session.ts`；webview 用插件会话表 |

#### 8.2.1 Schema / migration 所有权

- 通用插件表、设置、宿主采集基础表由 host core 维护；todos、snippets、clipboard history 等领域表的 migration 与查询代码随对应 `tempo-plugin-*` crate 维护。
- 第一方 backend catalog 在构建时汇总各 crate 的 namespaced migration（如 `tempo.todo/0003`）；宿主在 resolver / enable 之前统一排序并事务执行。migration 不依赖插件当前 enabled，因为降级启用状态不能回滚 schema。
- migration 只允许追加且必须幂等；失败时本次应用启动进入可诊断的 degraded mode，对应包 `availability_state=unavailable`，不得发布其 handlers / contributes。
- 跨领域外键和直接读表默认禁止；确需共享的数据结构提升为 host-owned schema / typed service，避免两个 crate 同时拥有同一张表。
- first-party 数据随 Tempo 备份与应用版本回滚策略处理，不复用第三方包的独立 data migration 协议。

### 8.3 安全边界（三类，沿用插件设计 §0.4）

1. 系统权限：第一方 react-in-host 与 Tempo 同进程，**天然同权**（比第三方 WebView 更大攻击面——故必须是编译进宿主的受控代码）。  
2. 进程隔离：官方 Node main（若有）仍每插件一进程。  
3. UI Bridge：第三方仍无 Tauri IPC；第一方 react 页例外（已知且接受），但业务调用仍经过 enabled gate，避免“界面已禁用、旧 invoke 仍可写数据”。

**风险接受**：react-in-host 意味着官方 UI 代码漏洞 = 宿主漏洞。这与今天完全一致；插件化不恶化、也不自动改善。

---

## 9. UI / UX

### 9.1 命令面板

- 「应用」区：官方 + 第三方同一网格（已基本如此）。  
- 角标：**决策推荐**——官方不打角标（减少噪音）；第三方打「插件」。设置里用分组区分。  
- 搜索 keywords 继续来自 manifest。  
- usage key：统一 `plugin:{runtimeAppId}`；迁移时丢弃旧 `builtin:`（破坏性 OK）。  
- 打开逻辑：`ui.type === "react"` → 现有组件树；`plugin-webview` → `PluginAppHost`。

### 9.2 设置 → 插件

```text
插件运行时（Node）…          # 现有

官方插件
  [开关] 待办事项    tempo.todo     不可卸载
  [开关] Hosts       tempo.hosts
  …

已安装插件
  [开关] Hello 示例  com.example…  [卸载]
```

- 官方组展示：名称、简介、贡献点数量、availability 与 backend 状态；仅有 Node `main` 时再展示 Node Runtime 状态。  
- 「恢复默认官方插件」：恢复编译期 catalog 的逐包 `defaultEnabled`，不是无条件把全部 first-party 置为 `enabled=1`。
- `failed` / `unavailable` 不能伪装成关闭开关；展示不可用原因与诊断，避免用户反复切换无效。

### 9.3 设置 App 入口

- 面板不再注册 `settings` 为 TempoApp；改为：  
  - 托盘 / 面板固定「设置」命令（宿主 action，owner = `host`），或  
  - 保留搜索关键词「设置」但 `open` 走宿主 API `open_settings_app()`。  
- **决策推荐**：宿主注册一条 **非插件** 的固定 palette 项（不进 plugins 表），避免用户禁用掉设置。

---

## 10. Risks & Open questions

### 10.1 风险

| 风险 | 影响 | 缓解 |
|------|------|------|
| react-in-host 与标准 WebView 双轨过久 | 标准 UI 路径未被官方验证 | Phase 3 至少迁 1 个工具包到标准 WebView；其它包可永久保留 React |
| 业务 crate 仍偷偷依赖 host 内部 | 解耦失败 | CI：deny 依赖 / 模块边界检查；`plugin-api` 保持极瘦 |
| schema 仍由 host 或多个 crate 共同拥有 | 升级顺序不确定、跨包耦合 | namespaced migration 汇总；单表单 owner；加载前事务执行 |
| 特权 API 面膨胀 | 维护成本；误暴露给第三方 | 方法级 allowlist + 集成测试 |
| 过早上 sidecar / cdylib | 复杂度暴涨 | 默认同进程；隔离按包评估 |
| 禁用核心包导致 MCP / 快捷操作空洞 | 用户困惑 | 设置文案；禁用确认；恢复默认 |
| seed 覆盖用户 enabled | 每次升级后插件被意外重开 | 首次 INSERT 才写默认值；升级 / 重启测试 |
| 双来源解析产生两个事实源 | manifest、backend、React binding 漂移 | 单一 manifest + catalog 集合一致性校验 |
| 只隐藏贡献、未关闭后端 | 旧页面 / MCP 仍可读写已禁用业务 | 所有入口统一 enabled gate + 并发停用测试 |
| 错信 `install_source` / 自报 pluginId | 第三方获得特权 API | origin 由 resolver / Supervisor 构造；reserved ID 安装测试 |
| bundled 行残留或同名 user 包遮蔽 | 升级后加载错误资源 | stale builtin 标 `availability_state=unavailable`；永不回退 user root |
| App id 变更导致用户习惯 / 外部文档失效 | 文档、MCP 示例 | README / MCP 说明同步；接受 usage 重置 |
| 官方包贡献 MCP 后与静态 tool 重复 | 工具列表重复 | Phase B 单路径 |

### 10.2 剩余产品问题（不再混入技术契约）

| # | 问题 | 详设默认倾向 |
|---|------|--------------|
| Q1 | 核心包（todo / clipboard / snippets / pomodoro / reports）是否允许禁用？ | 允许，带恢复默认 |
| Q2 | 面板是否给官方打「官方」角标？ | 不打；仅第三方打「插件」 |
| Q3 | 设置是否保留为可搜索的「应用」外观？ | 宿主固定项，外观可像应用但不进 plugins 表 |
| Q4 | `tempo.pomodoro` 浮动窗是否算插件贡献的 window？ | 否，维持宿主窗口；插件设计 §16#1 另案 |
| Q5 | 禁用剪贴板插件是否停止新增历史？ | 是；保留已有历史，底层 watcher 可供其它宿主能力复用 |

已冻结的技术选择不再列为开放问题：first-party manifest `version` 为唯一包版本（当前各包可与 Tempo semver 同步，未来独立版本无需改 schema）；官方 MCP 可见条件为总开关 + plugin enabled；Phase 1 删除 `source: "builtin"`。

---

## 11. 推荐分期与里程碑

### Phase 0 — 契约冻结（0.5～1 周）

- [ ] 确认 §10.2 Q1–Q5  
- [ ] 冻结 pluginId 列表与 runtime id 规则  
- [ ] 冻结 `tempo-plugin-api` 的 `FirstPartyPlugin` / `HostApiHub` / `PluginContext` 边界（哪些窄能力进入 services）  
- [ ] 冻结 `PackageOrigin`、`ResolvedPlugin`、App entry union 与统一 enabled gate  
- [ ] 在 `plugin-system-design.md` 标注开放问题 #2 已由本文回答  
- **里程碑**：ID / origin / entry kind / **Rust 后端模型（D9）** 书面批准  

### Phase 1 — `tempo-plugin-api` + 清单化启停（业务可仍在原文件，但开始转调）

- [ ] 新建 `tempo-plugin-api` crate（trait + 空 hub）  
- [ ] Rust：origin-aware ID / manifest 校验、`install_source=builtin`、BundledResolver、事务 reconcile  
- [ ] 每个官方能力一份 `manifest.json`（可先只有 apps/actions 元数据）；示例进入 parser fixture  
- [ ] 前端：去掉 `BUILTIN_APP_DEFS` 静态唯一来源；改为「贡献同步 + react module map」  
- [ ] 删除 `source: "builtin"`；usage 改 `plugin:`  
- [ ] 设置页「官方插件」分组 + 禁用  
- [ ] 设置从 App Registry 挪到宿主固定入口  
- [ ] 修正 disable 删除 `plugin_storage`、uninstall 在 origin guard 前执行 destructive steps 的现有行为  
- **里程碑**：禁用 `tempo.hosts` 后面板入口消失且后端调用被拒绝；重启后仍保持禁用；待办 UI 仍为现有 React 页  

### Phase 2 — 业务迁入 Rust 插件 crate + 特权 API / MCP

- [ ] 按域拆出 `tempo-plugin-todo` 等 crate；`commands/*.rs` 业务逻辑迁入；宿主只注册 + 转发  
- [ ] 领域 migration 迁入对应 crate，由 host migration runner 在加载贡献前统一执行  
- [ ] Bridge：`host.tempo.*` → `HostApiHub` + allowlist + enabled gate  
- [ ] Action / MCP / 旧 Tauri adapter 随官方插件 enable 显隐，经同一 hub  
- [ ] 集成测试：第三方调用 `host.tempo.todos.*` → FORBIDDEN；disabled 后所有入口拒绝；crate 不依赖 host UI 模块  
- **里程碑**：至少一个核心域（建议 `hosts` 或 `todo`）业务代码不在 host 根模块树内  

### Phase 3 — Dogfood 标准 WebView 接入形态

- [ ] 迁移 `tempo.hosts` / `tempo.port-manager` / `tempo.translate` 之一到真 `index.html`（UI 解耦），**后端仍用 Rust crate**  
- [ ] 使用与第三方相同的 WebView / SDK / session / Bridge 路径，仅 `host.tempo.*` 授权不同  
- [ ] 文档标明：官方样板验证 manifest / UI / Bridge 接入；第三方 backend 模板仍以 Node 为准  
- **里程碑**：至少一个官方包端到端经过标准 WebView 路径；不能以 sidecar 替代此验收  

### Phase 4 — 收敛（可选）

- [ ] 评估核心包是否迁 webview（可永久 react-in-host）  
- [ ] 按实际隔离需求评估 sidecar；若采用则实现版本化 IPC adapter  
- [ ] MCP 全面改为 `contributes.mcpTools`  
- [ ] 清理仅页面使用的 invoke，统一经 hub  
- [ ] 评估 workspace 物理目录是否迁出 `src-tauri/crates` |
---

## 12. 与现有模块的映射（实现时对照）

| 改动点 | 文件 / 模块 |
|--------|-------------|
| 静态 App 表拆除 | `src/apps/registry.tsx` |
| React 映射 | 新建 `src/apps/first-party/modules.tsx` |
| 快捷操作 | `src/apps/actions/builtin.ts` → 各包贡献或 first-party bootstrap |
| 贡献同步 | `src/apps/plugins/syncContributions.ts`（识别 firstParty + 填充 react ui） |
| 面板 | `src/pages/CommandPalettePage.tsx`（settings 入口、角标、usage） |
| 设置 UI | `PluginSettingsSection.tsx` 增官方分组 |
| Resolver / reconcile | `src-tauri/src/plugins/loader.rs`, `trust.rs`, `paths.rs`, `ids.rs`, `manifest.rs`, `package.rs` |
| Bridge 特权 / Hub | `src-tauri/src/plugins/bridge.rs` + `HostApiHub` + 各 `tempo-plugin-*` |
| 契约 crate | 新建 `tempo-plugin-api` |
| 业务 crate | 新建 `tempo-plugin-{todo,hosts,…}`；迁出原 `commands/*.rs` 业务 |
| 插件命令 | `src-tauri/src/commands/plugins.rs`（禁止卸载 builtin） |
| MCP | `src-tauri/src/mcp/server.rs` |
| 示例 / 资源 | `builtin-plugins/tempo.*`, `examples/plugins/…` |
| 类型 | `src/apps/types.ts`, `src/types` 中 `InstalledPlugin` |

### 12.1 当前实现与本设计的必改差异

| 当前点 | 现状 | 实现要求 |
|--------|------|----------|
| `plugins/ids.rs` | `is_valid_plugin_id` 明确拒绝 `tempo.*` | 改为接收 `PackageOrigin` 的校验；外部导入仍必须拒绝保留前缀 |
| `plugins/manifest.rs` | `ContributedApp.entry` 仅为字符串，且固定 `index.html` | 支持 `index.html` 或 `{type:"host-react"}`，后者仅 bundled；未知行为字段不得静默忽略 |
| `plugins/package.rs` | 所有 UI 包都强制根级 `index.html` | verifier 按归一化 entry kind 校验；host-react 不造空壳文件 |
| `plugins/loader.rs` | 对所有 DB 行拼接 user `packages/{id}/{version}` | 先按 origin 解析为 `ResolvedPlugin`，后续不得再猜路径 |
| `InstalledPlugin` / `PluginContributionBundle` | 无 origin / trustTier / availability / entry kind | Rust DTO 与 TS 类型同步增加；React binding 只按 runtimeAppId 绑定 |
| `plugin_call_command` | 只路由 Node Supervisor | 先路由 first-party hub，再路由第三方 Runtime；两者都过 enabled gate |
| `plugin_bridge_invoke` / `ConnectionContext` | 无 first-party trust tier | 身份由 view registry / Supervisor / bundled catalog 构造，不接受 payload 自报 |
| `set_plugin_enabled_command` | disable 会 `storage::delete_all` | 移出禁用路径；仅 reset / uninstall 显式删除 |
| `plugin_uninstall` | 尚无 builtin origin guard | 任何 stop / DB delete / 文件移动前先拒绝 builtin |

这张表是 Phase 1 / 2 的迁移约束，不表示在本文中实现代码。

---

## 13. 验收标准（相对本详设）

1. 启动后无代码路径再依赖 `BUILTIN_OWNER` 静态注册作为唯一来源。  
2. `listPlugins` 能列出 `install_source=builtin` 的官方包；卸载 API 在任何 destructive step 前返回 `FORBIDDEN`。  
3. 第三方导入 `tempo.*` / `builtin.*` 必须失败；修改 manifest、DB `install_source` 或调用参数不能获得 first-party。  
4. 禁用官方包后 App / Action 消失，host-react 页面关闭，command / Bridge / MCP / 旧 invoke adapter 均拒绝新调用；重启后仍保持禁用。  
5. seed / Tempo 升级更新 manifest version 时保留用户 enabled；首次出现的新包才使用 `defaultEnabled`。  
6. disable 不删除 `plugin_storage`、核心表或数据目录；显式 reset 另测。  
7. 第三方插件调用任意 `host.tempo.*` 返回 `FORBIDDEN`；first-party 调用未在方法 allowlist 中的域也返回 `FORBIDDEN`。  
8. 设置始终可通过宿主入口打开（即使所有官方插件禁用）。  
9. bundled manifest、Rust backend catalog、host-react bindings 不一致时，该包原子进入 `availability_state=unavailable`，其它包照常加载，Registry 无半注册。  
10. 文档中的 first-party manifest fixture 通过实际 parser / verifier；`index.html` WebView 旧格式继续通过。  
11. Phase 2：至少一个官方业务域的实现与 namespaced migrations 位于独立 crate，不依赖宿主 UI / 托盘模块，也不直接读取其它插件拥有的数据。  
12. 禁用 `tempo.clipboard` 后不再新增历史；禁用 `tempo.reports` 后 tracker 仍按宿主设置采集，二者 UI 均明确说明。  
13. Phase 3：至少一个官方包端到端使用标准 WebView / SDK / Bridge 路径；sidecar 不能替代该项。  

---

## 14. 文档关系

| 文档 | 职责 |
|------|------|
| [plugin-system-design.md](./plugin-system-design.md) | 通用插件平台（**第三方** Node Runtime、信任、Bridge MVP、市场） |
| **本文** | 第一方包装、**Rust 解耦模型**、特权 API、迁移分期 |
| README | 实现落地后补充「官方插件 / 禁用」用户说明（非本详设任务） |

---

## 附录 A：决策摘要

| 编号 | 决策 |
|------|------|
| D1 | 内置应用迁移为 `tempo.*` 第一方插件；双来源解析后共用 manifest 归一化 / Registry / Settings |
| D2 | 设置、采集、热键、托盘、独立窗口不插件化 |
| D3 | 过渡期（可永久）允许 App entry `{type:"host-react"}`，仅限 bundled origin；不使用包级 `uiHost` |
| D4 | 核心业务数据留在 `tempo.db`；特权 `host.tempo.*` + 身份 / enabled / 方法 allowlist；实现挂在 `HostApiHub` |
| D5 | First-party 不可卸载、默认可禁用、随 Tempo 更新、跳过信任对话框 |
| D6 | 删除 `source: "builtin"`；新增宿主下发的 `origin` / `trustTier` / `availabilityState` |
| D7 | BundledResolver + UserPackageResolver 产出统一 `ResolvedPlugin`；不复制 bundled 包到 user packages |
| D8 | 迁移分阶段；本期详设可迭代，实现另开 |
| D9 | **第一方执行后端 = Rust crate（默认同进程）；不用 Node 重写；sidecar 可选；不做 cdylib 默认路径** |
| D10 | manifest 是元数据唯一事实源；reconcile 保留 enabled；disable 不删除持久数据 |

## 附录 B：修订记录

| 版本 | 说明 |
|------|------|
| v0.1 | 初稿：库存、架构、特权 API、分期、开放问题 |
| v0.2 | 明确解耦目标：Rust Plugin API / crate 划分 / D9；修正「官方改 Node」表述；分期加入业务迁 crate |
| v0.3 | 评审收敛：双来源解析、origin-aware 校验、App entry union、catalog 一致性、事务 reconcile、统一 enabled gate、禁用数据保留与现状迁移清单 |
