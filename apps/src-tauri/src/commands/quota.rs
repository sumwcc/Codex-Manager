use crate::commands::shared::rpc_call_in_background;

#[tauri::command]
pub async fn service_quota_overview(addr: Option<String>) -> Result<serde_json::Value, String> {
    rpc_call_in_background("quota/overview", addr, None).await
}

#[tauri::command]
pub async fn service_quota_model_usage(
    addr: Option<String>,
    start_ts: Option<i64>,
    end_ts: Option<i64>,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background(
        "quota/modelUsage",
        addr,
        Some(serde_json::json!({
            "startTs": start_ts,
            "endTs": end_ts,
        })),
    )
    .await
}

#[tauri::command]
pub async fn service_quota_api_key_usage(
    addr: Option<String>,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background("quota/apiKeyUsage", addr, None).await
}

#[tauri::command]
pub async fn service_quota_source_list(addr: Option<String>) -> Result<serde_json::Value, String> {
    rpc_call_in_background("quota/sourceList", addr, None).await
}

#[tauri::command]
pub async fn service_quota_refresh_sources(
    addr: Option<String>,
    kinds: Option<Vec<String>>,
    source_ids: Option<Vec<String>>,
) -> Result<serde_json::Value, String> {
    rpc_call_in_background(
        "quota/refreshSources",
        addr,
        Some(serde_json::json!({
            "kinds": kinds.unwrap_or_default(),
            "sourceIds": source_ids.unwrap_or_default(),
        })),
    )
    .await
}
