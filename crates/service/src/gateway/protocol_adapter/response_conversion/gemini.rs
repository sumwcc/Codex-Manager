use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

use super::json_conversion::{
    extract_function_call_arguments_raw, parse_tool_arguments_as_object,
    summarize_special_response_item_text,
};
use super::stream_events::{is_response_completed_event_type, parse_openai_sse_event_value};
use super::tool_mapping::restore_openai_tool_name;
use super::ToolNameRestoreMap;

#[derive(Default)]
struct PendingToolCall {
    call_id: Option<String>,
    name: Option<String>,
    arguments: String,
}

#[derive(Default)]
struct GeminiSseAggregationState {
    response_id: Option<String>,
    model: Option<String>,
    prompt_tokens: i64,
    output_tokens: i64,
    total_tokens: Option<i64>,
    output_text: String,
    pending_tool_calls: BTreeMap<i64, PendingToolCall>,
    emitted_tool_calls: BTreeMap<i64, String>,
}

pub(super) fn build_gemini_error_body(message: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "error": { "code": 500, "message": message, "status": "INTERNAL" }
    }))
    .unwrap_or_else(|_| {
        b"{\"error\":{\"code\":500,\"message\":\"unknown error\",\"status\":\"INTERNAL\"}}".to_vec()
    })
}

pub(super) fn convert_openai_json_to_gemini(
    body: &[u8],
    tool_name_restore_map: Option<&ToolNameRestoreMap>,
) -> Result<(Vec<u8>, &'static str), String> {
    let value: Value =
        serde_json::from_slice(body).map_err(|_| "invalid upstream json response".to_string())?;
    if let Some(error_payload) = map_openai_error_to_gemini(&value) {
        return serde_json::to_vec(&error_payload)
            .map(|bytes| (bytes, "application/json"))
            .map_err(|err| format!("serialize gemini error response failed: {err}"));
    }
    let payload = if value.get("choices").is_some() {
        build_gemini_response_from_chat_completions(&value, tool_name_restore_map)?
    } else {
        build_gemini_response_from_responses(&value, tool_name_restore_map)?
    };
    serde_json::to_vec(&payload)
        .map(|bytes| (bytes, "application/json"))
        .map_err(|err| format!("serialize gemini response failed: {err}"))
}

pub(super) fn convert_gemini_json_to_sse(
    body: &[u8],
) -> Result<(Vec<u8>, &'static str), String> {
    let value: Value =
        serde_json::from_slice(body).map_err(|_| "invalid gemini json response".to_string())?;
    let mut out = String::new();
    append_gemini_sse_event(&mut out, &value);
    Ok((out.into_bytes(), "text/event-stream"))
}

pub(super) fn convert_openai_sse_to_gemini_json(
    body: &[u8],
    tool_name_restore_map: Option<&ToolNameRestoreMap>,
) -> Result<(Vec<u8>, &'static str), String> {
    let payload = collect_gemini_response_from_openai_sse(body, tool_name_restore_map)?;
    serde_json::to_vec(&payload)
        .map(|bytes| (bytes, "application/json"))
        .map_err(|err| format!("serialize gemini response failed: {err}"))
}

pub(super) fn convert_openai_sse_to_gemini(
    body: &[u8],
    tool_name_restore_map: Option<&ToolNameRestoreMap>,
) -> Result<(Vec<u8>, &'static str), String> {
    let text = std::str::from_utf8(body).map_err(|_| "invalid upstream sse body".to_string())?;
    let mut out = String::new();
    let mut state = GeminiSseAggregationState::default();
    for frame in split_sse_frames(text) {
        let mut event_name = None;
        let mut data_lines = Vec::new();
        for line in frame {
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if let Some(rest) = trimmed.strip_prefix("event:") {
                event_name = Some(rest.trim().to_string());
            } else if let Some(rest) = trimmed.strip_prefix("data:") {
                data_lines.push(rest.trim_start().to_string());
            }
        }
        if data_lines.is_empty() {
            continue;
        }
        let data = data_lines.join("\n");
        if data.trim() == "[DONE]" {
            break;
        }
        let Some(value) = parse_openai_sse_event_value(&data, event_name.as_deref()) else {
            continue;
        };
        for chunk in convert_openai_event_to_gemini_chunks(&value, &mut state, tool_name_restore_map)
        {
            append_gemini_sse_event(&mut out, &chunk);
        }
    }
    Ok((out.into_bytes(), "text/event-stream"))
}

