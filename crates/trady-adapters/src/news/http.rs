//! HTTPS-only fetching with connection-pinned public DNS and checked redirects.

use super::{MAX_BODY_BYTES, SourceFailure};
use reqwest::{Url, blocking::Client, redirect::Policy};
use std::{
    io::Read,
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    sync::mpsc,
    time::Duration,
};

pub struct HttpRequest {
    pub url: Url,
    pub allowed_hosts: Vec<String>,
    pub bearer: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub redirects: usize,
}

#[derive(Default)]
pub struct HttpResponse {
    pub status: u16,
    pub final_url: Option<Url>,
    pub body: Vec<u8>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub retry_after: Option<String>,
    pub rate_reset: Option<i64>,
}

pub trait HttpTransport {
    fn get(&mut self, request: HttpRequest) -> std::result::Result<HttpResponse, SourceFailure>;
    fn get_with_headers(
        &mut self,
        request: HttpRequest,
        headers: &[(String, String)],
    ) -> std::result::Result<HttpResponse, SourceFailure> {
        if headers.is_empty() {
            self.get(request)
        } else {
            Err(SourceFailure::unavailable("custom_headers_unsupported"))
        }
    }
    fn x_budget_enforced(&self) -> bool {
        false
    }
    fn set_x_scope(
        &mut self,
        _source: &trady_application::research::SourceConfig,
    ) -> std::result::Result<(), SourceFailure> {
        Ok(())
    }
    /// Analysis requests only. This port has no broker order endpoint or credentials.
    fn post_json(
        &mut self,
        _request: HttpRequest,
        _body: Vec<u8>,
    ) -> std::result::Result<HttpResponse, SourceFailure> {
        Err(SourceFailure::unavailable("analysis_post_unsupported"))
    }
}
pub struct SafeHttp;

pub fn public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(a) => {
            let [x, y, z, _] = a.octets();
            !a.is_private()
                && !a.is_loopback()
                && !a.is_link_local()
                && !a.is_multicast()
                && !a.is_broadcast()
                && !a.is_unspecified()
                && !a.is_documentation()
                && x != 0
                && x < 224
                && !(x == 100 && (64..=127).contains(&y))
                && !(x == 198 && (y == 18 || y == 19))
                && !(x == 192 && y == 0 && z == 0)
        }
        IpAddr::V6(a) => {
            let s = a.segments();
            s[0] & 0xe000 == 0x2000
                && s[0] != 0x2002
                && !(s[0] == 0x2001 && (s[1] == 0 || s[1] == 0x0db8))
                && a.to_ipv4_mapped().is_none()
        }
    }
}

pub fn validate_destination(
    url: &Url,
    allowed_hosts: &[String],
) -> std::result::Result<(), SourceFailure> {
    let host = url
        .host_str()
        .ok_or_else(|| SourceFailure::unavailable("url_host_missing"))?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
        || !allowed_hosts.iter().any(|h| h == host)
        || host == "localhost"
        || host.ends_with(".local")
        || host.ends_with(".localhost")
    {
        return Err(SourceFailure::unavailable("unapproved_fetch_destination"));
    }
    if let Ok(address) = host.trim_matches(['[', ']']).parse()
        && !public_address(address)
    {
        return Err(SourceFailure::unavailable("nonpublic_fetch_destination"));
    }
    Ok(())
}

fn resolve(host: &str) -> std::result::Result<Vec<SocketAddr>, SourceFailure> {
    let host = host.to_string();
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let result = (host.as_str(), 443)
            .to_socket_addrs()
            .map(|a| a.collect::<Vec<_>>());
        let _ = sender.send(result);
    });
    let addresses = receiver
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| SourceFailure::unavailable("dns_resolution_timeout"))?
        .map_err(|_| SourceFailure::unavailable("dns_resolution_failed"))?;
    if addresses.is_empty() || addresses.iter().any(|a| !public_address(a.ip())) {
        return Err(SourceFailure::unavailable("nonpublic_dns_destination"));
    }
    Ok(addresses)
}

impl HttpTransport for SafeHttp {
    fn get(&mut self, request: HttpRequest) -> std::result::Result<HttpResponse, SourceFailure> {
        self.send(request, None, &[])
    }
    fn get_with_headers(
        &mut self,
        mut request: HttpRequest,
        headers: &[(String, String)],
    ) -> std::result::Result<HttpResponse, SourceFailure> {
        request.redirects = 0;
        self.send(request, None, headers)
    }
    fn post_json(
        &mut self,
        mut request: HttpRequest,
        body: Vec<u8>,
    ) -> std::result::Result<HttpResponse, SourceFailure> {
        if body.len() > MAX_BODY_BYTES {
            return Err(SourceFailure::unavailable("analysis_request_too_large"));
        }
        request.redirects = 0;
        self.send(request, Some(body), &[])
    }
}

