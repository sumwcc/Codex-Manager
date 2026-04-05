use crate::apikey_profile::{
    is_gemini_generate_content_request_path, PROTOCOL_ANTHROPIC_NATIVE, PROTOCOL_GEMINI_NATIVE,
    PROTOCOL_OPENAI_COMPAT,
};

use super::request_mapping;
use super::{AdaptedGatewayRequest, ResponseAdapter, ToolNameRestoreMap};

/// 函数 `adapt_request_for_protocol`
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
pub(crate) fn adapt_request_for_protocol(
    protocol_type: &str,
    path: &str,
    body: Vec<u8>,
) -> Result<AdaptedGatewayRequest, String> {
    if protocol_type == PROTOCOL_OPENAI_COMPAT
        && (path == "/v1/chat/completions" || path.starts_with("/v1/chat/completions?"))
    {
        let (adapted_body, request_stream, tool_name_restore_map) =
            request_mapping::convert_openai_chat_completions_request(&body)?;
        return Ok(AdaptedGatewayRequest {
            path: rewrite_responses_path(path, "/v1/chat/completions"),
            body: adapted_body,
            response_adapter: if request_stream {
                ResponseAdapter::OpenAIChatCompletionsSse
            } else {
                ResponseAdapter::OpenAIChatCompletionsJson
            },
            tool_name_restore_map,
        });
    }

    if protocol_type == PROTOCOL_OPENAI_COMPAT
        && (path == "/v1/completions" || path.starts_with("/v1/completions?"))
    {
        let (chat_body, _) = request_mapping::convert_openai_completions_request(&body)?;
        let (adapted_body, request_stream, tool_name_restore_map) =
            request_mapping::convert_openai_chat_completions_request(&chat_body)?;
        return Ok(AdaptedGatewayRequest {
            path: rewrite_responses_path(path, "/v1/completions"),
            body: adapted_body,
            response_adapter: if request_stream {
                ResponseAdapter::OpenAICompletionsSse
            } else {
                ResponseAdapter::OpenAICompletionsJson
            },
            tool_name_restore_map,
        });
    }

    if protocol_type == PROTOCOL_ANTHROPIC_NATIVE
        && (path == "/v1/messages" || path.starts_with("/v1/messages?"))
    {
        let (adapted_body, request_stream, tool_name_restore_map) =
            request_mapping::convert_anthropic_messages_request(&body)?;
        return Ok(AdaptedGatewayRequest {
            // 说明：non-stream 也统一走 /v1/responses。
            // 在部分账号/环境下 /v1/responses/compact 更容易触发 challenge 或非预期拦截。
            path: "/v1/responses".to_string(),
            body: adapted_body,
            response_adapter: if request_stream {
                ResponseAdapter::AnthropicSse
            } else {
                ResponseAdapter::AnthropicJson
            },
            tool_name_restore_map,
        });
    }

    if protocol_type == PROTOCOL_GEMINI_NATIVE && is_gemini_generate_content_request_path(path) {
        let (adapted_body, request_stream, tool_name_restore_map) =
            request_mapping::convert_gemini_generate_content_request(path, &body)?;
        return Ok(AdaptedGatewayRequest {
            path: "/v1/responses".to_string(),
            body: adapted_body,
            response_adapter: if request_stream {
                ResponseAdapter::GeminiSse
            } else {
                ResponseAdapter::GeminiJson
            },
            tool_name_restore_map,
        });
    }

    Ok(AdaptedGatewayRequest {
        path: path.to_string(),
        body,
        response_adapter: ResponseAdapter::Passthrough,
        tool_name_restore_map: ToolNameRestoreMap::new(),
    })
}

/// 函数 `rewrite_responses_path`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// - path: 参数 path
/// - prefix: 参数 prefix
///
/// # 返回
/// 返回函数执行结果
fn rewrite_responses_path(path: &str, prefix: &str) -> String {
    if let Some(suffix) = path.strip_prefix(prefix) {
        format!("/v1/responses{suffix}")
    } else {
        "/v1/responses".to_string()
    }
}
