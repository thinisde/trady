//! Explicit, read-only diagnostics. No collection, database or broker mutation port.

use crate::{
    credentials::CredentialStore,
    financial,
    news::{HttpRequest, HttpResponse, HttpTransport, MAX_BODY_BYTES, SafeHttp},
};
use chrono::{DateTime, Duration, Utc};
use reqwest::Url;
use serde::Serialize;
use serde_json::{Value, json};
use std::time::Instant;
use trady_application::financial::{DataWindow, FinancialConfig};
use trady_domain::{Error, Result, require};

pub const PROVIDERS: &[&str] = &[
    "sec",
    "fred",
    "twelve_data",
    "finnhub",
    "alpha_vantage",
    "fmp",
    "coingecko",
    "tiingo",
    "massive",
    "gdelt",
    "google_news",
    "publisher_rss",
    "telegram",
    "x",
    "yfinance",
];

#[derive(Debug, Serialize)]
pub struct HealthCheck {
    pub provider: String,
    pub credential_name: Option<&'static str>,
    pub credential_configured: bool,
    pub status: &'static str,
    pub http_status: Option<u16>,
    pub latency_ms: Option<u128>,
    pub response_records: Option<usize>,
    pub retry_after_seconds: Option<u64>,
    pub failure_code: Option<&'static str>,
    pub network_requests: u8,
}

fn credential_name(provider: &str) -> Option<&'static str> {
    match provider {
        "sec" => Some("TRADY_SEC_USER_AGENT"),
        "fred" => Some("TRADY_FRED_API_KEY"),
        "twelve_data" => Some("TRADY_TWELVE_DATA_API_KEY"),
        "finnhub" => Some("TRADY_FINNHUB_API_KEY"),
        "alpha_vantage" => Some("TRADY_ALPHA_VANTAGE_API_KEY"),
        "fmp" => Some("TRADY_FMP_API_KEY"),
        "coingecko" => Some("TRADY_COINGECKO_API_KEY"),
        "tiingo" => Some("TRADY_TIINGO_API_KEY"),
        "massive" => Some("TRADY_MASSIVE_API_KEY"),
        "telegram" => Some("TRADY_TELEGRAM_BOT_TOKEN"),
        "x" => Some("TRADY_X_BEARER_TOKEN"),
        _ => None,
    }
}

pub fn run(online: bool, provider: Option<&str>, now: DateTime<Utc>) -> Result<Value> {
    require(
        provider.is_none_or(|p| PROVIDERS.contains(&p)),
        "unknown API health provider",
    )?;
    let store = CredentialStore::load()?;
    let mut transport = SafeHttp;
    let mut checks = vec![];
    for name in PROVIDERS
        .iter()
        .copied()
        .filter(|p| provider.is_none_or(|selected| selected == *p))
    {
        let token = credential_name(name)
            .map(|key| store.optional(key))
            .transpose()?
            .flatten();
        checks.push(probe(
            name,
            online,
            token.as_ref().map(|v| v.expose()),
            now,
            &mut transport,
        )?);
    }
    let network_requests: u32 = checks.iter().map(|c| u32::from(c.network_requests)).sum();
    Ok(json!({
        "schema_version":1,"checked_at":now,"mode":if online {"ONLINE"} else {"OFFLINE"},
        "network_requests":network_requests,"checks":checks,"entry_authorized":false,
        "collection_settings_changed":false
    }))
}

