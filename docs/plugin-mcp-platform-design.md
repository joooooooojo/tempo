# 插件 MCP 平台化详细设计

## 1. 背景与目标

Tempo 已经具备本机 Streamable HTTP MCP Server、Bearer Token 鉴权、插件 Runtime、
`contributes.mcpTools` 和插件级暴露开关。当前插件工具通过
`tempo_list_exposed_plugin_tools` 与 `tempo_call_plugin_tool` 两个平台代理工具间接调用。

本次改造的目标是：

1. 插件继续以声明式 Manifest 配置 MCP 工具，不自行监听端口或实现 MCP 协议。
2. Tempo 是唯一 MCP Server，在 `/mcp` 聚合内置工具和插件工具。
3. 外部 MCP Client 在 `tools/list` 中直接看到每个获准暴露的插件工具及其 Schema。
4. `tools/call` 经过 Tempo 的授权、Schema 校验、限流、超时和审计后再进入插件 Runtime。
5. 插件升级或工具契约变化时，旧授权自动失效，不能静默暴露新增能力。

## 2. 范围

### 2.1 本次实现

- 动态一级 MCP tools。
- 稳定、无冲突的外部工具名称。
- Manifest 输入/输出 Schema 与 MCP annotations。
- 宿主侧 JSON Schema 校验。
- MCP 与 UI/Action 共用 Runtime command 调用安全边界。
- 插件工具集指纹授权。
- `notifications/tools/list_changed`。
- 保留现有两个平台代理工具作为兼容入口。

### 2.2 非目标

- 插件贡献 MCP resources、resource templates 或 prompts。
- 让插件连接并反向代理第三方 MCP Server。
- 将 MCP Server 监听地址从 `127.0.0.1` 扩展到局域网或公网。
- MCP tasks/长任务恢复。
- 每个工具独立授权；本次仍为插件级开关，但授权精确绑定工具集指纹。

## 3. 总体架构

```text
External MCP Client
        |
        | Streamable HTTP + Bearer Token
        v
Tempo MCP Server (/mcp)
        |
        +-- Builtin ToolRouter
        |
        +-- Plugin MCP Registry Snapshot
                |
                +-- installed
                +-- enabled
                +-- trusted
                +-- exposure enabled
                +-- approved fingerprint == current fingerprint
                        |
                        v
                Plugin MCP Bridge
                        |
                        +-- resolve external name
                        +-- validate input schema
                        +-- shared command invoker
                        +-- validate output schema
                        |
                        v
                Plugin Supervisor -> Node Runtime command
```

插件包不获得 MCP 连接信息，也不直接接触 MCP Session。外部调用者不能指定任意
`pluginId` 或 command；只能调用 `tools/list` 中由 Tempo 生成的工具名称。

## 4. Manifest 契约

Manifest v1 保持兼容，在现有 `contributes.mcpTools` 项上增加可选字段：

```json
{
  "name": "summarize-note",
  "description": "Summarize one stored note",
  "command": "summarize-note",
  "inputSchema": {
    "type": "object",
    "properties": {
      "id": {
        "type": "string",
        "description": "Stored note id to summarize"
      }
    },
    "required": ["id"],
    "additionalProperties": false
  },
  "outputSchema": {
    "type": "object",
    "properties": {
      "summary": {
        "type": "string",
        "description": "Short summary of the note"
      }
    },
    "required": ["summary"]
  },
  "annotations": {
    "readOnlyHint": true,
    "destructiveHint": false,
    "idempotentHint": true,
    "openWorldHint": false
  }
}
```

校验规则：

- `name` 使用现有 local id 规则，并且在同一插件内唯一。
- `description` 去除首尾空白后不能为空。
- 每个插件最多声明 64 个 MCP tools，description 最长 1024 字符。
- `command` 必须引用同包 `commands[].id`，且插件必须包含 `main`。
- `inputSchema` 缺省为 `{ "type": "object", "properties": {} }`。
- 建议为 `inputSchema` / `outputSchema` 的每个 `properties.*` 提供非空 `description`，便于 MCP 客户端填参与理解返回值；**description 不是必填**，缺失不会导致校验失败。
- 输入、输出 Schema 必须是 JSON object，且能够被 JSON Schema 编译器接受。
- 每个输入或输出 Schema 序列化后不得超过 64 KiB。
- 输入 Schema 顶层必须声明 `type: "object"`，与 MCP arguments 模型一致。
- annotations 是提示信息，不参与授权判断，也不能替代宿主策略。

