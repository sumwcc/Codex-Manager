use codexmanager_core::auth::{
    extract_chatgpt_account_id, extract_workspace_id, normalize_chatgpt_account_id,
    normalize_workspace_id, parse_id_token_claims, DEFAULT_CLIENT_ID, DEFAULT_ISSUER,
};
use codexmanager_core::rpc::types::LoginStartResult;
use codexmanager_core::storage::{now_ts, Account, Storage, Token};
use crossbeam_channel::unbounded;
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use crate::account_identity::{
    build_account_storage_id, build_fallback_subject_key, clean_value,
    pick_existing_account_id_by_identity,
};
use crate::account_status::{
    mark_account_unavailable_for_auth_error, restore_account_after_successful_manual_token_refresh,
};
use crate::app_settings::{get_persisted_app_setting, save_persisted_app_setting};
use crate::storage_helpers::open_storage;
use crate::usage_http::{fetch_account_subscription, refresh_token_auth_error_reason_from_message};
use crate::usage_token_refresh::{
    record_token_refresh_failure, record_token_refresh_success, refresh_and_persist_access_token,
    token_refresh_ahead_secs, TokenRefreshOutcome, TOKEN_REFRESH_SOURCE_ACCOUNT_READ_REFRESH,
    TOKEN_REFRESH_SOURCE_MANUAL_ALL_BATCH, TOKEN_REFRESH_SOURCE_MANUAL_ALL_SYNC,
    TOKEN_REFRESH_SOURCE_MANUAL_SINGLE,
};

const CURRENT_AUTH_ACCOUNT_ID_KEY: &str = "auth.current_account_id";
const CURRENT_AUTH_MODE_KEY: &str = "auth.current_auth_mode";
const AUTH_MODE_CHATGPT: &str = "chatgpt";
const AUTH_MODE_CHATGPT_AUTH_TOKENS: &str = "chatgptAuthTokens";
const REFRESH_ALL_BATCH_STATUS_RUNNING: &str = "running";
const REFRESH_ALL_BATCH_STATUS_COMPLETED: &str = "completed";
const REFRESH_ALL_BATCH_STATUS_FAILED: &str = "failed";
const REFRESH_ALL_ITEM_STATUS_PENDING: &str = "pending";
const REFRESH_ALL_ITEM_STATUS_RUNNING: &str = "running";
const REFRESH_ALL_ITEM_STATUS_SUCCESS: &str = "success";
const REFRESH_ALL_ITEM_STATUS_FAILED: &str = "failed";
const REFRESH_ALL_ITEM_STATUS_SKIPPED: &str = "skipped";
const REFRESH_ALL_MAX_ATTEMPTS: usize = 3;
const REFRESH_ALL_RETRY_DELAY_MS: u64 = 500;
const DEFAULT_REFRESH_ALL_WORKERS: usize = 4;
const ENV_REFRESH_ALL_WORKERS: &str = "CODEXMANAGER_USAGE_REFRESH_WORKERS";

