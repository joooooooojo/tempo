use netstat2::{
    get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, SocketInfo,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortRecord {
    pub protocol: &'static str,
    pub local_address: String,
    pub local_port: u16,
    pub remote_address: Option<String>,
    pub remote_port: Option<u16>,
    pub state: String,
    pub pid: Option<u32>,
    pub process_name: String,
    pub process_path: Option<String>,
    pub process_started_at: Option<u64>,
    pub can_terminate: bool,
    pub protected_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminatePortProcessRequest {
    pub protocol: String,
    pub local_address: String,
    pub local_port: u16,
    pub pid: u32,
    pub process_started_at: u64,
}

#[derive(Debug, Clone)]
struct ProcessDetails {
    name: String,
    path: Option<String>,
    started_at: u64,
    protected_reason: Option<String>,
}

#[tauri::command]
pub async fn get_port_records(
    include_active_connections: Option<bool>,
) -> Result<Vec<PortRecord>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        collect_port_records(include_active_connections.unwrap_or(false))
    })
    .await
    .map_err(|error| format!("读取端口信息失败: {error}"))?
}

#[tauri::command]
pub async fn terminate_port_process(request: TerminatePortProcessRequest) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || terminate_process_for_port(&request))
        .await
        .map_err(|error| format!("结束进程失败: {error}"))?
}

fn collect_port_records(include_active_connections: bool) -> Result<Vec<PortRecord>, String> {
    let mut sockets = all_sockets()?;
    if !include_active_connections {
        sockets.retain(|socket| {
            matches!(
                &socket.protocol_socket_info,
                ProtocolSocketInfo::Udp(_)
                    | ProtocolSocketInfo::Tcp(netstat2::TcpSocketInfo {
                        state: netstat2::TcpState::Listen,
                        ..
                    })
            )
        });
    }
    let process_details = load_process_details(&sockets);
    let mut records = Vec::new();

    for socket in sockets {
        let pids: Vec<Option<u32>> = if socket.associated_pids.is_empty() {
            vec![None]
        } else {
            socket.associated_pids.iter().copied().map(Some).collect()
        };

        for pid in pids {
            records.push(port_record(&socket, pid, &process_details));
        }
    }

    records.sort_by(|left, right| {
        left.local_port
            .cmp(&right.local_port)
            .then_with(|| left.protocol.cmp(right.protocol))
            .then_with(|| left.pid.cmp(&right.pid))
            .then_with(|| left.local_address.cmp(&right.local_address))
    });
    Ok(records)
}

fn all_sockets() -> Result<Vec<SocketInfo>, String> {
    get_sockets_info(
        AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6,
        ProtocolFlags::TCP | ProtocolFlags::UDP,
    )
    .map_err(|error| format!("无法读取系统 socket 表: {error}"))
}

fn load_process_details(sockets: &[SocketInfo]) -> HashMap<u32, ProcessDetails> {
    let pids: Vec<Pid> = sockets
        .iter()
        .flat_map(|socket| socket.associated_pids.iter().copied())
        .filter(|pid| *pid > 0)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(Pid::from_u32)
        .collect();
    if pids.is_empty() {
        return HashMap::new();
    }

    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&pids),
        true,
        ProcessRefreshKind::nothing()
            .with_exe(UpdateKind::OnlyIfNotSet)
            .without_tasks(),
    );

    system
        .processes()
        .iter()
        .map(|(pid, process)| {
            let pid = pid.as_u32();
            let name = process.name().to_string_lossy().into_owned();
            let path = process
                .exe()
                .filter(|path| !path.as_os_str().is_empty())
                .map(|path| path.to_string_lossy().into_owned());
            let protected_reason = protected_process_reason(pid, &name);
            (
                pid,
                ProcessDetails {
                    name,
                    path,
                    started_at: process.start_time(),
                    protected_reason,
                },
            )
        })
        .collect()
}

fn port_record(
    socket: &SocketInfo,
    pid: Option<u32>,
    process_details: &HashMap<u32, ProcessDetails>,
) -> PortRecord {
    let details = pid.and_then(|pid| process_details.get(&pid));
    let (protocol, local_address, local_port, remote_address, remote_port, state) =
        socket_fields(socket);
    let protected_reason = match (pid, details) {
        (None, _) => Some("系统未提供该端口的进程信息".to_string()),
        (Some(_), None) => Some("进程已退出或当前账户无权读取".to_string()),
        (_, Some(details)) => details.protected_reason.clone(),
    };

    PortRecord {
        protocol,
        local_address,
        local_port,
        remote_address,
        remote_port,
        state,
        pid,
        process_name: details
            .map(|details| details.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "未知进程".to_string()),
        process_path: details.and_then(|details| details.path.clone()),
        process_started_at: details.map(|details| details.started_at),
        can_terminate: details.is_some() && protected_reason.is_none(),
        protected_reason,
    }
}

