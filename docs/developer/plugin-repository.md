---
title: 维护插件仓库
description: 在 Tempo 里从官方模板创建 Git 插件仓库，提交 dist，再添加为插件源。
---

# 维护插件仓库

Tempo 可以从 Git 仓库发现和安装插件。仓库是一个普通 monorepo：根目录有索引文件，每个插件提供已经构建好的 `dist/`。

## 从模板创建仓库

在 Tempo 里操作，不必自己去拉 Git 模板：

1. 打开 **设置 → 插件管理 → 插件仓库**。
2. 点 **从模板创建**。
3. 选择保存位置，可选填写文件夹名称和说明。
4. Tempo 把官方模板写到本机目录（含示例插件 `com.example.welcome`）、自动生成仓库 ID 并初始化 Git，然后打开该文件夹。
5. 把目录推到 GitHub、GitLab 或其他 Git 托管。
6. 回到 Tempo，点 **添加源**，填入远程 Git 地址并同步。不要添加 `file://` 本地路径。

模板随 Tempo 一起打包，离线也能创建。源文件在 [`templates/plugin-repository`](https://github.com/joooooooojo/tempo/tree/master/templates/plugin-repository)，包含：

- `tempo-plugin-repository.json` 根索引
- 示例插件 `plugins/com.example.welcome`（含可安装 `dist/`）
- `node scripts/validate.mjs` 本地 / CI 校验
- GitHub Actions 工作流

私有仓库在 Tempo 里配置 PAT 或 SSH 凭证。秘密只留在当前应用会话，退出后需重新输入。

## 仓库约定

Tempo 读取仓库根目录的 `tempo-plugin-repository.json`：

```json
{
  "schemaVersion": 1,
  "id": "7c2f1a90-4b3e-4d8a-9c1b-2e5f6a7b8c9d",
  "plugins": [
    { "id": "com.acme.notes", "path": "plugins/com.acme.notes" }
  ]
}
```

索引只声明仓库 UUID、插件 ID 和根目录。插件名称、版本、平台和入口都以 `<path>/dist/manifest.json` 为准。

每个插件的 `dist/` 必须：

1. 包含通过校验的 `manifest.json`，且 `id` 与索引一致。
2. 包含 Manifest 声明的 UI / Runtime 入口。
3. 独立可运行，不依赖插件根目录的 `src/` 或 `node_modules/`。
4. 作为普通 Git 文件提交。Tempo 不执行构建、hook、submodule 或 LFS smudge。

更新插件时提升 SemVer。同版本替换 `dist/` 会被拒绝。

## 在 Tempo 中使用

1. **设置 → 插件管理 → 插件仓库 → 添加源**。
2. 填入 HTTPS、SSH 或 `git@host:path` 地址。
3. 测试连接后同步。Tempo 拉取目标 ref 的当前快照（depth 1），不安装、不信任、不运行插件。
4. 用户选择安装后，才把该 commit 中的 `dist/` 送进现有的校验、信任和启用流程。

添加仓库只表示信任「这个 Git 源可以出现在目录里」。每个插件包仍要单独信任。

## 推荐阅读

1. [做出第一个插件](/developer/first-plugin)：生成并构建单个插件。
2. [安装与管理插件](/guide/plugins)：最终用户如何导入、信任和启用。
3. 模板内 [CONTRIBUTING.md](https://github.com/joooooooojo/tempo/blob/master/templates/plugin-repository/CONTRIBUTING.md)：发布和更新检查清单。