impl SafeHttp {
    fn send(
        &mut self,
        request: HttpRequest,
        body: Option<Vec<u8>>,
        headers: &[(String, String)],
    ) -> std::result::Result<HttpResponse, SourceFailure> {
        let mut url = request.url;
        for hop in 0..=request.redirects.min(3) {
            validate_destination(&url, &request.allowed_hosts)?;
            let host = url
                .host_str()
                .ok_or_else(|| SourceFailure::unavailable("url_host_missing"))?;
            let addresses = resolve(host)?;
            let client = Client::builder()
                .no_proxy()
                .https_only(true)
                .redirect(Policy::none())
                .retry(reqwest::retry::never())
                .no_gzip()
                .no_brotli()
                .no_deflate()
                .no_zstd()
                .timeout(Duration::from_secs(15))
                .connect_timeout(Duration::from_secs(5))
                .resolve_to_addrs(host, &addresses)
                .user_agent("Trady/0.1 research-reader")
                .build()
                .map_err(|_| SourceFailure::unavailable("http_client_initialization_failed"))?;
            let mut builder = if let Some(body) = &body {
                client
                    .post(url.clone())
                    .header("Content-Type", "application/json")
                    .body(body.clone())
            } else {
                client.get(url.clone())
            }
            .header("Accept-Encoding", "identity");
            for (name, value) in headers {
                if !matches!(
                    name.as_str(),
                    "User-Agent" | "x-cg-demo-api-key" | "Authorization"
                ) || value.len() > 4096
                    || value.chars().any(char::is_control)
                {
                    return Err(SourceFailure::unavailable("invalid_financial_auth_header"));
                }
                builder = builder.header(name, value);
            }
            if let Some(token) = &request.bearer {
                builder = builder.bearer_auth(token);
            }
            if let Some(etag) = &request.etag {
                builder = builder.header("If-None-Match", etag);
            }
            if let Some(last_modified) = &request.last_modified {
                builder = builder.header("If-Modified-Since", last_modified);
            }
            let response = builder
                .send()
                .map_err(|_| SourceFailure::unavailable("http_transport_failed"))?;
            if response.status().is_redirection() && response.status().as_u16() != 304 {
                // Authenticated API calls never follow a redirect, including query-string API keys.
                if request.redirects == 0
                    || request.bearer.is_some()
                    || hop == request.redirects.min(3)
                {
                    return Err(SourceFailure::unavailable("redirect_not_permitted"));
                }
                let location = response
                    .headers()
                    .get("location")
                    .and_then(|h| h.to_str().ok())
                    .ok_or_else(|| SourceFailure::unavailable("invalid_redirect"))?;
                url = url
                    .join(location)
                    .map_err(|_| SourceFailure::unavailable("invalid_redirect"))?;
                continue;
            }
            let header = |key: &str| {
                response
                    .headers()
                    .get(key)
                    .and_then(|h| h.to_str().ok())
                    .filter(|h| h.len() <= 2048)
                    .map(String::from)
            };
            let mut output = HttpResponse {
                status: response.status().as_u16(),
                final_url: Some(url.clone()),
                etag: header("etag"),
                last_modified: header("last-modified"),
                retry_after: header("retry-after"),
                rate_reset: header("x-rate-limit-reset").and_then(|s| s.parse().ok()),
                body: vec![],
            };
            if header("content-encoding").is_some_and(|v| !v.eq_ignore_ascii_case("identity")) {
                return Err(SourceFailure::unavailable(
                    "compressed_response_not_permitted",
                ));
            }
            if response
                .content_length()
                .is_some_and(|n| n > MAX_BODY_BYTES as u64)
            {
                return Err(SourceFailure::unavailable("response_too_large"));
            }
            response
                .take((MAX_BODY_BYTES + 1) as u64)
                .read_to_end(&mut output.body)
                .map_err(|_| SourceFailure::unavailable("response_read_failed"))?;
            if output.body.len() > MAX_BODY_BYTES {
                return Err(SourceFailure::unavailable("response_too_large"));
            }
            return Ok(output);
        }
        Err(SourceFailure::unavailable("redirect_budget_exhausted"))
    }
}