Runtime API 不增加新的注册方法。插件仍使用 `ctx.registerCommand(commandId, handler)`；
Manifest 是公开协议描述，command handler 是执行实现。

## 5. 外部工具命名

外部名称由 Tempo 生成，插件不能覆盖：

```text
tempo_plugin_{normalized_plugin_id}__{normalized_local_name}
```

规范化规则：

- `.` 与 `-` 转为 `_`，仅保留 ASCII 字母、数字和下划线。
- 名称以 `tempo_plugin_` 开头，不与内置工具空间冲突。
- 常规名称保持可读。
- 超过安全长度时截断可读部分并追加完整原始标识的 SHA-256 短摘要。
- Registry 构建时检测最终名称冲突；冲突工具不暴露并记录明确错误。

Manifest-local `name` 仍用于数据库授权、设置 UI 和插件内部解析。外部名称只属于
MCP 协议层，插件升级不能依赖它作为业务数据。

## 6. Registry Snapshot

`plugins::mcp_bridge::list_exposed_tools` 扩展为动态 Registry Snapshot 的唯一来源。
每个条目至少包含：

```text
external_name
plugin_id
plugin_name
tool_name
description
input_schema
output_schema
annotations
```

构建时按以下顺序过滤：

1. 插件记录存在。
2. `enabled == true`。
3. 当前版本已信任。
4. Manifest 可读取且通过完整校验。
5. MCP 暴露开关已开启。
6. 数据库批准指纹等于当前 Manifest 工具集指纹。

Registry 读取失败不能扩大权限：无法读取、无法解析、指纹不匹配时一律不暴露。
错误写入日志，设置页显示为未暴露状态。

首版直接从数据库和不可变插件包生成快照，避免引入跨线程缓存一致性问题。插件数量
增长后可在不改变接口的情况下增加按插件版本缓存。

## 7. MCP Server 路由

`TempoMcpServer` 保留静态 `ToolRouter` 处理内置工具，并手工扩展三个
`ServerHandler` 方法：

### 7.1 list_tools

1. 读取 `tool_router.list_all()`。
2. 构建插件 Registry Snapshot。
3. 将插件声明转换为 `rmcp::model::Tool`。
4. 按最终名称排序并返回。

### 7.2 get_tool

- 先查询静态 ToolRouter。
- 未命中时查询插件 Registry Snapshot。
- 返回动态 Tool 使 rmcp 能执行协议级工具检查。

### 7.3 call_tool

- 命中静态 ToolRouter：沿用现有 `ToolCallContext` 调用。
- 命中插件外部名称：将 MCP arguments 转为 JSON object，交给 Plugin MCP Bridge。
- 未命中：返回 MCP `invalid_params/tool not found`，不允许回退为任意 command 调用。

插件返回值通过 `CallToolResult::structured` 同时提供 text content 与
`structuredContent`。插件业务错误返回 tool-level error；协议无法路由才返回 JSON-RPC
错误。

现有 `tempo_list_exposed_plugin_tools` 与 `tempo_call_plugin_tool` 至少保留一个兼容周期，
但文档和 Server instructions 改为优先直接调用一级工具。

## 8. 统一 Runtime Command Invoker

当前普通 Plugin Bridge 在进入 Supervisor 前执行 1 MiB payload 检查和每插件 32 并发
限制，而 MCP Bridge 直接调用 Supervisor。改造后抽出共享入口：

```text
invoke_runtime_command(plugin_id, command_id, params, timeout)
```

职责：

- command id 非空校验。
- 序列化 payload 大小限制。
- 获取/释放每插件并发槽。
- 调用 Supervisor 并继承懒启动、30 秒超时和 cancel frame。
- 保留结构化 `RpcError.code`，由调用面转换为 UI 或 MCP 错误。

UI、Action 和 MCP 都通过该入口调用 Runtime command。MCP 审计日志记录外部工具名、
插件 id、local tool name、结果与耗时，不记录 arguments 或结果正文。

## 9. JSON Schema 校验

宿主必须校验，不能依赖 MCP Client：

```text
tools/call arguments
    -> compile/lookup inputSchema
    -> validate
    -> Runtime command
    -> validate outputSchema (when declared)
    -> structured MCP result
```

