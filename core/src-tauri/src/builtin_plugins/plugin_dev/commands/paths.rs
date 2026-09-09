use std::path::{Path, PathBuf};
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

pub(super) fn hash_text(raw: &str) -> String {
    hex::encode(Sha256::digest(raw.as_bytes()))
}

pub(super) fn path_from_input(path: &str) -> Result<PathBuf, String> {
    let value = path.trim();
    if value.to_ascii_lowercase().starts_with("file:") {
        return reqwest::Url::parse(value)
            .map_err(|error| format!("无效文件路径: {error}"))?
            .to_file_path()
            .map_err(|_| "文件 URL 不是本机路径".to_string());
    }
    Ok(PathBuf::from(value))
}

#[cfg(windows)]
pub(super) fn path_to_storage_string(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    value.strip_prefix(r"\\?\").unwrap_or(&value).to_string()
}

#[cfg(not(windows))]
pub(super) fn path_to_storage_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(windows)]
pub(super) fn migrate_plugin_dev_paths(conn: &Connection) -> Result<(), String> {
    let projects = {
        let mut stmt = conn
            .prepare("SELECT id, root_path FROM plugin_dev_projects")
            .map_err(|error| format!("读取开发项目路径失败: {error}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| format!("读取开发项目路径失败: {error}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("读取开发项目路径失败: {error}"))?
    };
    for (id, path) in projects {
        let normalized = path_to_storage_string(Path::new(&path));
        if normalized != path {
            conn.execute(
                "UPDATE plugin_dev_projects SET root_path = ?1 WHERE id = ?2",
                params![normalized, id],
            )
            .map_err(|error| format!("更新开发项目路径失败: {error}"))?;
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub(super) fn migrate_plugin_dev_paths(_conn: &Connection) -> Result<(), String> {
    Ok(())
}

pub(super) fn canonical_directory(path: &str) -> Result<PathBuf, String> {
    let path = path_from_input(path)?;
    if !path.is_dir() {
        return Err(format!("目录不存在: {}", path.display()));
    }
    path.canonicalize()
        .map_err(|error| format!("读取目录失败 {}: {error}", path.display()))
}

pub(super) fn manifest_path(root: &Path) -> PathBuf {
    root.join("manifest.json")
}


#[cfg(not(windows))]
pub(super) fn replace_file(temp: &Path, destination: &Path) -> Result<(), String> {
    std::fs::rename(temp, destination).map_err(|error| format!("替换 manifest.json 失败: {error}"))
}

#[cfg(windows)]
pub(super) fn replace_file(temp: &Path, destination: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = temp
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let target = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(target.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
        .map_err(|error| format!("替换 manifest.json 失败: {error}"))
    }
}