static REFRESH_ALL_BATCH: OnceLock<Mutex<Option<RefreshAllBatchState>>> = OnceLock::new();
static REFRESH_ALL_BATCH_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountReadResponse {
    pub(crate) account: Option<CurrentAuthAccount>,
    pub(crate) requires_openai_auth: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CurrentAuthAccount {
    #[serde(rename = "type")]
    pub(crate) kind: String,
    pub(crate) account_id: String,
    pub(crate) email: String,
    pub(crate) plan_type: String,
    pub(crate) plan_type_raw: Option<String>,
    pub(crate) has_subscription: Option<bool>,
    pub(crate) subscription_plan: Option<String>,
    pub(crate) subscription_expires_at: Option<i64>,
    pub(crate) subscription_renews_at: Option<i64>,
    pub(crate) chatgpt_account_id: Option<String>,
    pub(crate) workspace_id: Option<String>,
    pub(crate) status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatgptAuthTokensRefreshResponse {
    pub(crate) access_token: String,
    pub(crate) chatgpt_account_id: String,
    pub(crate) chatgpt_plan_type: Option<String>,
    pub(crate) has_subscription: Option<bool>,
    pub(crate) subscription_plan: Option<String>,
    pub(crate) subscription_expires_at: Option<i64>,
    pub(crate) subscription_renews_at: Option<i64>,
    pub(crate) access_token_changed: bool,
    pub(crate) refresh_token_returned: bool,
    pub(crate) refresh_token_changed: bool,
    pub(crate) id_token_changed: bool,
    pub(crate) access_token_expires_at: Option<i64>,
    pub(crate) refresh_token_expires_at: Option<i64>,
    pub(crate) next_refresh_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatgptAuthTokensRefreshAllItem {
    pub(crate) account_id: String,
    pub(crate) account_name: String,
    pub(crate) status: String,
    pub(crate) ok: bool,
    pub(crate) message: Option<String>,
    pub(crate) started_at: Option<i64>,
    pub(crate) finished_at: Option<i64>,
    pub(crate) access_token_changed: bool,
    pub(crate) refresh_token_returned: bool,
    pub(crate) refresh_token_changed: bool,
    pub(crate) id_token_changed: bool,
    pub(crate) access_token_expires_at: Option<i64>,
    pub(crate) refresh_token_expires_at: Option<i64>,
    pub(crate) next_refresh_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ChatgptAuthTokensRefreshAllResponse {
    pub(crate) batch_id: Option<String>,
    pub(crate) status: String,
    pub(crate) total: usize,
    pub(crate) requested: usize,
    pub(crate) processed: usize,
    pub(crate) succeeded: usize,
    pub(crate) failed: usize,
    pub(crate) skipped: usize,
    pub(crate) refresh_token_returned: usize,
    pub(crate) refresh_token_changed: usize,
    pub(crate) refresh_token_missing: usize,
    pub(crate) started_at: Option<i64>,
    pub(crate) finished_at: Option<i64>,
    pub(crate) results: Vec<ChatgptAuthTokensRefreshAllItem>,
}

#[derive(Debug, Clone)]
struct RefreshAllBatchState {
    batch_id: String,
    status: String,
    total: usize,
    requested: usize,
    started_at: i64,
    finished_at: Option<i64>,
    results: Vec<ChatgptAuthTokensRefreshAllItem>,
}

#[derive(Debug, Clone)]
struct RefreshAllTokenTask {
    account_id: String,
    issuer: String,
    client_id: String,
    token: Token,
}

impl RefreshAllBatchState {
    fn to_response(&self) -> ChatgptAuthTokensRefreshAllResponse {
        let succeeded = self
            .results
            .iter()
            .filter(|item| item.status == REFRESH_ALL_ITEM_STATUS_SUCCESS)
            .count();
        let failed = self
            .results
            .iter()
            .filter(|item| item.status == REFRESH_ALL_ITEM_STATUS_FAILED)
            .count();
        let skipped = self
            .results
            .iter()
            .filter(|item| item.status == REFRESH_ALL_ITEM_STATUS_SKIPPED)
            .count();
        let refresh_token_returned = self
            .results
            .iter()
            .filter(|item| {
                item.status == REFRESH_ALL_ITEM_STATUS_SUCCESS && item.refresh_token_returned
            })
            .count();
        let refresh_token_changed = self
            .results
            .iter()
            .filter(|item| {
                item.status == REFRESH_ALL_ITEM_STATUS_SUCCESS && item.refresh_token_changed
            })
            .count();
        let refresh_token_missing = self
            .results
            .iter()
            .filter(|item| {
                item.status == REFRESH_ALL_ITEM_STATUS_SUCCESS && !item.refresh_token_returned
            })
            .count();
        let processed = succeeded.saturating_add(failed).saturating_add(skipped);
        ChatgptAuthTokensRefreshAllResponse {
            batch_id: Some(self.batch_id.clone()),
            status: self.status.clone(),
            total: self.total,
            requested: self.requested,
            processed,
            succeeded,
            failed,
            skipped,
            refresh_token_returned,
            refresh_token_changed,
            refresh_token_missing,
            started_at: Some(self.started_at),
            finished_at: self.finished_at,
            results: self.results.clone(),
        }
    }

    fn has_unfinished_items(&self) -> bool {
        self.results.iter().any(|item| {
            item.status == REFRESH_ALL_ITEM_STATUS_PENDING
                || item.status == REFRESH_ALL_ITEM_STATUS_RUNNING
        })
    }
}

fn refresh_all_batch_slot() -> &'static Mutex<Option<RefreshAllBatchState>> {
    REFRESH_ALL_BATCH.get_or_init(|| Mutex::new(None))
}

fn lock_refresh_all_batch() -> std::sync::MutexGuard<'static, Option<RefreshAllBatchState>> {
    refresh_all_batch_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn refresh_all_batch_id() -> String {
    let counter = REFRESH_ALL_BATCH_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("chatgpt-auth-refresh-{}-{counter}", now_ts())
}

fn refresh_all_item(
    account_id: String,
    account_name: String,
    status: &str,
    message: Option<String>,
) -> ChatgptAuthTokensRefreshAllItem {
    let finished_at = matches!(
        status,
        REFRESH_ALL_ITEM_STATUS_SUCCESS
            | REFRESH_ALL_ITEM_STATUS_FAILED
            | REFRESH_ALL_ITEM_STATUS_SKIPPED
    )
    .then(now_ts);
    ChatgptAuthTokensRefreshAllItem {
        account_id,
        account_name,
        status: status.to_string(),
        ok: status == REFRESH_ALL_ITEM_STATUS_SUCCESS,
        message,
        started_at: None,
        finished_at,
        access_token_changed: false,
        refresh_token_returned: false,
        refresh_token_changed: false,
        id_token_changed: false,
        access_token_expires_at: None,
        refresh_token_expires_at: None,
        next_refresh_at: None,
    }
}

fn apply_refresh_outcome_to_item(
    item: &mut ChatgptAuthTokensRefreshAllItem,
    outcome: TokenRefreshOutcome,
) {
    item.access_token_changed = outcome.access_token_changed;
    item.refresh_token_returned = outcome.refresh_token_returned;
    item.refresh_token_changed = outcome.refresh_token_changed;
    item.id_token_changed = outcome.id_token_changed;
    item.access_token_expires_at = outcome.access_token_expires_at;
    item.refresh_token_expires_at = outcome.refresh_token_expires_at;
    item.next_refresh_at = outcome.next_refresh_at;
}

#[derive(Debug, Clone)]
pub(crate) struct ChatgptAuthTokensLoginInput {
    pub(crate) access_token: String,
    pub(crate) refresh_token: Option<String>,
    pub(crate) id_token: Option<String>,
    pub(crate) chatgpt_account_id: Option<String>,
    pub(crate) workspace_id: Option<String>,
    pub(crate) chatgpt_plan_type: Option<String>,
}

/// 函数 `login_with_chatgpt_auth_tokens`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn login_with_chatgpt_auth_tokens(
    input: ChatgptAuthTokensLoginInput,
) -> Result<LoginStartResult, String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let access_token = input.access_token.trim();
    if access_token.is_empty() {
        return Err("accessToken is required".to_string());
    }
    let _requested_plan_type = input
        .chatgpt_plan_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let claims = parse_id_token_claims(access_token)
        .map_err(|err| format!("invalid access token jwt: {err}"))?;
    let subject_account_id = claims.sub.trim();
    if subject_account_id.is_empty() {
        return Err("access token missing subject".to_string());
    }
    let claim_chatgpt_account_id = claims
        .auth
        .as_ref()
        .and_then(|auth| normalize_chatgpt_account_id(auth.chatgpt_account_id.as_deref()));
    let claim_workspace_id = normalize_workspace_id(claims.workspace_id.as_deref());

    let chatgpt_account_id = clean_value(
        input
            .chatgpt_account_id
            .as_deref()
            .and_then(|value| normalize_chatgpt_account_id(Some(value)))
            .or_else(|| extract_chatgpt_account_id(access_token))
            .or(claim_chatgpt_account_id),
    );
    let workspace_id = clean_value(
        input
            .workspace_id
            .as_deref()
            .and_then(|value| normalize_workspace_id(Some(value)))
            .or_else(|| extract_workspace_id(access_token))
            .or(claim_workspace_id)
            .or_else(|| chatgpt_account_id.clone()),
    );
    let resolved_scope_id = workspace_id
        .clone()
        .or_else(|| chatgpt_account_id.clone())
        .ok_or_else(|| "chatgptAccountId/workspaceId is required".to_string())?;

    let fallback_subject_key = build_fallback_subject_key(Some(subject_account_id), None);
    let account_storage_id = build_account_storage_id(
        subject_account_id,
        chatgpt_account_id.as_deref(),
        workspace_id.as_deref(),
        None,
    );
    let accounts = storage.list_accounts().map_err(|err| err.to_string())?;
    let account_id = pick_existing_account_id_by_identity(
        accounts.iter(),
        chatgpt_account_id.as_deref(),
        workspace_id.as_deref(),
        fallback_subject_key.as_deref(),
        None,
    )
    .unwrap_or(account_storage_id);

    let existing_account = storage
        .find_account_by_id(&account_id)
        .map_err(|err| err.to_string())?;
    let now = now_ts();
    let account = Account {
        id: account_id.clone(),
        label: claims
            .email
            .clone()
            .unwrap_or_else(|| resolved_scope_id.clone()),
        issuer: std::env::var("CODEXMANAGER_ISSUER").unwrap_or_else(|_| DEFAULT_ISSUER.to_string()),
        chatgpt_account_id: chatgpt_account_id.clone(),
        workspace_id: workspace_id.clone(),
        group_name: existing_account
            .as_ref()
            .and_then(|account| account.group_name.clone()),
        sort: existing_account
            .as_ref()
            .map(|account| account.sort)
            .unwrap_or_else(|| super::tokens::next_account_sort(&storage)),
        status: "active".to_string(),
        created_at: existing_account
            .as_ref()
            .map(|account| account.created_at)
            .unwrap_or(now),
        updated_at: now,
    };
    storage
        .insert_account(&account)
        .map_err(|err| err.to_string())?;

    let mut token = Token {
        account_id: account_id.clone(),
        id_token: input.id_token.unwrap_or_default(),
        access_token: access_token.to_string(),
        refresh_token: input.refresh_token.unwrap_or_default(),
        api_key_access_token: None,
        last_refresh: now,
    };
    if token.id_token.trim().is_empty() {
        token.id_token = token.access_token.clone();
    }
    storage
        .insert_token(&token)
        .map_err(|err| err.to_string())?;

    set_current_auth_account_id(Some(&account_id))?;
    set_current_auth_mode(Some(AUTH_MODE_CHATGPT_AUTH_TOKENS))?;
    let _ = crate::usage_refresh::enqueue_usage_refresh_after_account_add(&account_id);

    Ok(LoginStartResult::ChatgptAuthTokens {})
}

/// 函数 `read_current_account`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn read_current_account(refresh_token: bool) -> Result<AccountReadResponse, String> {
    let Some(storage) = open_storage() else {
        return Ok(AccountReadResponse {
            account: None,
            requires_openai_auth: true,
        });
    };
    let Some((account, token)) = resolve_current_account_with_token(&storage)? else {
        return Ok(AccountReadResponse {
            account: None,
            requires_openai_auth: true,
        });
    };

    let mut token = token;
    if refresh_token && !token.refresh_token.trim().is_empty() {
        let issuer =
            std::env::var("CODEXMANAGER_ISSUER").unwrap_or_else(|_| DEFAULT_ISSUER.to_string());
        let client_id = std::env::var("CODEXMANAGER_CLIENT_ID")
            .unwrap_or_else(|_| DEFAULT_CLIENT_ID.to_string());
        match refresh_and_persist_access_token(
            &storage,
            &mut token,
            &issuer,
            &client_id,
            token_refresh_ahead_secs(),
        ) {
            Ok(outcome) => record_token_refresh_success(
                &storage,
                &account.id,
                TOKEN_REFRESH_SOURCE_ACCOUNT_READ_REFRESH,
                outcome,
            ),
            Err(err) => {
                record_token_refresh_failure(
                    &storage,
                    &account.id,
                    TOKEN_REFRESH_SOURCE_ACCOUNT_READ_REFRESH,
                    &err,
                );
                let _ = mark_account_unavailable_for_auth_error(&storage, &account.id, &err);
                return Err(err);
            }
        }
    }

    let auth_mode = resolve_current_auth_mode(&token);
    Ok(AccountReadResponse {
        account: Some(current_account_payload(
            &storage,
            &account,
            &token,
            auth_mode.as_str(),
        )),
        requires_openai_auth: true,
    })
}