fn map_openai_error_to_gemini(value: &Value) -> Option<Value> {
    let error = value.get("error")?.as_object()?;
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("upstream request failed");
    let error_type = error
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("server_error");
    let (code, status) = match error_type {
        "authentication_error" => (401, "UNAUTHENTICATED"),
        "permission_error" => (403, "PERMISSION_DENIED"),
        "rate_limit_error" => (429, "RESOURCE_EXHAUSTED"),
        "invalid_request_error" | "not_found_error" => (400, "INVALID_ARGUMENT"),
        _ => (500, "INTERNAL"),
    };
    Some(json!({
        "error": { "code": code, "message": message, "status": status }
    }))
}

fn build_gemini_response_from_chat_completions(
    value: &Value,
    tool_name_restore_map: Option<&ToolNameRestoreMap>,
) -> Result<Value, String> {
    let model = value.get("model").and_then(Value::as_str).unwrap_or("unknown");
    let response_id = value
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("resp_codexmanager");
    let choice = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .ok_or_else(|| "missing upstream choice".to_string())?;
    let message = choice
        .get("message")
        .and_then(Value::as_object)
        .ok_or_else(|| "missing upstream message object".to_string())?;

    let mut parts = Vec::new();
    if let Some(text) = extract_openai_chat_text_content(message.get("content").unwrap_or(&Value::Null))
    {
        if !text.trim().is_empty() {
            parts.push(json!({ "text": text }));
        }
    }
    if let Some(tool_calls) = message.get("tool_calls").and_then(Value::as_array) {
        for tool_call in tool_calls {
            let Some(tool_obj) = tool_call.as_object() else {
                continue;
            };
            let Some(name) = tool_obj
                .get("function")
                .and_then(|item| item.get("name"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            let name = restore_openai_tool_name(name, tool_name_restore_map);
            let args = tool_obj
                .get("function")
                .and_then(|item| item.get("arguments"))
                .and_then(Value::as_str)
                .map(parse_tool_arguments_as_object)
                .unwrap_or_else(|| json!({}));
            let mut function_call = serde_json::Map::new();
            function_call.insert("name".to_string(), Value::String(name));
            function_call.insert("args".to_string(), args);
            if let Some(call_id) = tool_obj
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                function_call.insert("id".to_string(), Value::String(call_id.to_string()));
            }
            parts.push(json!({ "functionCall": Value::Object(function_call) }));
        }
    }

    Ok(build_gemini_response(
        response_id,
        model,
        parts,
        Some(map_chat_finish_reason_to_gemini(
            choice
                .get("finish_reason")
                .and_then(Value::as_str)
                .unwrap_or("stop"),
        )),
        value.get("usage").and_then(Value::as_object),
    ))
}

fn build_gemini_response_from_responses(
    value: &Value,
    tool_name_restore_map: Option<&ToolNameRestoreMap>,
) -> Result<Value, String> {
    let model = value.get("model").and_then(Value::as_str).unwrap_or("unknown");
    let response_id = value
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("resp_codexmanager");
    let mut parts = Vec::new();
    if let Some(output_items) = value.get("output").and_then(Value::as_array) {
        for output_item in output_items {
            let Some(item_obj) = output_item.as_object() else {
                continue;
            };
            let item_type = item_obj.get("type").and_then(Value::as_str).unwrap_or_default();
            match item_type {
                "message" => {
                    if let Some(content) = item_obj.get("content").and_then(Value::as_array) {
                        for block in content {
                            let Some(block_obj) = block.as_object() else {
                                continue;
                            };
                            if matches!(
                                block_obj.get("type").and_then(Value::as_str),
                                Some("output_text" | "text")
                            ) {
                                if let Some(text) = block_obj
                                    .get("text")
                                    .and_then(Value::as_str)
                                    .map(str::trim)
                                    .filter(|value| !value.is_empty())
                                {
                                    parts.push(json!({ "text": text }));
                                }
                            }
                        }
                    }
                }
                "function_call" | "custom_tool_call" => {
                    if let Some(part) =
                        map_response_function_call_to_gemini_part(item_obj, tool_name_restore_map)
                    {
                        parts.push(part);
                    }
                }
                _ => {
                    if let Some(summary) = summarize_special_response_item_text(item_obj) {
                        let trimmed = summary.trim();
                        if !trimmed.is_empty() {
                            parts.push(json!({ "text": trimmed }));
                        }
                    }
                }
            }
        }
    }
    if parts.is_empty() {
        if let Some(output_text) = value
            .get("output_text")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            parts.push(json!({ "text": output_text }));
        }
    }
    Ok(build_gemini_response(
        response_id,
        model,
        parts,
        Some(map_responses_finish_reason_to_gemini(value)),
        value.get("usage").and_then(Value::as_object),
    ))
}

