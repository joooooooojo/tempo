---
title: 使用 SDK
description: 通过 @tempo/sdk 调用 Tempo 注入的 UI 与 Runtime API。
---

# 使用 SDK

`@tempo/sdk` 把 Tempo Host 注入的全局对象封装成可导入、可补全的 TypeScript API。SDK 不实现宿主能力，也不改变权限；它只在插件启动时读取 Host 已经注入的对象。

模板已经包含依赖。手动创建项目时安装 SDK：

```bash
pnpm add @tempo/sdk
```

## UI 入口

```ts
import { ipcRenderer, tempo } from "@tempo/sdk/ui";

const context = await tempo.ready();
const result = await ipcRenderer.invoke("load", context.params);
```

`@tempo/sdk/ui` 只导出 UI 可以使用的 `tempo` 与 `ipcRenderer`。

## Runtime 入口

```ts
import {
  ipcMain,
  onMounted,
  onUnmounted,
  tempo,
} from "@tempo/sdk/runtime";

onMounted(() => {
  ipcMain.handle("load", async () => tempo.storage.get("value"));
});

onUnmounted(() => {
  console.log("Runtime stopped");
});
```

`@tempo/sdk/runtime` 面向 Deno Runtime，不会引用 DOM 或浏览器 API。Vite 模板会把 SDK 打进最终的 `main.mjs`，Tempo 不会在安装插件时下载 npm 依赖。

## 类型

两个入口都会重新导出公共类型，可以按需导入：

```ts
import type { TempoJsonObject, TempoWindowRect } from "@tempo/sdk/ui";
```

UI 与 Runtime 使用不同入口，TypeScript 不会把另一侧的宿主对象加入全局作用域。宿主原始全局仍然存在，用于运行 SDK 和兼容旧插件。