/// 函数 `refresh_current_chatgpt_auth_tokens`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn refresh_current_chatgpt_auth_tokens(
    target_account_id: Option<&str>,
) -> Result<ChatgptAuthTokensRefreshResponse, String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let (account, mut token) = resolve_refresh_target(&storage, target_account_id)?
        .ok_or_else(|| "no current chatgptAuthTokens account".to_string())?;
    if token.refresh_token.trim().is_empty() {
        return Err("target account does not have refresh_token".to_string());
    }

    let issuer =
        std::env::var("CODEXMANAGER_ISSUER").unwrap_or_else(|_| DEFAULT_ISSUER.to_string());
    let client_id =
        std::env::var("CODEXMANAGER_CLIENT_ID").unwrap_or_else(|_| DEFAULT_CLIENT_ID.to_string());
    let refresh_outcome = match refresh_and_persist_access_token(
        &storage,
        &mut token,
        &issuer,
        &client_id,
        token_refresh_ahead_secs(),
    ) {
        Ok(outcome) => {
            record_token_refresh_success(
                &storage,
                &account.id,
                TOKEN_REFRESH_SOURCE_MANUAL_SINGLE,
                outcome,
            );
            let _ = restore_account_after_successful_manual_token_refresh(&storage, &account.id);
            outcome
        }
        Err(err) => {
            record_token_refresh_failure(
                &storage,
                &account.id,
                TOKEN_REFRESH_SOURCE_MANUAL_SINGLE,
                &err,
            );
            let _ = mark_account_unavailable_for_auth_error(&storage, &account.id, &err);
            return Err(err);
        }
    };

    let refreshed_account = storage
        .find_account_by_id(&account.id)
        .map_err(|err| err.to_string())?
        .unwrap_or(account);
    let stored_chatgpt_account_id =
        normalize_chatgpt_account_id(refreshed_account.chatgpt_account_id.as_deref());
    let stored_workspace_id = normalize_workspace_id(refreshed_account.workspace_id.as_deref());
    let chatgpt_account_id = stored_chatgpt_account_id
        .clone()
        .or_else(|| extract_chatgpt_account_id(&token.access_token))
        .or_else(|| stored_workspace_id.clone())
        .or_else(|| extract_workspace_id(&token.access_token))
        .ok_or_else(|| "refreshed token missing chatgptAccountId".to_string())?;
    let workspace_id = stored_workspace_id
        .clone()
        .or_else(|| extract_workspace_id(&token.access_token))
        .or_else(|| stored_chatgpt_account_id.clone())
        .or_else(|| extract_chatgpt_account_id(&token.access_token));
    let access_claims = parse_id_token_claims(&token.access_token).ok();
    let plan_type_resolution = resolve_plan_type_resolution(&token, access_claims.as_ref());
    let base_url = std::env::var("CODEXMANAGER_USAGE_BASE_URL")
        .unwrap_or_else(|_| "https://chatgpt.com".to_string());
    let subscription = fetch_account_subscription(
        &base_url,
        &token.access_token,
        &chatgpt_account_id,
        workspace_id.as_deref(),
    )?;
    storage
        .upsert_account_subscription(
            &refreshed_account.id,
            subscription.has_subscription,
            subscription.plan_type.as_deref(),
            subscription.expires_at,
            subscription.renews_at,
        )
        .map_err(|err| format!("store account subscription failed: {err}"))?;
    let chatgpt_plan_type = subscription.plan_type.clone().or_else(|| {
        plan_type_resolution
            .as_ref()
            .map(|plan| plan.normalized.clone())
    });

    Ok(ChatgptAuthTokensRefreshResponse {
        access_token: token.access_token,
        chatgpt_account_id,
        chatgpt_plan_type,
        has_subscription: Some(subscription.has_subscription),
        subscription_plan: subscription.plan_type,
        subscription_expires_at: subscription.expires_at,
        subscription_renews_at: subscription.renews_at,
        access_token_changed: refresh_outcome.access_token_changed,
        refresh_token_returned: refresh_outcome.refresh_token_returned,
        refresh_token_changed: refresh_outcome.refresh_token_changed,
        id_token_changed: refresh_outcome.id_token_changed,
        access_token_expires_at: refresh_outcome.access_token_expires_at,
        refresh_token_expires_at: refresh_outcome.refresh_token_expires_at,
        next_refresh_at: refresh_outcome.next_refresh_at,
    })
}