fn build_gemini_response(
    response_id: &str,
    model: &str,
    parts: Vec<Value>,
    finish_reason: Option<&str>,
    usage: Option<&Map<String, Value>>,
) -> Value {
    let candidate = json!({
        "index": 0,
        "content": { "role": "model", "parts": parts },
        "finishReason": finish_reason.unwrap_or("STOP"),
    });
    let mut payload = serde_json::Map::new();
    payload.insert("responseId".to_string(), Value::String(response_id.to_string()));
    payload.insert("modelVersion".to_string(), Value::String(model.to_string()));
    payload.insert("candidates".to_string(), Value::Array(vec![candidate]));
    if let Some(usage_metadata) = build_gemini_usage_metadata(usage) {
        payload.insert("usageMetadata".to_string(), usage_metadata);
    }
    Value::Object(payload)
}

fn build_gemini_usage_metadata(usage: Option<&Map<String, Value>>) -> Option<Value> {
    let usage = usage?;
    let prompt = extract_usage_i64(usage, &["input_tokens", "prompt_tokens"])?;
    let candidates =
        extract_usage_i64(usage, &["output_tokens", "completion_tokens"]).unwrap_or(0);
    let total = extract_usage_i64(usage, &["total_tokens"]).unwrap_or(prompt + candidates);
    Some(json!({
        "promptTokenCount": prompt,
        "candidatesTokenCount": candidates,
        "totalTokenCount": total,
    }))
}

fn extract_usage_i64(usage: &Map<String, Value>, paths: &[&str]) -> Option<i64> {
    for path in paths {
        let mut cursor = None;
        for (index, segment) in path.split('.').enumerate() {
            cursor = if index == 0 {
                usage.get(segment)
            } else {
                cursor
                    .and_then(Value::as_object)
                    .and_then(|map| map.get(segment))
            };
        }
        if let Some(value) = cursor.and_then(Value::as_i64) {
            return Some(value);
        }
    }
    None
}

fn map_response_function_call_to_gemini_part(
    item_obj: &Map<String, Value>,
    tool_name_restore_map: Option<&ToolNameRestoreMap>,
) -> Option<Value> {
    let name = item_obj
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let name = restore_openai_tool_name(name, tool_name_restore_map);
    let args = extract_function_call_arguments_raw(item_obj)
        .map(|raw| parse_tool_arguments_as_object(&raw))
        .unwrap_or_else(|| json!({}));
    let mut function_call = serde_json::Map::new();
    function_call.insert("name".to_string(), Value::String(name));
    function_call.insert("args".to_string(), args);
    if let Some(call_id) = item_obj
        .get("call_id")
        .or_else(|| item_obj.get("id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        function_call.insert("id".to_string(), Value::String(call_id.to_string()));
    }
    Some(json!({ "functionCall": Value::Object(function_call) }))
}

fn extract_openai_chat_text_content(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(text) => Some(text.to_string()),
        Value::Array(items) => {
            let mut parts = Vec::new();
            for item in items {
                let Some(item_obj) = item.as_object() else {
                    continue;
                };
                if matches!(item_obj.get("type").and_then(Value::as_str), Some("text")) {
                    if let Some(text) = item_obj.get("text").and_then(Value::as_str) {
                        parts.push(text.to_string());
                    }
                }
            }
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(""))
            }
        }
        _ => None,
    }
}

fn map_chat_finish_reason_to_gemini(reason: &str) -> &'static str {
    match reason {
        "length" => "MAX_TOKENS",
        _ => "STOP",
    }
}

fn map_responses_finish_reason_to_gemini(value: &Value) -> &'static str {
    match value
        .get("incomplete_details")
        .and_then(Value::as_object)
        .and_then(|details| details.get("reason"))
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "max_output_tokens" => "MAX_TOKENS",
        _ => "STOP",
    }
}

fn append_gemini_sse_event(buffer: &mut String, payload: &Value) {
    let data = serde_json::to_string(payload).unwrap_or_else(|_| "{}".to_string());
    buffer.push_str("data: ");
    buffer.push_str(&data);
    buffer.push_str("\n\n");
}

