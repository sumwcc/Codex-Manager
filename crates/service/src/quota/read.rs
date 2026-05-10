use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Duration, Local, LocalResult, TimeZone};
use codexmanager_core::rpc::types::{
    QuotaAggregateApiOverviewResult, QuotaApiKeyModelUsageItem, QuotaApiKeyOverviewResult,
    QuotaApiKeyUsageItem, QuotaApiKeyUsageResult, QuotaModelUsageItem, QuotaModelUsageResult,
    QuotaOpenAiAccountOverviewResult, QuotaOverviewResult, QuotaRefreshSourceResult,
    QuotaRefreshSourcesResult, QuotaSourceListResult, QuotaSourceSummary, QuotaTodayUsageResult,
};
use codexmanager_core::storage::{Account, AggregateApi, ApiKey, UsageSnapshotRecord};
use serde_json::Value;

use super::model_pricing;
use crate::{refresh_aggregate_api_balance, storage_helpers::open_storage, usage_refresh};

#[derive(Debug, Clone, Default)]
pub(crate) struct QuotaRefreshSourcesInput {
    pub(crate) kinds: Vec<String>,
    pub(crate) source_ids: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct BalanceSnapshot {
    remaining: Option<f64>,
    total: Option<f64>,
    used: Option<f64>,
    unit: Option<String>,
}

fn local_day_bounds_ts() -> Result<(i64, i64), String> {
    let now = Local::now();
    let today = now.date_naive();
    let start_naive = today
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| "build local start-of-day failed".to_string())?;
    let tomorrow_naive = (today + Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| "build local end-of-day failed".to_string())?;
    let start = match Local.from_local_datetime(&start_naive) {
        LocalResult::Single(value) => value.timestamp(),
        LocalResult::Ambiguous(a, b) => a.timestamp().min(b.timestamp()),
        LocalResult::None => now.timestamp(),
    };
    let end = match Local.from_local_datetime(&tomorrow_naive) {
        LocalResult::Single(value) => value.timestamp(),
        LocalResult::Ambiguous(a, b) => a.timestamp().max(b.timestamp()),
        LocalResult::None => start + 86_400,
    };
    Ok((start, end.max(start)))
}

fn token_total(input: i64, cached: i64, output: i64) -> i64 {
    input.saturating_sub(cached).saturating_add(output).max(0)
}

fn parse_balance_snapshot(api: &AggregateApi) -> BalanceSnapshot {
    let Some(raw) = api
        .last_balance_json
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return BalanceSnapshot::default();
    };
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return BalanceSnapshot::default();
    };
    BalanceSnapshot {
        remaining: value.get("remaining").and_then(Value::as_f64),
        total: value.get("total").and_then(Value::as_f64),
        used: value.get("used").and_then(Value::as_f64),
        unit: value
            .get("unit")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
    }
}

fn balance_usd(api: &AggregateApi) -> Option<f64> {
    parse_balance_snapshot(api)
        .remaining
        .filter(|value| value.is_finite() && *value > 0.0)
}

fn remaining_percent(used_percent: Option<f64>) -> Option<f64> {
    used_percent.map(|used| (100.0 - used.clamp(0.0, 100.0)).max(0.0))
}

fn average_percent(values: impl Iterator<Item = Option<f64>>) -> Option<i64> {
    let mut count = 0_i64;
    let mut total = 0.0_f64;
    for value in values.flatten() {
        total += value;
        count += 1;
    }
    (count > 0).then(|| (total / count as f64).round() as i64)
}

fn is_low_quota(usage: Option<&UsageSnapshotRecord>) -> bool {
    let Some(usage) = usage else {
        return false;
    };
    for remain in [
        remaining_percent(usage.used_percent),
        remaining_percent(usage.secondary_used_percent),
    ]
    .into_iter()
    .flatten()
    {
        if remain > 0.0 && remain <= 20.0 {
            return true;
        }
    }
    false
}

fn account_is_available(account: &Account) -> bool {
    matches!(account.status.as_str(), "active" | "available")
}

fn api_display_name(api: &AggregateApi) -> String {
    api.supplier_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(api.url.as_str())
        .to_string()
}

fn key_display_name(key: &ApiKey) -> String {
    key.name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(key.id.as_str())
        .to_string()
}

