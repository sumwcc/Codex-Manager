use codexmanager_core::auth::extract_token_exp;
use codexmanager_core::storage::{now_ts, Storage, Token};

use crate::auth_tokens::obtain_api_key;
use crate::usage_http::refresh_access_token;

pub(crate) const DEFAULT_TOKEN_REFRESH_AHEAD_SECS: i64 = 3600;

/// 函数 `refresh_and_persist_access_token`
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
pub(crate) fn refresh_and_persist_access_token(
    storage: &Storage,
    token: &mut Token,
    issuer: &str,
    client_id: &str,
    refresh_ahead_secs: i64,
) -> Result<(), String> {
    let refreshed = refresh_access_token(issuer, client_id, &token.refresh_token)?;
    token.access_token = refreshed.access_token;

    if let Some(refresh_token) = refreshed.refresh_token {
        token.refresh_token = refresh_token;
    }

    if let Some(id_token) = refreshed.id_token {
        token.id_token = id_token.clone();
        if let Ok(api_key) = obtain_api_key(issuer, client_id, &id_token) {
            token.api_key_access_token = Some(api_key);
        }
    }

    token.last_refresh = now_ts();
    storage.insert_token(token).map_err(|err| err.to_string())?;
    let access_exp = extract_token_exp(&token.access_token);
    let next_refresh_at = next_refresh_at_from_token(token, refresh_ahead_secs);
    let _ = storage.update_token_refresh_schedule(&token.account_id, access_exp, next_refresh_at);
    Ok(())
}

fn next_refresh_at_from_token(token: &Token, ahead_secs: i64) -> Option<i64> {
    let access_refresh_at = extract_token_exp(&token.access_token)
        .map(|exp| exp.saturating_sub(ahead_secs));
    let refresh_refresh_at = extract_token_exp(&token.refresh_token)
        .map(|exp| exp.saturating_sub(ahead_secs));

    match (access_refresh_at, refresh_refresh_at) {
        (Some(access_at), Some(refresh_at)) => Some(access_at.min(refresh_at)),
        (Some(access_at), None) => Some(access_at),
        (None, Some(refresh_at)) => Some(refresh_at),
        (None, None) => None,
    }
}