/// 函数 `refresh_all_chatgpt_auth_tokens`
///
/// 作者: gaohongshun
///
/// 时间: 2026-05-03
///
/// # 参数
/// 无
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn refresh_all_chatgpt_auth_tokens(
) -> Result<ChatgptAuthTokensRefreshAllResponse, String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let accounts = storage.list_accounts().map_err(|err| err.to_string())?;
    let total = accounts.len();
    let started_at = now_ts();
    let client_id =
        std::env::var("CODEXMANAGER_CLIENT_ID").unwrap_or_else(|_| DEFAULT_CLIENT_ID.to_string());
    let default_issuer =
        std::env::var("CODEXMANAGER_ISSUER").unwrap_or_else(|_| DEFAULT_ISSUER.to_string());

    let mut results = Vec::with_capacity(accounts.len());
    let mut requested = 0usize;
    let mut succeeded = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut refresh_token_returned = 0usize;
    let mut refresh_token_changed = 0usize;
    let mut refresh_token_missing = 0usize;

    for account in accounts {
        let account_name = account.label.clone();
        let Some(mut token) = storage
            .find_token_by_account_id(&account.id)
            .map_err(|err| err.to_string())?
        else {
            skipped = skipped.saturating_add(1);
            results.push(refresh_all_item(
                account.id,
                account_name,
                REFRESH_ALL_ITEM_STATUS_SKIPPED,
                Some("missing token".to_string()),
            ));
            continue;
        };
        if token.refresh_token.trim().is_empty() {
            skipped = skipped.saturating_add(1);
            results.push(refresh_all_item(
                account.id,
                account_name,
                REFRESH_ALL_ITEM_STATUS_SKIPPED,
                Some("missing refresh_token".to_string()),
            ));
            continue;
        }

        requested = requested.saturating_add(1);
        let item_started_at = now_ts();
        let issuer = if account.issuer.trim().is_empty() {
            default_issuer.as_str()
        } else {
            account.issuer.as_str()
        };
        match refresh_and_persist_access_token(
            &storage,
            &mut token,
            issuer,
            &client_id,
            token_refresh_ahead_secs(),
        ) {
            Ok(outcome) => {
                succeeded = succeeded.saturating_add(1);
                if outcome.refresh_token_returned {
                    refresh_token_returned = refresh_token_returned.saturating_add(1);
                } else {
                    refresh_token_missing = refresh_token_missing.saturating_add(1);
                }
                if outcome.refresh_token_changed {
                    refresh_token_changed = refresh_token_changed.saturating_add(1);
                }
                record_token_refresh_success(
                    &storage,
                    &account.id,
                    TOKEN_REFRESH_SOURCE_MANUAL_ALL_SYNC,
                    outcome,
                );
                let _ =
                    restore_account_after_successful_manual_token_refresh(&storage, &account.id);
                let mut item = ChatgptAuthTokensRefreshAllItem {
                    account_id: account.id,
                    account_name,
                    status: REFRESH_ALL_ITEM_STATUS_SUCCESS.to_string(),
                    ok: true,
                    message: None,
                    started_at: Some(item_started_at),
                    finished_at: Some(now_ts()),
                    access_token_changed: false,
                    refresh_token_returned: false,
                    refresh_token_changed: false,
                    id_token_changed: false,
                    access_token_expires_at: None,
                    refresh_token_expires_at: None,
                    next_refresh_at: None,
                };
                apply_refresh_outcome_to_item(&mut item, outcome);
                results.push(item);
            }
            Err(err) => {
                failed = failed.saturating_add(1);
                record_token_refresh_failure(
                    &storage,
                    &account.id,
                    TOKEN_REFRESH_SOURCE_MANUAL_ALL_SYNC,
                    &err,
                );
                let _ = mark_account_unavailable_for_auth_error(&storage, &account.id, &err);
                results.push(ChatgptAuthTokensRefreshAllItem {
                    account_id: account.id,
                    account_name,
                    status: REFRESH_ALL_ITEM_STATUS_FAILED.to_string(),
                    ok: false,
                    message: Some(err),
                    started_at: Some(item_started_at),
                    finished_at: Some(now_ts()),
                    access_token_changed: false,
                    refresh_token_returned: false,
                    refresh_token_changed: false,
                    id_token_changed: false,
                    access_token_expires_at: None,
                    refresh_token_expires_at: None,
                    next_refresh_at: None,
                });
            }
        }
    }

    Ok(ChatgptAuthTokensRefreshAllResponse {
        batch_id: None,
        status: REFRESH_ALL_BATCH_STATUS_COMPLETED.to_string(),
        total,
        requested,
        processed: succeeded.saturating_add(failed).saturating_add(skipped),
        succeeded,
        failed,
        skipped,
        refresh_token_returned,
        refresh_token_changed,
        refresh_token_missing,
        started_at: Some(started_at),
        finished_at: Some(now_ts()),
        results,
    })
}

