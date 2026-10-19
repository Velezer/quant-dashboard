//! Bounded synchronous HTTP requests for public data providers.

use std::io::Read;
use std::time::Duration;
use ureq::{
    config::Config,
    tls::{RootCerts, TlsConfig},
    Agent,
};

const MAX_RESPONSE_HEADER_BYTES: usize = 16 * 1024;

pub(crate) struct RequestOptions<'a> {
    pub connect_timeout: Duration,
    pub total_timeout: Duration,
    pub max_body_bytes: usize,
    pub max_redirects: u32,
    pub accept: &'a str,
    pub user_agent: Option<&'a str>,
    pub accept_encoding: Option<&'a str>,
}

pub(crate) struct BoundedResponse {
    pub status: u16,
    pub retry_after_seconds: Option<u64>,
    pub body: Vec<u8>,
}

pub(crate) fn get_bounded(
    url: &str,
    options: RequestOptions<'_>,
) -> Result<BoundedResponse, String> {
    let tls = TlsConfig::builder()
        .root_certs(RootCerts::PlatformVerifier)
        .build();
    let config = Config::builder()
        .https_only(true)
        .http_status_as_error(false)
        .max_redirects(options.max_redirects)
        .max_response_header_size(MAX_RESPONSE_HEADER_BYTES)
        .timeout_connect(Some(options.connect_timeout))
        .timeout_global(Some(options.total_timeout))
        .tls_config(tls)
        .build();
    let agent = Agent::new_with_config(config);
    let mut request = agent.get(url).header("Accept", options.accept);
    if let Some(user_agent) = options.user_agent {
        request = request.header("User-Agent", user_agent);
    }
    if let Some(accept_encoding) = options.accept_encoding {
        request = request.header("Accept-Encoding", accept_encoding);
    }
    let mut response = request
        .call()
        .map_err(|error| format!("public HTTP request failed: {error}"))?;
    let retry_after_seconds = response
        .headers()
        .get("Retry-After")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok());
    let body = read_bounded(response.body_mut().as_reader(), options.max_body_bytes)?;
    Ok(BoundedResponse {
        status: response.status().as_u16(),
        retry_after_seconds,
        body,
    })
}

fn read_bounded(reader: impl Read, maximum: usize) -> Result<Vec<u8>, String> {
    let mut body = Vec::with_capacity(maximum.min(16 * 1024));
    let read_result = reader
        .take(maximum.saturating_add(1) as u64)
        .read_to_end(&mut body);
    if read_result.is_err() || body.len() > maximum {
        return Err(if body.len() > maximum {
            format!("public HTTP response exceeded {maximum} byte body limit")
        } else {
            format!(
                "public HTTP response could not be read: {}",
                read_result.unwrap_err()
            )
        });
    }
    Ok(body)
}

#[cfg(test)]
#[path = "public_http_tests.rs"]
mod tests;
