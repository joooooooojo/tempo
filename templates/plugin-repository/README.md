# Tempo 插件仓库模板

这个目录是 Tempo 内置的官方插件仓库模板。在应用里打开 **设置 → 插件管理 → 插件仓库 → 从模板创建**，选择保存位置即可生成本地 Git 仓库。仓库 ID 会自动生成。

Tempo 会拉取仓库快照，读取根索引，并只从每个插件的 `dist/` 安装。贡献者提交源码、构建产物和索引；用户添加 Git 地址后即可浏览和安装。Tempo **不会**在用户设备上执行 `npm install` 或构建脚本。

## 用模板创建仓库

推荐在 Tempo 里创建：

1. 打开 **设置 → 插件管理 → 插件仓库**。
2. 点 **从模板创建**。
3. 选择保存位置，可选填写文件夹名称和说明。
4. Tempo 写入模板文件、自动生成仓库 ID、初始化 Git，并打开本地文件夹。
5. 把该目录推到 GitHub / GitLab 后，再用 **添加源** 填远程地址。

也可以把本目录拷贝到新仓库，再生成 ID：

```bash
node scripts/init.mjs
```

可选：

- `--homepage https://example.com/plugins`
- `--remove-example` 删除自带的 `com.example.welcome`

`id` 由 Tempo 或 `init.mjs` 自动生成，是小写 UUID。

应用内创建已经带好初始提交。推到远程：

```bash
git remote add origin https://github.com/<you>/my-tempo-plugins.git
git push -u origin HEAD
```

## 在 Tempo 中添加这个源

1. 打开 **设置 → 插件管理 → 插件仓库**。
2. 点 **添加源**，填入仓库 HTTPS 或 SSH 地址。
3. 测试连接并同步。
4. 从目录安装插件。首次安装默认不信任、不启用。

添加仓库只让 Tempo 浏览索引，不会执行插件代码。安装后仍需单独信任。

## 发布插件

1. 用 Tempo **插件开发助手**创建 UI / Hybrid / Headless 项目，或拷贝 [插件模板](https://github.com/joooooooojo/tempo/tree/master/templates/plugins)。
2. 把项目放到 `plugins/<plugin-id>/`。
3. 构建并确认 `plugins/<plugin-id>/dist/manifest.json` 存在，且 `id` 与目录、索引一致。
4. 在 `tempo-plugin-repository.json` 增加：

```json
{ "id": "com.acme.notes", "path": "plugins/com.acme.notes" }
```

5. **提交 `dist/`**。Tempo 只安装 Git 里的构建产物。
6. 运行校验：

```bash
node scripts/validate.mjs
```

7. 推送或发起 PR。

更新已有插件时必须提升 `dist/manifest.json` 的 SemVer。同版本替换 `dist/` 会被 Tempo 视为发布错误。

## 目录结构

```text
tempo-plugin-repository.json     仓库索引，Tempo 读取这个文件
schema/                          索引 Schema，供编辑器和校验脚本使用
plugins/
  com.example.welcome/
    README.md
    dist/                        可安装包（必需，且必须提交）
      manifest.json
      index.html
scripts/
  init.mjs                       填写仓库 id / name
  validate.mjs                   校验索引和每个 dist
.github/workflows/validate.yml
```

索引里的 `path` 可以指向仓库内任意合法相对目录，不强制使用 `plugins/`。Tempo 固定在 `<path>/dist` 寻找可安装包。

## 校验规则（与 Tempo 一致）

- 索引协议版本为 `1`，未知字段和重复 JSON 键会被拒绝。
- 插件 ID、路径不得重复；路径不得嵌套、不得含 `..`、反斜杠或 `.git`。
- 每个索引项必须有 `dist/manifest.json`，且 Manifest `id` 与索引 `id` 完全一致。
- `dist/` 不得包含符号链接或 Git LFS pointer。
- UI 插件需要 `dist/index.html`；含 Runtime 的插件需要 Manifest 声明的 `main` 文件。

更完整的约定见 Tempo 文档：[维护插件仓库](https://joooooooojo.github.io/tempo/developer/plugin-repository)。