pub(crate) fn start_refresh_all_chatgpt_auth_tokens_batch(
) -> Result<ChatgptAuthTokensRefreshAllResponse, String> {
    {
        let guard = lock_refresh_all_batch();
        if let Some(state) = guard.as_ref() {
            if state.status == REFRESH_ALL_BATCH_STATUS_RUNNING {
                return Ok(state.to_response());
            }
        }
    }

    let (mut state, tasks) = build_refresh_all_batch_state()?;
    if tasks.is_empty() {
        state.status = REFRESH_ALL_BATCH_STATUS_COMPLETED.to_string();
        state.finished_at = Some(now_ts());
    }

    let batch_id = state.batch_id.clone();
    {
        let mut guard = lock_refresh_all_batch();
        if let Some(current) = guard.as_ref() {
            if current.status == REFRESH_ALL_BATCH_STATUS_RUNNING {
                return Ok(current.to_response());
            }
        }
        *guard = Some(state);
    }

    if !tasks.is_empty() {
        let thread_batch_id = batch_id.clone();
        if let Err(err) = thread::Builder::new()
            .name("chatgpt-auth-refresh-all".to_string())
            .spawn(move || run_refresh_all_batch(thread_batch_id, tasks))
        {
            finish_refresh_all_batch(
                &batch_id,
                REFRESH_ALL_BATCH_STATUS_FAILED,
                Some(format!("spawn refresh batch failed: {err}")),
            );
        }
    }

    refresh_all_chatgpt_auth_tokens_batch_status(&batch_id)
}

pub(crate) fn refresh_all_chatgpt_auth_tokens_batch_status(
    batch_id: &str,
) -> Result<ChatgptAuthTokensRefreshAllResponse, String> {
    let normalized_batch_id = batch_id.trim();
    if normalized_batch_id.is_empty() {
        return Err("batchId is required".to_string());
    }
    let guard = lock_refresh_all_batch();
    let Some(state) = guard.as_ref() else {
        return Err("refresh batch not found".to_string());
    };
    if state.batch_id != normalized_batch_id {
        return Err("refresh batch not found".to_string());
    }
    Ok(state.to_response())
}

