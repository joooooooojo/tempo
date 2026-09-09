//! Deno Runtime permissions declared by a plugin manifest.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginPermission {
    Read,
    Write,
    Net,
    Env,
    Sys,
    Run,
    Ffi,
    Import,
}

impl PluginPermission {
    const ALL: [Self; 8] = [
        Self::Read,
        Self::Write,
        Self::Net,
        Self::Env,
        Self::Sys,
        Self::Run,
        Self::Ffi,
        Self::Import,
    ];

    fn runtime_flag(self) -> &'static str {
        match self {
            Self::Read => "--allow-read",
            Self::Write => "--allow-write",
            Self::Net => "--allow-net",
            Self::Env => "--allow-env",
            Self::Sys => "--allow-sys",
            Self::Run => "--allow-run",
            Self::Ffi => "--allow-ffi",
            Self::Import => "--allow-import",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyPluginPermissions {
    #[serde(default)]
    all: bool,
    #[serde(default)]
    read: Vec<String>,
    #[serde(default)]
    write: Vec<String>,
    #[serde(default)]
    net: Vec<String>,
    #[serde(default)]
    env: Vec<String>,
    /// Accepted so packages created before Host API grants were removed keep loading.
    #[serde(default, rename = "host", skip_serializing)]
    legacy_host: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PermissionFormat {
    Current(Vec<PluginPermission>),
    Legacy(LegacyPluginPermissions),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginPermissions(PermissionFormat);

impl Default for PluginPermissions {
    fn default() -> Self {
        Self(PermissionFormat::Current(Vec::new()))
    }
}

impl Serialize for PluginPermissions {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self.0 {
            PermissionFormat::Current(permissions) => permissions.serialize(serializer),
            PermissionFormat::Legacy(permissions) => permissions.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for PluginPermissions {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum ManifestPermissions {
            Current(Vec<PluginPermission>),
            Legacy(LegacyPluginPermissions),
        }

        Ok(match ManifestPermissions::deserialize(deserializer)? {
            ManifestPermissions::Current(permissions) => {
                Self(PermissionFormat::Current(permissions))
            }
            ManifestPermissions::Legacy(permissions) => Self(PermissionFormat::Legacy(permissions)),
        })
    }
}

impl PluginPermissions {
    pub fn contains(&self, permission: PluginPermission) -> bool {
        match &self.0 {
            PermissionFormat::Current(permissions) => permissions.contains(&permission),
            PermissionFormat::Legacy(permissions) => {
                permissions.all
                    || match permission {
                        PluginPermission::Read => !permissions.read.is_empty(),
                        PluginPermission::Write => !permissions.write.is_empty(),
                        PluginPermission::Net => !permissions.net.is_empty(),
                        PluginPermission::Env => !permissions.env.is_empty(),
                        PluginPermission::Sys
                        | PluginPermission::Run
                        | PluginPermission::Ffi
                        | PluginPermission::Import => false,
                    }
            }
        }
    }

    /// Whether the permission is unrestricted rather than an old scoped grant.
    pub fn allows_global(&self, permission: PluginPermission) -> bool {
        match &self.0 {
            PermissionFormat::Current(permissions) => permissions.contains(&permission),
            PermissionFormat::Legacy(permissions) => permissions.all,
        }
    }

    pub fn legacy_net_endpoints(&self) -> Option<&[String]> {
        match &self.0 {
            PermissionFormat::Legacy(permissions) if !permissions.all => Some(&permissions.net),
            _ => None,
        }
    }

    pub fn legacy_env_keys(&self) -> Option<&[String]> {
        match &self.0 {
            PermissionFormat::Legacy(permissions) if !permissions.all => Some(&permissions.env),
            _ => None,
        }
    }

    pub fn is_empty(&self) -> bool {
        match &self.0 {
            PermissionFormat::Current(permissions) => permissions.is_empty(),
            PermissionFormat::Legacy(permissions) => {
                !permissions.all
                    && permissions.read.is_empty()
                    && permissions.write.is_empty()
                    && permissions.net.is_empty()
                    && permissions.env.is_empty()
            }
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        match &self.0 {
            PermissionFormat::Current(permissions) => {
                let unique = permissions.iter().copied().collect::<HashSet<_>>();
                if unique.len() != permissions.len() {
                    return Err("permissions cannot contain duplicate values".into());
                }
                Ok(())
            }
            PermissionFormat::Legacy(permissions) => validate_legacy_permissions(permissions),
        }
    }

    pub fn runtime_args(
        &self,
        package: &Path,
        data: &Path,
        port: u16,
    ) -> Result<Vec<String>, String> {
        self.validate()?;

        if PluginPermission::ALL
            .iter()
            .all(|permission| self.allows_global(*permission))
        {
            return Ok(["run", "--no-config", "--no-lock", "--no-prompt", "-A"]
                .into_iter()
                .map(str::to_string)
                .collect());
        }

        let scope = |path: &Path| -> Result<String, String> {
            let value = path
                .canonicalize()
                .map_err(|error| format!("canonicalize runtime scope: {error}"))?
                .to_string_lossy()
                .into_owned();
            if value.contains([',', '\n', '\r']) {
                return Err("runtime path contains a permission delimiter".into());
            }
            Ok(value)
        };

        let mut args: Vec<String> = ["run", "--no-config", "--no-lock", "--no-prompt"]
            .into_iter()
            .map(str::to_string)
            .collect();
        if !self.allows_global(PluginPermission::Import) {
            args.extend(
                ["--cached-only", "--no-remote", "--no-npm"]
                    .into_iter()
                    .map(str::to_string),
            );
        }
        args.push("--node-modules-dir=none".into());

        if self.allows_global(PluginPermission::Read) {
            args.push(PluginPermission::Read.runtime_flag().into());
        } else {
            let mut reads = vec![scope(package)?];
            if matches!(
                &self.0,
                PermissionFormat::Legacy(permissions) if !permissions.read.is_empty()
            ) {
                reads.push(scope(data)?);
            }
            args.push(format!("--allow-read={}", reads.join(",")));
        }

        if self.allows_global(PluginPermission::Net) {
            args.push(PluginPermission::Net.runtime_flag().into());
        } else {
            let mut endpoints = vec![format!("127.0.0.1:{port}")];
            if let Some(legacy) = self.legacy_net_endpoints() {
                endpoints.extend_from_slice(legacy);
            }
            args.push(format!("--allow-net={}", endpoints.join(",")));
        }

        if self.allows_global(PluginPermission::Write) {
            args.push(PluginPermission::Write.runtime_flag().into());
        } else if matches!(
            &self.0,
            PermissionFormat::Legacy(permissions) if !permissions.write.is_empty()
        ) {
            args.push(format!("--allow-write={}", scope(data)?));
        }

        if self.allows_global(PluginPermission::Env) {
            args.push(PluginPermission::Env.runtime_flag().into());
        } else if let Some(keys) = self.legacy_env_keys().filter(|keys| !keys.is_empty()) {
            args.push(format!("--allow-env={}", keys.join(",")));
        }

        for permission in [
            PluginPermission::Sys,
            PluginPermission::Run,
            PluginPermission::Ffi,
            PluginPermission::Import,
        ] {
            if self.allows_global(permission) {
                args.push(permission.runtime_flag().into());
            }
        }
        Ok(args)
    }
}

fn validate_legacy_permissions(permissions: &LegacyPluginPermissions) -> Result<(), String> {
    if permissions.all
        && (!permissions.read.is_empty()
            || !permissions.write.is_empty()
            || !permissions.net.is_empty()
            || !permissions.env.is_empty())
    {
        return Err("permissions.all cannot be combined with read, write, net, or env".into());
    }
    for scope in permissions.read.iter().chain(&permissions.write) {
        if scope != "$DATA" {
            return Err("legacy permissions.read/write only support $DATA".into());
        }
    }
    for endpoint in &permissions.net {
        let url = url::Url::parse(&format!("http://{endpoint}"))
            .map_err(|_| "legacy permissions.net requires host:port")?;
        let (_, port) = endpoint
            .rsplit_once(':')
            .ok_or("legacy permissions.net requires host:port")?;
        if port.parse::<u16>().ok().filter(|port| *port > 0).is_none()
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
            || endpoint.chars().any(|character| {
                character.is_whitespace()
                    || matches!(
                        character,
                        ',' | '*' | '/' | '\\' | '@' | '%' | '\'' | '"' | ';' | '`' | '<' | '>'
                    )
            })
        {
            return Err("legacy permissions.net requires an exact host:port".into());
        }
    }
    for key in &permissions.env {
        if key.is_empty()
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
            || key.starts_with("DENO_")
            || key.starts_with("NODE_")
            || key.starts_with("TEMPO_")
            || matches!(key.as_str(), "PATH" | "LD_PRELOAD" | "LD_LIBRARY_PATH")
            || key.starts_with("DYLD_")
            || key.starts_with("LD_")
        {
            return Err(format!(
                "legacy permissions.env contains a reserved or invalid key: {key}"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn permissions(raw: &str) -> PluginPermissions {
        serde_json::from_str(raw).unwrap()
    }

    #[test]
    fn current_format_is_an_array_and_rejects_unknown_or_duplicate_values() {
        let policy = permissions(r#"["read","net"]"#);
        assert!(policy.contains(PluginPermission::Read));
        assert!(policy.allows_global(PluginPermission::Net));
        assert_eq!(
            serde_json::to_value(policy).unwrap(),
            serde_json::json!(["read", "net"])
        );
        assert!(serde_json::from_str::<PluginPermissions>(r#"["unknown"]"#).is_err());
        assert!(permissions(r#"["read","read"]"#).validate().is_err());
    }

    #[test]
    fn manifest_schema_accepts_only_unique_permission_arrays() {
        let schema: Value = serde_json::from_str(include_str!(
            "../../../../docs/schemas/plugin-manifest.schema.json"
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
            "permissions": ["read", "net", "import"]
        });
        assert!(validator.is_valid(&manifest));

        let mut duplicate = manifest.clone();
        duplicate["permissions"] = serde_json::json!(["read", "read"]);
        assert!(!validator.is_valid(&duplicate));

        let mut legacy = manifest;
        legacy["permissions"] = serde_json::json!({ "all": true });
        assert!(!validator.is_valid(&legacy));
    }

    #[test]
    fn each_current_permission_maps_to_an_independent_global_flag() {
        let root = std::env::temp_dir();
        for selected in PluginPermission::ALL {
            let policy = PluginPermissions(PermissionFormat::Current(vec![selected]));
            let args = policy.runtime_args(&root, &root, 12345).unwrap();
            assert!(args
                .iter()
                .any(|argument| argument == selected.runtime_flag()));
            assert!(!args.iter().any(|argument| argument == "-A"));
            assert_eq!(
                args.iter().any(|argument| argument == "--no-remote"),
                selected != PluginPermission::Import
            );
            for other in PluginPermission::ALL {
                if other != selected {
                    assert!(!args.iter().any(|argument| argument == other.runtime_flag()));
                }
            }
        }

        let policy = permissions(r#"["read","write","net","env","sys","run","ffi","import"]"#);
        assert_eq!(
            policy.runtime_args(&root, &root, 12345).unwrap(),
            ["run", "--no-config", "--no-lock", "--no-prompt", "-A"]
        );
    }

    #[test]
    fn empty_permissions_keep_only_host_bootstrap_grants() {
        let root = std::env::temp_dir();
        let args = PluginPermissions::default()
            .runtime_args(&root, &root, 12345)
            .unwrap();
        assert!(args
            .iter()
            .any(|argument| argument.starts_with("--allow-read=")));
        assert!(args.contains(&"--allow-net=127.0.0.1:12345".into()));
        for forbidden in [
            "--allow-read",
            "--allow-write",
            "--allow-net",
            "--allow-env",
            "--allow-sys",
            "--allow-run",
            "--allow-ffi",
            "--allow-import",
            "-A",
        ] {
            assert!(!args.iter().any(|argument| argument == forbidden));
        }
        assert!(args.contains(&"--cached-only".into()));
    }

    #[test]
    fn legacy_scopes_load_without_becoming_global() {
        let root = std::env::temp_dir();
        let policy = permissions(
            r#"{"read":["$DATA"],"write":["$DATA"],"net":["api.example.com:443"],"env":["API_KEY"]}"#,
        );
        for permission in [
            PluginPermission::Read,
            PluginPermission::Write,
            PluginPermission::Net,
            PluginPermission::Env,
        ] {
            assert!(policy.contains(permission));
            assert!(!policy.allows_global(permission));
        }
        let args = policy.runtime_args(&root, &root, 12345).unwrap();
        assert!(args
            .iter()
            .any(|argument| argument.starts_with("--allow-read=")));
        assert!(args
            .iter()
            .any(|argument| argument.starts_with("--allow-write=")));
        assert!(args
            .iter()
            .any(|argument| argument == "--allow-net=127.0.0.1:12345,api.example.com:443"));
        assert!(args
            .iter()
            .any(|argument| argument == "--allow-env=API_KEY"));
        for global in [
            "--allow-read",
            "--allow-write",
            "--allow-net",
            "--allow-env",
        ] {
            assert!(!args.iter().any(|argument| argument == global));
        }
    }

    #[test]
    fn legacy_allow_all_still_uses_deno_allow_all() {
        let root = std::env::temp_dir();
        let policy = permissions(r#"{"all":true}"#);
        assert_eq!(
            policy.runtime_args(&root, &root, 12345).unwrap(),
            ["run", "--no-config", "--no-lock", "--no-prompt", "-A"]
        );
    }

    #[test]
    fn legacy_host_permissions_are_accepted_but_not_serialized() {
        let policy =
            permissions(r#"{"host":{"notify":true,"externalOpen":true,"openApps":["settings"]}}"#);
        let serialized = serde_json::to_value(policy).unwrap();
        assert!(serialized.get("host").is_none());
    }
}
