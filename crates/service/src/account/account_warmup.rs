use codexmanager_core::storage::{now_ts, Account, Event, RequestLog, Storage, Token};
use crossbeam_channel::unbounded;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::Serialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use crate::account_status::mark_account_unavailable_for_auth_error;
use crate::apikey_models::read_managed_model_catalog_from_storage;
use crate::storage_helpers::open_storage;
use crate::usage_account_meta::workspace_header_for_account;
use crate::usage_token_refresh::{refresh_and_persist_access_token, token_refresh_ahead_secs};

const DEFAULT_WARMUP_MESSAGE: &str = "hi";
const FALLBACK_WARMUP_MESSAGE: &str = "你好";
const WARMUP_UPSTREAM_URL: &str = "https://chatgpt.com/backend-api/codex/responses";
const DEFAULT_WARMUP_MODEL: &str = "gpt-5.3-codex";
const WARMUP_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const WARMUP_TOTAL_TIMEOUT: Duration = Duration::from_secs(90);
const WARMUP_BATCH_STATUS_RUNNING: &str = "running";
const WARMUP_BATCH_STATUS_COMPLETED: &str = "completed";
const WARMUP_BATCH_STATUS_FAILED: &str = "failed";
const WARMUP_ITEM_STATUS_PENDING: &str = "pending";
const WARMUP_ITEM_STATUS_RUNNING: &str = "running";
const WARMUP_ITEM_STATUS_SUCCESS: &str = "success";
const WARMUP_ITEM_STATUS_FAILED: &str = "failed";
const WARMUP_ITEM_STATUS_SKIPPED: &str = "skipped";
const ENV_WARMUP_WORKERS: &str = "CODEXMANAGER_WARMUP_WORKERS";
const DEFAULT_WARMUP_WORKERS: usize = 4;

static WARMUP_BATCH: OnceLock<Mutex<Option<WarmupBatchState>>> = OnceLock::new();
static WARMUP_BATCH_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountWarmupResult {
    pub(crate) requested: usize,
    pub(crate) succeeded: usize,
    pub(crate) failed: usize,
    pub(crate) results: Vec<AccountWarmupItemResult>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountWarmupItemResult {
    pub(crate) account_id: String,
    pub(crate) account_name: String,
    pub(crate) status: String,
    pub(crate) ok: bool,
    pub(crate) message: String,
    pub(crate) started_at: Option<i64>,
    pub(crate) finished_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountWarmupBatchResponse {
    pub(crate) batch_id: Option<String>,
    pub(crate) status: String,
    pub(crate) total: usize,
    pub(crate) requested: usize,
    pub(crate) processed: usize,
    pub(crate) succeeded: usize,
    pub(crate) failed: usize,
    pub(crate) skipped: usize,
    pub(crate) started_at: Option<i64>,
    pub(crate) finished_at: Option<i64>,
    pub(crate) results: Vec<AccountWarmupItemResult>,
}

#[derive(Debug, Clone)]
struct WarmupBatchState {
    batch_id: String,
    status: String,
    total: usize,
    requested: usize,
    started_at: i64,
    finished_at: Option<i64>,
    results: Vec<AccountWarmupItemResult>,
}

#[derive(Debug, Clone)]
struct WarmupBatchTask {
    account: Account,
}

impl WarmupBatchState {
    fn to_response(&self) -> AccountWarmupBatchResponse {
        let succeeded = self
            .results
            .iter()
            .filter(|item| item.status == WARMUP_ITEM_STATUS_SUCCESS)
            .count();
        let failed = self
            .results
            .iter()
            .filter(|item| item.status == WARMUP_ITEM_STATUS_FAILED)
            .count();
        let skipped = self
            .results
            .iter()
            .filter(|item| item.status == WARMUP_ITEM_STATUS_SKIPPED)
            .count();
        AccountWarmupBatchResponse {
            batch_id: Some(self.batch_id.clone()),
            status: self.status.clone(),
            total: self.total,
            requested: self.requested,
            processed: succeeded.saturating_add(failed).saturating_add(skipped),
            succeeded,
            failed,
            skipped,
            started_at: Some(self.started_at),
            finished_at: self.finished_at,
            results: self.results.clone(),
        }
    }
}

/// 函数 `warmup_accounts`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-14
///
/// # 参数
/// - account_ids: 参数 account_ids
/// - message: 参数 message
///
/// # 返回
/// 返回函数执行结果
pub(crate) fn warmup_accounts(
    account_ids: Vec<String>,
    message: &str,
) -> Result<AccountWarmupResult, String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let mut accounts = resolve_target_accounts(&storage, &account_ids)?;
    if accounts.is_empty() {
        return Err("no account available for warmup".to_string());
    }

    let client = build_warmup_client()?;
    let warmup_message = normalize_warmup_message(message);
    let warmup_model = resolve_warmup_model_slug(&storage);
    let mut results = Vec::with_capacity(accounts.len());
    let mut succeeded = 0usize;

    for account in accounts.drain(..) {
        let item = warmup_single_account(
            &storage,
            &client,
            account,
            warmup_model.as_str(),
            warmup_message.as_str(),
        );
        if item.ok {
            succeeded += 1;
        }
        results.push(item);
    }

    Ok(AccountWarmupResult {
        requested: results.len(),
        succeeded,
        failed: results.len().saturating_sub(succeeded),
        results,
    })
}

fn resolve_target_accounts(
    storage: &Storage,
    account_ids: &[String],
) -> Result<Vec<Account>, String> {
    let accounts = storage
        .list_gateway_candidates()
        .map_err(|err| err.to_string())?
        .into_iter()
        .map(|(account, _token)| account)
        .collect::<Vec<_>>();

    if account_ids.is_empty() {
        return Ok(accounts);
    }

    let mut selected = Vec::new();
    for account_id in account_ids {
        let normalized = account_id.trim();
        if normalized.is_empty() {
            continue;
        }
        if let Some(account) = accounts.iter().find(|item| item.id == normalized) {
            selected.push(account.clone());
        }
    }
    Ok(selected)
}

fn normalize_warmup_message(message: &str) -> String {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        DEFAULT_WARMUP_MESSAGE.to_string()
    } else {
        trimmed.to_string()
    }
}