fn build_refresh_all_batch_state(
) -> Result<(RefreshAllBatchState, Vec<RefreshAllTokenTask>), String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let accounts = storage.list_accounts().map_err(|err| err.to_string())?;
    let total = accounts.len();
    let batch_id = refresh_all_batch_id();
    let started_at = now_ts();
    let client_id =
        std::env::var("CODEXMANAGER_CLIENT_ID").unwrap_or_else(|_| DEFAULT_CLIENT_ID.to_string());
    let default_issuer =
        std::env::var("CODEXMANAGER_ISSUER").unwrap_or_else(|_| DEFAULT_ISSUER.to_string());

    let mut results = Vec::with_capacity(total);
    let mut tasks = Vec::new();
    for account in accounts {
        let account_id = account.id.clone();
        let account_name = account.label.clone();
        let Some(token) = storage
            .find_token_by_account_id(&account_id)
            .map_err(|err| err.to_string())?
        else {
            results.push(refresh_all_item(
                account_id,
                account_name,
                REFRESH_ALL_ITEM_STATUS_SKIPPED,
                Some("missing token".to_string()),
            ));
            continue;
        };
        if token.refresh_token.trim().is_empty() {
            results.push(refresh_all_item(
                account_id,
                account_name,
                REFRESH_ALL_ITEM_STATUS_SKIPPED,
                Some("missing refresh_token".to_string()),
            ));
            continue;
        }

        let issuer = if account.issuer.trim().is_empty() {
            default_issuer.clone()
        } else {
            account.issuer.clone()
        };
        results.push(refresh_all_item(
            account_id.clone(),
            account_name.clone(),
            REFRESH_ALL_ITEM_STATUS_PENDING,
            None,
        ));
        tasks.push(RefreshAllTokenTask {
            account_id,
            issuer,
            client_id: client_id.clone(),
            token,
        });
    }

    Ok((
        RefreshAllBatchState {
            batch_id,
            status: REFRESH_ALL_BATCH_STATUS_RUNNING.to_string(),
            total,
            requested: tasks.len(),
            started_at,
            finished_at: None,
            results,
        },
        tasks,
    ))
}

fn refresh_all_worker_count(total: usize) -> usize {
    if total == 0 {
        return 0;
    }
    std::env::var(ENV_REFRESH_ALL_WORKERS)
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .unwrap_or(DEFAULT_REFRESH_ALL_WORKERS)
        .max(1)
        .min(total)
}

fn run_refresh_all_batch(batch_id: String, tasks: Vec<RefreshAllTokenTask>) {
    let result = run_refresh_all_batch_inner(&batch_id, tasks);
    match result {
        Ok(()) => finish_refresh_all_batch(&batch_id, REFRESH_ALL_BATCH_STATUS_COMPLETED, None),
        Err(err) => finish_refresh_all_batch(&batch_id, REFRESH_ALL_BATCH_STATUS_FAILED, Some(err)),
    }
}

fn run_refresh_all_batch_inner(
    batch_id: &str,
    tasks: Vec<RefreshAllTokenTask>,
) -> Result<(), String> {
    let worker_count = refresh_all_worker_count(tasks.len());
    if worker_count <= 1 {
        let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
        for task in tasks {
            run_refresh_all_batch_task(batch_id, &storage, task);
        }
        return Ok(());
    }

    let (sender, receiver) = unbounded::<RefreshAllTokenTask>();
    for task in tasks {
        sender
            .send(task)
            .map_err(|_| "enqueue refresh task failed".to_string())?;
    }
    drop(sender);

    thread::scope(|scope| -> Result<(), String> {
        let mut handles = Vec::with_capacity(worker_count);
        for worker_index in 0..worker_count {
            let receiver = receiver.clone();
            handles.push(scope.spawn(move || {
                let storage = open_storage().ok_or_else(|| {
                    format!("refresh all AT/RT worker {worker_index} storage unavailable")
                })?;
                while let Ok(task) = receiver.recv() {
                    run_refresh_all_batch_task(batch_id, &storage, task);
                }
                Ok::<(), String>(())
            }));
        }

        for handle in handles {
            match handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(err)) => return Err(err),
                Err(_) => return Err("refresh all AT/RT worker panicked".to_string()),
            }
        }
        Ok(())
    })
}

fn run_refresh_all_batch_task(batch_id: &str, storage: &Storage, task: RefreshAllTokenTask) {
    mark_refresh_all_item_running(batch_id, &task.account_id);
    let result = refresh_batch_token_with_retries(storage, &task);
    match result {
        Ok(outcome) => {
            record_token_refresh_success(
                storage,
                &task.account_id,
                TOKEN_REFRESH_SOURCE_MANUAL_ALL_BATCH,
                outcome,
            );
            let _ =
                restore_account_after_successful_manual_token_refresh(storage, &task.account_id);
            mark_refresh_all_item_finished(
                batch_id,
                &task.account_id,
                REFRESH_ALL_ITEM_STATUS_SUCCESS,
                None,
                Some(outcome),
            );
        }
        Err(err) => {
            record_token_refresh_failure(
                storage,
                &task.account_id,
                TOKEN_REFRESH_SOURCE_MANUAL_ALL_BATCH,
                &err,
            );
            let _ = mark_account_unavailable_for_auth_error(storage, &task.account_id, &err);
            mark_refresh_all_item_finished(
                batch_id,
                &task.account_id,
                REFRESH_ALL_ITEM_STATUS_FAILED,
                Some(err),
                None,
            );
        }
    }
}

fn refresh_batch_token_with_retries(
    storage: &Storage,
    task: &RefreshAllTokenTask,
) -> Result<TokenRefreshOutcome, String> {
    let mut last_err = None;
    for attempt in 0..REFRESH_ALL_MAX_ATTEMPTS {
        let mut token = storage
            .find_token_by_account_id(&task.account_id)
            .map_err(|err| err.to_string())?
            .unwrap_or_else(|| task.token.clone());
        if token.refresh_token.trim().is_empty() {
            return Err("missing refresh_token".to_string());
        }
        match refresh_and_persist_access_token(
            storage,
            &mut token,
            &task.issuer,
            &task.client_id,
            token_refresh_ahead_secs(),
        ) {
            Ok(outcome) => return Ok(outcome),
            Err(err) => {
                let retryable =
                    attempt + 1 < REFRESH_ALL_MAX_ATTEMPTS && should_retry_refresh_all_error(&err);
                last_err = Some(err);
                if retryable {
                    thread::sleep(Duration::from_millis(REFRESH_ALL_RETRY_DELAY_MS));
                    continue;
                }
                break;
            }
        }
    }
    Err(last_err.unwrap_or_else(|| "refresh failed".to_string()))
}

