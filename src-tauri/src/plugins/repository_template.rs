//! Scaffold a local Git plugin repository from the bundled official template.

use std::fs;
use std::path::{Path, PathBuf};

use include_dir::{include_dir, Dir, DirEntry};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use super::ids::new_repository_index_id;

static TEMPLATE: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../templates/plugin-repository");
const INDEX_FILE: &str = "tempo-plugin-repository.json";
const DEFAULT_FOLDER_NAME: &str = "tempo-plugins";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePluginRepositoryTemplateInput {
    pub parent_path: String,
    #[serde(default)]
    pub folder_name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedPluginRepository {
    pub path: String,
    pub repository_id: String,
    pub git_initialized: bool,
}

pub fn create_from_template(
    app: &AppHandle,
    args: CreatePluginRepositoryTemplateInput,
) -> Result<CreatedPluginRepository, String> {
    let repository_id = new_repository_index_id();
    let description = validate_description(args.description.as_deref())?;
    let homepage = validate_homepage(args.homepage.as_deref())?;
    let dest = destination_path(&args.parent_path, args.folder_name.as_deref())?;
    prepare_destination(&dest)?;
    write_template(&dest)?;
    write_index(
        &dest,
        &repository_id,
        description.as_deref(),
        homepage.as_deref(),
    )?;
    let git_initialized = initialize_git(&dest)?;
    reveal_destination(app, &dest);
    Ok(CreatedPluginRepository {
        path: dest.display().to_string(),
        repository_id,
        git_initialized,
    })
}

fn validate_description(value: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.chars().count() > 1024 {
        return Err("仓库说明不能超过 1024 个字符".into());
    }
    Ok(Some(value.to_string()))
}

fn validate_homepage(value: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.len() > 2048 {
        return Err("仓库主页过长".into());
    }
    let parsed = url::Url::parse(value).map_err(|error| format!("仓库主页无效: {error}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("仓库主页必须使用 HTTP(S)".into());
    }
    Ok(Some(value.to_string()))
}

fn destination_path(parent_path: &str, folder_name: Option<&str>) -> Result<PathBuf, String> {
    let parent = PathBuf::from(parent_path.trim());
    if parent.as_os_str().is_empty() {
        return Err("请选择保存位置".into());
    }
    let folder = folder_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_FOLDER_NAME);
    if folder.contains('/')
        || folder.contains('\\')
        || folder.contains('\0')
        || folder == "."
        || folder == ".."
    {
        return Err("文件夹名称不能包含路径分隔符".into());
    }
    Ok(parent.join(folder))
}

fn prepare_destination(dest: &Path) -> Result<(), String> {
    if dest.exists() {
        if !dest.is_dir() {
            return Err(format!("目标已存在且不是文件夹: {}", dest.display()));
        }
        if !is_effectively_empty(dest)? {
            return Err(format!(
                "目标文件夹不是空的，未覆盖: {}",
                dest.display()
            ));
        }
    } else {
        fs::create_dir_all(dest)
            .map_err(|error| format!("创建仓库目录失败 {}: {error}", dest.display()))?;
    }
    Ok(())
}

fn is_effectively_empty(dir: &Path) -> Result<bool, String> {
    let entries = fs::read_dir(dir)
        .map_err(|error| format!("读取目标目录失败 {}: {error}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("读取目标目录失败: {error}"))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == ".DS_Store" || name == "Thumbs.db" {
            continue;
        }
        return Ok(false);
    }
    Ok(true)
}

fn write_template(dest: &Path) -> Result<(), String> {
    if TEMPLATE.entries().is_empty() {
        return Err("应用内未找到插件仓库模板".into());
    }
    write_dir(&TEMPLATE, dest)
}

fn write_dir(dir: &Dir<'_>, dest: &Path) -> Result<(), String> {
    for entry in dir.entries() {
        match entry {
            DirEntry::Dir(child) => {
                write_dir(child, dest)?;
            }
            DirEntry::File(file) => {
                let relative = file.path();
                if relative.file_name().is_some_and(|name| {
                    name == ".DS_Store" || name == "Thumbs.db"
                }) {
                    continue;
                }
                let target = dest.join(relative);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent).map_err(|error| {
                        format!("创建模板目录失败 {}: {error}", parent.display())
                    })?;
                }
                fs::write(&target, file.contents()).map_err(|error| {
                    format!("写入模板文件失败 {}: {error}", target.display())
                })?;
            }
        }
    }
    Ok(())
}