fn build_warmup_client() -> Result<Client, String> {
    let mut builder = Client::builder()
        .connect_timeout(WARMUP_CONNECT_TIMEOUT)
        .timeout(WARMUP_TOTAL_TIMEOUT)
        .pool_max_idle_per_host(4)
        .pool_idle_timeout(Some(Duration::from_secs(60)))
        .user_agent(crate::gateway::current_codex_user_agent());
    if let Some(proxy_url) = crate::gateway::current_upstream_proxy_url() {
        let proxy = reqwest::Proxy::all(proxy_url.as_str())
            .map_err(|err| format!("invalid upstream proxy url: {err}"))?;
        builder = builder.proxy(proxy);
    }
    builder
        .build()
        .map_err(|err| format!("build warmup client failed: {err}"))
}

fn warmup_single_account(
    storage: &Storage,
    client: &Client,
    account: Account,
    model_slug: &str,
    message: &str,
) -> AccountWarmupItemResult {
    let account_name = warmup_account_name(&account);
    let started_at = Instant::now();
    match load_account_token(storage, &account) {
        Ok(mut token) => {
            let mut outcome =
                send_warmup_request_with_fallback(client, &account, &token, model_slug, message);

            if let Err(err) = outcome.as_ref() {
                if should_retry_warmup_with_refresh(&token, err) {
                    let issuer = std::env::var("CODEXMANAGER_ISSUER")
                        .unwrap_or_else(|_| codexmanager_core::auth::DEFAULT_ISSUER.to_string());
                    let client_id = std::env::var("CODEXMANAGER_CLIENT_ID")
                        .unwrap_or_else(|_| codexmanager_core::auth::DEFAULT_CLIENT_ID.to_string());
                    outcome = refresh_and_persist_access_token(
                        storage,
                        &mut token,
                        &issuer,
                        &client_id,
                        token_refresh_ahead_secs(),
                    )
                    .and_then(|_| {
                        send_warmup_request_with_fallback(
                            client, &account, &token, model_slug, message,
                        )
                    });
                }
            }

            match outcome {
                Ok(ok_message) => {
                    persist_warmup_observability(
                        storage,
                        &account,
                        200,
                        None,
                        model_slug,
                        started_at.elapsed().as_millis() as i64,
                        ok_message.as_str(),
                    );
                    AccountWarmupItemResult {
                        account_id: account.id,
                        account_name,
                        status: WARMUP_ITEM_STATUS_SUCCESS.to_string(),
                        ok: true,
                        message: ok_message,
                        started_at: None,
                        finished_at: Some(now_ts()),
                    }
                }
                Err(err) => {
                    let _ = maybe_mark_account_auth_error(storage, &account.id, &err);
                    let status_code = extract_status_code_from_message(&err);
                    persist_warmup_observability(
                        storage,
                        &account,
                        status_code,
                        Some(err.as_str()),
                        model_slug,
                        started_at.elapsed().as_millis() as i64,
                        "预热失败",
                    );
                    AccountWarmupItemResult {
                        account_id: account.id,
                        account_name,
                        status: WARMUP_ITEM_STATUS_FAILED.to_string(),
                        ok: false,
                        message: err,
                        started_at: None,
                        finished_at: Some(now_ts()),
                    }
                }
            }
        }
        Err(err) => {
            let _ = maybe_mark_account_auth_error(storage, &account.id, &err);
            let status_code = extract_status_code_from_message(&err);
            persist_warmup_observability(
                storage,
                &account,
                status_code,
                Some(err.as_str()),
                model_slug,
                started_at.elapsed().as_millis() as i64,
                "预热失败",
            );
            AccountWarmupItemResult {
                account_id: account.id,
                account_name,
                status: WARMUP_ITEM_STATUS_FAILED.to_string(),
                ok: false,
                message: err,
                started_at: None,
                finished_at: Some(now_ts()),
            }
        }
    }
}

