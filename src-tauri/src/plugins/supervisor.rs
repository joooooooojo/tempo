//! Per-plugin Node Runtime supervisor (design §3.2, §6.2, §6.3).
//!
//! One Node child process per plugin, talking to the host over a length-prefixed JSON frame
//! protocol on a Unix domain socket (0600) / Windows named pipe, handshaking via a token passed
//! on stdin (never argv/env — design §7). Activation is always lazy: `ensure_started` is only
//! called from a Command/private IPC invocation (or `onStartup`), never at boot/enable time.
//!
//! Simplifications explicitly taken for Phase 1 given scope (documented, not hidden):
//! - Crash backoff (1/5/30s, max 3 per 10 min) is evaluated lazily on the *next* triggered
//!   call rather than via a persistent background auto-restart timer — consistent with "no
//!   Runtime unless activated" and avoids a scheduler thread per plugin.
//! - Process-tree cleanup uses a POSIX process group + `kill`/taskkill best effort rather than
//!   a full Job Object implementation on Windows.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex as SyncMutex;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot, Mutex as AsyncMutex};

use crate::db::AppState;

use super::bridge::{self, ConnectionContext, RpcError};
use super::host::{generate_id, PluginHost};
use super::package::verify_package_hash;
#[cfg(unix)]
use super::paths::plugin_ipc_dir;
use super::runtime::resolved_node_path;
use super::trust::{ensure_plugin_tables, get_installed_plugin, set_last_error, set_runtime_state};

/// Runtime state machine (design §6.2): `disabled | enabled | starting | active | draining | failed`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeState {
    Starting,
    Active,
    Draining,
    Failed,
    Stopped,
}

impl RuntimeState {
    fn as_db_str(self) -> &'static str {
        match self {
            RuntimeState::Starting => "starting",
            RuntimeState::Active => "active",
            RuntimeState::Draining => "draining",
            RuntimeState::Failed => "failed",
            RuntimeState::Stopped => "enabled",
        }
    }
}

const MAX_RESTARTS_PER_WINDOW: usize = 3;
const RESTART_WINDOW: Duration = Duration::from_secs(10 * 60);
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const ACTIVATE_TIMEOUT: Duration = Duration::from_secs(10);
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

fn backoff_for(crash_count: usize) -> Duration {
    match crash_count {
        0 => Duration::from_secs(0),
        1 => Duration::from_secs(1),
        2 => Duration::from_secs(5),
        _ => Duration::from_secs(30),
    }
}

struct RuntimeProcess {
    plugin_id: String,
    state: SyncMutex<RuntimeState>,
    write_tx: mpsc::UnboundedSender<Vec<u8>>,
    pending: SyncMutex<HashMap<String, oneshot::Sender<Result<Value, RpcError>>>>,
    pid: Option<u32>,
    crash_history: SyncMutex<VecDeque<Instant>>,
}

impl RuntimeProcess {
    fn is_usable(&self) -> bool {
        matches!(
            *self.state.lock(),
            RuntimeState::Active | RuntimeState::Starting
        )
    }
}

pub struct Supervisor {
    app: AppHandle,
    processes: SyncMutex<HashMap<String, Arc<RuntimeProcess>>>,
    start_locks: SyncMutex<HashMap<String, Arc<AsyncMutex<()>>>>,
}