fn request(
    provider: &str,
    token: Option<&str>,
    now: DateTime<Utc>,
) -> Result<(HttpRequest, Vec<(String, String)>)> {
    if let Some(token) = token {
        require(
            !token.trim().is_empty() && token.len() <= 4096 && !token.chars().any(char::is_control),
            "invalid diagnostic credential",
        )?;
    }
    if provider == "telegram" {
        let (id, secret) = token
            .and_then(|s| s.split_once(':'))
            .ok_or_else(|| Error("invalid Telegram diagnostic credential".into()))?;
        require(
            !id.is_empty()
                && id.bytes().all(|b| b.is_ascii_digit())
                && !secret.is_empty()
                && secret
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
            "invalid Telegram diagnostic credential",
        )?;
    }
    let source_id = match provider {
        "sec" => Some("sec_filings"),
        "fred" => Some("fred_treasury_10y"),
        "twelve_data" => Some("twelve_daily"),
        "fmp" => Some("fmp_revenue"),
        "coingecko" => Some("crypto_context"),
        "tiingo" => Some("tiingo_daily"),
        "massive" => Some("massive_daily"),
        _ => None,
    };
    if let Some(id) = source_id {
        let config: FinancialConfig =
            serde_json::from_str(include_str!("../../../config/financial-data.json"))
                .map_err(|_| Error("invalid diagnostic financial routes".into()))?;
        let mut source = config
            .sources
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| Error("diagnostic financial route missing".into()))?;
        source.max_records = 1;
        let end = now.date_naive() - Duration::days(1);
        return financial::request(
            &source,
            &DataWindow {
                start: end - Duration::days(6),
                end,
            },
            token,
        );
    }
    let endpoint = match provider {
        "finnhub" => "https://api.finnhub.io/api/v1/quote",
        "alpha_vantage" => "https://www.alphavantage.co/query",
        "gdelt" => "https://api.gdeltproject.org/api/v2/doc/doc",
        "google_news" => "https://news.google.com/rss?hl=en-US&gl=US&ceid=US:en",
        "publisher_rss" => "https://feeds.bbci.co.uk/news/business/rss.xml",
        "telegram" => "https://api.telegram.org/",
        _ => return Err(Error("API health route unavailable".into())),
    };
    let mut url = Url::parse(endpoint).map_err(|_| Error("invalid API health route".into()))?;
    match provider {
        "finnhub" => {
            url.query_pairs_mut()
                .append_pair("symbol", "MSFT")
                .append_pair("token", token.unwrap_or_default());
        }
        "alpha_vantage" => {
            url.query_pairs_mut()
                .append_pair("function", "GLOBAL_QUOTE")
                .append_pair("symbol", "MSFT")
                .append_pair("apikey", token.unwrap_or_default());
        }
        "gdelt" => {
            url.query_pairs_mut()
                .append_pair("query", "stock market")
                .append_pair("mode", "artlist")
                .append_pair("format", "json")
                .append_pair("maxrecords", "1")
                .append_pair("timespan", "24h");
        }
        "telegram" => url.set_path(&format!("/bot{}/getMe", token.unwrap_or_default())),
        _ => {}
    }
    let host = url
        .host_str()
        .ok_or_else(|| Error("API health host missing".into()))?
        .to_owned();
    Ok((
        HttpRequest {
            url,
            allowed_hosts: vec![host],
            bearer: None,
            etag: None,
            last_modified: None,
            redirects: 0,
        },
        vec![],
    ))
}

fn probe<T: HttpTransport>(
    provider: &str,
    online: bool,
    token: Option<&str>,
    now: DateTime<Utc>,
    transport: &mut T,
) -> Result<HealthCheck> {
    let credential_name = credential_name(provider);
    let configured = token.is_some_and(|s| !s.trim().is_empty());
    let mut check = HealthCheck {
        provider: provider.into(),
        credential_name,
        credential_configured: configured,
        status: "not_checked",
        http_status: None,
        latency_ms: None,
        response_records: None,
        retry_after_seconds: None,
        failure_code: None,
        network_requests: 0,
    };
    if provider == "x" {
        check.status = "requires_existing_cost_gate";
    } else if provider == "yfinance" {
        check.status = "offline_import_only";
    } else if credential_name.is_some() && !configured {
        check.status = "missing_credential";
    } else if !online {
        check.status = if configured {
            "credential_present"
        } else {
            "public_endpoint_not_checked"
        };
    } else {
        let (request, headers) = match request(provider, token, now) {
            Ok(r) => r,
            Err(_) => {
                check.status = "invalid_credential_or_route";
                return Ok(check);
            }
        };
        check.network_requests = 1;
        let start = Instant::now();
        let result = transport.get_with_headers(request, &headers);
        check.latency_ms = Some(start.elapsed().as_millis());
        match result {
            Err(error) => {
                check.status = "transport_failed";
                check.failure_code = Some(match error.reason.as_str() {
                    "dns_resolution_timeout" => "dns_resolution_timeout",
                    "dns_resolution_failed" => "dns_resolution_failed",
                    "nonpublic_dns_destination" => "nonpublic_dns_destination",
                    "redirect_not_permitted" => "redirect_not_permitted",
                    "http_transport_failed" => "http_transport_failed",
                    "http_timeout" => "http_timeout",
                    "http_connection_failed" => "http_connection_failed",
                    _ => "transport_failed",
                });
            }
            Ok(response) => {
                check.http_status = Some(response.status);
                check.retry_after_seconds = response
                    .retry_after
                    .as_deref()
                    .and_then(|s| s.parse::<u64>().ok());
                let (status, records) = classify(provider, &response);
                check.status = status;
                check.response_records = records;
                if provider == "gdelt" {
                    if status == "invalid_response" {
                        check.failure_code = Some("gdelt_response_invalid");
                    }
                    if matches!(status, "rate_limited" | "invalid_response")
                        && check.retry_after_seconds.is_none()
                    {
                        check.retry_after_seconds = Some(60);
                    }
                }
            }
        }
    }
    Ok(check)
}

