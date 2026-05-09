use super::*;
use codexmanager_core::storage::RequestTokenStat;

const MISSING_AUTH_JSON_OPENAI_API_KEY_ERROR: &str =
    "配置错误：未配置auth.json的OPENAI_API_KEY(invalid api key)";

/// 函数 `gateway_logs_invalid_api_key_error`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn gateway_logs_invalid_api_key_error() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-logs");
    let db_path: PathBuf = dir.join("codexmanager.db");

    let _guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let server = TestServer::start();
    let req_body = r#"{"model":"gpt-5.3-codex","input":"hello"}"#;
    let (status, body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        req_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", "Bearer invalid-platform-key"),
        ],
    );
    assert_eq!(status, 403);
    assert!(
        body.contains("invalid api key"),
        "gateway should return raw upstream message, got {body}"
    );
    assert!(
        !body.contains("未配置auth.json"),
        "gateway response should not expose bilingual log text, got {body}"
    );

    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init schema");
    let mut logs = Vec::new();
    for _ in 0..40 {
        logs = storage
            .list_request_logs(None, 100)
            .expect("list request logs");
        if !logs.is_empty() {
            break;
        }
        thread::sleep(Duration::from_millis(50));
    }
    let found = logs.iter().any(|item| {
        item.request_path == "/v1/responses"
            && item.status_code == Some(403)
            && item.input_tokens.is_none()
            && item.cached_input_tokens.is_none()
            && item.output_tokens.is_none()
            && item.total_tokens.is_none()
            && item.reasoning_output_tokens.is_none()
            && item.error.as_deref() == Some(MISSING_AUTH_JSON_OPENAI_API_KEY_ERROR)
    });
    assert!(
        found,
        "expected missing auth.json OPENAI_API_KEY request to be logged, got {:?}",
        logs.iter()
            .map(|v| (&v.request_path, v.status_code, v.error.as_deref()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn gateway_rejects_api_key_after_quota_limit() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-key-quota");
    let db_path: PathBuf = dir.join("codexmanager.db");
    let _guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let platform_key = "pk_quota_limit_reached";
    let storage = Storage::open(&db_path).expect("open db");
    storage.init().expect("init schema");
    let now = now_ts();
    storage
        .insert_api_key(&ApiKey {
            id: "gk_quota_limit_reached".to_string(),
            name: Some("quota-limit".to_string()),
            model_slug: None,
            reasoning_effort: None,
            service_tier: None,
            rotation_strategy: "account_rotation".to_string(),
            aggregate_api_id: None,
            account_plan_filter: None,
            aggregate_api_url: None,
            client_type: "codex".to_string(),
            protocol_type: "openai_compat".to_string(),
            auth_scheme: "authorization_bearer".to_string(),
            upstream_base_url: None,
            static_headers_json: None,
            key_hash: hash_platform_key_for_test(platform_key),
            status: "active".to_string(),
            created_at: now,
            last_used_at: None,
        })
        .expect("insert api key");
    storage
        .upsert_api_key_quota_limit("gk_quota_limit_reached", Some(100))
        .expect("upsert quota");
    storage
        .insert_request_token_stat(&RequestTokenStat {
            request_log_id: 1,
            key_id: Some("gk_quota_limit_reached".to_string()),
            total_tokens: Some(100),
            created_at: now,
            ..RequestTokenStat::default()
        })
        .expect("insert token stat");

    let server = TestServer::start();
    let req_body = r#"{"model":"gpt-5.3-codex","input":"hello"}"#;
    let (status, body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        req_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", &format!("Bearer {platform_key}")),
        ],
    );
    assert_eq!(status, 429, "response body: {body}");
    assert!(
        body.contains("quota") || body.contains("额度"),
        "gateway should report quota exhaustion, got {body}"
    );
}

/// 函数 `gateway_tolerates_non_ascii_turn_metadata_header`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn gateway_tolerates_non_ascii_turn_metadata_header() {
    let _lock = test_env_guard();
    let dir = new_test_dir("codexmanager-gateway-logs-nonascii");
    let db_path: PathBuf = dir.join("codexmanager.db");

    let _guard = EnvGuard::set("CODEXMANAGER_DB_PATH", db_path.to_string_lossy().as_ref());

    let server = TestServer::start();
    let req_body = r#"{"model":"gpt-5.3-codex","input":"hello"}"#;
    let metadata = r#"{"workspaces":{"D:\\MyComputer\\own\\GPTTeam相关\\CodexManager\\CodexManager":{"latest_git_commit_hash":"abc123"}}}"#;
    let (status, body) = post_http_raw(
        &server.addr,
        "/v1/responses",
        req_body,
        &[
            ("Content-Type", "application/json"),
            ("Authorization", "Bearer invalid-platform-key"),
            ("x-codex-turn-metadata", metadata),
        ],
    );
    assert_eq!(status, 403, "response body: {body}");
}
