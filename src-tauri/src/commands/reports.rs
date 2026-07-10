use crate::db::{get_daily_total, today_str, AppState, AppUsage, DailyReport, HourlyData, WeeklyDay, WeeklyReport, MAX_DAILY_SECONDS, MAX_HOURLY_SECONDS};
use base64::Engine as _;
use chrono::Local;
use rusqlite::{params, Connection};

pub(crate) const DAILY_RECOMMENDED_LIMIT_SECONDS: i64 = 8 * 60 * 60;

fn hydrate_app_icons(state: &AppState, date: &str, apps: &mut [AppUsage]) {
    for app in apps.iter_mut() {
        if app
            .icon_data_url
            .as_deref()
            .is_some_and(|icon| !icon_needs_refresh(icon))
        {
            continue;
        }

        let Some(icon) =
            crate::platform::resolve_app_icon_data_url(&app.app_name, &app.process_name)
        else {
            continue;
        };

        {
            let conn = state.db.lock();
            conn.execute(
                "UPDATE app_usage SET icon_data_url = ?1 WHERE date = ?2 AND app_name = ?3",
                params![icon, date, app.app_name],
            )
            .ok();
        }

        app.icon_data_url = Some(icon);
    }
}

fn icon_needs_refresh(icon_data_url: &str) -> bool {
    if icon_data_url.trim().is_empty() {
        return true;
    }

    let Some(payload) = icon_data_url.strip_prefix("data:image/png;base64,") else {
        return false;
    };

    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(payload) else {
        return true;
    };

    if bytes.len() < 24 || &bytes[0..8] != b"\x89PNG\r\n\x1a\n" {
        return false;
    }

    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    width < 64
}
#[tauri::command]
pub fn get_daily_report(state: tauri::State<AppState>, date: Option<String>) -> DailyReport {
    let date = date.unwrap_or_else(today_str);
    let (total, hourly, mut top_apps) = {
        let conn = state.db.lock();
        let mut hourly = Vec::new();
        for h in 0..24 {
            let secs: i64 = conn
                .query_row(
                    "SELECT COALESCE(seconds, 0) FROM screen_time_hourly WHERE date = ?1 AND hour = ?2",
                    params![date, h],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            hourly.push(HourlyData {
                hour: h,
                seconds: secs.clamp(0, MAX_HOURLY_SECONDS),
            });
        }
        let total = hourly
            .iter()
            .map(|item| item.seconds)
            .sum::<i64>()
            .min(MAX_DAILY_SECONDS);

        (total, hourly, crate::db::top_apps(&conn, &date, i64::MAX))
    };
    hydrate_app_icons(&state, &date, &mut top_apps);

    let peak = hourly
        .iter()
        .max_by_key(|h| h.seconds)
        .cloned()
        .unwrap_or(HourlyData {
            hour: 0,
            seconds: 0,
        });

    let active_hours = hourly.iter().filter(|h| h.seconds > 0).count().max(1) as i64;
    let average = total / active_hours;

    DailyReport {
        date: date.clone(),
        total_seconds: total,
        average_seconds: average,
        peak_hour: peak.hour,
        peak_seconds: peak.seconds,
        hourly,
        top_apps,
    }
}

#[tauri::command]
pub fn get_weekly_report(state: tauri::State<AppState>) -> WeeklyReport {
    let today = Local::now().date_naive();
    let today_text = today.format("%Y-%m-%d").to_string();
    let start_text = (today - chrono::Duration::days(6))
        .format("%Y-%m-%d")
        .to_string();
    let (mut days, average, mut top_apps) = {
        let conn = state.db.lock();
        let mut days = Vec::new();
        let mut total = 0i64;

        for i in (0..7).rev() {
            let d = today - chrono::Duration::days(i);
            let ds = d.format("%Y-%m-%d").to_string();
            let secs = get_daily_total(&conn, &ds);
            total += secs;
            days.push(WeeklyDay {
                date: ds,
                seconds: secs,
                is_over_limit: false,
            });
        }

        (
            days,
            total / 7,
            weekly_top_apps(&conn, &start_text, &today_text),
        )
    };

    hydrate_app_icons(&state, &today_text, &mut top_apps);
    for day in &mut days {
        day.is_over_limit = day.seconds > DAILY_RECOMMENDED_LIMIT_SECONDS;
    }
    WeeklyReport {
        days,
        average_seconds: average,
        daily_limit_seconds: DAILY_RECOMMENDED_LIMIT_SECONDS,
        top_apps,
    }
}

fn weekly_top_apps(conn: &Connection, start_date: &str, end_date: &str) -> Vec<AppUsage> {
    let mut stmt = match conn.prepare(
        "SELECT app_name,
                COALESCE(MAX(process_name), ''),
                COALESCE(MAX(category), ''),
                COALESCE(SUM(seconds), 0),
                MAX(icon_data_url)
         FROM app_usage
         WHERE date >= ?1 AND date <= ?2
         GROUP BY app_name
         ORDER BY SUM(seconds) DESC, app_name ASC",
    ) {
        Ok(stmt) => stmt,
        Err(_) => return Vec::new(),
    };

    let rows = match stmt.query_map(params![start_date, end_date], |r| {
        Ok(AppUsage {
            app_name: r.get(0)?,
            process_name: r.get(1)?,
            category: r.get(2)?,
            seconds: r.get(3)?,
            icon_data_url: r.get(4)?,
        })
    }) {
        Ok(rows) => rows,
        Err(_) => return Vec::new(),
    };

    rows.filter_map(|row| row.ok())
        .filter(|app| !crate::db::is_system_host_usage(&app.app_name, &app.process_name))
        .collect()
}
pub(crate) fn format_duration(seconds: i64) -> String {
    let h = seconds / 3600;
    let m = (seconds % 3600) / 60;
    let s = seconds % 60;
    if h > 0 {
        format!("{}小时{}分钟", h, m)
    } else {
        format!("{}分钟{}秒", m, s)
    }
}
#[tauri::command]
pub fn get_known_apps(state: tauri::State<AppState>) -> Vec<AppUsage> {
    let today = today_str();
    let mut apps = {
        let conn = state.db.lock();
        crate::db::top_apps(&conn, &today, 50)
    };
    hydrate_app_icons(&state, &today, &mut apps);
    apps
}
