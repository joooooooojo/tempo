use crate::clipboard_db::{
    clear_clipboard_history, count_clipboard_entries, delete_clipboard_entry,
    list_clipboard_entries, set_clipboard_entry_pinned, ClipboardEntry,
};
use crate::clipboard_images::{
    hydrate_clipboard_image_content, hydrate_clipboard_image_urls, maybe_delete_clipboard_image_file,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ClipboardHistoryPage {
    pub entries: Vec<ClipboardEntry>,
    pub total: u32,
    pub has_more: bool,
}
use crate::clipboard_watcher::prewarm_clipboard_image_cache;
use crate::clipboard_watcher::{copy_clipboard_entry_by_id, write_clipboard_text};
use crate::db::{AppState, PALETTE_CLIPBOARD_SEED_MAX_AGE_MS};

fn hydrate_clipboard_icons(entries: &mut [ClipboardEntry]) {
    for entry in entries.iter_mut() {
        let app_name = entry.source_app.as_deref().unwrap_or("").trim();
        if app_name.is_empty() {
            continue;
        }
        let process_name = entry
            .source_process
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(app_name);
        entry.source_icon_data_url =
            crate::app_icons::AppIconService::global().icon_url(app_name, process_name);
    }
}

#[tauri::command]
pub fn get_clipboard_history(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    query: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> ClipboardHistoryPage {
    let conn = state.db.lock();
    let limit = limit.unwrap_or(200).min(500);
    let offset = offset.unwrap_or(0);
    let total = count_clipboard_entries(&conn, query.as_deref());
    let mut entries = list_clipboard_entries(&conn, query.as_deref(), limit, offset);
    hydrate_clipboard_icons(&mut entries);
    hydrate_clipboard_image_urls(&mut entries);
    drop(conn);
    let image_contents = entries
        .iter()
        .filter(|entry| entry.kind == "image")
        .map(|entry| entry.content.clone())
        .collect::<Vec<_>>();
    if !image_contents.is_empty() {
        prewarm_clipboard_image_cache(app.clone(), state.inner().clone(), image_contents);
    }
    let loaded = offset.saturating_add(entries.len() as u32);
    ClipboardHistoryPage {
        has_more: loaded < total,
        total,
        entries,
    }
}

#[tauri::command]
pub fn delete_clipboard_history_entry(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    id: i64,
) -> Result<(), String> {
    let conn = state.db.lock();
    match delete_clipboard_entry(&conn, id) {
        Ok(image_content) => {
            drop(conn);
            if let Some(content) = image_content {
                let conn = state.db.lock();
                maybe_delete_clipboard_image_file(&conn, &app, &content);
            }
            Ok(())
        }
        Err(()) => Err("记录不存在".into()),
    }
}

#[tauri::command]
pub fn clear_clipboard_history_command(state: tauri::State<AppState>) -> Result<u32, String> {
    let conn = state.db.lock();
    Ok(clear_clipboard_history(&conn))
}

#[tauri::command]
pub fn pin_clipboard_history_entry(
    state: tauri::State<AppState>,
    id: i64,
    pinned: bool,
) -> Result<ClipboardEntry, String> {
    let conn = state.db.lock();
    let mut entry =
        set_clipboard_entry_pinned(&conn, id, pinned).ok_or("记录不存在".to_string())?;
    hydrate_clipboard_icons(std::slice::from_mut(&mut entry));
    hydrate_clipboard_image_urls(std::slice::from_mut(&mut entry));
    drop(conn);
    Ok(entry)
}

#[tauri::command]
pub fn copy_text_to_clipboard(state: tauri::State<AppState>, text: String) -> Result<(), String> {
    write_clipboard_text(&state, &text)
}

#[tauri::command]
pub fn copy_clipboard_entry(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    id: i64,
) -> Result<(), String> {
    tracing::debug!(target: "tempo::clipboard", entry_id = id, "copy clipboard entry command");
    copy_clipboard_entry_by_id(&state, &app, id)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandPaletteClipboardSeed {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_height: Option<u32>,
}

#[tauri::command]
pub fn get_command_palette_clipboard_seed(
    state: tauri::State<AppState>,
) -> Result<Option<CommandPaletteClipboardSeed>, String> {
    let recent = {
        let runtime = state.clipboard.lock();
        runtime.recent_for_palette.clone()
    };
    let Some(recent) = recent else {
        return Ok(None);
    };

    let now_ms = chrono::Utc::now().timestamp_millis();
    if now_ms.saturating_sub(recent.captured_at_ms) > PALETTE_CLIPBOARD_SEED_MAX_AGE_MS {
        return Ok(None);
    }

    if recent.kind == "text" {
        let Some(full_text) = recent.text.filter(|value| !value.trim().is_empty()) else {
            return Ok(None);
        };
        return Ok(Some(CommandPaletteClipboardSeed {
            kind: "text".into(),
            full_text: Some(full_text),
            entry_id: recent.entry_id,
            image_url: None,
            image_width: None,
            image_height: None,
        }));
    }

    if recent.kind == "image" {
        let Some(entry_id) = recent.entry_id else {
            return Ok(None);
        };
        let conn = state.db.lock();
        let Some(entry) = crate::clipboard_db::get_clipboard_entry(&conn, entry_id) else {
            return Ok(None);
        };
        drop(conn);
        let image_url = if entry.kind == "image" {
            Some(hydrate_clipboard_image_content(&entry.content))
        } else {
            None
        };
        return Ok(Some(CommandPaletteClipboardSeed {
            kind: "image".into(),
            full_text: None,
            entry_id: Some(entry_id),
            image_url,
            image_width: recent.image_width.or(entry.image_width),
            image_height: recent.image_height.or(entry.image_height),
        }));
    }

    Ok(None)
}
