use serde::{Deserialize, Serialize};

pub(super) const MAX_BACKUPS: usize = 20;
pub(super) const PROFILES_META: &str = "profiles.json";
pub(super) const STATE_FILE: &str = "state.json";
pub(super) const PUBLIC_FILE: &str = "public.hosts";

pub(super) const MARK_PUBLIC_BEGIN: &str = "# >>> TEMPO:PUBLIC:BEGIN";
pub(super) const MARK_PUBLIC_END: &str = "# <<< TEMPO:PUBLIC:END";
pub(super) const MARK_PROFILE_BEGIN_PREFIX: &str = "# >>> TEMPO:PROFILE:BEGIN";
pub(super) const MARK_PROFILE_END: &str = "# <<< TEMPO:PROFILE:END";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum HostsProfileKind {
    #[default]
    Local,
    Remote,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostsWorkspace {
    pub path: String,
    pub writable: bool,
    pub authorized: bool,
    /// Whether the on-disk system hosts contains Tempo profile markers.
    pub managed: bool,
    pub active_profile_ids: Vec<String>,
    pub profiles: Vec<HostsProfile>,
    /// Exact content currently on the system hosts file.
    pub system_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostsProfile {
    pub id: String,
    pub name: String,
    pub updated_at: String,
    pub active: bool,
    pub kind: HostsProfileKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    #[serde(default)]
    pub refresh_interval_secs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_fetched_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_fetch_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostsBackup {
    pub id: String,
    pub source: String,
    pub created_at: String,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(super) struct HostsState {
    #[serde(default)]
    pub(super) active_profile_ids: Vec<String>,
    /// Legacy single-active field retained only for migration from v1 state.json.
    #[serde(default, alias = "activeProfileId")]
    pub(super) active_profile_id: Option<String>,
    #[serde(default)]
    pub(super) initialized: bool,
    /// Legacy seed flag; no longer used after v2.
    #[serde(default)]
    pub(super) defaults_seeded: bool,
    /// True after public→profile migration and multi-active upgrade.
    #[serde(default)]
    pub(super) migrated_v2: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ProfilesFile {
    pub(super) profiles: Vec<ProfileMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ProfileMeta {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) updated_at: String,
    #[serde(default)]
    pub(super) kind: HostsProfileKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) remote_url: Option<String>,
    #[serde(default)]
    pub(super) refresh_interval_secs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) last_fetched_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) last_fetch_error: Option<String>,
}

impl ProfileMeta {
    pub(super) fn to_public(&self, active_ids: &[String]) -> HostsProfile {
        HostsProfile {
            id: self.id.clone(),
            name: self.name.clone(),
            updated_at: self.updated_at.clone(),
            active: active_ids.iter().any(|id| id == &self.id),
            kind: self.kind,
            remote_url: self.remote_url.clone(),
            refresh_interval_secs: self.refresh_interval_secs,
            last_fetched_at: self.last_fetched_at.clone(),
            last_fetch_error: self.last_fetch_error.clone(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct ParsedSystemHosts {
    pub(super) managed: bool,
    /// Non-Tempo content preserved across applies.
    pub(super) preamble: String,
    /// Profile sections found in the system file (id, body).
    pub(super) profiles: Vec<(String, String)>,
    /// Legacy PUBLIC section body (migration aid).
    pub(super) legacy_public: String,
}
