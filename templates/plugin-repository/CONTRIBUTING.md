# 贡献指南

## 发布新插件

1. 在 `plugins/<plugin-id>/` 放入插件项目。
2. 构建到该目录下的 `dist/`。`dist` 必须是独立可运行包，不依赖 `src/` 或 `node_modules/`。
3. 确认 `dist/manifest.json` 的 `id`、`version`、`engines` 和入口正确。
4. 在根索引 `tempo-plugin-repository.json` 增加 `{ "id", "path" }`。
5. 运行 `node scripts/validate.mjs`。
6. 一次提交源码、`dist/` 和索引变更。

## 更新已有插件

- 提升 SemVer，再提交新的 `dist/`。
- 不要修改已经发布的同一版本内容。Tempo 会把「版本不变、包哈希变了」当作发布错误。
- 索引项保持不变；路径变了才改 `path`。

## 删除插件

从根索引移除对应项。已经安装到用户设备上的插件不会被卸载，只是不再从本仓库提供更新。

## 审查建议

- 保护默认分支，要求 PR 和 `validate` CI 通过。
- 有写权限的人都能发布插件。Tempo 把仓库维护者视为发布者，不会再验证 commit 作者身份。
- 不要把密钥、私钥或 `.env` 提交进仓库。
