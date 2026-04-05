use super::*;

#[test]
fn gemini_json_response_maps_from_openai_responses_shape() {
    let upstream = serde_json::json!({
        "id": "resp_gemini_1",
        "model": "gpt-5.4",
        "status": "completed",
        "output": [
            {
                "type": "message",
                "role": "assistant",
                "content": [
                    { "type": "output_text", "text": "已完成" }
                ]
            },
            {
                "type": "function_call",
                "call_id": "call_ls_1",
                "name": "list_files",
                "arguments": "{\"path\":\".\"}"
            }
        ],
        "usage": {
            "input_tokens": 8,
            "output_tokens": 5,
            "total_tokens": 13
        }
    });
    let upstream = serde_json::to_vec(&upstream).expect("serialize upstream");
    let (body, content_type) = adapt_upstream_response(
        ResponseAdapter::GeminiJson,
        Some("application/json"),
        &upstream,
    )
    .expect("adapt response");
    assert_eq!(content_type, "application/json");

    let value: serde_json::Value = serde_json::from_slice(&body).expect("gemini response");
    assert_eq!(value["candidates"][0]["content"]["role"], "model");
    assert_eq!(value["candidates"][0]["content"]["parts"][0]["text"], "已完成");
    assert_eq!(
        value["candidates"][0]["content"]["parts"][1]["functionCall"]["name"],
        "list_files"
    );
    assert_eq!(
        value["candidates"][0]["content"]["parts"][1]["functionCall"]["args"]["path"],
        "."
    );
    assert_eq!(value["usageMetadata"]["promptTokenCount"], 8);
    assert_eq!(value["usageMetadata"]["candidatesTokenCount"], 5);
    assert_eq!(value["usageMetadata"]["totalTokenCount"], 13);
}

#[test]
fn gemini_sse_response_maps_openai_responses_event_stream() {
    let upstream = concat!(
        "data: {\"type\":\"response.output_text.delta\",\"response_id\":\"resp_gemini_stream\",\"model\":\"gpt-5.4\",\"delta\":\"你好\"}\n\n",
        "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_gemini_stream\",\"model\":\"gpt-5.4\",\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":2,\"total_tokens\":3}}}\n\n",
        "data: [DONE]\n\n",
    );
    let (body, content_type) = adapt_upstream_response(
        ResponseAdapter::GeminiSse,
        Some("text/event-stream"),
        upstream.as_bytes(),
    )
    .expect("adapt stream");
    assert_eq!(content_type, "text/event-stream");

    let text = String::from_utf8(body).expect("utf8");
    assert!(text.contains("\"text\":\"你好\""));
    assert!(text.contains("\"usageMetadata\""));
}
