use std::{
    io::Read,
    process::{Command, Stdio},
};

const MAX_RESPONSE_BYTES: usize = 512 * 1024;

pub(crate) fn fetch_bounded_url(url: &str) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    {
        fetch_bounded_url_with_powershell(url)
    }
    #[cfg(not(windows))]
    {
        fetch_bounded_url_with_curl(url)
    }
}

#[cfg(not(windows))]
fn fetch_bounded_url_with_curl(url: &str) -> Result<Vec<u8>, String> {
    let mut child = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--fail",
            "--max-time",
            "10",
            "--connect-timeout",
            "4",
            "--max-filesize",
            "524288",
            "--header",
            "Accept: application/json",
            "--header",
            "User-Agent: Mozilla/5.0",
        ])
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("market-data request could not start: {e}"))?;
    let mut body = Vec::new();
    let read = child
        .stdout
        .take()
        .ok_or("market-data response unavailable")?
        .take((MAX_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut body);
    if read.is_err() || body.len() > MAX_RESPONSE_BYTES {
        let _ = child.kill();
        let _ = child.wait();
        return Err("market-data response exceeded bound or could not be read".into());
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!(
            "market-data request failed (curl exit code {:?})",
            status.code()
        ));
    }
    Ok(body)
}

#[cfg(windows)]
pub(crate) const POWERSHELL_RESEARCH_REQUEST: &str = r#"$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
$url = $env:QUANT_MARKET_REQUEST_URL
$response = Invoke-WebRequest -Uri $url -Method Get -TimeoutSec 10 -MaximumRedirection 3 -UseBasicParsing -Headers @{ Accept = 'application/json'; 'User-Agent' = 'Mozilla/5.0' }
$stream = $response.RawContentStream
$stream.Position = 0
$buffer = New-Object byte[] 8192
$output = New-Object System.IO.MemoryStream
while (($read = $stream.Read($buffer, 0, $buffer.Length)) -gt 0) {
    if ($output.Length + $read -gt 524288) { exit 22 }
    $output.Write($buffer, 0, $read)
}
[Console]::OpenStandardOutput().Write($output.ToArray(), 0, [int]$output.Length)
"#;

#[cfg(windows)]
fn fetch_bounded_url_with_powershell(url: &str) -> Result<Vec<u8>, String> {
    let mut child = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            POWERSHELL_RESEARCH_REQUEST,
        ])
        .env("QUANT_MARKET_REQUEST_URL", url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("market-data request could not start: {error}"))?;
    let mut body = Vec::new();
    child
        .stdout
        .take()
        .ok_or("market-data response unavailable")?
        .take((MAX_RESPONSE_BYTES + 1) as u64)
        .read_to_end(&mut body)
        .map_err(|error| format!("market-data response could not be read: {error}"))?;
    let status = child.wait().map_err(|error| error.to_string())?;
    if !status.success() || body.len() > MAX_RESPONSE_BYTES {
        let mut error = Vec::new();
        if let Some(stderr) = child.stderr.take() {
            let _ = stderr.take(2_048).read_to_end(&mut error);
        }
        let detail = String::from_utf8_lossy(&error).trim().to_string();
        return Err(if detail.is_empty() {
            "market-data request failed or exceeded its response bound".into()
        } else {
            format!("market-data request failed: {detail}")
        });
    }
    Ok(body)
}

#[cfg(any(not(windows), test))]
pub(crate) fn curl_ssl_revocation_options(windows: bool) -> &'static [&'static str] {
    if windows {
        &["--ssl-revoke-best-effort"]
    } else {
        &[]
    }
}