fn warmup_batch_slot() -> &'static Mutex<Option<WarmupBatchState>> {
    WARMUP_BATCH.get_or_init(|| Mutex::new(None))
}

fn lock_warmup_batch() -> std::sync::MutexGuard<'static, Option<WarmupBatchState>> {
    warmup_batch_slot()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn warmup_batch_id() -> String {
    let counter = WARMUP_BATCH_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("account-warmup-{}-{counter}", now_ts())
}

fn warmup_batch_item(
    account_id: String,
    account_name: String,
    status: &str,
    message: Option<String>,
) -> AccountWarmupItemResult {
    let finished_at = matches!(
        status,
        WARMUP_ITEM_STATUS_SUCCESS | WARMUP_ITEM_STATUS_FAILED | WARMUP_ITEM_STATUS_SKIPPED
    )
    .then(now_ts);
    AccountWarmupItemResult {
        account_id,
        account_name,
        status: status.to_string(),
        ok: status == WARMUP_ITEM_STATUS_SUCCESS,
        message: message.unwrap_or_default(),
        started_at: None,
        finished_at,
    }
}

fn warmup_account_name(account: &Account) -> String {
    let label = account.label.trim();
    if label.is_empty() {
        account.id.clone()
    } else {
        account.label.clone()
    }
}

pub(crate) fn warmup_accounts_batch_start(
    account_ids: Vec<String>,
    message: &str,
) -> Result<AccountWarmupBatchResponse, String> {
    {
        let guard = lock_warmup_batch();
        if let Some(state) = guard.as_ref() {
            if state.status == WARMUP_BATCH_STATUS_RUNNING {
                return Ok(state.to_response());
            }
        }
    }

    let (state, tasks, warmup_message, warmup_model) =
        build_warmup_batch_state(account_ids, message)?;
    let batch_id = state.batch_id.clone();
    {
        let mut guard = lock_warmup_batch();
        *guard = Some(state);
    }

    if tasks.is_empty() {
        finish_warmup_batch(&batch_id, WARMUP_BATCH_STATUS_COMPLETED, None);
        return warmup_accounts_batch_status(&batch_id);
    }

    let spawn_result = thread::Builder::new()
        .name("account-warmup-batch".to_string())
        .spawn({
            let batch_id = batch_id.clone();
            move || run_warmup_batch(batch_id, tasks, warmup_message, warmup_model)
        });
    if let Err(err) = spawn_result {
        finish_warmup_batch(
            &batch_id,
            WARMUP_BATCH_STATUS_FAILED,
            Some(format!("spawn warmup batch failed: {err}")),
        );
    }

    warmup_accounts_batch_status(&batch_id)
}