fn write_index(
    dest: &Path,
    repository_id: &str,
    description: Option<&str>,
    homepage: Option<&str>,
) -> Result<(), String> {
    let index_path = dest.join(INDEX_FILE);
    let raw = fs::read_to_string(&index_path)
        .map_err(|error| format!("读取仓库索引失败: {error}"))?;
    let mut value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|error| format!("仓库索引 JSON 无效: {error}"))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "仓库索引必须是对象".to_string())?;
    object.insert(
        "id".into(),
        serde_json::Value::String(repository_id.to_string()),
    );
    object.remove("name");
    if let Some(description) = description {
        object.insert(
            "description".into(),
            serde_json::Value::String(description.to_string()),
        );
    }
    if let Some(homepage) = homepage {
        object.insert(
            "homepage".into(),
            serde_json::Value::String(homepage.to_string()),
        );
    }
    let pretty = serde_json::to_string_pretty(&value)
        .map_err(|error| format!("写入仓库索引失败: {error}"))?;
    fs::write(&index_path, format!("{pretty}\n"))
        .map_err(|error| format!("写入仓库索引失败: {error}"))
}

fn initialize_git(dest: &Path) -> Result<bool, String> {
    if dest.join(".git").exists() {
        return Ok(true);
    }
    let repository = git2::Repository::init(dest)
        .map_err(|error| format!("初始化 Git 仓库失败: {error}"))?;
    let mut index = repository
        .index()
        .map_err(|error| format!("打开 Git 索引失败: {error}"))?;
    index
        .add_all(["."].iter(), git2::IndexAddOption::DEFAULT, None)
        .map_err(|error| format!("加入 Git 文件失败: {error}"))?;
    index
        .write()
        .map_err(|error| format!("写入 Git 索引失败: {error}"))?;
    let tree_id = index
        .write_tree()
        .map_err(|error| format!("写入 Git tree 失败: {error}"))?;
    let tree = repository
        .find_tree(tree_id)
        .map_err(|error| format!("读取 Git tree 失败: {error}"))?;
    let signature = git2::Signature::now("Tempo", "plugins@tempo.local")
        .map_err(|error| format!("创建 Git 签名失败: {error}"))?;
    repository
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            "Initial Tempo plugin repository",
            &tree,
            &[],
        )
        .map_err(|error| format!("创建初始提交失败: {error}"))?;
    Ok(true)
}

fn reveal_destination(app: &AppHandle, dest: &Path) {
    if let Err(error) = app
        .opener()
        .open_path(dest.display().to_string(), None::<String>)
    {
        tracing::warn!(
            path = %dest.display(),
            error = %error,
            "failed to open created plugin repository"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::ids::is_valid_repository_index_id;
    use std::fs;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "tempo-repo-template-test-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("clock")
                    .as_nanos()
            ));
            fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn bundled_template_includes_index_and_example_dist() {
        assert!(
            TEMPLATE.get_file(INDEX_FILE).is_some(),
            "embedded template must include {INDEX_FILE}"
        );
        assert!(
            TEMPLATE
                .get_file("plugins/com.example.welcome/dist/manifest.json")
                .is_some(),
            "embedded template must include the example plugin dist"
        );
    }

    #[test]
    fn writes_identity_and_example_plugin() {
        let parent = TempDir::new();
        let dest = destination_path(parent.0.to_str().unwrap(), Some("acme-plugins"))
            .expect("dest");
        prepare_destination(&dest).expect("prepare");
        write_template(&dest).expect("write template");
        let repository_id = new_repository_index_id();
        write_index(
            &dest,
            &repository_id,
            Some("Internal plugins"),
            None,
        )
        .expect("write index");
        initialize_git(&dest).expect("git init");

        let index: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(dest.join(INDEX_FILE)).expect("read index"),
        )
        .expect("parse index");
        assert_eq!(index["id"], repository_id);
        assert!(is_valid_repository_index_id(index["id"].as_str().unwrap()));
        assert!(index.get("name").is_none());
        assert_eq!(index["description"], "Internal plugins");
        assert!(dest.join("plugins/com.example.welcome/dist/manifest.json").is_file());
        assert!(dest.join(".gitignore").is_file());
        assert!(dest.join(".git").exists());
    }

    #[test]
    fn defaults_folder_name_and_keeps_example_plugin() {
        let parent = TempDir::new();
        let dest = destination_path(parent.0.to_str().unwrap(), None).expect("dest");
        prepare_destination(&dest).expect("prepare");
        write_template(&dest).expect("write template");
        write_index(&dest, &new_repository_index_id(), None, None).expect("write index");

        let index: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(dest.join(INDEX_FILE)).expect("read index"),
        )
        .expect("parse index");
        assert_eq!(
            index["plugins"].as_array().map(|plugins| plugins[0]["id"].as_str()),
            Some(Some("com.example.welcome"))
        );
        assert!(dest.join("plugins/com.example.welcome/dist/manifest.json").is_file());
        assert!(dest.join("scripts/validate.mjs").is_file());
        assert!(dest.ends_with(DEFAULT_FOLDER_NAME));
    }
}
