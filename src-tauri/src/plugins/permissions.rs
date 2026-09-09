//! Manifest v2 policy. Package reads and the authenticated IPC endpoint are host grants.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginPermissions {
    #[serde(default)]
    pub all: bool,
    #[serde(default)]
    pub read: Vec<String>,
    #[serde(default)]
    pub write: Vec<String>,
    #[serde(default)]
    pub net: Vec<String>,
    #[serde(default)]
    pub env: Vec<String>,
    /// Accepted only so packages created before host permissions were removed keep loading.
    #[serde(default, rename = "host", skip_serializing)]
    pub(crate) legacy_host: Option<Value>,
}

impl PluginPermissions {
    pub fn validate(&self) -> Result<(), String> {
        if self.all
            && (!self.read.is_empty()
                || !self.write.is_empty()
                || !self.net.is_empty()
                || !self.env.is_empty())
        {
            return Err("permissions.all cannot be combined with read, write, net, or env".into());
        }
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
                || endpoint.chars().any(|c| {
                    c.is_whitespace()
                        || matches!(
                            c,
                            ',' | '*' | '/' | '\\' | '@' | '%' | '\'' | '"' | ';' | '`' | '<' | '>'
                        )
                })
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
        Ok(())
    }

    pub fn runtime_args(
        &self,
        package: &Path,
        data: &Path,
        port: u16,
    ) -> Result<Vec<String>, String> {
        self.validate()?;
        if self.all {
            return Ok(["run", "--no-config", "--no-lock", "--no-prompt", "-A"]
                .into_iter()
                .map(str::to_string)
                .collect());
        }
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
    fn all_uses_deno_allow_all_and_rejects_mixed_scopes() {
        let root = std::env::temp_dir();
        let policy = PluginPermissions {
            all: true,
            ..Default::default()
        };
        let args = policy.runtime_args(&root, &root, 12345).unwrap();
        assert_eq!(
            args,
            ["run", "--no-config", "--no-lock", "--no-prompt", "-A"]
        );

        assert!(PluginPermissions {
            all: true,
            net: vec!["api.example.com:443".into()],
            ..Default::default()
        }
        .validate()
        .is_err());
    }
    #[test]
    fn manifest_schema_enforces_allow_all_exclusivity() {
        let schema: Value = serde_json::from_str(include_str!(
            "../../../docs/schemas/plugin-manifest.schema.json"
        ))
        .unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        let manifest = serde_json::json!({
            "manifestVersion": 2,
            "id": "com.example.permissions",
            "name": "Permissions",
            "version": "1.0.0",
            "engines": {
                "tempo": ">=2.2.6",
                "pluginApi": "^2.1.0"
            },
            "main": "main.mjs",
            "permissions": {
                "all": true
            }
        });
        assert!(validator.is_valid(&manifest));

        let mut mixed = manifest;
        mixed["permissions"]["net"] = serde_json::json!(["api.example.com:443"]);
        assert!(!validator.is_valid(&mixed));
    }
    #[test]
    fn rejects_broad_or_injected_scopes() {
        for net in [
            "*",
            "example.com",
            "example.com:443,evil.com:80",
            "http://example.com:443",
            "example.com:0",
            "example.com;script-src:443",
            "example.com'connect-src:443",
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
    fn legacy_host_permissions_are_accepted_but_not_serialized() {
        let policy: PluginPermissions = serde_json::from_str(
            r#"{"host":{"notify":true,"externalOpen":true,"openApps":["settings"]}}"#,
        )
        .unwrap();
        assert!(policy.legacy_host.is_some());
        let serialized = serde_json::to_value(policy).unwrap();
        assert!(serialized.get("host").is_none());
    }
}
