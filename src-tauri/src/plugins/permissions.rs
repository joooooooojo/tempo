//! Manifest v2 policy. Package reads and the authenticated IPC endpoint are host grants.
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginPermissions {
    #[serde(default)]
    pub read: Vec<String>,
    #[serde(default)]
    pub write: Vec<String>,
    #[serde(default)]
    pub net: Vec<String>,
    #[serde(default)]
    pub env: Vec<String>,
    #[serde(default)]
    pub host: HostPermissions,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HostPermissions {
    #[serde(default)]
    pub notify: bool,
    #[serde(default)]
    pub external_open: bool,
    #[serde(default)]
    pub open_apps: Vec<String>,
}

impl PluginPermissions {
    pub fn allows_host(&self, method: &str, app_id: Option<&str>) -> bool {
        match method {
            "notify.show" => self.host.notify,
            "external.open" => self.host.external_open,
            "app.open" => {
                app_id.is_some_and(|id| self.host.open_apps.iter().any(|allowed| allowed == id))
            }
            _ => false,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        for scope in self.read.iter().chain(&self.write) {
            if scope != "$DATA" {
                return Err("permissions.read/write only support $DATA".into());
            }
        }
        for endpoint in &self.net {
            let url = url::Url::parse(&format!("http://{endpoint}"))
                .map_err(|_| "permissions.net requires host:port")?;
            let (_, port) = endpoint
                .rsplit_once(':')
                .ok_or("permissions.net requires host:port")?;
            if port.parse::<u16>().ok().filter(|p| *p > 0).is_none()
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.path() != "/"
                || url.query().is_some()
                || url.fragment().is_some()
                || endpoint
                    .chars()
                    .any(|c| c.is_whitespace() || matches!(c, ',' | '*' | '/' | '\\' | '@' | '%'))
            {
                return Err(
                    "permissions.net requires an exact host:port (no wildcards or URLs)".into(),
                );
            }
        }
        for key in &self.env {
            if key.is_empty()
                || !key
                    .bytes()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_')
                || key.starts_with("DENO_")
                || key.starts_with("NODE_")
                || key.starts_with("TEMPO_")
                || matches!(key.as_str(), "PATH" | "LD_PRELOAD" | "LD_LIBRARY_PATH")
                || key.starts_with("DYLD_")
                || key.starts_with("LD_")
            {
                return Err(format!(
                    "permissions.env contains a reserved or invalid key: {key}"
                ));
            }
        }
        if self
            .host
            .open_apps
            .iter()
            .any(|id| id.is_empty() || id.contains('*'))
        {
            return Err("permissions.host.openApps requires exact app IDs".into());
        }
        Ok(())
    }

    pub fn runtime_args(
        &self,
        package: &Path,
        data: &Path,
        port: u16,
    ) -> Result<Vec<String>, String> {
        self.validate()?;
        let scope = |p: &Path| -> Result<String, String> {
            let value = p
                .canonicalize()
                .map_err(|e| format!("canonicalize runtime scope: {e}"))?
                .to_string_lossy()
                .into_owned();
            if value.contains([',', '\n', '\r']) {
                return Err("runtime path contains a permission delimiter".into());
            }
            Ok(value)
        };
        let mut reads = vec![scope(package)?];
        if !self.read.is_empty() {
            reads.push(scope(data)?);
        }
        let mut net = vec![format!("127.0.0.1:{port}")];
        net.extend(self.net.clone());
        let mut args: Vec<String> = [
            "run",
            "--no-config",
            "--no-lock",
            "--no-prompt",
            "--cached-only",
            "--no-remote",
            "--no-npm",
            "--node-modules-dir=none",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        args.push(format!("--allow-read={}", reads.join(",")));
        args.push(format!("--allow-net={}", net.join(",")));
        if !self.write.is_empty() {
            args.push(format!("--allow-write={}", scope(data)?));
        }
        if !self.env.is_empty() {
            args.push(format!("--allow-env={}", self.env.join(",")));
        }
        Ok(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_is_denied_and_unknown_permissions_fail() {
        assert!(PluginPermissions::default().validate().is_ok());
        assert!(serde_json::from_str::<PluginPermissions>(r#"{"run":true}"#).is_err());
    }
    #[test]
    fn rejects_broad_or_injected_scopes() {
        for net in [
            "*",
            "example.com",
            "example.com:443,evil.com:80",
            "http://example.com:443",
            "example.com:0",
        ] {
            let p = PluginPermissions {
                net: vec![net.into()],
                ..Default::default()
            };
            assert!(p.validate().is_err(), "{net}");
        }
        let p = PluginPermissions {
            net: vec!["example.com:443".into()],
            read: vec!["$DATA".into()],
            ..Default::default()
        };
        assert!(p.validate().is_ok());
        for key in ["DENO_PERMISSION_BROKER_PATH", "NODE_OPTIONS", "PATH", "*"] {
            assert!(PluginPermissions {
                env: vec![key.into()],
                ..Default::default()
            }
            .validate()
            .is_err());
        }
    }
    #[test]
    fn args_never_broaden_empty_grants() {
        let root = std::env::temp_dir();
        let args = PluginPermissions::default()
            .runtime_args(&root, &root, 12345)
            .unwrap();
        assert_eq!(args[0], "run");
        assert!(args.contains(&"--allow-net=127.0.0.1:12345".into()));
        for forbidden in [
            "--allow-read",
            "--allow-net",
            "--allow-env",
            "--allow-write",
            "--allow-run",
            "--allow-ffi",
            "-A",
        ] {
            assert!(!args.iter().any(|arg| arg == forbidden));
        }
        assert!(!args.iter().any(|arg| arg.starts_with("--allow-write=")));
    }
    #[test]
    fn host_permissions_default_deny_and_use_exact_app_ids() {
        let mut policy = PluginPermissions::default();
        for method in ["notify.show", "external.open", "app.open"] {
            assert!(!policy.allows_host(method, Some("settings")));
        }
        policy.host.notify = true;
        policy.host.open_apps.push("com.example.target/main".into());
        assert!(policy.allows_host("notify.show", None));
        assert!(policy.allows_host("app.open", Some("com.example.target/main")));
        assert!(!policy.allows_host("app.open", Some("com.example.target/other")));
        assert!(!policy.allows_host("external.open", None));
    }
}
