use serde_json::json;

use crate::gateway::request_helpers::is_html_content_type;

use super::super::ResponseAdapter;
use super::gemini::{
    convert_gemini_json_to_sse, convert_openai_json_to_gemini, convert_openai_sse_to_gemini,
    convert_openai_sse_to_gemini_json,
};
use super::json_conversion::convert_openai_json_to_anthropic;
use super::openai_chat::{
    convert_openai_json_to_chat_completions, convert_openai_sse_to_chat_completions_json,
};
use super::openai_completions::{
    convert_openai_json_to_completions, convert_openai_sse_to_completions_json,
};
use super::sse_conversion::{
    convert_anthropic_json_to_sse, convert_anthropic_sse_to_json, convert_openai_sse_to_anthropic,
};
use super::ToolNameRestoreMap;

/// 函数 `adapt_upstream_response`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 返回函数执行结果
pub(super) fn adapt_upstream_response(
    adapter: ResponseAdapter,
    upstream_content_type: Option<&str>,
    body: &[u8],
    tool_name_restore_map: Option<&ToolNameRestoreMap>,
) -> Result<(Vec<u8>, &'static str), String> {
    match adapter {
        ResponseAdapter::Passthrough => Ok((body.to_vec(), "application/octet-stream")),
        ResponseAdapter::AnthropicJson => {
            reject_html_challenge(upstream_content_type)?;
            if is_sse_payload(upstream_content_type, body) {
                let (anthropic_sse, _) =
                    convert_openai_sse_to_anthropic(body, tool_name_restore_map)?;
                return convert_anthropic_sse_to_json(&anthropic_sse);
            }
            convert_openai_json_to_anthropic(body, tool_name_restore_map)
        }
        ResponseAdapter::AnthropicSse => {
            reject_html_challenge(upstream_content_type)?;
            if is_json_payload(upstream_content_type) {
                let (anthropic_json, _) =
                    convert_openai_json_to_anthropic(body, tool_name_restore_map)?;
                return convert_anthropic_json_to_sse(&anthropic_json);
            }
            convert_openai_sse_to_anthropic(body, tool_name_restore_map)
        }
        ResponseAdapter::GeminiJson => {
            reject_html_challenge(upstream_content_type)?;
            if is_sse_payload(upstream_content_type, body) {
                return convert_openai_sse_to_gemini_json(body, tool_name_restore_map);
            }
            convert_openai_json_to_gemini(body, tool_name_restore_map)
        }
        ResponseAdapter::GeminiSse => {
            reject_html_challenge(upstream_content_type)?;
            if is_json_payload(upstream_content_type) {
                let (gemini_json, _) = convert_openai_json_to_gemini(body, tool_name_restore_map)?;
                return convert_gemini_json_to_sse(&gemini_json);
            }
            convert_openai_sse_to_gemini(body, tool_name_restore_map)
        }
        ResponseAdapter::OpenAIChatCompletionsJson | ResponseAdapter::OpenAIChatCompletionsSse => {
            reject_html_challenge(upstream_content_type)?;
            if is_sse_payload(upstream_content_type, body) {
                return convert_openai_sse_to_chat_completions_json(body, tool_name_restore_map);
            }
            convert_openai_json_to_chat_completions(body, tool_name_restore_map)
        }
        ResponseAdapter::OpenAICompletionsJson | ResponseAdapter::OpenAICompletionsSse => {
            reject_html_challenge(upstream_content_type)?;
            if is_sse_payload(upstream_content_type, body) {
                return convert_openai_sse_to_completions_json(body);
            }
            convert_openai_json_to_completions(body)
        }
    }
}

/// 函数 `build_anthropic_error_body`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - super: 参数 super
///
/// # 返回
/// 返回函数执行结果
pub(super) fn build_anthropic_error_body(message: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "type": "error",
        "error": {
            "type": "api_error",
            "message": message,
        }
    }))
    .unwrap_or_else(|_| {
        b"{\"type\":\"error\",\"error\":{\"type\":\"api_error\",\"message\":\"unknown error\"}}"
            .to_vec()
    })
}

/// 函数 `reject_html_challenge`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - upstream_content_type: 参数 upstream_content_type
///
/// # 返回
/// 返回函数执行结果
fn reject_html_challenge(upstream_content_type: Option<&str>) -> Result<(), String> {
    if upstream_content_type.is_some_and(is_html_content_type) {
        Err("upstream returned html challenge".to_string())
    } else {
        Ok(())
    }
}

/// 函数 `is_json_payload`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - upstream_content_type: 参数 upstream_content_type
///
/// # 返回
/// 返回函数执行结果
fn is_json_payload(upstream_content_type: Option<&str>) -> bool {
    upstream_content_type
        .map(|value| {
            value
                .trim()
                .to_ascii_lowercase()
                .starts_with("application/json")
        })
        .unwrap_or(false)
}

/// 函数 `is_sse_payload`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - upstream_content_type: 参数 upstream_content_type
/// - body: 参数 body
///
/// # 返回
/// 返回函数执行结果
fn is_sse_payload(upstream_content_type: Option<&str>, body: &[u8]) -> bool {
    upstream_content_type
        .map(|value| value.to_ascii_lowercase().starts_with("text/event-stream"))
        .unwrap_or(false)
        || looks_like_sse_payload(body)
}

/// 函数 `looks_like_sse_payload`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - body: 参数 body
///
/// # 返回
/// 返回函数执行结果
fn looks_like_sse_payload(body: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(body) else {
        return false;
    };
    let trimmed = text.trim_start();
    trimmed.starts_with("data:")
        || trimmed.starts_with("event:")
        || text.contains("\ndata:")
        || text.contains("\nevent:")
}
