# Tempo 文档

## 插件开发

- [插件开发指南](./plugin-development.md)：包结构、UI、Runtime、贡献点、窗口、会话和调试流程。
- [Plugin SDK](./plugin-sdk.md)：`@tempo/plugin-sdk`（推荐：`definePlugin` / `createPluginClient`）。
- [Host API 参考](./plugin-host-api.md)：`window.plugin`、`ctx.host`、参数、返回值、限制和错误码。
- [内置插件开发助手详细设计](./plugin-development-assistant-design.md)：根目录项目、Manifest 可视化编辑、UI 开发连接与 Headless 测试。
- [插件 MCP 平台化详细设计](./plugin-mcp-platform-design.md)：动态工具注册、授权、Schema 校验与调用链。
- [manifest.json JSON Schema](./schemas/plugin-manifest.schema.json)：Manifest v1 的编辑器提示与结构校验。
- [Hello 示例插件](../examples/plugins/com.example.hello/)：可直接导入的混合插件。