fn classify(provider: &str, response: &HttpResponse) -> (&'static str, Option<usize>) {
    match response.status {
        200 => {}
        401 | 403 => return ("access_denied", None),
        402 => return ("credits_or_subscription_required", None),
        429 => return ("rate_limited", None),
        500..=599 => return ("provider_unavailable", None),
        _ => return ("http_error", None),
    }
    if response.body.len() > MAX_BODY_BYTES {
        return ("invalid_response", None);
    }
    if provider == "gdelt"
        && std::str::from_utf8(&response.body).is_ok_and(|text| {
            text.trim_start()
                .starts_with("Please limit requests to one every 5 seconds")
        })
    {
        return ("rate_limited", None);
    }
    if matches!(provider, "google_news" | "publisher_rss") {
        return match feed_records(&response.body) {
            Some(0) => ("empty_data", Some(0)),
            Some(n) => ("healthy", Some(n)),
            None => ("invalid_response", None),
        };
    }
    let Ok(body) = serde_json::from_slice::<Value>(&response.body) else {
        return ("invalid_response", None);
    };
    if body
        .get("code")
        .or_else(|| body.get("error_code"))
        .and_then(Value::as_u64)
        == Some(429)
    {
        return ("rate_limited", None);
    }
    for key in ["error", "Error Message", "Error", "Note", "Information"] {
        if let Some(error) = body.get(key).filter(|v| !v.is_null()) {
            let text = error.to_string().to_lowercase();
            return (
                if text.contains("rate")
                    || text.contains("frequency")
                    || text.contains("limit")
                    || text.contains("quota")
                {
                    "rate_limited"
                } else if text.contains("premium")
                    || text.contains("subscription")
                    || text.contains("payment")
                {
                    "credits_or_subscription_required"
                } else {
                    "provider_error"
                },
                None,
            );
        }
    }
    if body
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|s| s.eq_ignore_ascii_case("error"))
        || body.get("ok") == Some(&Value::Bool(false))
    {
        return ("provider_error", None);
    }
    let records = match provider {
        "sec" => body
            .pointer("/filings/recent/form")
            .and_then(Value::as_array)
            .map(Vec::len),
        "fred" => body
            .get("observations")
            .and_then(Value::as_array)
            .map(Vec::len),
        "twelve_data" => body.get("values").and_then(Value::as_array).map(Vec::len),
        "fmp" | "coingecko" | "tiingo" => body
            .as_array()
            .filter(|rows| {
                rows.iter().all(|row| match provider {
                    "fmp" => {
                        row.get("symbol").is_some_and(Value::is_string)
                            && row.get("revenue").is_some_and(Value::is_number)
                    }
                    "coingecko" => {
                        row.get("id").is_some_and(Value::is_string)
                            && row.get("current_price").is_some_and(Value::is_number)
                    }
                    _ => {
                        row.get("date").is_some_and(Value::is_string)
                            && row.get("close").is_some_and(Value::is_number)
                    }
                })
            })
            .map(Vec::len),
        "massive" => body
            .get("results")
            .and_then(Value::as_array)
            .map(Vec::len)
            .or_else(|| {
                (body.get("status").and_then(Value::as_str) == Some("OK")
                    && body.get("resultsCount").and_then(Value::as_u64) == Some(0))
                .then_some(0)
            }),
        "gdelt" => body.get("articles").and_then(Value::as_array).map(Vec::len),
        "finnhub" => body
            .get("c")
            .and_then(Value::as_f64)
            .filter(|p| *p > 0.0)
            .map(|_| 1),
        "alpha_vantage" => body
            .pointer("/Global Quote/05. price")
            .and_then(Value::as_str)
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|p| p.is_finite() && *p > 0.0)
            .map(|_| 1),
        "telegram" => (body.get("ok") == Some(&Value::Bool(true))
            && body.pointer("/result/is_bot") == Some(&Value::Bool(true)))
        .then_some(1),
        _ => None,
    };
    match records {
        Some(0) => ("empty_data", Some(0)),
        Some(n) => ("healthy", Some(n)),
        None => ("invalid_response", None),
    }
}

