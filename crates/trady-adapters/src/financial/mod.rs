//! Fixed read-only routes, central credentials and bounded provider normalization.

mod parse;

use crate::{
    credentials,
    news::{HttpRequest, HttpTransport, MAX_BODY_BYTES, SafeHttp},
};
use chrono::{DateTime, Duration, Utc};
use reqwest::Url;
use trady_application::financial::*;
use trady_domain::{Error, Result, financial::*};

pub const PARSER_VERSION: &str = "trady-financial-1";
pub struct FinancialClient<T = SafeHttp> {
    pub transport: T,
    credential: fn(&str) -> Result<Option<String>>,
    receipt_clock: fn() -> DateTime<Utc>,
}
impl Default for FinancialClient<SafeHttp> {
    fn default() -> Self {
        Self {
            transport: SafeHttp,
            credential: |name| credentials::optional(name).map(|v| v.map(|s| s.into_inner())),
            receipt_clock: Utc::now,
        }
    }
}
impl<T: HttpTransport> FinancialClient<T> {
    pub fn new(
        transport: T,
        credential: fn(&str) -> Result<Option<String>>,
        receipt_clock: fn() -> DateTime<Utc>,
    ) -> Self {
        Self {
            transport,
            credential,
            receipt_clock,
        }
    }
}
fn failure(code: &'static str) -> FinancialFailure {
    FinancialFailure {
        status: DataStatus::Error,
        code,
        retry_at: None,
    }
}

/// No arbitrary URL, credential name, pagination URL, or paid fallback is accepted.
pub fn request(
    source: &FinancialSource,
    window: &DataWindow,
    credential: Option<&str>,
) -> Result<(HttpRequest, Vec<(String, String)>)> {
    source.validate()?;
    let token = if source.request.provider().credential_name().is_some() {
        Some(
            credential
                .filter(|s| {
                    !s.trim().is_empty() && s.len() <= 4096 && !s.chars().any(char::is_control)
                })
                .ok_or_else(|| Error("financial credential unavailable".into()))?,
        )
    } else {
        None
    };
    let mut headers = vec![];
    let mut url = match &source.request {
        DataRequest::SecConcept {
            cik,
            taxonomy,
            concept,
        } => Url::parse(&format!(
            "https://data.sec.gov/api/xbrl/companyconcept/CIK{cik}/{taxonomy}/{concept}.json"
        )),
        DataRequest::SecSubmissions { cik } => {
            Url::parse(&format!("https://data.sec.gov/submissions/CIK{cik}.json"))
        }
        DataRequest::FredObservations { .. } => {
            Url::parse("https://api.stlouisfed.org/fred/series/observations")
        }
        DataRequest::TwelveDaily { .. } => Url::parse("https://api.twelvedata.com/time_series"),
        DataRequest::FmpIncome { .. } => {
            Url::parse("https://financialmodelingprep.com/stable/income-statement")
        }
        DataRequest::CoinGeckoMarkets { .. } => {
            Url::parse("https://api.coingecko.com/api/v3/coins/markets")
        }
        DataRequest::TiingoDaily { symbol } => Url::parse(&format!(
            "https://api.tiingo.com/tiingo/daily/{symbol}/prices"
        )),
        DataRequest::MassiveDaily { symbol } => Url::parse(&format!(
            "https://api.massive.com/v2/aggs/ticker/{symbol}/range/1/day/{}/{}",
            window.start, window.end
        )),
        DataRequest::YfinanceImport => {
            return Err(Error("yfinance is an offline import only".into()));
        }
    }
    .map_err(|_| Error("financial route invalid".into()))?;
    {
        let mut q = url.query_pairs_mut();
        match &source.request {
            DataRequest::SecConcept { .. } | DataRequest::SecSubmissions { .. } => {
                let agent = token.unwrap();
                if !agent.contains('@') || agent.len() < 12 {
                    return Err(Error(
                        "SEC requires an operator-supplied contact-bearing user agent".into(),
                    ));
                }
                headers.push(("User-Agent".into(), agent.into()));
            }
            DataRequest::FredObservations { series_id, vintage } => {
                q.append_pair("series_id", series_id)
                    .append_pair("file_type", "json")
                    .append_pair("observation_start", &window.start.to_string())
                    .append_pair("observation_end", &window.end.to_string())
                    .append_pair("limit", &source.max_records.to_string())
                    .append_pair("sort_order", "asc")
                    .append_pair("api_key", token.unwrap());
                if let Some(day) = vintage {
                    q.append_pair("realtime_start", &day.to_string())
                        .append_pair("realtime_end", &day.to_string());
                }
            }
            DataRequest::TwelveDaily { symbol, exchange } => {
                q.append_pair("symbol", symbol)
                    .append_pair("exchange", exchange)
                    .append_pair("interval", "1day")
                    .append_pair("start_date", &window.start.to_string())
                    .append_pair("end_date", &window.end.to_string())
                    .append_pair("outputsize", &source.max_records.to_string())
                    .append_pair("format", "JSON")
                    .append_pair("order", "asc")
                    .append_pair("adjust", "none")
                    .append_pair("apikey", token.unwrap());
            }
            DataRequest::FmpIncome { symbol, .. } => {
                q.append_pair("symbol", symbol)
                    .append_pair("limit", &source.max_records.to_string())
                    .append_pair("apikey", token.unwrap());
            }
            DataRequest::CoinGeckoMarkets { coin_id } => {
                q.append_pair("vs_currency", source.currency.as_deref().unwrap_or("USD"))
                    .append_pair("ids", coin_id)
                    .append_pair("per_page", "1")
                    .append_pair("page", "1");
                headers.push(("x-cg-demo-api-key".into(), token.unwrap().into()));
            }
            DataRequest::TiingoDaily { .. } => {
                if token.unwrap().len() > 4090 {
                    return Err(Error("financial credential exceeds header bound".into()));
                }
                q.append_pair("startDate", &window.start.to_string())
                    .append_pair("endDate", &window.end.to_string())
                    .append_pair("resampleFreq", "daily");
                headers.push(("Authorization".into(), format!("Token {}", token.unwrap())));
            }
            DataRequest::MassiveDaily { .. } => {
                q.append_pair("adjusted", "false")
                    .append_pair("sort", "asc")
                    .append_pair("limit", &source.max_records.to_string())
                    .append_pair("apiKey", token.unwrap());
            }
            DataRequest::YfinanceImport => unreachable!(),
        }
    }
    // Query credentials never become provenance or error text.
    if url.query() == Some("") {
        url.set_query(None);
    }
    let host = url
        .host_str()
        .ok_or_else(|| Error("financial route missing host".into()))?
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
        headers,
    ))
}