pub(crate) fn warmup_accounts_batch_status(
    batch_id: &str,
) -> Result<AccountWarmupBatchResponse, String> {
    let normalized_batch_id = batch_id.trim();
    if normalized_batch_id.is_empty() {
        return Err("batchId is required".to_string());
    }
    let guard = lock_warmup_batch();
    let Some(state) = guard.as_ref() else {
        return Err("warmup batch not found".to_string());
    };
    if state.batch_id != normalized_batch_id {
        return Err("warmup batch not found".to_string());
    }
    Ok(state.to_response())
}

fn build_warmup_batch_state(
    account_ids: Vec<String>,
    message: &str,
) -> Result<(WarmupBatchState, Vec<WarmupBatchTask>, String, String), String> {
    let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
    let candidates = storage
        .list_gateway_candidates()
        .map_err(|err| err.to_string())?
        .into_iter()
        .map(|(account, _token)| account)
        .collect::<Vec<_>>();
    let candidate_by_id = candidates
        .iter()
        .map(|account| (account.id.clone(), account.clone()))
        .collect::<HashMap<_, _>>();
    let selected_ids = normalize_warmup_account_ids(&account_ids);
    let all_account_by_id = if selected_ids.is_empty() {
        HashMap::new()
    } else {
        storage
            .list_accounts()
            .map_err(|err| err.to_string())?
            .into_iter()
            .map(|account| (account.id.clone(), account))
            .collect::<HashMap<_, _>>()
    };
    let accounts = if selected_ids.is_empty() {
        candidates
    } else {
        selected_ids
            .iter()
            .filter_map(|account_id| candidate_by_id.get(account_id).cloned())
            .collect::<Vec<_>>()
    };
    let total = if selected_ids.is_empty() {
        accounts.len()
    } else {
        selected_ids.len()
    };
    let batch_id = warmup_batch_id();
    let started_at = now_ts();
    let warmup_message = normalize_warmup_message(message);
    let warmup_model = resolve_warmup_model_slug(&storage);
    let mut results = Vec::with_capacity(total);
    let mut tasks = Vec::new();

    if selected_ids.is_empty() {
        for account in accounts {
            results.push(warmup_batch_item(
                account.id.clone(),
                warmup_account_name(&account),
                WARMUP_ITEM_STATUS_PENDING,
                None,
            ));
            tasks.push(WarmupBatchTask { account });
        }
    } else {
        for account_id in selected_ids {
            if let Some(account) = candidate_by_id.get(&account_id).cloned() {
                results.push(warmup_batch_item(
                    account.id.clone(),
                    warmup_account_name(&account),
                    WARMUP_ITEM_STATUS_PENDING,
                    None,
                ));
                tasks.push(WarmupBatchTask { account });
            } else {
                let account_name = all_account_by_id
                    .get(&account_id)
                    .map(warmup_account_name)
                    .unwrap_or_else(|| account_id.clone());
                results.push(warmup_batch_item(
                    account_id.clone(),
                    account_name,
                    WARMUP_ITEM_STATUS_SKIPPED,
                    Some("账号不可用或缺少有效 token".to_string()),
                ));
            }
        }
    }

    Ok((
        WarmupBatchState {
            batch_id,
            status: WARMUP_BATCH_STATUS_RUNNING.to_string(),
            total,
            requested: tasks.len(),
            started_at,
            finished_at: None,
            results,
        },
        tasks,
        warmup_message,
        warmup_model,
    ))
}

fn normalize_warmup_account_ids(account_ids: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for account_id in account_ids {
        let normalized = account_id.trim();
        if normalized.is_empty() || out.iter().any(|item: &String| item == normalized) {
            continue;
        }
        out.push(normalized.to_string());
    }
    out
}

fn warmup_worker_count(total: usize) -> usize {
    if total == 0 {
        return 0;
    }
    std::env::var(ENV_WARMUP_WORKERS)
        .ok()
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .unwrap_or(DEFAULT_WARMUP_WORKERS)
        .max(1)
        .min(total)
}

