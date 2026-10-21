use super::*;

#[test]
fn entry_run_accepts_same_origin_or_nonbrowser_json_and_rejects_cross_site_origins() {
    assert!(same_origin_request("POST /api/entry-validation/run HTTP/1.1\r\nHost: example.com\r\nOrigin: https://example.com\r\n\r\n{}"));
    assert!(same_origin_request("POST /api/entry-validation/run HTTP/1.1\r\nHost: localhost:17777\r\nOrigin: http://localhost:17777\r\n\r\n{}"));
    assert!(same_origin_request("POST /api/entry-validation/run HTTP/1.1\r\nHost: example.com\r\nContent-Type: application/json\r\n\r\n{}"));
    for origin in ["https://elsewhere.com","null","https://example.com.evil.test","https://example.com/path"] {
        assert!(!same_origin_request(&format!("POST / HTTP/1.1\r\nHost: example.com\r\nOrigin: {origin}\r\n\r\n{{}}")));
    }
    assert!(!same_origin_request("POST / HTTP/1.1\r\nHost: example.com\r\nSec-Fetch-Site: cross-site\r\n\r\n{}"));
}