fn should_retry_refresh_all_error(err: &str) -> bool {
    if refresh_token_auth_error_reason_from_message(err).is_some() {
        return false;
    }
    let normalized = err.to_ascii_lowercase();
    normalized.contains("temporarily unavailable")
        || normalized.contains("timed out")
        || normalized.contains("timeout")
        || normalized.contains("connection")
        || normalized.contains("dns")
        || normalized.contains("error sending request")
        || normalized.contains("retry_after_client_rebuild")
        || normalized.contains("os error")
        || normalized.contains("status 5")
}

fn mark_refresh_all_item_running(batch_id: &str, account_id: &str) {
    let mut guard = lock_refresh_all_batch();
    let Some(state) = guard.as_mut() else {
        return;
    };
    if state.batch_id != batch_id || state.status != REFRESH_ALL_BATCH_STATUS_RUNNING {
        return;
    }
    if let Some(item) = state
        .results
        .iter_mut()
        .find(|item| item.account_id == account_id)
    {
        item.status = REFRESH_ALL_ITEM_STATUS_RUNNING.to_string();
        item.ok = false;
        item.message = None;
        item.started_at = Some(now_ts());
        item.finished_at = None;
    }
}

fn mark_refresh_all_item_finished(
    batch_id: &str,
    account_id: &str,
    status: &str,
    message: Option<String>,
    outcome: Option<TokenRefreshOutcome>,
) {
    let mut guard = lock_refresh_all_batch();
    let Some(state) = guard.as_mut() else {
        return;
    };
    if state.batch_id != batch_id {
        return;
    }
    if let Some(item) = state
        .results
        .iter_mut()
        .find(|item| item.account_id == account_id)
    {
        item.status = status.to_string();
        item.ok = status == REFRESH_ALL_ITEM_STATUS_SUCCESS;
        item.message = message;
        if let Some(outcome) = outcome {
            apply_refresh_outcome_to_item(item, outcome);
        }
        if item.started_at.is_none() {
            item.started_at = Some(now_ts());
        }
        item.finished_at = Some(now_ts());
    }
}

fn finish_refresh_all_batch(batch_id: &str, status: &str, unfinished_message: Option<String>) {
    let mut guard = lock_refresh_all_batch();
    let Some(state) = guard.as_mut() else {
        return;
    };
    if state.batch_id != batch_id || state.status != REFRESH_ALL_BATCH_STATUS_RUNNING {
        return;
    }
    if let Some(message) = unfinished_message {
        for item in state.results.iter_mut().filter(|item| {
            item.status == REFRESH_ALL_ITEM_STATUS_PENDING
                || item.status == REFRESH_ALL_ITEM_STATUS_RUNNING
        }) {
            item.status = REFRESH_ALL_ITEM_STATUS_FAILED.to_string();
            item.ok = false;
            item.message = Some(message.clone());
            if item.started_at.is_none() {
                item.started_at = Some(now_ts());
            }
            item.finished_at = Some(now_ts());
        }
    }
    state.status = if status == REFRESH_ALL_BATCH_STATUS_COMPLETED && state.has_unfinished_items() {
        REFRESH_ALL_BATCH_STATUS_FAILED.to_string()
    } else {
        status.to_string()
    };
    state.finished_at = Some(now_ts());
}

/// 函数 `logout_current_account`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn logout_current_account() -> Result<serde_json::Value, String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let current_account_id = get_persisted_app_setting(CURRENT_AUTH_ACCOUNT_ID_KEY);
    if let Some(account_id) = current_account_id.as_deref() {
        if storage
            .find_account_by_id(account_id)
            .map_err(|err| err.to_string())?
            .is_some()
        {
            storage
                .update_account_status(account_id, "inactive")
                .map_err(|err| format!("update account status failed: {err}"))?;
        }
    }
    set_current_auth_account_id(None)?;
    set_current_auth_mode(None)?;
    Ok(serde_json::json!({}))
}

/// 函数 `resolve_current_account_with_token`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - storage: 参数 storage
///
/// # 返回
/// 返回函数执行结果
fn resolve_current_account_with_token(
    storage: &Storage,
) -> Result<Option<(Account, Token)>, String> {
    let Some(account_id) = get_persisted_app_setting(CURRENT_AUTH_ACCOUNT_ID_KEY) else {
        return Ok(None);
    };
    let account = storage
        .find_account_by_id(&account_id)
        .map_err(|err| err.to_string())?;
    let token = storage
        .find_token_by_account_id(&account_id)
        .map_err(|err| err.to_string())?;
    match (account, token) {
        (Some(account), Some(token)) => Ok(Some((account, token))),
        _ => {
            set_current_auth_account_id(None)?;
            set_current_auth_mode(None)?;
            Ok(None)
        }
    }
}

/// 函数 `resolve_refresh_target`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - storage: 参数 storage
/// - previous_account_id: 参数 previous_account_id
///
/// # 返回
/// 返回函数执行结果
fn resolve_refresh_target(
    storage: &Storage,
    target_account_id: Option<&str>,
) -> Result<Option<(Account, Token)>, String> {
    let Some(target_account_id) = target_account_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return resolve_current_account_with_token(storage);
    };

    let accounts = storage.list_accounts().map_err(|err| err.to_string())?;
    let found = accounts.into_iter().find(|account| {
        account.id == target_account_id
            || normalize_chatgpt_account_id(account.chatgpt_account_id.as_deref()).as_deref()
                == Some(target_account_id)
            || normalize_workspace_id(account.workspace_id.as_deref()).as_deref()
                == Some(target_account_id)
    });
    let Some(account) = found else {
        return Ok(None);
    };
    let token = storage
        .find_token_by_account_id(&account.id)
        .map_err(|err| err.to_string())?;
    Ok(token.map(|token| (account, token)))
}

