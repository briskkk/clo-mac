//! 读接口失败时怎么对待登录态。不使用全局只读 Key，也不根据本机是否还留着令牌来猜测。

/// 服务端读接口失败后的菜单栏结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadFailure {
    /// 没有可用会话。菜单栏显示「请登录」，不保留上一笔数字。
    SignedOut,
    /// 会话还在，但这轮没读到。保留上一笔数字并标明过时；没有上一笔则显示「暂无数据」。
    Unavailable,
}

/// `401` 是未登录。生产在只读 Key 未配置时，无会话也会回 `503`，正文是
/// `{"ok":false,"error":"PORTFOLIO_API_KEY not configured"}`。只有这份正文算未登录。
/// 其它 `503`（网关、过载、别的错误）保持「暂时不可用」，避免把仍在登录的账本收成「请登录」。
pub fn classify_read_failure(status: u16, body: &str) -> ReadFailure {
    if status == 401 || status == 403 {
        return ReadFailure::SignedOut;
    }
    if status == 503 && missing_read_key_body(body) {
        return ReadFailure::SignedOut;
    }
    ReadFailure::Unavailable
}

fn missing_read_key_body(body: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(body) else {
        return false;
    };
    value.get("ok") == Some(&serde_json::Value::Bool(false))
        && value.get("error").and_then(|item| item.as_str())
            == Some("PORTFOLIO_API_KEY not configured")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unauthorized_is_signed_out() {
        assert_eq!(classify_read_failure(401, ""), ReadFailure::SignedOut);
        assert_eq!(
            classify_read_failure(401, r#"{"ok":false,"error":"Unauthorized"}"#),
            ReadFailure::SignedOut
        );
        assert_eq!(
            classify_read_failure(403, "forbidden"),
            ReadFailure::SignedOut
        );
    }

    #[test]
    fn missing_read_key_503_is_signed_out_and_other_503_is_not() {
        let missing = r#"{"ok":false,"error":"PORTFOLIO_API_KEY not configured"}"#;
        assert_eq!(classify_read_failure(503, missing), ReadFailure::SignedOut);
        assert_eq!(
            classify_read_failure(503, r#"{"ok":false,"error":"upstream timeout"}"#),
            ReadFailure::Unavailable
        );
        assert_eq!(
            classify_read_failure(503, "<html>503 Service Unavailable</html>"),
            ReadFailure::Unavailable
        );
        assert_eq!(
            classify_read_failure(503, r#"{"ok":false,"error":"待办读取失败"}"#),
            ReadFailure::Unavailable
        );
        assert_eq!(
            classify_read_failure(
                500,
                r#"{"ok":false,"error":"PORTFOLIO_API_KEY not configured"}"#
            ),
            ReadFailure::Unavailable
        );
        assert_eq!(classify_read_failure(503, ""), ReadFailure::Unavailable);
    }
}
