use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
#[cfg(debug_assertions)]
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::EnvFilter;

static LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();
static PANIC_HOOK_INSTALLED: OnceLock<()> = OnceLock::new();

const DEFAULT_LOG_FILE_COUNT: usize = 15;
const MAX_LOG_VALUE_CHARS: usize = 1024;

pub fn init(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;

    install_panic_hook();
    let log_dir = app
        .path()
        .app_log_dir()
        .map_err(|error| error.to_string())?;
    init_with_dir(&log_dir)?;
    Ok(log_dir)
}

fn init_with_dir(log_dir: &Path) -> Result<(), String> {
    if LOG_GUARD.get().is_some() {
        return Ok(());
    }

    std::fs::create_dir_all(log_dir).map_err(|error| error.to_string())?;

    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("tempo")
        .filename_suffix("log")
        .max_log_files(DEFAULT_LOG_FILE_COUNT)
        .build(log_dir)
        .map_err(|error| error.to_string())?;
    let (writer, guard) = tracing_appender::non_blocking(appender);

    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(default_filter()))
        .map_err(|error| error.to_string())?;

    #[cfg(debug_assertions)]
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer.and(std::io::stdout))
        .with_ansi(false)
        .with_thread_ids(true)
        .with_thread_names(true)
        .compact()
        .finish();

    #[cfg(not(debug_assertions))]
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(writer)
        .with_ansi(false)
        .with_thread_ids(true)
        .with_thread_names(true)
        .compact()
        .finish();

    let _ = tracing_log::LogTracer::init();
    tracing::subscriber::set_global_default(subscriber).map_err(|error| error.to_string())?;
    LOG_GUARD
        .set(guard)
        .map_err(|_| "runtime logger already initialized".to_string())?;

    Ok(())
}

fn default_filter() -> &'static str {
    if cfg!(debug_assertions) {
        "tempo=debug,tauri=info,wry=warn"
    } else {
        "tempo=info,tauri=warn,wry=warn"
    }
}

#[cfg(test)]
pub(crate) fn console_logging_enabled() -> bool {
    cfg!(debug_assertions)
}

pub fn install_panic_hook() {
    if PANIC_HOOK_INSTALLED.set(()).is_err() {
        return;
    }

    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("unnamed");
        let message = panic_message(info);
        if let Some(location) = info.location() {
            tracing::error!(
                target: "tempo::panic",
                thread = %thread_name,
                file = location.file(),
                line = location.line(),
                message = %message,
                "thread panicked"
            );
        } else {
            tracing::error!(
                target: "tempo::panic",
                thread = %thread_name,
                message = %message,
                "thread panicked"
            );
        }
        previous_hook(info);
    }));
}

fn panic_message(info: &std::panic::PanicHookInfo<'_>) -> String {
    if let Some(message) = info.payload().downcast_ref::<&str>() {
        return sanitize_log_value(message);
    }
    if let Some(message) = info.payload().downcast_ref::<String>() {
        return sanitize_log_value(message);
    }
    "<non-string panic payload>".to_string()
}

pub fn spawn_named(name: &'static str, run: impl FnOnce() + Send + 'static) {
    let thread_name = name.to_string();
    let spawn_result = std::thread::Builder::new()
        .name(thread_name.clone())
        .spawn(move || {
            tracing::debug!(target: "tempo::runtime", thread = %thread_name, "background thread started");
            run();
            tracing::debug!(target: "tempo::runtime", thread = %thread_name, "background thread stopped");
        });

    if let Err(error) = spawn_result {
        tracing::error!(
            target: "tempo::runtime",
            thread = %name,
            error = %error,
            "failed to spawn background thread"
        );
    }
}

pub fn warn_if_err<T, E: Display>(result: Result<T, E>, operation: &'static str) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            tracing::warn!(
                operation = %operation,
                error = %sanitize_log_value(&error.to_string()),
                "operation failed"
            );
            None
        }
    }
}

pub fn debug_if_err<T, E: Display>(result: Result<T, E>, operation: &'static str) -> Option<T> {
    match result {
        Ok(value) => Some(value),
        Err(error) => {
            tracing::debug!(
                operation = %operation,
                error = %sanitize_log_value(&error.to_string()),
                "operation skipped or failed"
            );
            None
        }
    }
}

pub fn sanitize_log_value(value: &str) -> String {
    let mut sanitized = String::new();
    let mut truncated = false;

    for (index, ch) in value.chars().enumerate() {
        if index >= MAX_LOG_VALUE_CHARS {
            truncated = true;
            break;
        }
        if ch.is_control() {
            sanitized.push(' ');
        } else {
            sanitized.push(ch);
        }
    }

    if truncated {
        sanitized.push_str("...");
    }
    sanitized
}
