---
title: 宿主事件
description: 使用 tempo.events 监听 Tempo 广播的平台事件。
---

# 宿主事件

宿主事件是 Tempo 广播的平台状态变化。UI 和 Runtime 都通过 `tempo.events` 监听，不需要在 Manifest 中声明 `hooks`，也不需要注册 Command。

## 监听方法

| 方法 | 作用 |
| --- | --- |
| `on(event, handler)` | 持续监听，返回取消函数 |
| `once(event, handler)` | 只处理下一次事件，返回取消函数 |
| `off(event, handler)` | 移除同一处理器，返回是否找到监听 |
| `removeAllListeners(event?)` | 移除指定事件或全部通用宿主事件监听 |
| `listenerCount(event)` | 返回当前监听器数量 |
| `eventNames()` | 返回当前有监听器的事件名称 |

`off` 可以使用传给 `on` 或 `once` 的原始函数。`once` 会在调用处理器前解除监听，即使处理器内部触发重入逻辑也只会执行一次。

`tempo.events` 不提供 `emit`。平台广播只能由 Tempo 发出；插件 UI 与 Runtime 之间主动发送消息应使用 `ipcRenderer` / `ipcMain`。

## 接收规则

一次广播只送到当时存在的接收方：

- 已经运行的插件 Runtime。
- 当前打开的插件页面。

Tempo 不会为了广播启动已停止的 Runtime，也不会保存事件等插件稍后上线。需要持续监听的 Hybrid 或 Headless 插件应在 Manifest 中配置：

```json
{
  "activationEvents": ["onStartup"]
}
```

这只决定 Runtime 的启动时机；事件监听仍然直接写在代码里。

## Runtime 监听

```js
let offClipboard;

onMounted(() => {
  offClipboard = tempo.events.on("clipboard.changed", (payload) => {
    console.log("clipboard changed at", payload.at);
  });
});

onUnmounted(() => offClipboard?.());
```

## UI 监听

```js
await window.tempo.ready();
window.tempo.events.on("clipboard.changed", (payload) => {
  console.log(payload.at);
});
```

只关心下一次变化：

```js
window.tempo.events.once("clipboard.changed", (payload) => {
  console.log("next change", payload.at);
});
```

使用原始处理器显式取消：

```js
function onClipboardChanged(payload) {
  console.log(payload.at);
}

window.tempo.events.on("clipboard.changed", onClipboardChanged);
window.tempo.events.off("clipboard.changed", onClipboardChanged);
```

UI 只在页面打开期间接收广播。页面销毁由 WebView 管理，纯 UI 插件不需要生命周期钩子，也不需要为了监听事件增加 Runtime。

## 事件一览

| 事件 | 触发时机 | Payload |
| --- | --- | --- |
| `clipboard.changed` | 本机剪贴板内容变化 | `{ schemaVersion: 1, at: string }` |

### clipboard.changed

```ts
interface ClipboardChangedPayload {
  schemaVersion: 1;
  /** ISO-8601 时间 */
  at: string;
}
```

Payload 不包含剪贴板正文或文件路径，避免敏感内容被广播给所有运行中的插件。需要读取正文时，应由用户信任的 Runtime 自行访问系统剪贴板。

## 专用订阅

以下变化使用独立订阅，不进入 `tempo.events` 的监听表：

- 主题变化：UI 使用 `window.tempo.theme.subscribe(...)`。
- 插件设置变化：UI 或 Runtime 使用 `tempo.settings.subscribe(...)`。

调用 `tempo.events.removeAllListeners()` 不会移除设置或主题订阅。平台事件和 Runtime IPC 即使频道同名也不会混在一起。`tempo.events` 只接收平台来源，`ipcRenderer` 只接收 Runtime 来源。
