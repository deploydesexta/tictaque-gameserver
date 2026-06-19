//! Session / auth helpers.
//!
//! PoC mock: reads the `userId` cookie set by the Go auth controller.
//! In production this would validate the `session` (Stytch JWT) and return the canonical user ID.

use axum::http::HeaderMap;

pub fn extract_user_id(headers: &HeaderMap) -> Option<String> {
    let cookie_header = headers.get("cookie")?.to_str().ok()?;

    // Try "userId" cookie first (set by Go auth controller, contains the real user ID).
    // Fall back to "session" for convenience during manual testing.
    for name in ["userId", "session"] {
        if let Some(value) = find_cookie(cookie_header, name) {
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn find_cookie<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    for part in header.split(';') {
        let part = part.trim();
        if let Some(val) = part.strip_prefix(name) {
            if let Some(val) = val.strip_prefix('=') {
                return Some(val.trim());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers_with_cookie(val: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("cookie", HeaderValue::from_str(val).unwrap());
        h
    }

    #[test]
    fn extracts_user_id_cookie() {
        let h = headers_with_cookie("userId=user-123; session=tok-abc");
        assert_eq!(extract_user_id(&h).as_deref(), Some("user-123"));
    }

    #[test]
    fn falls_back_to_session_cookie() {
        let h = headers_with_cookie("session=tok-abc");
        assert_eq!(extract_user_id(&h).as_deref(), Some("tok-abc"));
    }

    #[test]
    fn returns_none_when_no_cookies() {
        let h = HeaderMap::new();
        assert!(extract_user_id(&h).is_none());
    }
}
