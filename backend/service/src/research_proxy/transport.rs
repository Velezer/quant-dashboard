use crate::public_http::{get_bounded, RequestOptions};
use std::time::Duration;

const MAX_RESPONSE_BYTES: usize = 512 * 1024;

pub(crate) fn fetch_bounded_url(url: &str) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    let max_redirects = 3;
    #[cfg(not(windows))]
    let max_redirects = 0;

    let response = get_bounded(
        url,
        RequestOptions {
            connect_timeout: Duration::from_secs(4),
            total_timeout: Duration::from_secs(10),
            max_body_bytes: MAX_RESPONSE_BYTES,
            max_redirects,
            accept: "application/json",
            user_agent: Some("Mozilla/5.0"),
            accept_encoding: None,
        },
    )?;
    if response.status >= 400 {
        return Err(format!(
            "market-data request failed with HTTP {}",
            response.status
        ));
    }
    Ok(response.body)
}