fn split_sse_frames(text: &str) -> Vec<Vec<String>> {
    let mut frames = Vec::new();
    let mut current = Vec::new();
    for line in text.lines() {
        current.push(line.to_string());
        if line.trim().is_empty() {
            frames.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        frames.push(current);
    }
    frames
}

fn collect_gemini_response_from_openai_sse(
    body: &[u8],
    tool_name_restore_map: Option<&ToolNameRestoreMap>,
) -> Result<Value, String> {
    let text = std::str::from_utf8(body).map_err(|_| "invalid upstream sse body".to_string())?;
    let mut state = GeminiSseAggregationState::default();
    let mut completed_response = None;
    for frame in split_sse_frames(text) {
        let mut event_name = None;
        let mut data_lines = Vec::new();
        for line in frame {
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if let Some(rest) = trimmed.strip_prefix("event:") {
                event_name = Some(rest.trim().to_string());
            } else if let Some(rest) = trimmed.strip_prefix("data:") {
                data_lines.push(rest.trim_start().to_string());
            }
        }
        if data_lines.is_empty() {
            continue;
        }
        let data = data_lines.join("\n");
        if data.trim() == "[DONE]" {
            break;
        }
        let Some(value) = parse_openai_sse_event_value(&data, event_name.as_deref()) else {
            continue;
        };
        capture_sse_meta(&value, &mut state);
        if value
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(is_response_completed_event_type)
        {
            if let Some(response) = value.get("response") {
                completed_response = Some(response.clone());
            }
        }
        collect_sse_text_and_tools(&value, &mut state);
    }

    if let Some(response) = completed_response {
        let mut response_value =
            build_gemini_response_from_responses(&response, tool_name_restore_map)?;
        let empty_parts = response_value
            .get("candidates")
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|candidate| candidate.get("content"))
            .and_then(|content| content.get("parts"))
            .and_then(Value::as_array)
            .is_none_or(Vec::is_empty);
        if empty_parts {
            inject_synthesized_sse_parts(&mut response_value, &state, tool_name_restore_map);
        }
        return Ok(response_value);
    }

    let parts = synthesized_parts_from_state(&state, tool_name_restore_map);
    let usage_value = json!({
        "input_tokens": state.prompt_tokens,
        "output_tokens": state.output_tokens,
        "total_tokens": state.total_tokens.unwrap_or(state.prompt_tokens + state.output_tokens)
    });
    let usage_map = usage_value.as_object();
    Ok(build_gemini_response(
        state.response_id.as_deref().unwrap_or("resp_codexmanager"),
        state.model.as_deref().unwrap_or("unknown"),
        parts,
        Some("STOP"),
        usage_map,
    ))
}

fn inject_synthesized_sse_parts(
    response_value: &mut Value,
    state: &GeminiSseAggregationState,
    tool_name_restore_map: Option<&ToolNameRestoreMap>,
) {
    let synthesized = synthesized_parts_from_state(state, tool_name_restore_map);
    if synthesized.is_empty() {
        return;
    }
    let Some(parts) = response_value
        .get_mut("candidates")
        .and_then(Value::as_array_mut)
        .and_then(|items| items.first_mut())
        .and_then(|candidate| candidate.get_mut("content"))
        .and_then(|content| content.get_mut("parts"))
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    if parts.is_empty() {
        *parts = synthesized;
    }
}

fn synthesized_parts_from_state(
    state: &GeminiSseAggregationState,
    tool_name_restore_map: Option<&ToolNameRestoreMap>,
) -> Vec<Value> {
    let mut parts = Vec::new();
    let text = state.output_text.trim();
    if !text.is_empty() {
        parts.push(json!({ "text": text }));
    }
    for call in state.pending_tool_calls.values() {
        let Some(name) = call.name.as_deref() else {
            continue;
        };
        let name = restore_openai_tool_name(name, tool_name_restore_map);
        let args = parse_tool_arguments_as_object(&call.arguments);
        let mut function_call = serde_json::Map::new();
        function_call.insert("name".to_string(), Value::String(name));
        function_call.insert("args".to_string(), args);
        if let Some(call_id) = call.call_id.as_deref() {
            function_call.insert("id".to_string(), Value::String(call_id.to_string()));
        }
        parts.push(json!({ "functionCall": Value::Object(function_call) }));
    }
    parts
}

