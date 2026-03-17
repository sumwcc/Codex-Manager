use super::next_account_sort;
use crate::account_identity::{build_account_storage_id, pick_existing_account_id_by_identity};
use crate::auth_tokens::{
    ensure_workspace_allowed, exchange_code_for_tokens, format_api_key_exchange_status_error,
    format_token_endpoint_status_error, parse_token_endpoint_error,
};
use codexmanager_core::auth::parse_id_token_claims;
use codexmanager_core::storage::{now_ts, Account, Storage};
use reqwest::header::{HeaderMap, HeaderValue};
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Duration;
use tiny_http::{Response, Server};

static AUTH_RUNTIME_MUTEX: Mutex<()> = Mutex::new(());

struct GatewayRuntimeRestore {
    originator: String,
    residency: Option<String>,
}

impl GatewayRuntimeRestore {
    fn capture() -> Self {
        Self {
            originator: crate::current_gateway_originator(),
            residency: crate::current_gateway_residency_requirement(),
        }
    }
}

impl Drop for GatewayRuntimeRestore {
    fn drop(&mut self) {
        let _ = crate::set_gateway_originator(&self.originator);
        let _ = crate::set_gateway_residency_requirement(self.residency.as_deref());
    }
}

fn build_account(
    id: &str,
    chatgpt_account_id: Option<&str>,
    workspace_id: Option<&str>,
) -> Account {
    let now = now_ts();
    Account {
        id: id.to_string(),
        label: id.to_string(),
        issuer: "https://auth.openai.com".to_string(),
        chatgpt_account_id: chatgpt_account_id.map(|v| v.to_string()),
        workspace_id: workspace_id.map(|v| v.to_string()),
        group_name: None,
        sort: 0,
        status: "active".to_string(),
        created_at: now,
        updated_at: now,
    }
}

#[test]
fn pick_existing_account_requires_exact_scope_when_workspace_present() {
    let storage = Storage::open_in_memory().expect("open in memory");
    storage.init().expect("init");
    storage
        .insert_account(&build_account("acc-ws-a", Some("cgpt-1"), Some("ws-a")))
        .expect("insert ws-a");

    let found = pick_existing_account_id_by_identity(
        storage.list_accounts().expect("list accounts").iter(),
        Some("cgpt-1"),
        Some("ws-b"),
        Some("sub-fallback"),
        None,
    );

    assert_eq!(found, None);
}

#[test]
fn pick_existing_account_matches_exact_workspace_scope() {
    let storage = Storage::open_in_memory().expect("open in memory");
    storage.init().expect("init");
    storage
        .insert_account(&build_account("acc-ws-a", Some("cgpt-1"), Some("ws-a")))
        .expect("insert ws-a");
    storage
        .insert_account(&build_account("acc-ws-b", Some("cgpt-1"), Some("ws-b")))
        .expect("insert ws-b");

    let found = pick_existing_account_id_by_identity(
        storage.list_accounts().expect("list accounts").iter(),
        Some("cgpt-1"),
        Some("ws-b"),
        Some("sub-fallback"),
        None,
    );

    assert_eq!(found.as_deref(), Some("acc-ws-b"));
}

#[test]
fn build_account_storage_id_keeps_login_scope_shape() {
    let account_id = build_account_storage_id("sub-1", Some("cgpt-1"), Some("ws-a"), None);
    assert_eq!(account_id, "sub-1::cgpt=cgpt-1|ws=ws-a");
}

#[test]
fn next_account_sort_uses_step_five() {
    let storage = Storage::open_in_memory().expect("open in memory");
    storage.init().expect("init");
    storage
        .insert_account(&build_account("acc-1", Some("cgpt-1"), Some("ws-1")))
        .expect("insert account 1");
    storage
        .update_account_sort("acc-1", 2)
        .expect("update sort 1");
    storage
        .insert_account(&build_account("acc-2", Some("cgpt-2"), Some("ws-2")))
        .expect("insert account 2");
    storage
        .update_account_sort("acc-2", 7)
        .expect("update sort 2");

    assert_eq!(next_account_sort(&storage), 12);
}

fn jwt_with_claims(payload: &str) -> String {
    format!("eyJhbGciOiJIUzI1NiJ9.{payload}.sig")
}

#[test]
fn ensure_workspace_allowed_accepts_matching_auth_chatgpt_account_id() {
    let token = jwt_with_claims(
        "eyJzdWIiOiJ1c2VyLTEiLCJodHRwczovL2FwaS5vcGVuYWkuY29tL2F1dGgiOnsiY2hhdGdwdF9hY2NvdW50X2lkIjoib3JnX2FiYyJ9fQ",
    );
    let claims = parse_id_token_claims(&token).expect("claims");

    let result = ensure_workspace_allowed(Some("org_abc"), &claims, &token, &token);

    assert!(result.is_ok(), "workspace should match: {:?}", result);
}

#[test]
fn ensure_workspace_allowed_rejects_mismatched_workspace() {
    let token = jwt_with_claims("eyJzdWIiOiJ1c2VyLTEiLCJ3b3Jrc3BhY2VfaWQiOiJvcmdfYWJjIn0");
    let claims = parse_id_token_claims(&token).expect("claims");

    let result = ensure_workspace_allowed(Some("org_other"), &claims, &token, &token);

    assert_eq!(
        result.expect_err("should reject mismatch"),
        "Login is restricted to workspace id org_other."
    );
}

