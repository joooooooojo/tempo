# Welcome（示例插件）

模板自带的 UI 插件，用来验证仓库索引、校验脚本和 Tempo 同步是否通畅。

当前为 Manifest v2 / Host API ^2.0.0，要求 Tempo >=2.2.6。该示例是纯 UI，不需要 Deno 后台进程，也不申请额外权限。0.2.0 更新了 v2 Schema 地址；已有 0.1.0 安装应作为新版本导入。

Tempo 只安装这个目录下已经提交的 `dist/`。发布自己的插件时：

1. 用 [插件开发助手](https://joooooooojo.github.io/tempo/developer/first-plugin) 创建 UI / Hybrid / Headless 项目。
2. 在插件根目录运行 `pnpm build`，确认 `dist/manifest.json` 和入口文件齐全。
3. 把整个插件目录放进 `plugins/<plugin-id>/`，**提交 `dist/`**。
4. 在仓库根索引中增加 `{ "id", "path" }`。
5. 提升 SemVer 后再更新已有插件；不要只替换同版本 `dist/`。

确认仓库可用后，删除本示例：

```bash
node scripts/init.mjs --remove-example
```