impl<T: HttpTransport> FinancialProvider for FinancialClient<T> {
    fn fetch(
        &mut self,
        source: &FinancialSource,
        window: &DataWindow,
        now: DateTime<Utc>,
    ) -> std::result::Result<FinancialPage, FinancialFailure> {
        if !source.usable(now) {
            return Err(FinancialFailure {
                status: DataStatus::Denied,
                code: "financial_source_not_approved",
                retry_at: None,
            });
        }
        window
            .validate(now)
            .map_err(|_| failure("invalid_financial_window"))?;
        let token = source
            .request
            .provider()
            .credential_name()
            .map(self.credential)
            .transpose()
            .map_err(|_| failure("financial_credential_unavailable"))?
            .flatten();
        let (request, headers) = request(source, window, token.as_deref())
            .map_err(|_| failure("financial_request_or_credential_invalid"))?;
        let mut source_url = request.url.clone();
        source_url.set_query(None);
        let response = self
            .transport
            .get_with_headers(request, &headers)
            .map_err(|_| failure("financial_transport_unavailable"))?;
        let now = (self.receipt_clock)().max(now);
        if response.status == 429 || response.status == 503 {
            let retry_at = response
                .retry_after
                .as_deref()
                .and_then(|s| {
                    s.parse::<i64>().ok().map(|n| {
                        Duration::try_seconds(n.max(1))
                            .and_then(|d| now.checked_add_signed(d))
                            .unwrap_or(DateTime::<Utc>::MAX_UTC)
                    })
                })
                .or_else(|| {
                    response.retry_after.as_deref().and_then(|s| {
                        DateTime::parse_from_rfc2822(s)
                            .ok()
                            .map(|t| t.with_timezone(&Utc))
                    })
                })
                .or_else(|| {
                    response
                        .rate_reset
                        .and_then(|n| DateTime::from_timestamp(n, 0))
                })
                .unwrap_or(now + Duration::minutes(15))
                .max(now + Duration::seconds(1));
            return Err(FinancialFailure {
                status: DataStatus::Unavailable,
                code: "financial_provider_rate_limited",
                retry_at: Some(retry_at),
            });
        }
        if response.status != 200 {
            return Err(FinancialFailure {
                status: if matches!(response.status, 401..=403) {
                    DataStatus::Denied
                } else {
                    DataStatus::Error
                },
                code: "financial_provider_http_failure",
                retry_at: None,
            });
        }
        if response.body.len() > MAX_BODY_BYTES {
            return Err(failure("financial_response_too_large"));
        }
        parse::page(source, window, now, source_url.as_str(), &response.body)
    }
}

pub use parse::offline_page;

#[cfg(test)]
mod tests;
