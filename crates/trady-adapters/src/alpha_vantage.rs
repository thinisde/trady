//! Fixed, read-only Alpha Vantage historical crosscheck. The application must
//! reserve its shared PostgreSQL allowance before calling this port.
use crate::news::{HttpRequest, HttpTransport, SafeHttp};
use reqwest::Url;
use serde_json::{Value, json};
use std::{
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};
use trady_domain::{Error, Result, require};

// The provider's free response specifies one request per second. This paces
// historical calls within the process; the durable shared PostgreSQL budget
// remains the authority for all request allowances across processes.
static LAST_REQUEST: Mutex<Option<Instant>> = Mutex::new(None);
const REQUEST_SPACING: Duration = Duration::from_millis(1100);

pub fn daily_history(symbol: &str, api_key: &str) -> Result<Value> {
    require(
        !symbol.is_empty()
            && symbol.len() <= 48
            && symbol
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c)),
        "invalid reviewed Alpha Vantage symbol",
    )?;
    require(
        !api_key.is_empty() && !api_key.chars().any(char::is_control),
        "Alpha Vantage credential unavailable",
    )?;
    let mut url = Url::parse("https://www.alphavantage.co/query")
        .map_err(|_| Error("invalid fixed historical provider endpoint".into()))?;
    url.query_pairs_mut()
        .append_pair("function", "TIME_SERIES_DAILY")
        .append_pair("symbol", symbol)
        .append_pair("outputsize", "compact")
        .append_pair("apikey", api_key);
    let mut pacing = LAST_REQUEST
        .lock()
        .map_err(|_| Error("historical provider pacing unavailable".into()))?;
    if let Some(previous) = *pacing {
        thread::sleep(REQUEST_SPACING.saturating_sub(previous.elapsed()));
    }
    *pacing = Some(Instant::now());
    let response = SafeHttp
        .get(HttpRequest {
            url,
            allowed_hosts: vec!["www.alphavantage.co".into()],
            bearer: None,
            etag: None,
            last_modified: None,
            redirects: 0,
        })
        .map_err(|_| Error("Alpha Vantage historical transport unavailable".into()))?;
    drop(pacing);
    // Never retain a token echoed in a provider error, URL or body.
    let body = String::from_utf8_lossy(&response.body).replace(api_key, "[REDACTED]");
    let value: Value =
        serde_json::from_str(&body).unwrap_or_else(|_| json!({"unparseable_response":body}));
    let code = if response.status != 200 {
        Some("ALPHA_HTTP_UNAVAILABLE")
    } else if value.get("Note").is_some() || value.get("Information").is_some() {
        Some("ALPHA_QUOTA_OR_ENTITLEMENT_NOTICE")
    } else if value
        .pointer("/Meta Data/2. Symbol")
        .and_then(Value::as_str)
        != Some(symbol)
        || !value
            .get("Time Series (Daily)")
            .and_then(Value::as_object)
            .is_some_and(|rows| !rows.is_empty() && rows.len() <= 100)
    {
        Some("ALPHA_HISTORICAL_COVERAGE_UNAVAILABLE")
    } else {
        None
    };
    Ok(
        json!({"provider_status":if code.is_none(){"OK"}else{"UNAVAILABLE"},"error_code":code,
        "provider":"alpha_vantage","endpoint":"TIME_SERIES_DAILY","symbol":symbol,
        "role":"HISTORICAL_CROSSCHECK_ONLY","http_status":response.status,"raw_response":value,
        "executable_quote_authority":false,"entry_authorized":false}),
    )
}
