# Tempo 插件运行时 Deno 完整迁移详设

日期：2026-09-08。基线：`df7e825`。本文先完成计划与自审，随后按用户“完整迁移、允许破坏性修改”的要求修订为 Deno-only 并实施。

## 1. 决策与范围

- 插件后台唯一引擎为受管理的 Deno 2.9.6，无 Node 回退、无 `-A`。
- Manifest v2、Host API 2.0.0、模板 2.0.0。应用版本保持 2.2.6，旧客户端因未知 Manifest 版本拒绝新插件。
- Node/npm/TypeScript/Vite 保留为构建工具。用户设备不执行插件 npm install 或安装脚本。
- 采用稳定 CLI 权限，不使用实验性 Permission Broker。
- 每插件独立进程，Rust Supervisor 管生命周期；所有平台使用预绑定 loopback TCP + stdin token。
- v1 包必须更新版本号、Manifest、重新打包和信任。保留插件数据与历史发布模板。

这是运行时完整替换，不是 OS 级恶意代码隔离工程。资源配额、WebView 网络约束、静态模块闭包强制验证的边界见第 7 节。

## 2. Node 打包产物兼容性

可以执行 Node 工具链生成的兼容 JavaScript，不代表任意 Node 程序都可运行。

| 产物 | 合同 |
| --- | --- |
| Vite/Rollup/esbuild 生成的 ESM main.mjs | 支持；验收针对最终产物 |
| 内联的纯 JS npm 依赖 | 支持实际使用的兼容 API，已用 clsx 验证 |
| node:buffer/path/crypto/fs 等 | 使用 Deno Node 兼容层，敏感 I/O 受权限限制 |
| 本地 ESM chunks | 可读取包目录，发布前确认本地依赖闭包 |
| CommonJS、动态 require、外置 node_modules | 不承诺，需重新构建自包含 ESM |
| .node、FFI、child_process、Deno.Command | 不授予执行权限 |
| SEA/pkg/nexe 原生 exe、Electron 私有 API | 不属于支持的 JS 产物 |

`ssr.noExternal: true` 不是全兼容证明；动态依赖、资源文件、条件导出、原生扩展均须实测。Bootstrap 显式导入 node:process 和 node:buffer；Buffer 跨 IPC 仍须转 Uint8Array，保持现有结构化克隆合同。

## 3. Manifest 与授权

```json
{
  "manifestVersion": 2,
  "id": "com.example.notes",
  "name": "Notes",
  "version": "2.0.0",
  "engines": { "tempo": ">=2.2.6", "pluginApi": "^2.0.0" },
  "main": "main.mjs",
  "permissions": {
    "read": ["$DATA"],
    "write": ["$DATA"],
    "net": ["api.example.com:443"],
    "env": ["PLUGIN_API_KEY"],
    "host": { "notify": true, "externalOpen": false, "openApps": [] }
  }
}
```

权限缺省均为空或 false。未知权限字段拒绝导入；capabilities 只作说明。v2 后台引擎唯一为 Deno，无 runtime.engine/profile 字段。

| 权限 | 定义 |
| --- | --- |
| read/write | 仅 `$DATA`，宿主按连接身份解析为生产或开发数据目录 |
| net | 精确 host:port、端口 1–65535，无 URL/凭证/通配符/分隔符；可显式声明本机服务 |
| env | 声明的大写变量从宿主环境传入；拒绝 DENO_、NODE_、TEMPO_、LD_、DYLD_ 前缀及 PATH |
| host.notify | 系统通知 |
| host.externalOpen | 现有 http(s)/mailto 外部打开接口，不开放其他协议 |
| host.openApps | 精确目标 App ID，无通配符 |

包目录读取和专用 IPC 端口是宿主必要授权。tempo.storage 按插件身份隔离，无需磁盘权限。

批准复用 pluginId + version + packageHash 信任记录：Manifest 参与哈希，权限随包被绑定，无独立可漂移的策略副本。信任前、启用前、贡献加载与启动时校验包。升级显示新旧权限并重新确认；撤权停用、停止进程、关闭窗口和释放订阅。