impl Supervisor {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            processes: SyncMutex::new(HashMap::new()),
            start_locks: SyncMutex::new(HashMap::new()),
        }
    }

    fn start_lock_for(&self, plugin_id: &str) -> Arc<AsyncMutex<()>> {
        self.start_locks
            .lock()
            .entry(plugin_id.to_string())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    }

    fn get(&self, plugin_id: &str) -> Option<Arc<RuntimeProcess>> {
        self.processes.lock().get(plugin_id).cloned()
    }

    pub fn is_running(&self, plugin_id: &str) -> bool {
        self.get(plugin_id).is_some_and(|p| p.is_usable())
    }

    fn persist_state(&self, plugin_id: &str, state: RuntimeState) {
        if let Some(app_state) = self.app.try_state::<AppState>() {
            let conn = app_state.db.lock();
            let _ = ensure_plugin_tables(&conn);
            let _ = set_runtime_state(&conn, plugin_id, state.as_db_str());
        }
    }

    fn persist_error(&self, plugin_id: &str, message: Option<&str>) {
        if let Some(app_state) = self.app.try_state::<AppState>() {
            let conn = app_state.db.lock();
            let _ = set_last_error(&conn, plugin_id, message);
        }
    }

    /// Lazily activate a plugin's Runtime if it is not already active (design §6.2/§6.3).
    /// Never called for `onStartup` boot logic outside of an explicit trigger from the loader.
    pub async fn ensure_started(&self, plugin_id: &str) -> Result<(), RpcError> {
        if self.is_running(plugin_id) {
            return Ok(());
        }
        let lock = self.start_lock_for(plugin_id);
        let _guard = lock.lock().await;
        if self.is_running(plugin_id) {
            return Ok(());
        }
        self.spawn(plugin_id).await
    }

    /// Invoke an externally declared Command in this plugin's Runtime.
    pub async fn call(
        &self,
        plugin_id: &str,
        command_id: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, RpcError> {
        self.call_framed(
            plugin_id,
            json!({
                "type": "invoke",
                "commandId": command_id,
                "params": params,
            }),
            timeout,
            "command timed out",
        )
        .await
    }

    /// Invoke an MCP Tool registered with `tempo.mcpTools.register` in this plugin's Runtime.
    pub async fn call_mcp_tool(
        &self,
        plugin_id: &str,
        tool_name: &str,
        arguments: Value,
        timeout: Duration,
    ) -> Result<Value, RpcError> {
        self.call_framed(
            plugin_id,
            json!({
                "type": "mcp-invoke",
                "toolName": tool_name,
                "arguments": arguments,
            }),
            timeout,
            "MCP tool timed out",
        )
        .await
    }

    /// Private UI ↔ Runtime IPC invoke (`ipc.invoke` / `ipc-invoke` frame).
    /// `args` is an opaque SCA envelope (`{ "$sca": "..." }`) or legacy JSON.
    pub async fn call_ipc(
        &self,
        plugin_id: &str,
        channel: &str,
        args: Value,
        timeout: Duration,
    ) -> Result<Value, RpcError> {
        self.call_framed(
            plugin_id,
            json!({
                "type": "ipc-invoke",
                "channel": channel,
                "args": args,
            }),
            timeout,
            "ipc timed out",
        )
        .await
    }

    /// Fire-and-forget UI → Runtime (`ipc.send` / `ipc-send` frame).
    /// `args` is an opaque SCA envelope (`{ "$sca": "..." }`).
    pub async fn send_ipc(
        &self,
        plugin_id: &str,
        channel: &str,
        args: Value,
    ) -> Result<(), RpcError> {
        self.ensure_started(plugin_id).await?;
        let process = self.get(plugin_id).ok_or_else(|| {
            RpcError::new(
                bridge::codes::RUNTIME_UNAVAILABLE,
                "plugin runtime is not running",
            )
        })?;
        let frame = json!({
            "type": "ipc-send",
            "channel": channel,
            "args": args,
        });
        if encode_and_send(&process.write_tx, &frame).is_err() {
            return Err(RpcError::new(
                bridge::codes::RUNTIME_UNAVAILABLE,
                "plugin runtime connection closed",
            ));
        }
        Ok(())
    }

    async fn call_framed(
        &self,
        plugin_id: &str,
        mut frame: Value,
        timeout: Duration,
        timeout_message: &str,
    ) -> Result<Value, RpcError> {
        self.ensure_started(plugin_id).await?;
        let process = self.get(plugin_id).ok_or_else(|| {
            RpcError::new(
                bridge::codes::RUNTIME_UNAVAILABLE,
                "plugin runtime is not running",
            )
        })?;

        let id = generate_id();
        if let Some(object) = frame.as_object_mut() {
            object.insert("id".into(), json!(id));
        }
        let (tx, rx) = oneshot::channel();
        process.pending.lock().insert(id.clone(), tx);

        if encode_and_send(&process.write_tx, &frame).is_err() {
            process.pending.lock().remove(&id);
            return Err(RpcError::new(
                bridge::codes::RUNTIME_UNAVAILABLE,
                "plugin runtime connection closed",
            ));
        }

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => {
                process.pending.lock().remove(&id);
                Err(RpcError::internal("runtime call", "runtime channel closed"))
            }
            Err(_) => {
                process.pending.lock().remove(&id);
                let cancel = json!({ "type": "cancel", "id": id });
                let _ = encode_and_send(&process.write_tx, &cancel);
                Err(RpcError::new(bridge::codes::TIMEOUT, timeout_message))
            }
        }
    }

    /// Disable/uninstall path (design §6.2 `disable`): mark draining, ask for graceful
    /// shutdown, then kill the process tree if it doesn't exit in time.
    pub async fn stop(&self, plugin_id: &str) {
        self.stop_with_grace(plugin_id, SHUTDOWN_GRACE).await;
    }

    pub async fn stop_development(&self, plugin_id: &str) {
        self.stop_with_grace(plugin_id, Duration::from_millis(750))
            .await;
    }

    async fn stop_with_grace(&self, plugin_id: &str, grace: Duration) {
        let Some(process) = self.processes.lock().remove(plugin_id) else {
            return;
        };
        *process.state.lock() = RuntimeState::Draining;
        self.persist_state(plugin_id, RuntimeState::Draining);

        for (_, pending) in process.pending.lock().drain() {
            let _ = pending.send(Err(RpcError::new(
                bridge::codes::CANCELLED,
                "plugin disabled",
            )));
        }

        let _ = encode_and_send(&process.write_tx, &json!({ "type": "shutdown" }));
        tokio::time::sleep(grace).await;
        if let Some(pid) = process.pid {
            kill_process_tree(pid);
        }
        *process.state.lock() = RuntimeState::Stopped;
    }

    /// Best-effort host → runtime push event (e.g. `settings.changed`).
    pub fn emit_event(&self, plugin_id: &str, event: &str, payload: Value) -> bool {
        let Some(process) = self.get(plugin_id) else {
            return false;
        };
        if !process.is_usable() {
            return false;
        }
        encode_and_send(
            &process.write_tx,
            &json!({
                "type": "event",
                "event": event,
                "payload": payload,
            }),
        )
        .is_ok()
    }

    async fn spawn(&self, plugin_id: &str) -> Result<(), RpcError> {
        // Crash backoff bookkeeping is per-plugin and survives across `spawn` calls via the
        // process map entry that stays registered even after a crash (see `note_exit`).
        if let Some(existing) = self.get(plugin_id) {
            let count = {
                let mut history = existing.crash_history.lock();
                prune_crash_history(&mut history);
                history.len()
            };
            if count >= MAX_RESTARTS_PER_WINDOW {
                return Err(RpcError::new(
                    bridge::codes::ACTIVATION_FAILED,
                    "插件最近崩溃次数过多，已停止自动重启，请稍后再试或检查插件日志",
                ));
            }
            let delay = backoff_for(count);
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            self.processes.lock().remove(plugin_id);
        }

        self.persist_state(plugin_id, RuntimeState::Starting);
        let result = self.spawn_inner(plugin_id).await;
        match &result {
            Ok(()) => {
                self.persist_state(plugin_id, RuntimeState::Active);
                self.persist_error(plugin_id, None);
            }
            Err(error) => {
                self.persist_state(plugin_id, RuntimeState::Failed);
                self.persist_error(plugin_id, Some(&error.message));
            }
        }
        result
    }

    async fn spawn_inner(&self, plugin_id: &str) -> Result<(), RpcError> {
        if let Some(host) = self.app.try_state::<Arc<PluginHost>>() {
            if let Some(development) = host.development_plugin(plugin_id) {
                return self.spawn_development_inner(development).await;
            }
        }

        let app_state = self
            .app
            .try_state::<AppState>()
            .ok_or_else(|| RpcError::internal("spawn runtime", "app state unavailable"))?;
        let row = {
            let conn = app_state.db.lock();
            let _ = ensure_plugin_tables(&conn);
            get_installed_plugin(&conn, plugin_id)
                .map_err(|e| RpcError::internal("spawn runtime", e))?
        };
        let Some(row) = row else {
            return Err(RpcError::new(
                bridge::codes::NOT_FOUND,
                "plugin is not installed",
            ));
        };
        if !row.enabled || !row.trusted {
            return Err(RpcError::new(
                bridge::codes::FORBIDDEN,
                "plugin is not enabled/trusted",
            ));
        }

        let install_path = super::paths::packages_dir(&self.app)
            .map_err(|e| RpcError::internal("spawn runtime", e))?
            .join(&row.id)
            .join(&row.current_version);
        let manifest = super::ui::read_manifest(&install_path)
            .map_err(|e| RpcError::new(bridge::codes::ACTIVATION_FAILED, e))?;
        let Some(main_rel) = manifest.main.clone() else {
            return Err(RpcError::new(
                bridge::codes::RUNTIME_UNAVAILABLE,
                "plugin has no main entry (pure UI plugin)",
            ));
        };

        let package_hash = row.package_hash.clone().unwrap_or_default();
        if package_hash.is_empty() {
            return Err(RpcError::new(
                bridge::codes::FORBIDDEN,
                "plugin package hash is unknown; re-import required",
            ));
        }
        // Hash must be re-verified before executing any plugin code (design §8.4 step 9 / §15-1).
        verify_package_hash(&install_path, &package_hash)
            .map_err(|e| RpcError::new(bridge::codes::FORBIDDEN, e))?;

        let node_path = resolved_node_path(&self.app)
            .map_err(|e| RpcError::new(bridge::codes::RUNTIME_UNAVAILABLE, e))?;
        let data_dir = super::paths::plugin_data_dir(&self.app, plugin_id)
            .map_err(|e| RpcError::internal("spawn runtime", e))?;
        super::paths::ensure_dir(&data_dir).map_err(|e| RpcError::internal("spawn runtime", e))?;
        let bootstrap_path = write_bootstrap_script(&self.app)
            .map_err(|e| RpcError::internal("spawn runtime", e))?;
        let main_path = install_path.join(&main_rel);

        let (endpoint_path, token) =
            create_ipc_endpoint().map_err(|e| RpcError::internal("spawn runtime", e))?;

        let mut command = tokio::process::Command::new(&node_path);
        command
            .arg(&bootstrap_path)
            .current_dir(&install_path)
            .stdin(Stdio::piped())
            // Discard plugin stdout/stderr — never forward third-party logs into the host console.
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        apply_minimal_plugin_runtime_env(&mut command);
        prevent_plugin_runtime_console_window(&mut command);
        #[cfg(unix)]
        {
            command.process_group(0);
        }

        let mut child = command.spawn().map_err(|e| {
            RpcError::new(
                bridge::codes::ACTIVATION_FAILED,
                format!("spawn node failed: {e}"),
            )
        })?;
        let pid = child.id();

        let handshake = json!({
            "socketPath": endpoint_path,
            "token": token,
            "pluginId": plugin_id,
            "mainPath": main_path.display().to_string(),
            "dataPath": data_dir.display().to_string(),
            "nodeVersion": super::runtime::OFFICIAL_NODE_VERSION,
        });
        if let Some(mut stdin) = child.stdin.take() {
            let mut line = serde_json::to_vec(&handshake)
                .map_err(|e| RpcError::internal("spawn runtime", e))?;
            line.push(b'\n');
            if let Err(error) = stdin.write_all(&line).await {
                let _ = child.kill().await;
                return Err(RpcError::internal("spawn runtime", error));
            }
            drop(stdin);
        }

        let mut stream = accept_ipc_connection(&endpoint_path, HANDSHAKE_TIMEOUT)
            .await
            .map_err(|e| RpcError::new(bridge::codes::ACTIVATION_FAILED, e))?;
        verify_handshake_frame(&mut stream, &token, HANDSHAKE_TIMEOUT)
            .await
            .map_err(|e| RpcError::new(bridge::codes::ACTIVATION_FAILED, e))?;

        let (read_half, write_half) = tokio::io::split(stream);
        let (write_tx, write_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        spawn_writer(write_half, write_rx);

        let process = Arc::new(RuntimeProcess {
            plugin_id: plugin_id.to_string(),
            state: SyncMutex::new(RuntimeState::Starting),
            write_tx,
            pending: SyncMutex::new(HashMap::new()),
            pid,
            crash_history: SyncMutex::new(VecDeque::new()),
        });
        self.processes
            .lock()
            .insert(plugin_id.to_string(), process.clone());

        let (ready_tx, ready_rx) = oneshot::channel::<Result<(), String>>();
        spawn_reader(
            self.app.clone(),
            process.clone(),
            read_half,
            Some(ready_tx),
            child,
        );

        match tokio::time::timeout(ACTIVATE_TIMEOUT, ready_rx).await {
            Ok(Ok(Ok(()))) => {
                *process.state.lock() = RuntimeState::Active;
                Ok(())
            }
            Ok(Ok(Err(message))) => {
                self.processes.lock().remove(plugin_id);
                Err(RpcError::new(bridge::codes::ACTIVATION_FAILED, message))
            }
            Ok(Err(_)) => {
                self.processes.lock().remove(plugin_id);
                Err(RpcError::internal(
                    "spawn runtime",
                    "activation channel closed",
                ))
            }
            Err(_) => {
                self.processes.lock().remove(plugin_id);
                if let Some(pid) = pid {
                    kill_process_tree(pid);
                }
                Err(RpcError::new(
                    bridge::codes::ACTIVATION_FAILED,
                    "plugin Runtime startup did not complete within 10s",
                ))
            }
        }
    }

    async fn spawn_development_inner(
        &self,
        development: super::host::DevelopmentPlugin,
    ) -> Result<(), RpcError> {
        let plugin_id = development.manifest.id.clone();
        let main_path = development.runtime_entry.ok_or_else(|| {
            RpcError::new(
                bridge::codes::RUNTIME_UNAVAILABLE,
                "development plugin has no Runtime entry",
            )
        })?;
        if !main_path.is_file() {
            return Err(RpcError::new(
                bridge::codes::ACTIVATION_FAILED,
                format!("Runtime 开发入口不存在: {}", main_path.display()),
            ));
        }
        let work_dir = main_path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| development.root_path.clone());
        let node_path = resolved_node_path(&self.app)
            .map_err(|error| RpcError::new(bridge::codes::RUNTIME_UNAVAILABLE, error))?;
        let data_dir = if development.use_production_data {
            super::paths::plugin_data_dir(&self.app, &plugin_id)
        } else {
            super::paths::plugin_dev_data_dir(&self.app, &development.project_id)
        }
        .map_err(|error| RpcError::internal("spawn development runtime", error))?;
        super::paths::ensure_dir(&data_dir)
            .map_err(|error| RpcError::internal("spawn development runtime", error))?;
        let bootstrap_path = write_bootstrap_script(&self.app)
            .map_err(|error| RpcError::internal("spawn development runtime", error))?;
        let (endpoint_path, token) = create_ipc_endpoint()
            .map_err(|error| RpcError::internal("spawn development runtime", error))?;

        let mut command = tokio::process::Command::new(&node_path);
        command
            .arg(&bootstrap_path)
            .current_dir(&work_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        apply_minimal_plugin_runtime_env(&mut command);
        prevent_plugin_runtime_console_window(&mut command);
        #[cfg(unix)]
        command.process_group(0);

        let mut child = command.spawn().map_err(|error| {
            RpcError::new(
                bridge::codes::ACTIVATION_FAILED,
                format!("spawn development node failed: {error}"),
            )
        })?;
        let pid = child.id();
        if let Some(stdout) = child.stdout.take() {
            spawn_development_log_reader(self.app.clone(), plugin_id.clone(), "stdout", stdout);
        }
        if let Some(stderr) = child.stderr.take() {
            spawn_development_log_reader(self.app.clone(), plugin_id.clone(), "stderr", stderr);
        }

        let handshake = json!({
            "socketPath": endpoint_path,
            "token": token,
            "pluginId": plugin_id,
            "mainPath": main_path.display().to_string(),
            "dataPath": data_dir.display().to_string(),
            "nodeVersion": super::runtime::OFFICIAL_NODE_VERSION,
        });
        if let Some(mut stdin) = child.stdin.take() {
            let mut line = serde_json::to_vec(&handshake)
                .map_err(|error| RpcError::internal("spawn development runtime", error))?;
            line.push(b'\n');
            if let Err(error) = stdin.write_all(&line).await {
                let _ = child.kill().await;
                return Err(RpcError::internal("spawn development runtime", error));
            }
        }

        let mut stream = accept_ipc_connection(&endpoint_path, HANDSHAKE_TIMEOUT)
            .await
            .map_err(|error| RpcError::new(bridge::codes::ACTIVATION_FAILED, error))?;
        verify_handshake_frame(&mut stream, &token, HANDSHAKE_TIMEOUT)
            .await
            .map_err(|error| RpcError::new(bridge::codes::ACTIVATION_FAILED, error))?;
        let (read_half, write_half) = tokio::io::split(stream);
        let (write_tx, write_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        spawn_writer(write_half, write_rx);

        let process = Arc::new(RuntimeProcess {
            plugin_id: plugin_id.clone(),
            state: SyncMutex::new(RuntimeState::Starting),
            write_tx,
            pending: SyncMutex::new(HashMap::new()),
            pid,
            crash_history: SyncMutex::new(VecDeque::new()),
        });
        self.processes
            .lock()
            .insert(plugin_id.clone(), process.clone());

        let (ready_tx, ready_rx) = oneshot::channel::<Result<(), String>>();
        spawn_reader(
            self.app.clone(),
            process.clone(),
            read_half,
            Some(ready_tx),
            child,
        );
        match tokio::time::timeout(ACTIVATE_TIMEOUT, ready_rx).await {
            Ok(Ok(Ok(()))) => {
                *process.state.lock() = RuntimeState::Active;
                Ok(())
            }
            Ok(Ok(Err(message))) => {
                self.processes.lock().remove(&plugin_id);
                Err(RpcError::new(bridge::codes::ACTIVATION_FAILED, message))
            }
            Ok(Err(_)) => {
                self.processes.lock().remove(&plugin_id);
                Err(RpcError::internal(
                    "spawn development runtime",
                    "activation channel closed",
                ))
            }
            Err(_) => {
                self.processes.lock().remove(&plugin_id);
                Err(RpcError::new(
                    bridge::codes::ACTIVATION_FAILED,
                    "development plugin activation timed out",
                ))
            }
        }
    }
}

/// Minimal startup environment (design §3.3.1): forward just enough for Node and the plugin's
/// own child_process spawns, without leaking the host's full env.
fn apply_minimal_plugin_runtime_env(command: &mut tokio::process::Command) {
    command.env_clear().env("NODE_ENV", "production");
    const COMMON: &[&str] = &["PATH", "HOME", "USERPROFILE"];
    for key in COMMON {
        if let Ok(value) = std::env::var(key) {
            command.env(key, value);
        }
    }
    // Node 22+ asserts `ncrypto::CSPRNG` during init; on Windows that requires SystemRoot
    // (and related OS paths) to be present — a bare PATH/USERPROFILE is not enough.
    #[cfg(windows)]
    {
        const WINDOWS: &[&str] = &[
            "SystemRoot",
            "WINDIR",
            "ComSpec",
            "APPDATA",
            "LOCALAPPDATA",
            "TEMP",
            "TMP",
            "HOMEDRIVE",
            "HOMEPATH",
            "ProgramFiles",
            "ProgramFiles(x86)",
        ];
        for key in WINDOWS {
            if let Ok(value) = std::env::var(key) {
                command.env(key, value);
            }
        }
    }
}

/// Prevent Windows from allocating a console for the plugin Node process.
///
/// Packaged Tempo is a GUI subsystem app. Spawning `node.exe` (console subsystem)
/// without `CREATE_NO_WINDOW` makes Windows allocate a visible black console.
/// `CREATE_NO_WINDOW` means "never create a console" — it does not create-then-hide.
/// Do not combine with `DETACHED_PROCESS` / `CREATE_NEW_CONSOLE` (those ignore this
/// flag or force a new visible console).
fn prevent_plugin_runtime_console_window(command: &mut tokio::process::Command) {
    #[cfg(windows)]
    prevent_windows_console_window(command.as_std_mut());
    let _ = command;
}

#[cfg(windows)]
fn prevent_windows_console_window(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    use windows::Win32::System::Threading::CREATE_NO_WINDOW;

    command.creation_flags(CREATE_NO_WINDOW.0);
}

fn encode_frame(value: &Value) -> Result<Vec<u8>, String> {
    let body = serde_json::to_vec(value).map_err(|e| format!("encode frame: {e}"))?;
    let mut framed = Vec::with_capacity(4 + body.len());
    framed.extend_from_slice(&(body.len() as u32).to_be_bytes());
    framed.extend_from_slice(&body);
    Ok(framed)
}

fn encode_and_send(tx: &mpsc::UnboundedSender<Vec<u8>>, value: &Value) -> Result<(), String> {
    let framed = encode_frame(value)?;
    tx.send(framed).map_err(|_| "channel closed".to_string())
}

fn spawn_writer(
    mut write_half: tokio::io::WriteHalf<IpcStream>,
    mut rx: mpsc::UnboundedReceiver<Vec<u8>>,
) {
    tokio::spawn(async move {
        while let Some(frame) = rx.recv().await {
            if write_half.write_all(&frame).await.is_err() {
                break;
            }
            if write_half.flush().await.is_err() {
                break;
            }
        }
    });
}

fn spawn_reader(
    app: AppHandle,
    process: Arc<RuntimeProcess>,
    mut read_half: tokio::io::ReadHalf<IpcStream>,
    mut ready_tx: Option<oneshot::Sender<Result<(), String>>>,
    mut child: tokio::process::Child,
) {
    tokio::spawn(async move {
        loop {
            match read_frame(&mut read_half).await {
                Ok(Some(value)) => {
                    handle_frame(&app, &process, &value, &mut ready_tx).await;
                }
                Ok(None) => break,
                Err(error) => {
                    tracing::debug!(plugin_id = %process.plugin_id, error = %error, "plugin ipc read error");
                    break;
                }
            }
        }

        if let Some(ready_tx) = ready_tx.take() {
            let _ = ready_tx.send(Err(
                "plugin runtime disconnected before activation completed".into(),
            ));
        }

        for (_, pending) in process.pending.lock().drain() {
            let _ = pending.send(Err(RpcError::new(
                bridge::codes::RUNTIME_UNAVAILABLE,
                "plugin runtime disconnected",
            )));
        }

        let unexpected = !matches!(
            *process.state.lock(),
            RuntimeState::Draining | RuntimeState::Stopped
        );
        if unexpected {
            let mut history = process.crash_history.lock();
            prune_crash_history(&mut history);
            history.push_back(Instant::now());
            drop(history);
            *process.state.lock() = RuntimeState::Failed;
            if let Some(app_state) = app.try_state::<AppState>() {
                let conn = app_state.db.lock();
                let _ = set_runtime_state(&conn, &process.plugin_id, "failed");
                let _ = set_last_error(&conn, &process.plugin_id, Some("插件运行时意外退出"));
            }
        }

        let _ = child.kill().await;
    });
}

async fn handle_frame(
    app: &AppHandle,
    process: &Arc<RuntimeProcess>,
    value: &Value,
    ready_tx: &mut Option<oneshot::Sender<Result<(), String>>>,
) {
    let Some(kind) = value.get("type").and_then(Value::as_str) else {
        return;
    };
    match kind {
        "ready" => {
            let ok = value.get("ok").and_then(Value::as_bool).unwrap_or(true);
            if let Some(tx) = ready_tx.take() {
                if ok {
                    let _ = tx.send(Ok(()));
                } else {
                    let message = value
                        .get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(Value::as_str)
                        .unwrap_or("Runtime startup failed")
                        .to_string();
                    let _ = tx.send(Err(message));
                }
            }
        }
        "response" => {
            let Some(id) = value.get("id").and_then(Value::as_str) else {
                return;
            };
            let Some(sender) = process.pending.lock().remove(id) else {
                return;
            };
            let ok = value.get("ok").and_then(Value::as_bool).unwrap_or(false);
            if ok {
                let _ = sender.send(Ok(value.get("result").cloned().unwrap_or(Value::Null)));
            } else {
                let error = value.get("error").cloned().unwrap_or_else(
                    || json!({"code": bridge::codes::COMMAND_FAILED, "message": "unknown error"}),
                );
                let code = error
                    .get("code")
                    .and_then(Value::as_str)
                    .unwrap_or(bridge::codes::COMMAND_FAILED);
                let message = error
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("plugin command failed");
                let data = error.get("data").cloned();
                let mut rpc_error = RpcError::new(code, message);
                rpc_error.data = data;
                let _ = sender.send(Err(rpc_error));
            }
        }
        "request" => {
            let (Some(id), Some(method)) = (
                value.get("id").and_then(Value::as_str).map(str::to_string),
                value
                    .get("method")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            ) else {
                return;
            };
            let params = value.get("params").cloned().unwrap_or(Value::Null);
            let app = app.clone();
            let plugin_id = process.plugin_id.clone();
            let write_tx = process.write_tx.clone();
            tokio::spawn(async move {
                let ctx = ConnectionContext::runtime(plugin_id);
                let host_arc = app
                    .try_state::<Arc<PluginHost>>()
                    .map(|s| s.inner().clone());
                let result = if let Some(host_arc) = host_arc {
                    bridge::dispatch(&app, &host_arc, &ctx, &method, params).await
                } else {
                    Err(RpcError::internal(
                        "plugin bridge",
                        "host state unavailable",
                    ))
                };
                let frame = match result {
                    Ok(result) => {
                        json!({"type": "response", "id": id, "ok": true, "result": result})
                    }
                    Err(error) => {
                        json!({"type": "response", "id": id, "ok": false, "error": error})
                    }
                };
                let _ = encode_and_send(&write_tx, &frame);
            });
        }
        "event" => {
            let event = value.get("event").and_then(Value::as_str).unwrap_or("");
            let payload = value.get("payload").cloned().unwrap_or(Value::Null);
            let _ = app.emit(
                "plugin-runtime-event",
                json!({
                    "pluginId": process.plugin_id,
                    "source": "runtime",
                    "event": event,
                    "payload": payload,
                }),
            );
        }
        "log" => {
            // Intentionally ignored: third-party plugin logs stay private to the plugin process.
        }
        _ => {}
    }
}

async fn read_frame<R: tokio::io::AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<Option<Value>, String> {
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(format!("read frame length: {error}")),
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > bridge::MAX_MESSAGE_BYTES {
        return Err("frame exceeds max message size".into());
    }
    let mut body = vec![0u8; len];
    reader
        .read_exact(&mut body)
        .await
        .map_err(|e| format!("read frame body: {e}"))?;
    let value = serde_json::from_slice(&body).map_err(|e| format!("parse frame json: {e}"))?;
    Ok(Some(value))
}

fn prune_crash_history(history: &mut VecDeque<Instant>) {
    let cutoff = Instant::now() - RESTART_WINDOW;
    while history.front().is_some_and(|t| *t < cutoff) {
        history.pop_front();
    }
}

fn write_bootstrap_script(app: &AppHandle) -> Result<PathBuf, String> {
    // The embedded loader uses Node's pathToFileURL so canonical Windows paths remain importable.
    const BOOTSTRAP_SOURCE: &str = include_str!("../../../plugin-runtime/bootstrap.mjs");
    const SCA_SOURCE: &str = include_str!("../../../plugin-runtime/structured-clone.mjs");
    let root = super::paths::plugin_runtime_root(app)?;
    super::paths::ensure_dir(&root)?;
    let path = root.join("bootstrap.mjs");
    std::fs::write(&path, BOOTSTRAP_SOURCE).map_err(|e| format!("write bootstrap.mjs: {e}"))?;
    let sca_path = root.join("structured-clone.mjs");
    std::fs::write(&sca_path, SCA_SOURCE)
        .map_err(|e| format!("write structured-clone.mjs: {e}"))?;
    Ok(path)
}

fn spawn_development_log_reader<R>(
    app: AppHandle,
    plugin_id: String,
    stream: &'static str,
    reader: R,
) where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(mut line)) = lines.next_line().await {
            if line.len() > 16 * 1024 {
                line.truncate(16 * 1024);
                line.push_str(" ...[truncated]");
            }
            let _ = app.emit(
                "plugin-dev://log",
                json!({
                    "pluginId": plugin_id,
                    "source": stream,
                    "message": line,
                    "at": chrono::Local::now().to_rfc3339(),
                }),
            );
        }
    });
}

