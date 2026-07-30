---
title: 做出第一个插件
description: 用插件开发助手创建、运行并构建一个 UI 插件。
---

# 做出第一个插件

这个教程创建一个可以保存文字的 Notes 页面。项目由插件开发助手生成，不需要另外安装 Tempo 依赖。

## 创建项目

1. 打开 Tempo 的 **插件开发助手**。
2. 选择 **创建项目**。
3. 项目模板选择 **UI · Vite**。
4. 填写项目目录、插件 ID 和名称。

插件 ID 必须全局唯一，推荐使用小写反向域名，例如 `com.example.notes`。

插件助手会获取远端模板目录，选择与当前 Host API 兼容的最新 UI 模板，并校验每个文件。首次使用需要联网；成功创建后，该版本会保留在本地缓存中。

生成的目录包含：

```text
com.example.notes/
  manifest.json
  index.html
  package.json
  vite.config.ts
  tempo.vite.ts
  .tempo/              # 开发服务使用的 Host Bridge
  src/
    main.ts
    style.css
    tempo.d.ts
```

生成的 `manifest.json` 已包含版本化远端 `$schema`，编辑器可以直接获得字段补全和校验。

## 写页面代码

把 `index.html` 的 `<body>` 改成：

```html
<body>
  <main>
    <label for="notes">笔记</label>
    <textarea id="notes" rows="12"></textarea>
  </main>
  <script type="module" src="/src/main.ts"></script>
</body>
```

把 `src/main.ts` 的业务代码改成：

```ts
import "./style.css";

const notes = document.querySelector<HTMLTextAreaElement>("#notes");

await window.tempo.ready();

if (notes) {
  notes.value = (await window.tempo.storage.get<string>("notes")) ?? "";
  notes.addEventListener("input", () => {
    void window.tempo.storage.set("notes", notes.value);
  });
}
```

页面使用 WebView 自己的生命周期。模板把模块脚本放在 `body` 末尾，DOM 此时已经可用；`window.tempo.ready()` 只负责等待宿主上下文。`window.tempo.storage` 是当前插件独享的本地存储，不需要自己拼接插件 ID。

如果需要读取打开页面时的参数：

```ts
const context = await window.tempo.ready();
console.log(context.params);
```

## 启动开发服务

在项目目录运行：

```bash
pnpm install
pnpm dev
```

回到插件开发助手：

1. UI 来源选择 **服务 URL**。
2. 保持 `http://127.0.0.1:5173/`。
3. 点击 **连接**。
4. 在主面板搜索插件名称并打开页面。

模板中的 `tempo.vite.ts` 只在 Vite 开发服务中注入 Bridge。生产构建不会把 `.tempo` 目录打进插件包。

## 构建并导入

```bash
pnpm build
```

构建完成后，`dist` 中已经包含 `manifest.json`、`index.html` 和静态资源。可以直接把 `dist` 目录导入 Tempo，不需要再移动文件。

## 下一步

| 需求 | 阅读 |
| --- | --- |
| 通知、存储、设置、主题、窗口 | [平台 API](/reference/plugin-host-api) |
| Action 打开页面 | [Manifest：Actions](/reference/manifest-schema#actions) |
| 页面需要后台能力 | [加入后台能力](/developer/runtime) |
| UI、Hybrid、Headless 的启停区别 | [插件类型与生命周期](/developer/plugin-lifecycle) |
