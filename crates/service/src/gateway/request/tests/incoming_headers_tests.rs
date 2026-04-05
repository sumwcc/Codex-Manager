use super::*;

/// 函数 `strict_bearer_parsing_matches_auth_extraction_behavior`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn strict_bearer_parsing_matches_auth_extraction_behavior() {
    assert_eq!(strict_bearer_token("Bearer abc"), Some("abc".to_string()));
    assert_eq!(strict_bearer_token("bearer abc"), None);
    assert_eq!(strict_bearer_token("Bearer   "), None);
}

/// 函数 `case_insensitive_bearer_parsing_matches_sticky_derivation_behavior`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-02
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn case_insensitive_bearer_parsing_matches_sticky_derivation_behavior() {
    assert_eq!(
        case_insensitive_bearer_token("Bearer abc"),
        Some("abc".to_string())
    );
    assert_eq!(
        case_insensitive_bearer_token("bearer abc"),
        Some("abc".to_string())
    );
    assert_eq!(case_insensitive_bearer_token("basic abc"), None);
    assert_eq!(case_insensitive_bearer_token("bearer   "), None);
}

/// 函数 `goog_api_key_header_is_accepted_as_platform_key`
///
/// 作者: gaohongshun
///
/// 时间: 2026-04-05
///
/// # 参数
/// 无
///
/// # 返回
/// 无
#[test]
fn goog_api_key_header_is_accepted_as_platform_key() {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        "x-goog-api-key",
        axum::http::HeaderValue::from_static("platform-key-from-gemini"),
    );

    let snapshot = IncomingHeaderSnapshot::from_http_headers(&headers);
    assert_eq!(
        snapshot.platform_key(),
        Some("platform-key-from-gemini")
    );
    assert!(snapshot.has_x_api_key());
    assert_eq!(
        snapshot.sticky_key_material(),
        Some("platform-key-from-gemini")
    );
}
