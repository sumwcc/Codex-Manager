use codexmanager_core::rpc::types::{JsonRpcRequest, JsonRpcResponse};

use crate::quota::read::{self, QuotaRefreshSourcesInput};

fn string_array_param(req: &JsonRpcRequest, key: &str) -> Vec<String> {
    req.params
        .as_ref()
        .and_then(|value| value.get(key))
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn try_handle(req: &JsonRpcRequest) -> Option<JsonRpcResponse> {
    let result = match req.method.as_str() {
        "quota/overview" => super::value_or_error(read::read_quota_overview()),
        "quota/modelUsage" => {
            let start_ts = super::i64_param(req, "startTs");
            let end_ts = super::i64_param(req, "endTs");
            super::value_or_error(read::read_quota_model_usage(start_ts, end_ts))
        }
        "quota/apiKeyUsage" => super::value_or_error(read::read_quota_api_key_usage()),
        "quota/sourceList" => super::value_or_error(read::read_quota_source_list()),
        "quota/refreshSources" => {
            super::value_or_error(read::refresh_quota_sources(QuotaRefreshSourcesInput {
                kinds: string_array_param(req, "kinds"),
                source_ids: string_array_param(req, "sourceIds"),
            }))
        }
        _ => return None,
    };

    Some(super::response(req, result))
}
