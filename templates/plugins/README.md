# Plugin template release

`ui`、`hybrid` 和 `headless` 是下一次发布的模板源。`release.json` 独立维护模板版本、最低 Host API 和公开资源根地址。

当前模板 2.0.8 使用 Manifest v2、Host API ^2.1.0 和 `tempo-plugin-sdk` 1.x，要求 Tempo >=2.2.6。后台运行于 Deno 2.9.6；Node/npm/TypeScript/Vite 仅用于构建，默认继续输出自包含 ESM `main.mjs`。

模板通过 UI 的 `connect()` 和 Runtime 的 `defineRuntime()` 建立插件入口。Hybrid 模板在 `src/ipc.ts` 中共享 IPC 契约，Vite 将 SDK 与业务依赖打进最终插件产物。

Hybrid 的 `tsconfig.json`、`tsconfig.ui.json`、`tsconfig.runtime.json` 均位于项目根目录。根配置通过 `references` 组织两个独立类型环境，`pnpm typecheck` 使用 `tsc -b` 统一检查；也可执行 `typecheck:ui` 或 `typecheck:runtime` 单独检查。增量缓存写入 `node_modules/.cache`，不生成运行时代码。

| 模板 | 后台 | 默认权限 |
| --- | --- | --- |
| UI | 无 | 无额外权限 |
| Hybrid | Deno | 无额外权限（Command/MCP/私有 IPC） |
| Headless | Deno | 无额外权限 |

所有模板的 `build` 都先检查类型。插件私有文件优先使用无需声明权限的 SDK `files` API。Runtime 直接调用 Deno 文件 API 时，在 `permissions` 数组中选择全局 `read` 或 `write`；`net`、`env`、`sys`、`run`、`ffi`、`import` 也可独立选择，其中 `net` 同时控制 Runtime 和托管 UI。空数组默认关闭全部敏感权限。

`contributes.apps[].icon` 与 `contributes.actions[].icon` 使用包内相对路径，支持 SVG、PNG、JPEG（`.jpg` / `.jpeg`）、WebP 和 GIF；声明的文件必须随构建产物一起发布，且不超过 256 KiB。

在仓库根目录设置 `TEMPO_PLUGIN_DENO_PATH` 后执行 `pnpm test:plugin-templates`，可验证 SDK 构建、模板构建以及 Hello 演示的实际 Deno 调用。

发布模板：

```bash
pnpm plugin-assets:build
```

命令会把当前版本写入 `docs/public/plugin-assets/releases/<version>`，生成带文件大小和 SHA-256 的 `catalog.json`，并复制对应的版本化 Manifest Schema。

Git 插件仓库模板在 [`../plugin-repository`](../plugin-repository)，给维护者创建可被 Tempo 添加的插件源，不走这套远端发布流程。

已发布版本不可原地修改。模板、Bridge 或 Schema 发生变化时，先提升 `release.json` 的 `version`，再生成新 release。历史 release 会保留在远端目录中，旧版 Tempo 可以继续选择它支持的最新版本。