/// 函数 `current_account_payload`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - account: 参数 account
/// - token: 参数 token
/// - auth_mode: 参数 auth_mode
///
/// # 返回
/// 返回函数执行结果
fn current_account_payload(
    storage: &Storage,
    account: &Account,
    token: &Token,
    auth_mode: &str,
) -> CurrentAuthAccount {
    let claims = parse_id_token_claims(&token.access_token).ok();
    let plan_type_resolution = resolve_plan_type_resolution(token, claims.as_ref());
    let subscription = storage
        .find_account_subscription(&account.id)
        .ok()
        .flatten();
    let plan_type = subscription
        .as_ref()
        .and_then(|value| value.plan_type.clone())
        .or_else(|| {
            plan_type_resolution
                .as_ref()
                .map(|plan| plan.normalized.clone())
        })
        .unwrap_or_else(|| "unknown".to_string());
    CurrentAuthAccount {
        kind: auth_mode.to_string(),
        account_id: account.id.clone(),
        email: claims
            .as_ref()
            .and_then(|claims| claims.email.clone())
            .unwrap_or_else(|| account.label.clone()),
        plan_type,
        plan_type_raw: plan_type_resolution.and_then(|plan| plan.raw),
        has_subscription: subscription.as_ref().map(|value| value.has_subscription),
        subscription_plan: subscription
            .as_ref()
            .and_then(|value| value.plan_type.clone()),
        subscription_expires_at: subscription.as_ref().and_then(|value| value.expires_at),
        subscription_renews_at: subscription.as_ref().and_then(|value| value.renews_at),
        chatgpt_account_id: normalize_chatgpt_account_id(account.chatgpt_account_id.as_deref()),
        workspace_id: normalize_workspace_id(account.workspace_id.as_deref()),
        status: account.status.clone(),
    }
}

/// 函数 `resolve_plan_type`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - token: 参数 token
/// - parsed_claims: 参数 parsed_claims
///
/// # 返回
/// 返回函数执行结果
#[cfg(test)]
fn resolve_plan_type(
    token: &Token,
    parsed_claims: Option<&codexmanager_core::auth::IdTokenClaims>,
) -> Option<String> {
    resolve_plan_type_resolution(token, parsed_claims).map(|plan| plan.normalized)
}

/// 函数 `resolve_plan_type_raw`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - token: 参数 token
/// - parsed_claims: 参数 parsed_claims
///
/// # 返回
/// 返回函数执行结果
#[cfg(test)]
fn resolve_plan_type_raw(
    token: &Token,
    parsed_claims: Option<&codexmanager_core::auth::IdTokenClaims>,
) -> Option<String> {
    resolve_plan_type_resolution(token, parsed_claims).and_then(|plan| plan.raw)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedPlanType {
    normalized: String,
    raw: Option<String>,
}

/// 函数 `resolve_plan_type_resolution`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - token: 参数 token
/// - parsed_claims: 参数 parsed_claims
///
/// # 返回
/// 返回函数执行结果
fn resolve_plan_type_resolution(
    token: &Token,
    parsed_claims: Option<&codexmanager_core::auth::IdTokenClaims>,
) -> Option<ResolvedPlanType> {
    if let Some(claims) = parsed_claims {
        if let Some(plan_type) = claims
            .auth
            .as_ref()
            .and_then(|auth| auth.chatgpt_plan_type.clone())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            return normalize_plan_type(plan_type);
        }
    }
    if let Some(plan_type) = parse_id_token_claims(&token.access_token)
        .ok()
        .and_then(|claims| claims.auth.and_then(|auth| auth.chatgpt_plan_type))
        .and_then(normalize_plan_type)
    {
        return Some(plan_type);
    }
    parse_id_token_claims(&token.id_token)
        .ok()
        .and_then(|claims| claims.auth.and_then(|auth| auth.chatgpt_plan_type))
        .and_then(normalize_plan_type)
}

/// 函数 `normalize_plan_type`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - value: 参数 value
///
/// # 返回
/// 返回函数执行结果
fn normalize_plan_type(value: String) -> Option<ResolvedPlanType> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "free" | "go" | "plus" | "pro" | "team" | "business" | "enterprise" | "edu"
        | "education" => Some(ResolvedPlanType {
            normalized: match normalized.as_str() {
                "education" => "edu".to_string(),
                _ => normalized,
            },
            raw: None,
        }),
        "" => None,
        _ => Some(ResolvedPlanType {
            normalized: "unknown".to_string(),
            raw: Some(value.trim().to_string()),
        }),
    }
}

/// 函数 `set_current_auth_account_id`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn set_current_auth_account_id(account_id: Option<&str>) -> Result<(), String> {
    save_persisted_app_setting(CURRENT_AUTH_ACCOUNT_ID_KEY, account_id)
}

/// 函数 `set_current_auth_mode`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - crate: 参数 crate
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn set_current_auth_mode(auth_mode: Option<&str>) -> Result<(), String> {
    save_persisted_app_setting(CURRENT_AUTH_MODE_KEY, auth_mode)
}

/// 函数 `resolve_current_auth_mode`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - token: 参数 token
///
/// # 返回
/// 返回函数执行结果
fn resolve_current_auth_mode(token: &Token) -> String {
    get_persisted_app_setting(CURRENT_AUTH_MODE_KEY)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| infer_auth_mode_from_token(token).to_string())
}

/// 函数 `infer_auth_mode_from_token`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - token: 参数 token
///
/// # 返回
/// 返回函数执行结果
fn infer_auth_mode_from_token(token: &Token) -> &'static str {
    if token.id_token.trim() == token.access_token.trim() {
        AUTH_MODE_CHATGPT_AUTH_TOKENS
    } else {
        AUTH_MODE_CHATGPT
    }
}

#[cfg(test)]
#[path = "tests/auth_account_tests.rs"]
mod tests;
