# Plugin template release

`ui`、`hybrid` 和 `headless` 是下一次发布的模板源。`release.json` 独立维护模板版本、最低 Host API 和公开资源根地址。

发布模板：

```bash
pnpm plugin-assets:build
```

命令会把当前版本写入 `docs/public/plugin-assets/releases/<version>`，生成带文件大小和 SHA-256 的 `catalog.json`，并复制对应的版本化 Manifest Schema。

已发布版本不可原地修改。模板、Bridge 或 Schema 发生变化时，先提升 `release.json` 的 `version`，再生成新 release。历史 release 会保留在远端目录中，旧版 Tempo 可以继续选择它支持的最新版本。