pub(crate) fn read_quota_overview() -> Result<QuotaOverviewResult, String> {
    let storage = open_storage().ok_or_else(|| "open storage failed".to_string())?;
    let _price_rules = model_pricing::load_enabled_price_rules(&storage)?;

    let api_keys = storage
        .list_api_keys()
        .map_err(|err| format!("list api keys failed: {err}"))?;
    let quota_limits = storage
        .list_api_key_quota_limits()
        .map_err(|err| format!("list api key quota limits failed: {err}"))?;
    let usage_by_key = storage
        .summarize_request_token_stats_by_key()
        .map_err(|err| format!("summarize api key usage failed: {err}"))?;
    let usage_map = usage_by_key
        .iter()
        .map(|item| (item.key_id.as_str(), item))
        .collect::<HashMap<_, _>>();

    let mut total_limit_tokens = 0_i64;
    let mut total_used_tokens = 0_i64;
    let mut total_remaining_tokens = 0_i64;
    let mut estimated_cost_usd = 0.0_f64;
    for key in &api_keys {
        let used = usage_map
            .get(key.id.as_str())
            .map(|item| item.total_tokens.max(0))
            .unwrap_or(0);
        total_used_tokens = total_used_tokens.saturating_add(used);
        estimated_cost_usd += usage_map
            .get(key.id.as_str())
            .map(|item| item.estimated_cost_usd.max(0.0))
            .unwrap_or(0.0);
        if let Some(limit) = quota_limits
            .get(key.id.as_str())
            .copied()
            .filter(|value| *value > 0)
        {
            total_limit_tokens = total_limit_tokens.saturating_add(limit);
            total_remaining_tokens =
                total_remaining_tokens.saturating_add(limit.saturating_sub(used));
        }
    }

    let aggregate_apis = storage
        .list_aggregate_apis()
        .map_err(|err| format!("list aggregate APIs failed: {err}"))?;
    let mut aggregate_ok_count = 0_i64;
    let mut aggregate_error_count = 0_i64;
    let mut total_balance_usd = 0.0_f64;
    let mut has_balance = false;
    let mut last_refreshed_at: Option<i64> = None;
    for api in &aggregate_apis {
        match api.last_balance_status.as_deref() {
            Some("success") => aggregate_ok_count += 1,
            Some("error" | "failed") => aggregate_error_count += 1,
            _ => {}
        }
        if let Some(balance) = balance_usd(api) {
            total_balance_usd += balance;
            has_balance = true;
        }
        if let Some(ts) = api.last_balance_at {
            last_refreshed_at = Some(last_refreshed_at.map_or(ts, |current| current.max(ts)));
        }
    }

    let accounts = storage
        .list_accounts()
        .map_err(|err| format!("list accounts failed: {err}"))?;
    let usage_items = storage
        .latest_usage_snapshots_by_account()
        .map_err(|err| format!("list usage snapshots failed: {err}"))?;
    let usage_map = usage_items
        .iter()
        .map(|item| (item.account_id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let last_usage_at = usage_items.iter().map(|item| item.captured_at).max();
    let available_count = accounts
        .iter()
        .filter(|account| account_is_available(account))
        .count() as i64;
    let low_quota_count = accounts
        .iter()
        .filter(|account| is_low_quota(usage_map.get(account.id.as_str()).copied()))
        .count() as i64;

    let (day_start, day_end) = local_day_bounds_ts()?;
    let today = storage
        .summarize_request_token_stats_between(day_start, day_end)
        .map_err(|err| format!("summarize today token usage failed: {err}"))?;
    let today_input = today.input_tokens.max(0);
    let today_cached = today.cached_input_tokens.max(0);
    let today_output = today.output_tokens.max(0);

    Ok(QuotaOverviewResult {
        api_key: QuotaApiKeyOverviewResult {
            key_count: api_keys.len() as i64,
            limited_key_count: quota_limits.len() as i64,
            total_limit_tokens: (total_limit_tokens > 0).then_some(total_limit_tokens),
            total_used_tokens,
            total_remaining_tokens: (total_limit_tokens > 0).then_some(total_remaining_tokens),
            estimated_cost_usd: estimated_cost_usd.max(0.0),
        },
        aggregate_api: QuotaAggregateApiOverviewResult {
            source_count: aggregate_apis.len() as i64,
            enabled_balance_query_count: aggregate_apis
                .iter()
                .filter(|api| api.balance_query_enabled)
                .count() as i64,
            ok_count: aggregate_ok_count,
            error_count: aggregate_error_count,
            total_balance_usd: has_balance.then_some(total_balance_usd.max(0.0)),
            last_refreshed_at,
        },
        openai_account: QuotaOpenAiAccountOverviewResult {
            account_count: accounts.len() as i64,
            available_count,
            low_quota_count,
            primary_remain_percent: average_percent(
                usage_items
                    .iter()
                    .map(|item| remaining_percent(item.used_percent)),
            ),
            secondary_remain_percent: average_percent(
                usage_items
                    .iter()
                    .map(|item| remaining_percent(item.secondary_used_percent)),
            ),
            last_refreshed_at: last_usage_at,
        },
        today_usage: QuotaTodayUsageResult {
            input_tokens: today_input,
            cached_input_tokens: today_cached,
            output_tokens: today_output,
            reasoning_output_tokens: today.reasoning_output_tokens.max(0),
            total_tokens: token_total(today_input, today_cached, today_output),
            estimated_cost_usd: today.estimated_cost_usd.max(0.0),
        },
    })
}

pub(crate) fn read_quota_model_usage(
    start_ts: Option<i64>,
    end_ts: Option<i64>,
) -> Result<QuotaModelUsageResult, String> {
    let storage = open_storage().ok_or_else(|| "open storage failed".to_string())?;
    let price_rules = model_pricing::load_enabled_price_rules(&storage)?;
    let usage = storage
        .summarize_request_token_stats_by_model(start_ts, end_ts)
        .map_err(|err| format!("summarize token usage by model failed: {err}"))?;
    let quota_limits = storage
        .list_api_key_quota_limits()
        .map_err(|err| format!("list api key quota limits failed: {err}"))?;
    let key_usage = storage
        .summarize_request_token_stats_by_key()
        .map_err(|err| format!("summarize key usage failed: {err}"))?;
    let api_key_remaining_tokens = quota_limits
        .iter()
        .map(|(key_id, limit)| {
            let used = key_usage
                .iter()
                .find(|item| item.key_id == *key_id)
                .map(|item| item.total_tokens.max(0))
                .unwrap_or(0);
            limit.saturating_sub(used)
        })
        .sum::<i64>();

    let aggregate_balance_usd = storage
        .list_aggregate_apis()
        .map_err(|err| format!("list aggregate APIs failed: {err}"))?
        .iter()
        .filter_map(balance_usd)
        .sum::<f64>();
    let aggregate_balance_usd = (aggregate_balance_usd > 0.0).then_some(aggregate_balance_usd);

    let accounts = storage
        .list_accounts()
        .map_err(|err| format!("list accounts failed: {err}"))?;
    let usage_items = storage
        .latest_usage_snapshots_by_account()
        .map_err(|err| format!("list usage snapshots failed: {err}"))?;

    let openai_available_account_count = accounts
        .iter()
        .filter(|account| account_is_available(account))
        .count() as i64;
    let openai_primary_remain_percent = average_percent(
        usage_items
            .iter()
            .map(|item| remaining_percent(item.used_percent)),
    );
    let openai_secondary_remain_percent = average_percent(
        usage_items
            .iter()
            .map(|item| remaining_percent(item.secondary_used_percent)),
    );

    Ok(QuotaModelUsageResult {
        items: usage
            .into_iter()
            .map(|item| {
                let cost = model_pricing::estimate_cost_with_rules(
                    &price_rules,
                    Some(item.model.as_str()),
                    item.input_tokens,
                    item.cached_input_tokens,
                    item.output_tokens,
                );
                let aggregate_estimated_remaining_tokens =
                    aggregate_balance_usd.and_then(|balance| {
                        model_pricing::estimate_remaining_tokens_from_usd_with_rules(
                            &price_rules,
                            &item.model,
                            balance,
                        )
                    });
                QuotaModelUsageItem {
                    model: item.model,
                    provider: cost.provider,
                    input_tokens: item.input_tokens,
                    cached_input_tokens: item.cached_input_tokens,
                    output_tokens: item.output_tokens,
                    reasoning_output_tokens: item.reasoning_output_tokens,
                    total_tokens: item.total_tokens,
                    estimated_cost_usd: cost.cost_usd,
                    price_status: cost.price_status.to_string(),
                    api_key_remaining_tokens: (api_key_remaining_tokens > 0)
                        .then_some(api_key_remaining_tokens),
                    aggregate_estimated_remaining_tokens,
                    aggregate_balance_usd,
                    openai_available_account_count,
                    openai_primary_remain_percent,
                    openai_secondary_remain_percent,
                    openai_estimated_remaining_tokens: None,
                    openai_estimate_enabled: false,
                }
            })
            .collect(),
    })
}

pub(crate) fn read_quota_api_key_usage() -> Result<QuotaApiKeyUsageResult, String> {
    let storage = open_storage().ok_or_else(|| "open storage failed".to_string())?;
    let price_rules = model_pricing::load_enabled_price_rules(&storage)?;
    let api_keys = storage
        .list_api_keys()
        .map_err(|err| format!("list api keys failed: {err}"))?;
    let quota_limits = storage
        .list_api_key_quota_limits()
        .map_err(|err| format!("list api key quota limits failed: {err}"))?;
    let usage_by_key = storage
        .summarize_request_token_stats_by_key()
        .map_err(|err| format!("summarize api key usage failed: {err}"))?;
    let usage_map = usage_by_key
        .iter()
        .map(|item| (item.key_id.as_str(), item))
        .collect::<HashMap<_, _>>();
    let model_usage = storage
        .summarize_request_token_stats_by_key_and_model(None, None)
        .map_err(|err| format!("summarize api key model usage failed: {err}"))?;
    let mut models_by_key: BTreeMap<String, Vec<QuotaApiKeyModelUsageItem>> = BTreeMap::new();
    for item in model_usage {
        let cost = model_pricing::estimate_cost_with_rules(
            &price_rules,
            Some(item.model.as_str()),
            item.input_tokens,
            item.cached_input_tokens,
            item.output_tokens,
        );
        models_by_key
            .entry(item.key_id)
            .or_default()
            .push(QuotaApiKeyModelUsageItem {
                model: item.model,
                input_tokens: item.input_tokens,
                cached_input_tokens: item.cached_input_tokens,
                output_tokens: item.output_tokens,
                reasoning_output_tokens: item.reasoning_output_tokens,
                total_tokens: item.total_tokens,
                estimated_cost_usd: cost.cost_usd,
                price_status: cost.price_status.to_string(),
            });
    }

    Ok(QuotaApiKeyUsageResult {
        items: api_keys
            .into_iter()
            .map(|key| {
                let used = usage_map
                    .get(key.id.as_str())
                    .map(|item| item.total_tokens.max(0))
                    .unwrap_or(0);
                let limit = quota_limits.get(key.id.as_str()).copied();
                QuotaApiKeyUsageItem {
                    key_id: key.id.clone(),
                    name: key.name,
                    model_slug: key.model_slug,
                    quota_limit_tokens: limit,
                    used_tokens: used,
                    remaining_tokens: limit.map(|value| value.saturating_sub(used)),
                    estimated_cost_usd: usage_map
                        .get(key.id.as_str())
                        .map(|item| item.estimated_cost_usd.max(0.0))
                        .unwrap_or(0.0),
                    models: models_by_key.remove(key.id.as_str()).unwrap_or_default(),
                }
            })
            .collect(),
    })
}

pub(crate) fn read_quota_source_list() -> Result<QuotaSourceListResult, String> {
    let storage = open_storage().ok_or_else(|| "open storage failed".to_string())?;
    let _price_rules = model_pricing::load_enabled_price_rules(&storage)?;
    let mut items = Vec::new();

    let api_keys = storage
        .list_api_keys()
        .map_err(|err| format!("list api keys failed: {err}"))?;
    let quota_limits = storage
        .list_api_key_quota_limits()
        .map_err(|err| format!("list api key quota limits failed: {err}"))?;
    let usage_by_key = storage
        .summarize_request_token_stats_by_key()
        .map_err(|err| format!("summarize api key usage failed: {err}"))?;
    let usage_map = usage_by_key
        .iter()
        .map(|item| (item.key_id.as_str(), item))
        .collect::<HashMap<_, _>>();
    for key in api_keys {
        let used = usage_map
            .get(key.id.as_str())
            .map(|item| item.total_tokens.max(0))
            .unwrap_or(0);
        let limit = quota_limits.get(key.id.as_str()).copied();
        items.push(QuotaSourceSummary {
            id: key.id.clone(),
            kind: "api_key".to_string(),
            name: key_display_name(&key),
            status: key.status,
            metric_kind: "token_limit".to_string(),
            remaining: limit.map(|value| value.saturating_sub(used) as f64),
            total: limit.map(|value| value as f64),
            used: Some(used as f64),
            unit: Some("token".to_string()),
            models: key.model_slug.into_iter().collect(),
            provider: None,
            captured_at: key.last_used_at,
            error: None,
        });
    }

    for api in storage
        .list_aggregate_apis()
        .map_err(|err| format!("list aggregate APIs failed: {err}"))?
    {
        let balance = parse_balance_snapshot(&api);
        let status = match api.last_balance_status.as_deref() {
            Some("success") => "ok",
            Some("error" | "failed") => "error",
            _ if api.balance_query_enabled => "unknown",
            _ => "warning",
        };
        items.push(QuotaSourceSummary {
            id: api.id.clone(),
            kind: "aggregate_api".to_string(),
            name: api_display_name(&api),
            status: status.to_string(),
            metric_kind: "money_balance".to_string(),
            remaining: balance.remaining,
            total: balance.total,
            used: balance.used,
            unit: Some(balance.unit.unwrap_or_else(|| "USD".to_string())),
            models: api.model_override.into_iter().collect(),
            provider: Some(api.provider_type),
            captured_at: api.last_balance_at,
            error: api.last_balance_error,
        });
    }

    for account in storage
        .list_accounts()
        .map_err(|err| format!("list accounts failed: {err}"))?
    {
        let usage = storage
            .latest_usage_snapshot_for_account(&account.id)
            .map_err(|err| format!("read account usage failed: {err}"))?;
        let remaining = usage
            .as_ref()
            .and_then(|item| remaining_percent(item.used_percent));
        let used = usage.as_ref().and_then(|item| item.used_percent);
        items.push(QuotaSourceSummary {
            id: account.id.clone(),
            kind: "openai_account".to_string(),
            name: account.label.clone(),
            status: if account_is_available(&account) {
                "ok".to_string()
            } else {
                account.status
            },
            metric_kind: "window_percent".to_string(),
            remaining,
            total: Some(100.0),
            used,
            unit: Some("percent".to_string()),
            models: Vec::new(),
            provider: Some("openai".to_string()),
            captured_at: usage.map(|item| item.captured_at),
            error: None,
        });
    }

    Ok(QuotaSourceListResult { items })
}

pub(crate) fn refresh_quota_sources(
    input: QuotaRefreshSourcesInput,
) -> Result<QuotaRefreshSourcesResult, String> {
    let storage = open_storage().ok_or_else(|| "open storage failed".to_string())?;
    let kinds = if input.kinds.is_empty() {
        HashSet::from(["aggregate_api".to_string(), "openai_account".to_string()])
    } else {
        input.kinds.into_iter().collect::<HashSet<_>>()
    };
    let source_ids = input.source_ids.into_iter().collect::<HashSet<_>>();
    let mut items = Vec::new();

    if kinds.contains("aggregate_api") {
        let aggregate_apis = storage
            .list_aggregate_apis()
            .map_err(|err| format!("list aggregate APIs failed: {err}"))?;
        for api in aggregate_apis {
            if !source_ids.is_empty() && !source_ids.contains(api.id.as_str()) {
                continue;
            }
            if !api.balance_query_enabled {
                continue;
            }
            let result = refresh_aggregate_api_balance(api.id.as_str());
            items.push(QuotaRefreshSourceResult {
                id: api.id,
                kind: "aggregate_api".to_string(),
                ok: result.is_ok(),
                error: result.err(),
            });
        }
    }

    if kinds.contains("openai_account") {
        let accounts = storage
            .list_accounts()
            .map_err(|err| format!("list accounts failed: {err}"))?;
        for account in accounts {
            if !source_ids.is_empty() && !source_ids.contains(account.id.as_str()) {
                continue;
            }
            let result = usage_refresh::refresh_usage_for_account(account.id.as_str());
            items.push(QuotaRefreshSourceResult {
                id: account.id,
                kind: "openai_account".to_string(),
                ok: result.is_ok(),
                error: result.err(),
            });
        }
    }

    Ok(QuotaRefreshSourcesResult { items })
}