#[test]
fn parse_token_endpoint_error_prefers_error_description() {
    let detail = parse_token_endpoint_error(
        r#"{"error":"invalid_grant","error_description":"refresh token expired"}"#,
    );

    assert_eq!(detail.to_string(), "refresh token expired");
}

#[test]
fn parse_token_endpoint_error_reads_nested_error_message_and_code() {
    let detail = parse_token_endpoint_error(
        r#"{"error":{"code":"proxy_auth_required","message":"proxy authentication required"}}"#,
    );

    assert_eq!(detail.to_string(), "proxy authentication required");
}

#[test]
fn parse_token_endpoint_error_preserves_plain_text_for_display() {
    let detail = parse_token_endpoint_error("service unavailable");

    assert_eq!(detail.to_string(), "service unavailable");
}

#[test]
fn parse_token_endpoint_error_summarizes_challenge_html() {
    let detail =
        parse_token_endpoint_error("<html><title>Just a moment...</title><body>cf</body></html>");

    assert_eq!(
        detail.to_string(),
        "Cloudflare 安全验证页（title=Just a moment...）"
    );
}

#[test]
fn parse_token_endpoint_error_summarizes_generic_html() {
    let detail = parse_token_endpoint_error("<html><title>502 Bad Gateway</title></html>");

    assert_eq!(
        detail.to_string(),
        "上游返回 HTML 错误页（title=502 Bad Gateway）"
    );
}

#[test]
fn format_token_endpoint_status_error_appends_debug_headers() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-oai-request-id",
        HeaderValue::from_static("req_token_123"),
    );
    headers.insert("cf-ray", HeaderValue::from_static("ray_token_123"));
    headers.insert(
        "x-openai-authorization-error",
        HeaderValue::from_static("expired_session"),
    );

    let message = format_token_endpoint_status_error(
        reqwest::StatusCode::FORBIDDEN,
        &headers,
        "<html><title>Just a moment...</title></html>",
    );

    assert!(message.contains("token endpoint returned status 403 Forbidden"));
    assert!(message.contains("Cloudflare 安全验证页（title=Just a moment...）"));
    assert!(message.contains("request_id=req_token_123"));
    assert!(message.contains("cf_ray=ray_token_123"));
    assert!(message.contains("auth_error=expired_session"));
}

#[test]
fn format_api_key_exchange_status_error_appends_debug_headers() {
    let mut headers = HeaderMap::new();
    headers.insert("x-request-id", HeaderValue::from_static("req_api_key_123"));
    headers.insert("cf-ray", HeaderValue::from_static("ray_api_key_123"));

    let message = format_api_key_exchange_status_error(
        reqwest::StatusCode::BAD_GATEWAY,
        &headers,
        "<html><title>502 Bad Gateway</title></html>",
    );

    assert!(message.contains("api key exchange failed with status 502 Bad Gateway"));
    assert!(message.contains("上游返回 HTML 错误页（title=502 Bad Gateway）"));
    assert!(message.contains("request_id=req_api_key_123"));
    assert!(message.contains("cf_ray=ray_api_key_123"));
}

#[test]
fn exchange_code_for_tokens_uses_official_codex_headers() {
    let _guard = AUTH_RUNTIME_MUTEX.lock().expect("lock auth runtime");
    let _restore = GatewayRuntimeRestore::capture();
    crate::set_gateway_originator("codex_cli_rs_auth_test").expect("set originator");
    crate::set_gateway_residency_requirement(Some("us")).expect("set residency");

    let server = Server::http("127.0.0.1:0").expect("bind mock token server");
    let addr = server.server_addr().to_ip().expect("server addr");
    let issuer = format!("http://{addr}");
    let (tx, rx) = mpsc::sync_channel(1);

    let join = std::thread::spawn(move || {
        let request = server.recv().expect("receive token request");
        let headers = request
            .headers()
            .iter()
            .map(|header| {
                (
                    header.field.as_str().to_string(),
                    header.value.as_str().to_string(),
                )
            })
            .collect::<Vec<_>>();
        let _ = tx.send((request.url().to_string(), headers));
        let body = r#"{"id_token":"id_token_test","access_token":"access_token_test","refresh_token":"refresh_token_test"}"#;
        let response = Response::from_string(body).with_status_code(200);
        request.respond(response).expect("respond token");
    });

    let tokens = exchange_code_for_tokens(
        &issuer,
        "client-test",
        "http://localhost:1455/auth/callback",
        "verifier-test",
        "code-test",
    )
    .expect("exchange code for tokens");

    let (path, headers) = rx
        .recv_timeout(Duration::from_secs(2))
        .expect("receive captured token request");
    join.join().expect("join mock token server");

    let find = |name: &str| {
        headers
            .iter()
            .find(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    };

    assert_eq!(path, "/oauth/token");
    assert_eq!(find("Accept"), Some("application/json"));
    assert_eq!(find("Originator"), Some("codex_cli_rs_auth_test"));
    assert_eq!(find("x-openai-internal-codex-residency"), Some("us"));
    assert_eq!(
        find("Content-Type"),
        Some("application/x-www-form-urlencoded")
    );
    assert!(
        find("User-Agent").is_some_and(|value| value.contains("codex_cli_rs_auth_test/0.101.0"))
    );
    assert_eq!(tokens.access_token, "access_token_test");
}