fn run_warmup_batch(
    batch_id: String,
    tasks: Vec<WarmupBatchTask>,
    warmup_message: String,
    warmup_model: String,
) {
    let result = run_warmup_batch_inner(&batch_id, tasks, warmup_message, warmup_model);
    match result {
        Ok(()) => finish_warmup_batch(&batch_id, WARMUP_BATCH_STATUS_COMPLETED, None),
        Err(err) => finish_warmup_batch(&batch_id, WARMUP_BATCH_STATUS_FAILED, Some(err)),
    }
}

fn run_warmup_batch_inner(
    batch_id: &str,
    tasks: Vec<WarmupBatchTask>,
    warmup_message: String,
    warmup_model: String,
) -> Result<(), String> {
    let worker_count = warmup_worker_count(tasks.len());
    if worker_count <= 1 {
        let storage = open_storage().ok_or_else(|| "storage unavailable".to_string())?;
        let client = build_warmup_client()?;
        for task in tasks {
            run_warmup_batch_task(
                batch_id,
                &storage,
                &client,
                task,
                warmup_model.as_str(),
                warmup_message.as_str(),
            );
        }
        return Ok(());
    }

    let (sender, receiver) = unbounded::<WarmupBatchTask>();
    for task in tasks {
        sender
            .send(task)
            .map_err(|_| "enqueue warmup task failed".to_string())?;
    }
    drop(sender);

    thread::scope(|scope| -> Result<(), String> {
        let mut handles = Vec::with_capacity(worker_count);
        for worker_index in 0..worker_count {
            let receiver = receiver.clone();
            let warmup_message = warmup_message.clone();
            let warmup_model = warmup_model.clone();
            handles.push(scope.spawn(move || {
                let storage = open_storage()
                    .ok_or_else(|| format!("warmup worker {worker_index} storage unavailable"))?;
                let client = build_warmup_client()?;
                while let Ok(task) = receiver.recv() {
                    run_warmup_batch_task(
                        batch_id,
                        &storage,
                        &client,
                        task,
                        warmup_model.as_str(),
                        warmup_message.as_str(),
                    );
                }
                Ok::<(), String>(())
            }));
        }

        for handle in handles {
            match handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(err)) => return Err(err),
                Err(_) => return Err("warmup worker panicked".to_string()),
            }
        }
        Ok(())
    })
}

fn run_warmup_batch_task(
    batch_id: &str,
    storage: &Storage,
    client: &Client,
    task: WarmupBatchTask,
    warmup_model: &str,
    warmup_message: &str,
) {
    mark_warmup_item_running(batch_id, &task.account.id);
    let item = warmup_single_account(storage, client, task.account, warmup_model, warmup_message);
    mark_warmup_item_finished(batch_id, item);
}

fn mark_warmup_item_running(batch_id: &str, account_id: &str) {
    let mut guard = lock_warmup_batch();
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
        item.status = WARMUP_ITEM_STATUS_RUNNING.to_string();
        item.started_at = Some(now_ts());
        item.message.clear();
    }
}

fn mark_warmup_item_finished(batch_id: &str, item: AccountWarmupItemResult) {
    let mut guard = lock_warmup_batch();
    let Some(state) = guard.as_mut() else {
        return;
    };
    if state.batch_id != batch_id {
        return;
    }
    if let Some(existing) = state
        .results
        .iter_mut()
        .find(|existing| existing.account_id == item.account_id)
    {
        let started_at = existing.started_at;
        *existing = AccountWarmupItemResult {
            started_at,
            finished_at: Some(now_ts()),
            ..item
        };
    }
}

fn finish_warmup_batch(batch_id: &str, status: &str, error: Option<String>) {
    let mut guard = lock_warmup_batch();
    let Some(state) = guard.as_mut() else {
        return;
    };
    if state.batch_id != batch_id {
        return;
    }
    state.status = status.to_string();
    state.finished_at = Some(now_ts());
    if let Some(error) = error {
        for item in state
            .results
            .iter_mut()
            .filter(|item| {
                item.status == WARMUP_ITEM_STATUS_PENDING
                    || item.status == WARMUP_ITEM_STATUS_RUNNING
            })
        {
            item.status = WARMUP_ITEM_STATUS_FAILED.to_string();
            item.ok = false;
            item.message = error.clone();
            item.finished_at = Some(now_ts());
        }
    }
}