fn socket_fields(
    socket: &SocketInfo,
) -> (
    &'static str,
    String,
    u16,
    Option<String>,
    Option<u16>,
    String,
) {
    match &socket.protocol_socket_info {
        ProtocolSocketInfo::Tcp(info) => (
            "TCP",
            info.local_addr.to_string(),
            info.local_port,
            Some(info.remote_addr.to_string()),
            Some(info.remote_port),
            info.state.to_string(),
        ),
        ProtocolSocketInfo::Udp(info) => (
            "UDP",
            info.local_addr.to_string(),
            info.local_port,
            None,
            None,
            "BOUND".to_string(),
        ),
    }
}

fn terminate_process_for_port(request: &TerminatePortProcessRequest) -> Result<(), String> {
    validate_termination_request(request)?;

    let pid = Pid::from_u32(request.pid);
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing()
            .with_exe(UpdateKind::OnlyIfNotSet)
            .without_tasks(),
    );
    let process = system
        .process(pid)
        .ok_or_else(|| "进程已经退出，请刷新列表".to_string())?;
    let process_name = process.name().to_string_lossy();

    if process.start_time() != request.process_started_at {
        return Err("进程已经变化，请刷新列表后重试".to_string());
    }
    if let Some(reason) = protected_process_reason(request.pid, &process_name) {
        return Err(reason);
    }
    if !process_still_owns_port(request)? {
        return Err("该进程已不再占用此端口，请刷新列表".to_string());
    }
    if !process.kill() {
        return Err("系统拒绝结束该进程，请检查当前账户权限".to_string());
    }

    tracing::info!(
        pid = request.pid,
        process = %crate::logging::sanitize_log_value(&process_name),
        protocol = %request.protocol,
        local_address = %request.local_address,
        local_port = request.local_port,
        "terminated process from port manager"
    );
    Ok(())
}

fn validate_termination_request(request: &TerminatePortProcessRequest) -> Result<(), String> {
    if request.pid == 0 || request.local_port == 0 {
        return Err("无效的端口或进程信息".to_string());
    }
    if request.protocol != "TCP" && request.protocol != "UDP" {
        return Err("不支持的网络协议".to_string());
    }
    if request.local_address.parse::<std::net::IpAddr>().is_err() {
        return Err("无效的本地地址".to_string());
    }
    Ok(())
}

fn process_still_owns_port(request: &TerminatePortProcessRequest) -> Result<bool, String> {
    Ok(all_sockets()?.iter().any(|socket| {
        let protocol_matches = matches!(
            (&socket.protocol_socket_info, request.protocol.as_str()),
            (ProtocolSocketInfo::Tcp(_), "TCP") | (ProtocolSocketInfo::Udp(_), "UDP")
        );
        protocol_matches
            && socket.local_port() == request.local_port
            && socket.local_addr().to_string() == request.local_address
            && socket.associated_pids.contains(&request.pid)
    }))
}

fn protected_process_reason(pid: u32, process_name: &str) -> Option<String> {
    if pid == std::process::id() {
        return Some("不能结束 Tempo 自身进程".to_string());
    }
    if pid <= 1 {
        return Some("系统核心进程受保护".to_string());
    }

    let normalized = process_name.trim().to_ascii_lowercase();
    let is_critical = if cfg!(windows) {
        matches!(
            normalized.as_str(),
            "system"
                | "registry"
                | "smss.exe"
                | "csrss.exe"
                | "wininit.exe"
                | "services.exe"
                | "lsass.exe"
                | "winlogon.exe"
        )
    } else if cfg!(target_os = "macos") {
        matches!(
            normalized.as_str(),
            "kernel_task" | "launchd" | "windowserver" | "loginwindow"
        )
    } else {
        false
    };

    is_critical.then(|| "系统关键进程受保护".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn rejects_invalid_termination_requests() {
        let request = TerminatePortProcessRequest {
            protocol: "HTTP".to_string(),
            local_address: "127.0.0.1".to_string(),
            local_port: 80,
            pid: 12,
            process_started_at: 1,
        };
        assert!(validate_termination_request(&request).is_err());
    }

    #[test]
    fn protects_the_current_process() {
        let reason = protected_process_reason(std::process::id(), "tempo");
        assert_eq!(reason.as_deref(), Some("不能结束 Tempo 自身进程"));
    }

    #[test]
    fn finds_a_listener_owned_by_the_current_process() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let address = listener.local_addr().expect("read listener address");
        let request = TerminatePortProcessRequest {
            protocol: "TCP".to_string(),
            local_address: address.ip().to_string(),
            local_port: address.port(),
            pid: std::process::id(),
            process_started_at: 0,
        };

        assert!(process_still_owns_port(&request).expect("read socket table"));
    }

    #[test]
    fn collect_records_includes_the_current_process_listener() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test listener");
        let port = listener.local_addr().expect("read listener address").port();
        let records = collect_port_records(false).expect("collect port records");

        assert!(records.iter().any(|record| {
            record.protocol == "TCP"
                && record.local_port == port
                && record.pid == Some(std::process::id())
                && record.state == netstat2::TcpState::Listen.to_string()
        }));
    }

    #[test]
    fn listener_view_excludes_active_tcp_connections() {
        let records = collect_port_records(false).expect("collect listening port records");

        assert!(records
            .iter()
            .all(|record| record.protocol == "UDP" || record.state == "LISTEN"));
    }
}