序列化 API 从 requiresNodeRuntime/nodePath 改为 requiresRuntime/denoPath。内部 Rust/SQL 保留历史 requires_node_runtime 名称，含义为“存在 main”，避免无意义的数据迁移。

## 4. 启动与权限编译

生产和开发共同调用 permissions.rs::runtime_args，通过 Command.args 传入参数，不使用 shell。

```text
deno run --no-config --no-lock --no-prompt --cached-only
  --no-remote --no-npm --node-modules-dir=none
  --allow-read=<canonical-package>[,<canonical-data>]
  --allow-net=127.0.0.1:<ipc-port>[,<declared-endpoints>]
  [--allow-write=<canonical-data>] [--allow-env=<declared-keys>]
  <host-bootstrap.mjs>
```

非空可选权限才输出 allow 参数，绝不生成裸 allow。路径 canonicalize 后拒绝逗号/换行等列表分隔歧义。run/ffi/sys 未授权，no-prompt 静默拒绝交互提权。

env_clear 清空继承环境，仅保留 OS/temp 必需变量、声明变量和 DENO_DIR。缓存位于 plugin-runtime/cache/<pluginId>，在插件数据可写范围之外；不继承用户全局 npm/cache/broker 配置。开发目录本身仍是用户可编辑的可信工作区。

## 5. IPC 与生命周期

Rust 在 spawn 前绑定 127.0.0.1:0。stdin 首行包含 socketAddress、256-bit 随机 token、pluginId、mainPath、dataPath、runtimeVersion。token 不进入 argv/env。

Bootstrap 验证描述符、连接并发送 token，收到真实 ACK 后才导入 main；取消固定延迟推断。宿主在总握手期限内最多处理 16 个无效连接，每次认证有短超时。

帧继续采用 u32 BE 长度 + UTF-8 JSON，单帧 1 MiB，宿主写队列 64 帧，拥塞报错。Commands、MCP、私有 UI IPC、事件及 mount/unmount 合同保持不变。

启动/握手/激活超时失败并回收进程，开发激活超时同样终止。stop 与 start 使用同一插件锁。崩溃退避沿用原逻辑。运行中卸载 Deno 被拒绝，需先停用插件与断开开发连接。

tempo.runtime 为 `{ engine: "deno", version, nodeCompatVersion }`。测试保留 Node/旧传输对照，不代表产品保留 Node 运行路径。

## 6. 安装、升级与数据

固定官方 Deno 2.9.6 ZIP URL/SHA-256，覆盖 Windows x64、macOS arm64/x64、Linux x64。正式构建不读取二进制路径或下载清单的测试覆盖变量。

下载后校验 ZIP SHA-256，安全解压到 deno/<version>/<unique-id>，检查二进制、设置 Unix 执行位、记录 binarySha256，最后原子发布 deno-manifest.json。失败不覆盖旧安装；启动前再校验二进制，版本/target 不符拒绝使用。

卸载只删除 Deno 清单和 deno 子目录，不删除插件包、数据、旧 Node 文件及缓存。旧 Node 文件不再加载。中断解压可能留下未引用目录，可随 Deno 卸载回收。

停用不删除私有 storage。旧 v1 插件拒绝启用并在管理页报告迁移错误；导入 v2 新版本后保留同一插件 ID 的用户数据。

## 7. 安全边界与剩余风险

- 静态模块图存在 Deno 权限豁免。本次禁用远程/npm 下载，但未实现 AST 级包外静态模块闭包拒绝；不能声称 read 权限阻止所有模块方式读取包外源码。
- WebView CSP/网络独立管理，Deno net 白名单不是 Hybrid 整体网络白名单。信任提示明确说明。
- 没有 OS 级 CPU/内存/磁盘配额。进程隔离、超时、帧上限和有界队列不等于防止全部资源耗尽。
- 生产不转发第三方 stdout/stderr；激活错误沿 ready.error 返回。早期启动故障可能仅显示握手失败，开发连接可看日志。
- 本地同权限用户可更改 Tempo 自身文件，不在插件运行时防护范围内。
- 权限撤销以结束进程使旧句柄失效，不进行运行中句柄热撤权。

## 8. 实施与验收