fn persist_warmup_observability(
    storage: &Storage,
    account: &Account,
    status_code: i64,
    error: Option<&str>,
    model_slug: &str,
    duration_ms: i64,
    event_message: &str,
) {
    let created_at = now_ts();
    let trace_id = format!("warmup-{}-{created_at}", account.id);
    let _ = storage.insert_request_log(&RequestLog {
        trace_id: Some(trace_id),
        account_id: Some(account.id.clone()),
        initial_account_id: Some(account.id.clone()),
        attempted_account_ids_json: Some(format!(r#"["{}"]"#, account.id)),
        request_path: "/internal/account/warmup".to_string(),
        original_path: Some("/internal/account/warmup".to_string()),
        adapted_path: Some("/internal/account/warmup".to_string()),
        method: "POST".to_string(),
        request_type: Some("account_warmup".to_string()),
        gateway_mode: None,
        transparent_mode: None,
        enhanced_mode: None,
        model: Some(model_slug.to_string()),
        upstream_url: Some(WARMUP_UPSTREAM_URL.to_string()),
        status_code: Some(status_code),
        duration_ms: Some(duration_ms.max(0)),
        first_response_ms: None,
        error: error.map(str::to_string),
        created_at,
        ..RequestLog::default()
    });
    let _ = storage.insert_event(&Event {
        account_id: Some(account.id.clone()),
        event_type: "account_warmup".to_string(),
        message: match error {
            Some(err) => {
                format!("{event_message}; model={model_slug}; status={status_code}; error={err}")
            }
            None => format!("{event_message}; model={model_slug}; status={status_code}"),
        },
        created_at,
    });
}

fn extract_status_code_from_message(message: &str) -> i64 {
    let marker = "status=";
    let Some(index) = message.find(marker) else {
        return 500;
    };
    let digits: String = message[index + marker.len()..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    digits.parse::<i64>().unwrap_or(500)
}

fn load_account_token(storage: &Storage, account: &Account) -> Result<Token, String> {
    storage
        .find_token_by_account_id(&account.id)
        .map_err(|err| err.to_string())?
        .ok_or_else(|| "missing token".to_string())
}

fn resolve_warmup_model_slug(storage: &Storage) -> String {
    read_managed_model_catalog_from_storage(storage)
        .ok()
        .and_then(|catalog| {
            catalog
                .items
                .into_iter()
                .find(|item| item.model.supported_in_api)
                .map(|item| item.model.slug)
        })
        .filter(|slug| !slug.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_WARMUP_MODEL.to_string())
}

fn send_warmup_request_with_fallback(
    client: &Client,
    account: &Account,
    token: &Token,
    model_slug: &str,
    message: &str,
) -> Result<String, String> {
    let primary = send_warmup_request(client, account, token, model_slug, message);
    match primary {
        Ok(()) => Ok("已发送预热消息".to_string()),
        Err(primary_err) if message == DEFAULT_WARMUP_MESSAGE => {
            send_warmup_request(client, account, token, model_slug, FALLBACK_WARMUP_MESSAGE)
                .map(|_| "已发送预热消息".to_string())
                .map_err(|fallback_err| format!("{primary_err}; fallback={fallback_err}"))
        }
        Err(err) => Err(err),
    }
}

fn should_retry_warmup_with_refresh(token: &Token, err: &str) -> bool {
    if token.refresh_token.trim().is_empty() {
        return false;
    }
    let normalized = err.to_ascii_lowercase();
    normalized.contains("status=401")
        || normalized.contains("status=403")
        || normalized.contains("auth error")
        || normalized.contains("unauthorized")
        || normalized.contains("forbidden")
}

fn send_warmup_request(
    client: &Client,
    account: &Account,
    token: &Token,
    model_slug: &str,
    message: &str,
) -> Result<(), String> {
    let body = json!({
        "model": model_slug,
        "instructions": "",
        "input": [{
            "type": "message",
            "role": "user",
            "content": [{
                "type": "input_text",
                "text": message
            }]
        }],
        "stream": true,
        "store": false
    });

    let headers = build_warmup_headers(account, token.access_token.as_str())?;
    let response = client
        .post(WARMUP_UPSTREAM_URL)
        .headers(headers)
        .json(&body)
        .send()
        .map_err(|err| format!("warmup request failed: {err}"))?;

    if response.status().is_success() {
        return Ok(());
    }

    let status = response.status();
    let headers = response.headers().clone();
    let body_text = response.text().unwrap_or_default();
    Err(summarize_warmup_error(
        status.as_u16(),
        &headers,
        &body_text,
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        build_warmup_headers, resolve_target_accounts, resolve_warmup_model_slug,
        should_retry_warmup_with_refresh, DEFAULT_WARMUP_MODEL,
    };
    use crate::apikey_models::save_managed_model_catalog_with_storage;
    use codexmanager_core::rpc::types::{
        ManagedModelCatalogEntry, ManagedModelCatalogResult, ModelInfo,
    };
    use codexmanager_core::storage::{now_ts, Account, Storage, Token};

    fn make_model(slug: &str, sort_index: i64, supported_in_api: bool) -> ManagedModelCatalogEntry {
        ManagedModelCatalogEntry {
            model: ModelInfo {
                slug: slug.to_string(),
                display_name: slug.to_string(),
                supported_in_api,
                ..ModelInfo::default()
            },
            sort_index,
            ..ManagedModelCatalogEntry::default()
        }
    }

    #[test]
    fn resolve_warmup_model_slug_uses_first_supported_model_from_catalog_order() {
        let storage = Storage::open_in_memory().expect("open in-memory storage");
        storage.init().expect("init in-memory storage");
        save_managed_model_catalog_with_storage(
            &storage,
            &ManagedModelCatalogResult {
                items: vec![
                    make_model("gpt-hidden", 0, false),
                    make_model("gpt-latest", 1, true),
                    make_model("gpt-older", 2, true),
                ],
                ..ManagedModelCatalogResult::default()
            },
        )
        .expect("save model catalog");

        assert_eq!(resolve_warmup_model_slug(&storage), "gpt-latest");
    }

    #[test]
    fn resolve_warmup_model_slug_falls_back_when_catalog_missing() {
        let storage = Storage::open_in_memory().expect("open in-memory storage");
        storage.init().expect("init in-memory storage");
        assert_eq!(resolve_warmup_model_slug(&storage), DEFAULT_WARMUP_MODEL);
    }

    #[test]
    fn should_retry_warmup_with_refresh_only_for_auth_errors_with_refresh_token() {
        let mut token = Token {
            account_id: "account-1".to_string(),
            id_token: String::new(),
            access_token: String::new(),
            refresh_token: "refresh-token".to_string(),
            api_key_access_token: None,
            last_refresh: 0,
        };

        assert!(should_retry_warmup_with_refresh(
            &token,
            "status=401 body=Unauthorized"
        ));
        assert!(!should_retry_warmup_with_refresh(
            &token,
            "status=500 body=server error"
        ));

        token.refresh_token.clear();
        assert!(!should_retry_warmup_with_refresh(
            &token,
            "status=401 body=Unauthorized"
        ));
    }

    #[test]
    fn resolve_target_accounts_only_returns_gateway_available_accounts() {
        let storage = Storage::open_in_memory().expect("open in-memory storage");
        storage.init().expect("init in-memory storage");
        let now = now_ts();

        for (id, status) in [
            ("acc-active", "active"),
            ("acc-unavailable", "unavailable"),
            ("acc-disabled", "disabled"),
            ("acc-banned", "banned"),
            ("acc-inactive", "inactive"),
        ] {
            storage
                .insert_account(&Account {
                    id: id.to_string(),
                    label: id.to_string(),
                    issuer: "issuer".to_string(),
                    chatgpt_account_id: None,
                    workspace_id: None,
                    group_name: None,
                    sort: 0,
                    status: status.to_string(),
                    created_at: now,
                    updated_at: now,
                })
                .expect("insert account");
            storage
                .insert_token(&Token {
                    account_id: id.to_string(),
                    id_token: "id-token".to_string(),
                    access_token: "access-token".to_string(),
                    refresh_token: "refresh-token".to_string(),
                    api_key_access_token: None,
                    last_refresh: now,
                })
                .expect("insert token");
        }

        let all_targets = resolve_target_accounts(&storage, &[]).expect("resolve all targets");
        assert_eq!(all_targets.len(), 1);
        assert_eq!(all_targets[0].id, "acc-active");

        let selected_targets = resolve_target_accounts(
            &storage,
            &[
                "acc-unavailable".to_string(),
                "acc-active".to_string(),
                "acc-disabled".to_string(),
            ],
        )
        .expect("resolve selected targets");
        assert_eq!(selected_targets.len(), 1);
        assert_eq!(selected_targets[0].id, "acc-active");
    }

    #[test]
    fn build_warmup_headers_omits_non_codex_headers() {
        let account = Account {
            id: "acc-1".to_string(),
            label: "acc-1".to_string(),
            issuer: "issuer".to_string(),
            chatgpt_account_id: None,
            workspace_id: None,
            group_name: None,
            sort: 0,
            status: "active".to_string(),
            created_at: 0,
            updated_at: 0,
        };

        let headers = build_warmup_headers(&account, "bearer-token").expect("build warmup headers");

        assert!(headers.get("version").is_none());
        assert!(headers.get("openai-organization").is_none());
        assert!(headers.get("openai-project").is_none());
        assert!(headers.get("client_version").is_none());
    }
}

fn build_warmup_headers(account: &Account, bearer: &str) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        header_value(&format!("Bearer {bearer}"))?,
    );
    headers.insert(
        reqwest::header::ACCEPT,
        HeaderValue::from_static("text/event-stream"),
    );
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        reqwest::header::USER_AGENT,
        header_value(&crate::gateway::current_codex_user_agent())?,
    );
    headers.insert(
        HeaderName::from_static("originator"),
        header_value(&crate::gateway::current_wire_originator())?,
    );

    if let Some(residency_requirement) = crate::gateway::current_residency_requirement() {
        headers.insert(
            HeaderName::from_static("x-openai-internal-codex-residency"),
            header_value(&residency_requirement)?,
        );
    }
    if let Some(account_header) = workspace_header_for_account(account) {
        headers.insert(
            HeaderName::from_static("chatgpt-account-id"),
            header_value(&account_header)?,
        );
    }

    Ok(headers)
}

fn header_value(value: &str) -> Result<HeaderValue, String> {
    HeaderValue::from_str(value).map_err(|err| format!("invalid header value: {err}"))
}

fn summarize_warmup_error(status: u16, headers: &HeaderMap, body: &str) -> String {
    let body_hint =
        crate::gateway::summarize_upstream_error_hint_from_body(status, body.as_bytes())
            .or_else(|| {
                let trimmed = body.trim();
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            })
            .unwrap_or_else(|| "unknown error".to_string());

    let request_id = first_header(headers, &["x-request-id", "x-oai-request-id"]);
    let auth_error = first_header(headers, &["x-openai-authorization-error"]);
    let cf_ray = first_header(headers, &["cf-ray"]);

    let mut details = Vec::new();
    if let Some(value) = request_id {
        details.push(format!("request id: {value}"));
    }
    if let Some(value) = auth_error {
        details.push(format!("auth error: {value}"));
    }
    if let Some(value) = cf_ray {
        details.push(format!("cf-ray: {value}"));
    }

    if details.is_empty() {
        format!("status={status} body={body_hint}")
    } else {
        format!("status={status} body={body_hint}, {}", details.join(", "))
    }
}

fn first_header(headers: &HeaderMap, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        headers
            .get(*name)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    })
}

fn maybe_mark_account_auth_error(
    storage: &Storage,
    account_id: &str,
    err: &str,
) -> Result<(), String> {
    if err.to_ascii_lowercase().contains("auth error")
        || err.to_ascii_lowercase().contains("status=401")
        || err.to_ascii_lowercase().contains("status=403")
    {
        let _ = mark_account_unavailable_for_auth_error(storage, account_id, err);
    }
    Ok(())
}
