---
title: 使用 SDK
description: 使用 connect 和 defineRuntime 构建 Tempo 插件。
---

# 使用 SDK

`@tempo/sdk` 提供 UI Client、Runtime Context、生命周期管理和可选的强类型 IPC。它使用 Tempo 已注入的底层对象，不实现宿主能力，也不改变插件权限。

模板已经包含依赖。手动创建项目时安装：

```bash
pnpm add @tempo/sdk
```

## UI：connect

```ts
import { connect } from "@tempo/sdk/ui";

const app = await connect();
console.log(app.context.params);
await app.storage.set("ready", true);
```

`connect()` 会等待页面上下文，然后返回 `TempoUiClient`。它的 `context` 保证非空，因此业务代码不需要再调用 `ready()` 或处理 `null`。通知、存储、文件、设置、主题、窗口等 UI Host API 都在同一个 `app` 对象中。

## Runtime：defineRuntime

```ts
import { defineRuntime } from "@tempo/sdk/runtime";

defineRuntime(({ commands, events, storage, onDispose }) => {
  commands.register("load", async () => storage.get("value"));
  onDispose(events.on("clipboard.changed", console.log));

  return () => {
    console.log("Runtime stopped");
  };
});
```

`defineRuntime()` 在宿主挂载 Runtime 时执行 setup。可通过 `onDispose()` 注册多个清理函数，也可以从 setup 返回一个清理函数；停止时按注册的相反顺序执行，并在某项失败后继续清理剩余资源。setup 中途失败时，已经注册的清理函数也会立即执行。

`@tempo/sdk/runtime` 面向 Deno Runtime，不引用浏览器 DOM。Vite 会把 SDK 打进最终 `main.mjs`，Tempo 安装插件时不会下载 npm 依赖。

## 强类型 IPC

Hybrid 插件可以在共享文件中声明一次契约：

```ts
export type PluginIpc = {
  invokes: {
    load: (id: string) => { title: string };
  };
  messages: {
    changed: [id: string, dirty: boolean];
  };
};
```

UI 使用同一个类型：

```ts
import { connect } from "@tempo/sdk/ui";
import type { PluginIpc } from "../ipc.js";

const app = await connect<PluginIpc>();
const item = await app.ipc.invoke("load", "42");
app.ipc.send("changed", "42", true);
```

Runtime 侧也使用它：

```ts
import { defineRuntime } from "@tempo/sdk/runtime";
import type { PluginIpc } from "../ipc.js";

defineRuntime<PluginIpc>(({ ipc }) => {
  ipc.handle("load", async (_event, id) => ({ title: `Item ${id}` }));
  ipc.on("changed", (_event, id, dirty) => console.log(id, dirty));
});
```

`invokes` 描述请求参数和返回值；`messages` 描述 `send/on` 的参数元组。省略泛型时仍可使用任意字符串频道，适合 JavaScript 或逐步迁移的项目。

## 类型入口

两个入口都会重新导出公共类型：

```ts
import type {
  TempoJsonObject,
  TempoRuntimeContext,
  TempoUiClient,
  TempoWindowRect,
} from "@tempo/sdk";
```

UI 与 Runtime 应分别使用对应入口。底层宿主全局只用于 SDK 实现和兼容旧插件，新代码无需直接访问。