| 阶段 | 内容 | 状态 |
| --- | --- | --- |
| M0 | 详设、校对、自审 | 已完成；按用户要求改为一次性迁移 |
| M1 | Bootstrap TCP、真实 ACK、异常启动测试 | 已实现 |
| M2 | Deno 下载/校验/发布/卸载 | 已实现 |
| M3 | Manifest v2、默认拒绝、哈希绑定授权 | 已实现 |
| M4 | 生产/开发 Supervisor、Host 权限、撤权 | 已实现 |
| M5 | schema、模板、编辑器默认值、权限确认、文档 | 已实现 |
| M6 | Windows 测试/构建与跨平台发布验收 | 本机验证见下；跨平台未实测 |

```powershell
$env:TEMPO_PLUGIN_DENO_PATH = '<verified-deno-2.9.6.exe>'
node --test scripts/test-plugin-runtime.mjs
node scripts/test-plugin-events.mjs
cargo test --manifest-path src-tauri/Cargo.toml --lib plugins:: -- --test-threads=2
pnpm exec tsc --noEmit
pnpm exec vite build
node scripts/build-plugin-assets.mjs
git diff --check
```

Runtime 用例包括 Node/Vite 内联 npm、冷缓存 Deno、数据读写授予/拒绝、env/net/run/ffi 拒绝、远程/npm 导入拒绝、Command/MCP/IPC、克隆编码、延迟/拒绝 ACK、断连、非法描述符、EOF 和超时。未设置 Deno 路径时用例明确跳过，不能算 Deno 验收通过。

本机 Windows 最终结果：86 项 Rust 插件相关测试通过，16 项真实运行时测试通过（无跳过），事件回归、TypeScript 检查、Vite 生产构建、VitePress 文档构建、仓库模板校验和 diff 空白检查通过。Rust 有 3 个既有 dead_code 警告，Vite 有大 chunk/混合导入警告，不影响本轮构建。

macOS/Linux 实机安装、完整 Tauri WebView 工作流、性能基线和资源耗尽测试未执行，需要单独验收。没有启动或替换你当前正在使用的桌面应用，也没有发布线上模板。

旧插件升级步骤：安装新版应用的 Deno 运行时；把插件 Manifest 改为 v2、pluginApi 改为 ^2.0.0、声明实际所需 permissions；增加插件版本号以符合不可变包约束；使用原 Node/Vite 构建链重打包，导入并确认新权限。插件 ID 保持不变即可沿用原数据目录。Hello 示例已升为 2.0.0。

开发助手内置 v2 模板：默认远程目录不可用或仍返回旧模板时回退到随应用发布的模板和本地 schema，因此无需等待线上文档站部署。自定义模板源仍严格报告其自身错误，不静默回退。

## 9. 校对与自审记录

1. 原计划保留 Node 过渡；按用户新要求删除生产回退，旧包明确拒绝。
2. Windows node:net 管道在受限 Deno 要求 all access，改预绑定 TCP，不扩大权限。
3. 固定版本为 Deno 2.9.6，Node 兼容版本单独显示。
4. 原草案 permissions.runtime/sys/runtime.profile 未采用，以实际平铺 Rust/schema 合同为准。
5. Host 权限来自已校验注册表快照，新增升级权限确认与撤权清理。
6. 修复原停用删除私有 storage 的行为，避免迁移误删数据。
7. 修复空权限扩大、环境注入、列表分隔符、安装覆盖旧版本等风险。
8. 模板单独发布 2.0.0、Host API 升主版本，历史 1.x 不重写。
9. 不把构建成功/smoke 等同于全兼容、OS 沙箱或跨平台验收。
10. 开发连接期间修改权限/身份/入口要求先断开再保存，防止权限显示和正在运行的进程策略不同步。
11. 核对启动中/停止中的锁状态，阻止此时卸载 Deno；撤权后的 Bridge 调用立即拒绝。

参考：[Node 兼容](https://docs.deno.com/runtime/fundamentals/node/)、[权限](https://docs.deno.com/runtime/reference/permissions/)、[安全模型](https://docs.deno.com/runtime/fundamentals/security/)。以固定 Deno 二进制实测为准。
