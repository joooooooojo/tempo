use crate::auxiliary_windows;
use crate::commands;
use crate::db::AppState;
#[cfg(not(target_os = "macos"))]
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::TrayIconBuilder,
    App, AppHandle, Emitter, Manager, Wry,
};

pub struct TrayMenuState {
    pomodoro_float: CheckMenuItem<Wry>,
}

impl TrayMenuState {
    pub fn sync_pomodoro_float_checked(&self, visible: bool) {
        crate::logging::debug_if_err(
            self.pomodoro_float.set_checked(visible),
            "sync pomodoro float tray check state",
        );
    }
}

pub fn sync_pomodoro_float_checked(app: &AppHandle, visible: bool) {
    if let Some(state) = app.try_state::<TrayMenuState>() {
        state.sync_pomodoro_float_checked(visible);
    }
}

pub fn setup_tray(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "打开快捷面板", true, None::<&str>)?;
    let pomodoro_float = CheckMenuItem::with_id(
        app,
        "pomodoro_float",
        "番茄钟悬浮窗",
        true,
        auxiliary_windows::is_pomodoro_float_visible(app.handle()),
        None::<&str>,
    )?;
    let reset = MenuItem::with_id(app, "reset", "清空当日数据", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出软件", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &pomodoro_float, &reset, &quit])?;

    app.manage(TrayMenuState {
        pomodoro_float: pomodoro_float.clone(),
    });

    let tray_builder = TrayIconBuilder::with_id("main")
        .icon(
            app.default_window_icon()
                .ok_or("missing default window icon")?
                .clone(),
        )
        .menu(&menu)
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .tooltip("Tempo: 加载中...")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                crate::logging::warn_if_err(
                    auxiliary_windows::show_command_palette(app),
                    "tray show command palette",
                );
            }
            "pomodoro_float" => {
                crate::logging::warn_if_err(
                    auxiliary_windows::toggle_pomodoro_float_window(app),
                    "tray toggle pomodoro float",
                );
            }
            "reset" => {
                if let Some(state) = app.try_state::<AppState>() {
                    commands::do_reset_today(&state);
                    crate::logging::debug_if_err(
                        app.emit(
                            "toast",
                            serde_json::json!({ "message": "今日数据已清空" }),
                        ),
                        "emit reset toast",
                    );
                } else {
                    tracing::warn!("tray reset requested before app state was available");
                }
            }
            "quit" => {
                commands::quit_app(app.clone());
            }
            _ => {}
        });

    #[cfg(not(target_os = "macos"))]
    let tray = tray_builder
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                crate::logging::warn_if_err(
                    auxiliary_windows::show_command_palette(app),
                    "tray click show command palette",
                );
            }
        })
        .build(app)?;

    #[cfg(target_os = "macos")]
    let tray = tray_builder.build(app)?;

    tray.with_inner_tray_icon(|inner| {
        inner.set_show_menu_on_right_click(!cfg!(target_os = "macos"));
    })?;

    Ok(())
}
