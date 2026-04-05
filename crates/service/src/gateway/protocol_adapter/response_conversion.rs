use serde_json::Value;

mod adapter_dispatch;
mod gemini;
mod json_conversion;
mod openai_chat;
mod openai_completions;
mod sse_conversion;
mod stream_events;
mod tool_mapping;

type ToolNameRestoreMap = super::ToolNameRestoreMap;

pub(super) use self::openai_completions::convert_openai_completions_stream_chunk;

/// 函数 `convert_openai_chat_stream_chunk`
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
#[allow(dead_code)]
pub(super) fn convert_openai_chat_stream_chunk(value: &Value) -> Option<Value> {
    openai_chat::convert_openai_chat_stream_chunk(value)
}

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
    adapter: super::ResponseAdapter,
    upstream_content_type: Option<&str>,
    body: &[u8],
    tool_name_restore_map: Option<&ToolNameRestoreMap>,
) -> Result<(Vec<u8>, &'static str), String> {
    adapter_dispatch::adapt_upstream_response(
        adapter,
        upstream_content_type,
        body,
        tool_name_restore_map,
    )
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
    adapter_dispatch::build_anthropic_error_body(message)
}

/// 函数 `is_response_completed_event_type`
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
pub(super) fn is_response_completed_event_type(kind: &str) -> bool {
    stream_events::is_response_completed_event_type(kind)
}

/// 函数 `parse_openai_sse_event_value`
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
pub(super) fn parse_openai_sse_event_value(data: &str, event_name: Option<&str>) -> Option<Value> {
    stream_events::parse_openai_sse_event_value(data, event_name)
}

/// 函数 `stream_event_response_id`
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
pub(super) fn stream_event_response_id(value: &Value) -> String {
    stream_events::stream_event_response_id(value)
}

/// 函数 `stream_event_model`
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
pub(super) fn stream_event_model(value: &Value) -> String {
    stream_events::stream_event_model(value)
}

/// 函数 `stream_event_created`
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
pub(super) fn stream_event_created(value: &Value) -> i64 {
    stream_events::stream_event_created(value)
}

/// 函数 `convert_openai_chat_stream_chunk_with_tool_name_restore_map`
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
pub(super) fn convert_openai_chat_stream_chunk_with_tool_name_restore_map(
    value: &Value,
    tool_name_restore_map: Option<&ToolNameRestoreMap>,
) -> Option<Value> {
    openai_chat::convert_openai_chat_stream_chunk_with_tool_name_restore_map(
        value,
        tool_name_restore_map,
    )
}

pub(super) fn build_gemini_error_body(message: &str) -> Vec<u8> {
    gemini::build_gemini_error_body(message)
}
