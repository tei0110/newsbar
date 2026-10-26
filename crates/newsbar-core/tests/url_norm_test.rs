use newsbar_core::url_norm::normalize_url;

#[test]
fn normalizes_case_tracking_params_fragment_and_trailing_slash() {
    assert_eq!(
        normalize_url("HTTPS://Example.COM/a/b/?utm_source=x&id=3#frag").as_deref(),
        Some("https://example.com/a/b?id=3")
    );
}

#[test]
fn keeps_root_slash() {
    assert_eq!(
        normalize_url("https://example.com/").as_deref(),
        Some("https://example.com/")
    );
}

#[test]
fn drops_default_port() {
    assert_eq!(
        normalize_url("https://example.com:443/x").as_deref(),
        Some("https://example.com/x")
    );
}

#[test]
fn drops_click_ids_and_ref() {
    assert_eq!(
        normalize_url("https://example.com/x?fbclid=1&gclid=2&ref=hn").as_deref(),
        Some("https://example.com/x")
    );
}

#[test]
fn keeps_other_query_order() {
    assert_eq!(
        normalize_url("https://e.com/p?b=2&a=1").as_deref(),
        Some("https://e.com/p?b=2&a=1")
    );
}

#[test]
fn rejects_non_http_and_garbage() {
    assert_eq!(normalize_url("ftp://example.com/x"), None);
    assert_eq!(normalize_url("not a url"), None);
}