fn feed_records(bytes: &[u8]) -> Option<usize> {
    use quick_xml::{Reader, events::Event};
    let mut reader = Reader::from_reader(bytes);
    let mut root = false;
    let mut depth = 0;
    let mut count = 0;
    loop {
        match reader.read_event().ok()? {
            Event::Start(e) => {
                let name = e.local_name();
                if depth == 0 {
                    if root || !matches!(name.as_ref(), b"rss" | b"feed" | b"RDF") {
                        return None;
                    }
                    root = true;
                }
                if matches!(name.as_ref(), b"item" | b"entry") {
                    count += 1;
                }
                depth += 1;
            }
            Event::End(_) => depth -= 1,
            Event::Eof => return (root && depth == 0).then_some(count),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use trady_application::research::SourceFailure;

    #[derive(Default)]
    struct Stub {
        calls: usize,
        body: Vec<u8>,
        url: Option<Url>,
        headers: Vec<(String, String)>,
        failure_reason: Option<&'static str>,
    }
    impl HttpTransport for Stub {
        fn get(
            &mut self,
            request: HttpRequest,
        ) -> std::result::Result<HttpResponse, SourceFailure> {
            self.get_with_headers(request, &[])
        }
        fn get_with_headers(
            &mut self,
            request: HttpRequest,
            headers: &[(String, String)],
        ) -> std::result::Result<HttpResponse, SourceFailure> {
            assert_eq!(request.redirects, 0);
            assert_eq!(request.url.scheme(), "https");
            self.calls += 1;
            self.url = Some(request.url);
            self.headers = headers.to_vec();
            if let Some(reason) = self.failure_reason {
                return Err(SourceFailure::unavailable(reason));
            }
            Ok(HttpResponse {
                status: 200,
                body: self.body.clone(),
                ..Default::default()
            })
        }
    }
    #[test]
    fn transport_diagnostics_only_expose_known_safe_codes() {
        for (reason, expected) in [
            ("dns_resolution_timeout", "dns_resolution_timeout"),
            ("http_timeout", "http_timeout"),
            ("http_connection_failed", "http_connection_failed"),
            ("private-token", "transport_failed"),
        ] {
            let mut stub = Stub {
                failure_reason: Some(reason),
                ..Default::default()
            };
            let check = probe(
                "finnhub",
                true,
                Some("private-token"),
                Utc::now(),
                &mut stub,
            )
            .unwrap();
            assert_eq!(check.failure_code, Some(expected));
            assert!(
                !serde_json::to_string(&check)
                    .unwrap()
                    .contains("private-token")
            );
        }
    }
    #[test]
    fn gdelt_throttle_text_is_not_success_or_an_empty_article_list() {
        let response = HttpResponse {
            status: 200,
            body: b"Please limit requests to one every 5 seconds or contact the provider.".to_vec(),
            ..Default::default()
        };
        assert_eq!(classify("gdelt", &response), ("rate_limited", None));
        assert_eq!(classify("finnhub", &response), ("invalid_response", None));
    }
    #[test]
    fn gdelt_missing_article_collection_is_invalid_and_carries_a_safe_backoff() {
        for body in [
            json!({}),
            json!({"articles":null}),
            json!({"articles":"invalid"}),
        ] {
            let mut stub = Stub {
                body: serde_json::to_vec(&body).unwrap(),
                ..Default::default()
            };
            let check = probe("gdelt", true, None, Utc::now(), &mut stub).unwrap();
            assert_eq!(check.status, "invalid_response");
            assert_eq!(check.response_records, None);
            assert_eq!(check.failure_code, Some("gdelt_response_invalid"));
            assert_eq!(check.retry_after_seconds, Some(60));
            assert_eq!(stub.calls, 1);
        }
        let mut stub = Stub {
            body: br#"{"articles":[]}"#.to_vec(),
            ..Default::default()
        };
        let check = probe("gdelt", true, None, Utc::now(), &mut stub).unwrap();
        assert_eq!(check.status, "empty_data");
        assert_eq!(check.response_records, Some(0));
        assert_eq!(check.failure_code, None);
        assert_eq!(check.retry_after_seconds, None);
    }
    #[test]
    fn gdelt_throttle_notice_has_a_backoff_without_an_http_retry_header() {
        let mut stub = Stub {
            body: b"Please limit requests to one every 5 seconds".to_vec(),
            ..Default::default()
        };
        let check = probe("gdelt", true, None, Utc::now(), &mut stub).unwrap();
        assert_eq!(check.status, "rate_limited");
        assert_eq!(check.retry_after_seconds, Some(60));
        assert_eq!(stub.calls, 1);
    }
    #[test]
    fn offline_and_cost_gated_checks_never_call_transport() {
        let mut stub = Stub::default();
        for provider in PROVIDERS {
            let check = probe(
                provider,
                false,
                Some("private-token"),
                Utc::now(),
                &mut stub,
            )
            .unwrap();
            assert_eq!(check.network_requests, 0);
        }
        for provider in ["x", "yfinance"] {
            probe(provider, true, Some("private-token"), Utc::now(), &mut stub).unwrap();
        }
        assert_eq!(stub.calls, 0);
    }
    #[test]
    fn query_path_and_header_credentials_are_not_reported() {
        for provider in ["finnhub", "telegram", "tiingo"] {
            let token = if provider == "telegram" {
                "123456:private-token"
            } else {
                "private-token"
            };
            let mut stub = Stub {
                body: br#"{"error":"private-token"}"#.to_vec(),
                ..Default::default()
            };
            let check = probe(provider, true, Some(token), Utc::now(), &mut stub).unwrap();
            assert_eq!(stub.calls, 1);
            assert!(
                !serde_json::to_string(&check)
                    .unwrap()
                    .contains("private-token")
            );
            assert_eq!(check.status, "provider_error");
        }
    }
    #[test]
    fn http_200_error_payloads_and_invalid_shapes_do_not_pass() {
        for (body, status) in [
            (json!({"Information":"rate limit reached"}), "rate_limited"),
            (
                json!({"Error Message":"premium subscription required"}),
                "credits_or_subscription_required",
            ),
            (json!({"c":0}), "invalid_response"),
            (json!({"c":450}), "healthy"),
        ] {
            let response = HttpResponse {
                status: 200,
                body: serde_json::to_vec(&body).unwrap(),
                ..Default::default()
            };
            assert_eq!(classify("finnhub", &response).0, status);
        }
    }
    #[test]
    fn empty_and_malformed_feeds_are_distinguished() {
        assert_eq!(
            feed_records(b"<rss><channel><item><title>A</title></item></channel></rss>"),
            Some(1)
        );
        assert_eq!(feed_records(b"<rss><channel></channel></rss>"), Some(0));
        assert_eq!(feed_records(b"<html><body>denied</body></html>"), None);
        assert_eq!(feed_records(b"<rss><channel>"), None);
    }
    #[test]
    fn missing_and_malformed_credentials_do_not_reach_the_network() {
        let mut stub = Stub::default();
        assert_eq!(
            probe("fred", true, None, Utc::now(), &mut stub)
                .unwrap()
                .status,
            "missing_credential"
        );
        assert_eq!(
            probe(
                "telegram",
                true,
                Some("123:secret/../sendMessage"),
                Utc::now(),
                &mut stub
            )
            .unwrap()
            .status,
            "invalid_credential_or_route"
        );
        assert_eq!(stub.calls, 0);
        let response = HttpResponse {
            status: 200,
            body: br#"[{"error":"denied"}]"#.to_vec(),
            ..Default::default()
        };
        assert_eq!(classify("fmp", &response).0, "invalid_response");
    }
}