- 无 arguments 按空 object 处理。
- 输入不合法返回 caller-visible tool error，并包含有限、可读的字段路径。
- 输出不合法视为插件实现错误；对调用者返回稳定错误，不泄露 Runtime 堆栈。
- Schema 编译失败应在导入时被拒绝，运行期仍按 fail-closed 处理。

## 10. 授权与升级

现有 `plugin_mcp_exposure` 增加 `toolset_fingerprint`：

```sql
plugin_id TEXT PRIMARY KEY
exposed INTEGER NOT NULL DEFAULT 0
toolset_fingerprint TEXT NOT NULL DEFAULT ''
updated_at TEXT NOT NULL
```

指纹输入是按 tool name 排序后的 MCP 公共契约：name、description、command、
inputSchema、outputSchema、annotations。使用稳定 JSON 序列化后计算 SHA-256。

开启暴露时保存当前指纹。以下任一变化都会使授权自动失效：

- 新增或删除工具。
- 工具改名或换绑 command。
- 输入/输出 Schema 变化。
- description 或 annotations 变化。

关闭暴露或撤销当前插件版本的信任时清空批准指纹。插件被禁用或卸载时仍沿用现有
状态清理；即使数据库残留旧记录，Registry 的多重校验也不会暴露工具。

## 11. 工具清单变化通知

`McpController` 增加轻量 broadcast channel：

- MCP Session 初始化后订阅。
- 插件暴露开关、启用状态、信任状态、版本晋升或卸载完成后发布变更。
- Session 收到事件后调用 `peer.notify_tool_list_changed()`。
- `ServerCapabilities` 开启 `tools.listChanged`。
- 通知发送失败表示 Session 已断开，监听任务结束。

通知只提示客户端重新执行 `tools/list`，不携带工具详情或权限数据。

## 12. 安全边界

- MCP HTTP 仍只监听 `127.0.0.1`。
- `/mcp` 仍要求全局 Bearer Token；`/health` 不暴露工具和设置。
- 插件不能声明内置工具名，也不能调用其他插件 command。
- 每次调用都重新校验 enabled、trusted、exposed 和 fingerprint，避免 list/call 之间的
  TOCTOU 权限变化。
- Manifest annotations 只用于客户端提示；破坏性权限由用户授权和宿主策略决定。
- 参数、结果、Token 不写入日志。

## 13. 错误映射

| 场景 | MCP 表达 |
|---|---|
| 工具不存在或已撤权 | JSON-RPC invalid params / tool not found |
| 输入 Schema 不匹配 | tool-level error，可读字段路径 |
| 插件未启用、未信任或指纹失效 | tool-level error，提示在 Tempo 设置中检查 |
| 并发超过限制 | tool-level error，`RESOURCE_EXHAUSTED` |
| Runtime 超时 | tool-level error，`TIMEOUT` |
| 插件业务失败 | tool-level error，保留受控 message |
| 输出 Schema 不匹配 | tool-level error，插件返回格式错误 |
| Tempo 内部状态不可用 | JSON-RPC internal error，不泄露内部细节 |

## 14. 测试矩阵

### 14.1 Manifest

- 合法 input/output Schema 与 annotations。
- 重复 tool name、空 description、缺失 command、非 object Schema。
- 指纹与字段顺序无关，公共契约变化时指纹变化。

### 14.2 授权

- 新插件默认不暴露。
- 开启时记录当前指纹。
- 工具集变化后旧授权失效。
- disabled、untrusted、uninstalled 一律不进入 Registry。

### 14.3 MCP 路由

- `tools/list` 同时包含内置与已批准插件工具。
- 外部名称稳定、合法且无冲突。
- 一级工具调用正确路由到目标 command。
- 未知名称不能调用任意插件 command。
- 兼容 meta-tools 仍可用。

### 14.4 运行时边界

- 输入/输出 Schema 错误。
- 1 MiB payload、32 并发、30 秒超时。
- 插件在 list/call 间被禁用或撤权。
- 工具变化向活动 Session 发送 list_changed。

## 15. 发布与兼容

Manifest 字段都是向后兼容扩展；旧插件继续工作。旧 MCP Client 可以继续使用两个
meta-tools，新 Client 直接使用一级插件工具。数据库迁移为旧授权记录补空指纹，因此
升级后用户需要重新确认一次 MCP 暴露，这是有意的安全行为。