fn random_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// -- Transport (Unix domain socket / Windows named pipe) --------------------------------

#[cfg(unix)]
pub type IpcStream = tokio::net::UnixStream;
#[cfg(windows)]
pub type IpcStream = tokio::net::windows::named_pipe::NamedPipeServer;

#[cfg(unix)]
fn create_ipc_endpoint() -> Result<(String, String), String> {
    let dir = plugin_ipc_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("create ipc dir: {e}"))?;
    let name = format!("{}.sock", generate_id());
    let path = dir.join(name);
    let _ = std::fs::remove_file(&path);
    // The socket is bound lazily by `accept_ipc_connection` (tokio::net::UnixListener must be
    // created on the async runtime); here we only reserve the path + handshake token.
    Ok((path.display().to_string(), random_token()))
}

#[cfg(unix)]
async fn accept_ipc_connection(path: &str, timeout: Duration) -> Result<IpcStream, String> {
    use std::os::unix::fs::PermissionsExt;
    let listener =
        tokio::net::UnixListener::bind(path).map_err(|e| format!("bind unix socket: {e}"))?;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    let (stream, _addr) = tokio::time::timeout(timeout, listener.accept())
        .await
        .map_err(|_| "timed out waiting for plugin runtime to connect".to_string())?
        .map_err(|e| format!("accept unix socket: {e}"))?;
    let _ = std::fs::remove_file(path);
    Ok(stream)
}

