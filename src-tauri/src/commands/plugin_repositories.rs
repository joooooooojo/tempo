use tauri::{AppHandle, State};

use crate::db::AppState;
use crate::plugins::repository::{
    self, AddRepositoryInput, OperationStarted, PluginRepository, RepositoryCatalogPlugin,
    RepositoryConnectionTest, RepositoryCredentialProfile, RepositoryIssue, RepositoryOperation,
    SaveCredentialInput, TrustRepositoryConnectionInput, UpdateRepositoryInput,
};

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryIdArgs {
    pub repository_id: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetRepositoryEnabledArgs {
    pub repository_id: String,
    pub enabled: bool,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteCredentialArgs {
    pub credential_id: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRepositoryPluginArgs {
    pub repository_id: String,
    pub plugin_id: String,
    #[serde(default)]
    pub expected_commit: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderRepositoriesArgs {
    pub repository_ids: Vec<String>,
}

#[tauri::command]
pub fn list_plugin_repositories(
    state: State<'_, AppState>,
) -> Result<Vec<PluginRepository>, String> {
    repository::list_repositories(&state.db.lock())
}

#[tauri::command]
pub fn add_plugin_repository(
    state: State<'_, AppState>,
    args: AddRepositoryInput,
) -> Result<PluginRepository, String> {
    repository::add_repository(&state.db.lock(), args)
}

#[tauri::command]
pub fn update_plugin_repository(
    app: AppHandle,
    state: State<'_, AppState>,
    args: UpdateRepositoryInput,
) -> Result<PluginRepository, String> {
    repository::update_repository(&app, &state.db.lock(), args)
}

#[tauri::command]
pub fn reorder_plugin_repositories(
    state: State<'_, AppState>,
    args: ReorderRepositoriesArgs,
) -> Result<(), String> {
    repository::reorder_repositories(&state.db.lock(), &args.repository_ids)
}

#[tauri::command]
pub fn set_plugin_repository_enabled(
    state: State<'_, AppState>,
    args: SetRepositoryEnabledArgs,
) -> Result<(), String> {
    repository::set_repository_enabled(&state.db.lock(), &args.repository_id, args.enabled)
}

#[tauri::command]
pub fn remove_plugin_repository(
    app: AppHandle,
    state: State<'_, AppState>,
    args: RepositoryIdArgs,
) -> Result<(), String> {
    repository::remove_repository(&app, &state.db.lock(), &args.repository_id)
}

#[tauri::command]
pub fn sync_plugin_repository(app: AppHandle, args: RepositoryIdArgs) -> OperationStarted {
    repository::start_sync(app, args.repository_id)
}

#[tauri::command]
pub fn sync_all_plugin_repositories(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<OperationStarted>, String> {
    let repositories = repository::list_repositories(&state.db.lock())?;
    Ok(repositories
        .into_iter()
        .filter(|repository| repository.enabled)
        .map(|repository| repository::start_sync(app.clone(), repository.id))
        .collect())
}

#[tauri::command]
pub async fn test_plugin_repository_connection(
    app: AppHandle,
    args: RepositoryIdArgs,
) -> Result<RepositoryConnectionTest, String> {
    tauri::async_runtime::spawn_blocking(move || {
        repository::test_repository_connection(&app, &args.repository_id)
    })
    .await
    .map_err(|error| format!("测试仓库连接失败: {error}"))?
}

#[tauri::command]
pub fn trust_plugin_repository_tls_certificate(
    state: State<'_, AppState>,
    args: TrustRepositoryConnectionInput,
) -> Result<(), String> {
    repository::trust_tls_certificate(&state.db.lock(), args)
}

#[tauri::command]
pub fn trust_plugin_repository_ssh_host_key(
    state: State<'_, AppState>,
    args: TrustRepositoryConnectionInput,
) -> Result<(), String> {
    repository::trust_ssh_host_key(&state.db.lock(), args)
}

#[tauri::command]
pub fn list_plugin_repository_operations() -> Vec<RepositoryOperation> {
    repository::list_operations()
}

#[tauri::command]
pub fn search_repository_plugins(
    state: State<'_, AppState>,
    query: Option<String>,
) -> Result<Vec<RepositoryCatalogPlugin>, String> {
    repository::list_catalog_plugins(&state.db.lock(), query.as_deref())
}

#[tauri::command]
pub fn list_plugin_repository_issues(
    state: State<'_, AppState>,
    args: RepositoryIdArgs,
) -> Result<Vec<RepositoryIssue>, String> {
    repository::list_repository_issues(&state.db.lock(), &args.repository_id)
}

#[tauri::command]
pub fn install_repository_plugin(
    app: AppHandle,
    args: InstallRepositoryPluginArgs,
) -> OperationStarted {
    repository::start_install(
        app,
        args.repository_id,
        args.plugin_id,
        args.expected_commit,
    )
}

#[tauri::command]
pub fn list_plugin_repository_credentials(
    state: State<'_, AppState>,
) -> Result<Vec<RepositoryCredentialProfile>, String> {
    repository::list_credentials(&state.db.lock())
}

#[tauri::command]
pub fn save_plugin_repository_credential(
    state: State<'_, AppState>,
    args: SaveCredentialInput,
) -> Result<RepositoryCredentialProfile, String> {
    repository::save_credential(&state.db.lock(), args)
}

#[tauri::command]
pub fn delete_plugin_repository_credential(
    state: State<'_, AppState>,
    args: DeleteCredentialArgs,
) -> Result<(), String> {
    repository::delete_credential(&state.db.lock(), &args.credential_id)
}

#[tauri::command]
pub fn create_plugin_repository_from_template(
    app: AppHandle,
    args: crate::plugins::repository_template::CreatePluginRepositoryTemplateInput,
) -> Result<crate::plugins::repository_template::CreatedPluginRepository, String> {
    crate::plugins::repository_template::create_from_template(&app, args)
}