fn convert_openai_event_to_gemini_chunks(
    value: &Value,
    state: &mut GeminiSseAggregationState,
    tool_name_restore_map: Option<&ToolNameRestoreMap>,
) -> Vec<Value> {
    capture_sse_meta(value, state);
    let mut chunks = Vec::new();
    let Some(event_type) = value.get("type").and_then(Value::as_str) else {
        return chunks;
    };
    match event_type {
        "response.output_text.delta" => {
            let fragment = value.get("delta").and_then(Value::as_str).unwrap_or_default();
            if !fragment.is_empty() {
                state.output_text.push_str(fragment);
                chunks.push(build_gemini_chunk(state, vec![json!({ "text": fragment })], None));
            }
        }
        "response.output_item.added" | "response.output_item.done" => {
            let Some(item) = value
                .get("item")
                .or_else(|| value.get("output_item"))
                .and_then(Value::as_object)
            else {
                return chunks;
            };
            let item_type = item.get("type").and_then(Value::as_str).unwrap_or_default();
            if matches!(item_type, "function_call" | "custom_tool_call") {
                let output_index = value.get("output_index").and_then(Value::as_i64).unwrap_or(0);
                let entry = state.pending_tool_calls.entry(output_index).or_default();
                if let Some(call_id) = item
                    .get("call_id")
                    .or_else(|| item.get("id"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    entry.call_id = Some(call_id.to_string());
                }
                if let Some(name) = item
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    entry.name = Some(name.to_string());
                }
                if let Some(arguments) = extract_function_call_arguments_raw(item) {
                    merge_arguments(&mut entry.arguments, arguments.as_str());
                }
                if event_type == "response.output_item.done" {
                    let emitted = PendingToolCall {
                        call_id: entry.call_id.clone(),
                        name: entry.name.clone(),
                        arguments: entry.arguments.clone(),
                    };
                    if let Some(chunk) = build_gemini_function_call_chunk(
                        &emitted,
                        state,
                        output_index,
                        tool_name_restore_map,
                    ) {
                        chunks.push(chunk);
                    }
                }
            }
        }
        "response.function_call_arguments.delta" | "response.function_call_arguments.done" => {
            let output_index = value.get("output_index").and_then(Value::as_i64).unwrap_or(0);
            let entry = state.pending_tool_calls.entry(output_index).or_default();
            if let Some(call_id) = value
                .get("call_id")
                .or_else(|| value.get("item_id"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                entry.call_id = Some(call_id.to_string());
            }
            if let Some(delta) = value
                .get("delta")
                .or_else(|| value.get("arguments"))
                .and_then(Value::as_str)
            {
                merge_arguments(&mut entry.arguments, delta);
            }
            if event_type == "response.function_call_arguments.done" {
                let emitted = PendingToolCall {
                    call_id: entry.call_id.clone(),
                    name: entry.name.clone(),
                    arguments: entry.arguments.clone(),
                };
                if let Some(chunk) = build_gemini_function_call_chunk(
                    &emitted,
                    state,
                    output_index,
                    tool_name_restore_map,
                ) {
                    chunks.push(chunk);
                }
            }
        }
        _ if is_response_completed_event_type(event_type) => {
            if let Some(response) = value.get("response") {
                if let Ok(mut response_value) =
                    build_gemini_response_from_responses(response, tool_name_restore_map)
                {
                    inject_synthesized_sse_parts(&mut response_value, state, tool_name_restore_map);
                    if let Some(usage_metadata) = response_value.get("usageMetadata") {
                        chunks.push(json!({ "usageMetadata": usage_metadata }));
                    }
                }
            }
        }
        _ => {}
    }
    chunks
}

fn build_gemini_chunk(
    state: &GeminiSseAggregationState,
    parts: Vec<Value>,
    finish_reason: Option<&str>,
) -> Value {
    json!({
        "responseId": state.response_id.as_deref().unwrap_or("resp_codexmanager"),
        "modelVersion": state.model.as_deref().unwrap_or("unknown"),
        "candidates": [{
            "index": 0,
            "content": { "role": "model", "parts": parts },
            "finishReason": finish_reason.unwrap_or("STOP"),
        }]
    })
}

fn build_gemini_function_call_chunk(
    entry: &PendingToolCall,
    state: &mut GeminiSseAggregationState,
    output_index: i64,
    tool_name_restore_map: Option<&ToolNameRestoreMap>,
) -> Option<Value> {
    let Some(name) = entry.name.as_deref() else {
        return None;
    };
    let signature = format!(
        "{}:{}",
        entry.call_id.as_deref().unwrap_or(""),
        entry.arguments
    );
    if state
        .emitted_tool_calls
        .get(&output_index)
        .is_some_and(|current| current == &signature)
    {
        return None;
    }
    state.emitted_tool_calls.insert(output_index, signature);
    let name = restore_openai_tool_name(name, tool_name_restore_map);
    let args = parse_tool_arguments_as_object(&entry.arguments);
    let mut function_call = serde_json::Map::new();
    function_call.insert("name".to_string(), Value::String(name));
    function_call.insert("args".to_string(), args);
    if let Some(call_id) = entry.call_id.as_deref() {
        function_call.insert("id".to_string(), Value::String(call_id.to_string()));
    }
    Some(build_gemini_chunk(
        state,
        vec![json!({ "functionCall": Value::Object(function_call) })],
        None,
    ))
}

fn merge_arguments(existing: &mut String, fragment: &str) {
    let trimmed = fragment.trim();
    if trimmed.is_empty() {
        return;
    }
    if existing.is_empty() {
        existing.push_str(trimmed);
        return;
    }
    if existing == trimmed || existing.ends_with(trimmed) {
        return;
    }
    if trimmed.starts_with(existing.as_str()) {
        *existing = trimmed.to_string();
        return;
    }
    existing.push_str(trimmed);
}

fn capture_sse_meta(value: &Value, state: &mut GeminiSseAggregationState) {
    if let Some(response_id) = value
        .get("response_id")
        .or_else(|| value.get("id"))
        .and_then(Value::as_str)
    {
        state.response_id = Some(response_id.to_string());
    }
    if let Some(model) = value.get("model").and_then(Value::as_str) {
        state.model = Some(model.to_string());
    }
    if let Some(response) = value.get("response").and_then(Value::as_object) {
        if let Some(response_id) = response.get("id").and_then(Value::as_str) {
            state.response_id = Some(response_id.to_string());
        }
        if let Some(model) = response.get("model").and_then(Value::as_str) {
            state.model = Some(model.to_string());
        }
        if let Some(usage) = response.get("usage").and_then(Value::as_object) {
            state.prompt_tokens =
                extract_usage_i64(usage, &["input_tokens", "prompt_tokens"]).unwrap_or(state.prompt_tokens);
            state.output_tokens =
                extract_usage_i64(usage, &["output_tokens", "completion_tokens"])
                    .unwrap_or(state.output_tokens);
            state.total_tokens =
                extract_usage_i64(usage, &["total_tokens"]).or(state.total_tokens);
        }
    }
}

fn collect_sse_text_and_tools(value: &Value, state: &mut GeminiSseAggregationState) {
    let Some(event_type) = value.get("type").and_then(Value::as_str) else {
        return;
    };
    match event_type {
        "response.output_text.delta" => {
            if let Some(delta) = value.get("delta").and_then(Value::as_str) {
                state.output_text.push_str(delta);
            }
        }
        "response.output_item.added" | "response.output_item.done" => {
            let Some(item) = value
                .get("item")
                .or_else(|| value.get("output_item"))
                .and_then(Value::as_object)
            else {
                return;
            };
            if !matches!(
                item.get("type").and_then(Value::as_str).unwrap_or_default(),
                "function_call" | "custom_tool_call"
            ) {
                return;
            }
            let output_index = value.get("output_index").and_then(Value::as_i64).unwrap_or(0);
            let entry = state.pending_tool_calls.entry(output_index).or_default();
            if let Some(name) = item
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                entry.name = Some(name.to_string());
            }
            if let Some(call_id) = item
                .get("call_id")
                .or_else(|| item.get("id"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                entry.call_id = Some(call_id.to_string());
            }
            if let Some(arguments) = extract_function_call_arguments_raw(item) {
                merge_arguments(&mut entry.arguments, arguments.as_str());
            }
        }
        "response.function_call_arguments.delta" | "response.function_call_arguments.done" => {
            let output_index = value.get("output_index").and_then(Value::as_i64).unwrap_or(0);
            let entry = state.pending_tool_calls.entry(output_index).or_default();
            if let Some(delta) = value
                .get("delta")
                .or_else(|| value.get("arguments"))
                .and_then(Value::as_str)
            {
                merge_arguments(&mut entry.arguments, delta);
            }
        }
        _ => {}
    }
}