#[cfg(windows)]
fn create_ipc_endpoint() -> Result<(String, String), String> {
    let path = format!(r"\\.\pipe\tempo-plugin-{}", generate_id());
    Ok((path, random_token()))
}

#[cfg(windows)]
async fn accept_ipc_connection(path: &str, timeout: Duration) -> Result<IpcStream, String> {
    use tokio::net::windows::named_pipe::ServerOptions;
    let server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(path)
        .map_err(|e| format!("create named pipe: {e}"))?;
    tokio::time::timeout(timeout, server.connect())
        .await
        .map_err(|_| "timed out waiting for plugin runtime to connect".to_string())?
        .map_err(|e| format!("connect named pipe: {e}"))?;
    Ok(server)
}

async fn verify_handshake_frame(
    stream: &mut IpcStream,
    expected_token: &str,
    timeout: Duration,
) -> Result<(), String> {
    let frame = tokio::time::timeout(timeout, read_frame(stream))
        .await
        .map_err(|_| "timed out waiting for plugin handshake".to_string())?
        .map_err(|e| format!("read handshake: {e}"))?
        .ok_or_else(|| "plugin closed connection before handshake".to_string())?;

    let kind = frame
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let token = frame
        .get("token")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if kind != "handshake" || token != expected_token {
        return Err("plugin handshake token mismatch".into());
    }

    let ack = json!({"type": "response", "id": "handshake", "ok": true, "result": {}});
    let framed = encode_frame(&ack)?;
    stream
        .write_all(&framed)
        .await
        .map_err(|e| format!("write handshake ack: {e}"))?;
    Ok(())
}

#[cfg(unix)]
fn kill_process_tree(pid: u32) {
    // Best-effort: the child was spawned as its own process-group leader (`process_group(0)`),
    // so signalling `-pid` reaches every descendant it spawned. This is a lifecycle/cleanup
    // measure, not a security boundary (design §3.2).
    let _ = std::process::Command::new("kill")
        .args(["-TERM", &format!("-{pid}")])
        .status();
    std::thread::sleep(Duration::from_millis(200));
    let _ = std::process::Command::new("kill")
        .args(["-KILL", &format!("-{pid}")])
        .status();
}

#[cfg(windows)]
fn kill_process_tree(pid: u32) {
    // `taskkill.exe` is itself a console-subsystem application. Reconnect always passes
    // through this cleanup path, so spawning it without CREATE_NO_WINDOW causes the short
    // console flash even though the Node Runtime process is already hidden.
    let mut command = std::process::Command::new("taskkill");
    command
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    prevent_windows_console_window(&mut command);
    let _ = command.status();
}
