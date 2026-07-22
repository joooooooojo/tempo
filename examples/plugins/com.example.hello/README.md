# Hello 示例插件（com.example.hello）

Tempo 插件系统 Phase 1 的最小混合插件示例：一个面板应用（UI）+ 一个快捷操作，二者共用同一个
`main` Runtime 命令 `hello`。用于验证插件导入、信任、启用、Runtime 激活和 Host Bridge 的完整
链路（详见 `docs/plugin-system-design.md` 附录 C）。

```text
com.example.hello/
  manifest.json     # 与入口文件同级、包最外层
  index.html        # UI 插件必填：面板入口
  index.js          # UI 侧脚本（由 index.html 引用，名称不限）
  main.mjs          # Runtime：main.mjs 或 main.js（避免与 UI 的 index.js 撞名）
  icons/app.svg
```

包约定（导入时校验）：

1. 有 UI（`contributes.apps`）→ 根目录必须有 `index.html`，且 `apps[].entry` 必须为 `index.html`
2. 无 UI（headless）→ 根目录必须有 `main.js` 或 `main.mjs`，且 `main` 指向该文件
3. `manifest.json`、`index.html`、`main.mjs`（或 `main.js`）必须在同一级、包最外层

## 1. 安装插件运行时（仅本插件需要）

本插件声明了 `main`（`main.mjs`），需要按需下载的插件专用 Node 运行时：

1. 打开 Tempo 设置 → 插件
2. 在「插件运行时（Node）」卡片点击「安装」，等待下载完成

纯 UI 插件（无 `main`）可跳过这一步。

## 2. 导入插件目录

1. 在 Tempo 设置 → 插件 → 「已安装插件」点击「导入目录」
2. 选择本目录（`examples/plugins/com.example.hello`，需包含根级 `manifest.json`）
3. 导入后插件处于**未信任、已禁用**状态；此时宿主只解析了 `manifest.json`，未执行任何插件代码

## 3. 信任插件包

1. 在插件列表中点击「信任」
2. 因为本插件含 `main`，确认文案会如实提示：

   > 启用此插件将允许其在本机执行代码，权限与 Tempo 相近（可读写文件、访问网络、发起进程等），请仅安装信任的来源。

3. 确认后即可看到本插件包的**全量 SHA-256 hash**（用于后续「内容是否被篡改」的复核，可点击选中复制）

## 4. 启用插件

打开插件条目上的启用开关。启用只注册声明式贡献（`main` 应用会出现在快捷面板「插件」角标下，
「Hello 一下」会出现在快捷操作里）——此时 Runtime **尚未启动**，直到第一次调用才懒激活。

## 5. 使用

UI 侧宿主会自动挂载 `window.plugin`（无需 SDK）：

```js
await window.plugin.invoke("hello", { who: "Tempo" });       // Runtime only
await window.plugin.host("notify.show", { title: "Hi" });   // Host only
window.plugin.on("greeted", (p) => console.log(p));
```

- **面板应用**：快捷面板搜索「Hello 示例」打开，点「打招呼（Runtime）」
- **快捷操作**：搜索「Hello 一下」执行同一个 `hello` 命令

## 6. 卸载 / 清理

- 「打开数据目录」可查看 `hello.log` 等 Runtime 写入的文件
- 「卸载」会停止 Runtime、移除面板贡献，并将安装包移入回收目录（可选同时删除私有数据）
