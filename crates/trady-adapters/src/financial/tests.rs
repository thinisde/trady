use super::*;
use crate::news::HttpResponse;
use serde_json::json;
use trady_application::research::SourceFailure;

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-31T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}
fn source(id: &str) -> FinancialSource {
    let config: FinancialConfig =
        serde_json::from_str(include_str!("../../../../config/financial-data.json")).unwrap();
    config.validate().unwrap();
    let mut s = config.sources.into_iter().find(|s| s.id == id).unwrap();
    s.enabled = true;
    s.mapping_reference = "reviewed_mapping".into();
    s.capability.status = AccessStatus::Verified;
    s.capability.revision = "review_v1".into();
    s.capability.evidence_reference = "offline_fixture".into();
    s.capability.reviewed_at = Some(now() - Duration::days(1));
    s.capability.valid_until = Some(now() + Duration::days(30));
    s.rights.normalized_storage = true;
    s.rights.display = true;
    s.rights.terms_reference = "fixture_rights".into();
    s
}
fn window() -> DataWindow {
    DataWindow {
        start: "2026-10-01".parse().unwrap(),
        end: "2026-10-30".parse().unwrap(),
    }
}
struct Fixture {
    body: Vec<u8>,
    status: u16,
}
impl HttpTransport for Fixture {
    fn get(&mut self, request: HttpRequest) -> std::result::Result<HttpResponse, SourceFailure> {
        assert_eq!(request.redirects, 0);
        assert_eq!(request.allowed_hosts.len(), 1);
        Ok(HttpResponse {
            status: self.status,
            body: self.body.clone(),
            retry_after: Some("3600".into()),
            ..Default::default()
        })
    }
    fn get_with_headers(
        &mut self,
        request: HttpRequest,
        _: &[(String, String)],
    ) -> std::result::Result<HttpResponse, SourceFailure> {
        self.get(request)
    }
}
fn fetch(
    s: &FinancialSource,
    v: serde_json::Value,
) -> std::result::Result<FinancialPage, FinancialFailure> {
    FinancialClient::new(
        Fixture {
            body: serde_json::to_vec(&v).unwrap(),
            status: 200,
        },
        |name| {
            Ok(Some(
                if name == "TRADY_SEC_USER_AGENT" {
                    "Fixture owner owner@example.test"
                } else {
                    "fixture-private-key"
                }
                .into(),
            ))
        },
        now,
    )
    .fetch(s, &window(), now())
}
#[test]
fn credential_routes_are_fixed_and_private() {
    let s = source("fred_treasury_10y");
    let (r, h) = request(&s, &window(), Some("private_value")).unwrap();
    assert_eq!(r.url.host_str(), Some("api.stlouisfed.org"));
    assert_eq!(r.redirects, 0);
    assert!(h.is_empty());
    assert!(
        r.url
            .query_pairs()
            .any(|(k, v)| k == "api_key" && v == "private_value")
    );
    let s = source("sec_revenue");
    let (_, headers) = request(&s, &window(), Some("Trady owner owner@example.test")).unwrap();
    assert_eq!(headers[0].0, "User-Agent");
    assert!(request(&s, &window(), Some("fake")).is_err());
    assert!(request(&source("yfinance_offline"), &window(), None).is_err());
}
#[test]
fn fred_missing_values_are_null_and_downloads_never_backdate_ingestion() {
    let mut s = source("fred_treasury_10y");
    s.request = DataRequest::FredObservations {
        series_id: "DGS10".into(),
        vintage: Some("2026-10-29".parse().unwrap()),
    };
    let page=fetch(&s,json!({"count":2,"observations":[{"date":"2026-10-01","realtime_start":"2026-10-29","value":"4.25"},{"date":"2026-10-02","realtime_start":"2026-10-29","value":"."}]})).unwrap();
    assert!(page.complete);
    assert_eq!(page.observations[0].received_at, now());
    assert!(!page.observations[0].source_url.contains("api_key"));
    assert!(matches!(
        page.observations[1].payload,
        FinancialPayload::Metric { value: None, .. }
    ));
    let result = snapshot(
        &s,
        page.observations,
        now() - Duration::days(1),
        Consumer::Display,
        true,
        now(),
    )
    .unwrap();
    assert_eq!(result.status, DataStatus::Unavailable);
}
#[test]
fn price_identity_currency_completion_and_ohlc_are_checked() {
    let s = source("twelve_daily");
    let good = json!({"meta":{"symbol":"AAPL","exchange":"NASDAQ","interval":"1day","currency":"USD"},"values":[{"datetime":"2026-10-30","open":"10","high":"12","low":"9","close":"11","volume":"1000"}]});
    let page = fetch(&s, good.clone()).unwrap();
    assert!(page.complete);
    let mut wrong = good.clone();
    wrong["meta"]["currency"] = json!("EUR");
    assert!(fetch(&s, wrong).is_err());
    let mut wrong = good.clone();
    wrong["values"][0]["low"] = json!("13");
    assert!(fetch(&s, wrong).is_err());
    let mut wrong = good;
    wrong["values"][0]["datetime"] = json!("2026-10-31");
    assert!(fetch(&s, wrong).is_err());
}
#[test]
fn provider_errors_caps_and_retry_deadlines_never_become_empty_success() {
    let s = source("fred_treasury_10y");
    assert!(
        fetch(
            &s,
            json!({"error_code":400,"error_message":"private_value"})
        )
        .is_err()
    );
    let result = fetch(&s, json!({"count":20,"observations":[]})).unwrap();
    assert!(!result.complete);
    let mut client = FinancialClient::new(
        Fixture {
            body: vec![],
            status: 429,
        },
        |_| Ok(Some("private_value".into())),
        now,
    );
    let error = client.fetch(&s, &window(), now()).unwrap_err();
    assert_eq!(error.retry_at, Some(now() + Duration::hours(1)));
    assert!(!format!("{error:?}").contains("private_value"));
}
#[test]
fn sec_facts_preserve_fiscal_period_and_restatement_versions() {
    let s = source("sec_revenue");
    let p=fetch(&s,json!({"cik":320193,"taxonomy":"us-gaap","tag":"Revenues","units":{"USD":[
        {"start":"2026-07-01","end":"2026-10-01","filed":"2026-10-28","accn":"0001-26-1","fp":"Q4","val":100},
        {"start":"2026-07-01","end":"2026-10-01","filed":"2026-10-29","accn":"0001-26-2","fp":"Q4","val":110}]}})).unwrap();
    assert_eq!(p.observations.len(), 2);
    assert_eq!(
        p.observations[0].external_key,
        p.observations[1].external_key
    );
    assert_ne!(
        p.observations[0].content_hash,
        p.observations[1].content_hash
    );
    assert_eq!(
        p.observations[0].available_at,
        "2026-10-29T00:00:00Z".parse::<DateTime<Utc>>().unwrap()
    );
}
#[test]
fn current_rights_and_capability_gate_every_consumer() {
    let s = source("fred_treasury_10y");
    let page=fetch(&s,json!({"count":1,"observations":[{"date":"2026-10-01","realtime_start":"2026-10-29","value":"4.25"}]})).unwrap();
    assert_eq!(
        snapshot(
            &s,
            page.observations.clone(),
            now(),
            Consumer::ExternalLlm,
            true,
            now()
        )
        .unwrap()
        .status,
        DataStatus::Denied
    );
    let mut revoked = s.clone();
    revoked.capability.valid_until = Some(now());
    assert_eq!(
        snapshot(
            &revoked,
            page.observations,
            now(),
            Consumer::Display,
            true,
            now()
        )
        .unwrap()
        .status,
        DataStatus::Denied
    );
}

#[test]
fn retry_after_parses_seconds_dates_and_reset_fallback_with_bounded_deadlines() {
    struct RetryTransport {
        status: u16,
        retry_after: Option<String>,
        rate_reset: Option<i64>,
    }
    impl HttpTransport for RetryTransport {
        fn get(&mut self, _: HttpRequest) -> std::result::Result<HttpResponse, SourceFailure> {
            Ok(HttpResponse {
                status: self.status,
                body: vec![],
                retry_after: self.retry_after.clone(),
                rate_reset: self.rate_reset,
                ..Default::default()
            })
        }
        fn get_with_headers(
            &mut self,
            request: HttpRequest,
            _: &[(String, String)],
        ) -> std::result::Result<HttpResponse, SourceFailure> {
            self.get(request)
        }
    }
    let at = now();
    let reset = (at + Duration::hours(2)).timestamp();
    for status in [429, 503] {
        for (retry_after, rate_reset, expected) in [
            (Some("3600"), Some(reset), at + Duration::hours(1)),
            (Some("0"), None, at + Duration::seconds(1)),
            (Some("-10"), None, at + Duration::seconds(1)),
            (Some("1000000"), None, at + Duration::seconds(1000000)),
            (
                Some("Sat, 31 Oct 2026 13:00:00 +0000"),
                Some(reset),
                at + Duration::hours(1),
            ),
            (
                Some("Sat, 31 Oct 2026 10:00:00 +0000"),
                None,
                at + Duration::seconds(1),
            ),
            (Some("invalid"), Some(reset), at + Duration::hours(2)),
            (None, Some(reset), at + Duration::hours(2)),
            (
                None,
                Some((at - Duration::hours(1)).timestamp()),
                at + Duration::seconds(1),
            ),
            (Some("invalid"), Some(i64::MAX), at + Duration::minutes(15)),
            (None, None, at + Duration::minutes(15)),
        ] {
            let transport = RetryTransport {
                status,
                retry_after: retry_after.map(String::from),
                rate_reset,
            };
            let mut client = FinancialClient::new(
                transport,
                |_| Ok(Some("TEST_ONLY_PRIVATE_TOKEN".into())),
                now,
            );
            let error = client
                .fetch(&source("fred_treasury_10y"), &window(), at)
                .unwrap_err();
            assert_eq!(
                error.retry_at,
                Some(expected),
                "status={status} retry={retry_after:?} reset={rate_reset:?}"
            );
            assert_eq!(error.code, "financial_provider_rate_limited");
            assert!(!format!("{error:?}").contains("TEST_ONLY_PRIVATE_TOKEN"));
        }
    }
}

#[test]
fn financial_signals_keep_dependency_units_missingness_and_model_permissions() {
    let mut s = source("twelve_daily");
    s.rights.local_inference = true;
    let page=fetch(&s,json!({"meta":{"symbol":"AAPL","exchange":"NASDAQ","interval":"1day","currency":"USD"},"values":[
        {"datetime":"2026-10-29","open":"10","high":"10","low":"10","close":"10","volume":"100"},
        {"datetime":"2026-10-30","open":"11","high":"11","low":"11","close":"11","volume":"100"}]})).unwrap();
    let result = signals(
        snapshot(
            &s,
            page.observations.clone(),
            now(),
            Consumer::LocalInference,
            true,
            now(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        result.return_fraction.unwrap(),
        "0.1".parse::<rust_decimal::Decimal>().unwrap()
    );
    assert_eq!(
        result.mean_close.unwrap(),
        "10.5".parse::<rust_decimal::Decimal>().unwrap()
    );
    let features = model_features(&result).unwrap();
    assert_eq!(features.features.len(), 3);
    assert_eq!(features.features[0].lineage.len(), 2);
    let result = signals(
        snapshot(
            &s,
            page.observations,
            now(),
            Consumer::Display,
            false,
            now(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(result.last_close, None);
    assert!(model_features(&result).is_err());
}

#[test]
fn json_decimal_numbers_retain_financial_precision() {
    let s = source("sec_revenue");
    let body=br#"{"cik":320193,"taxonomy":"us-gaap","tag":"Revenues","units":{"USD":[{"end":"2026-10-01","filed":"2026-10-28","accn":"v1","val":12345678901234567.89}]}}"#;
    let page = super::parse::page(
        &s,
        &window(),
        now(),
        "https://data.sec.gov/api/xbrl/companyconcept/fixture.json",
        body,
    )
    .unwrap();
    match &page.observations[0].payload {
        FinancialPayload::Metric {
            value: Some(value), ..
        } => assert_eq!(value.to_string(), "12345678901234567.89"),
        _ => panic!("missing fact"),
    }
}

#[test]
fn catalog_enrichments_parse_reviewed_fields_and_leave_continuations_partial() {
    let fmp=fetch(&source("fmp_revenue"),json!([{"symbol":"AAPL","reportedCurrency":"USD","date":"2026-10-01","filingDate":"2026-10-29","revenue":1234}])).unwrap();
    assert!(matches!(
        fmp.observations[0].payload,
        FinancialPayload::Metric { value: Some(_), .. }
    ));
    let crypto=fetch(&source("crypto_context"),json!([{"id":"bitcoin","current_price":70000.25,"market_cap":1000000,"last_updated":"2026-10-31T11:00:00Z"}])).unwrap();
    assert!(matches!(
        crypto.observations[0].payload,
        FinancialPayload::MarketContext { .. }
    ));
    let tiingo=fetch(&source("tiingo_daily"),json!([{"date":"2026-10-30T00:00:00Z","open":10,"high":12,"low":9,"close":11,"volume":100}])).unwrap();
    assert!(tiingo.complete);
    assert_eq!(tiingo.observations.len(), 1);
    let massive=fetch(&source("massive_daily"),json!({"ticker":"AAPL","adjusted":false,"results":[{"t":1793318400000_i64,"o":10,"h":12,"l":9,"c":11,"v":100}],"next_url":"https://127.0.0.1/private?apiKey=private"})).unwrap();
    assert!(!massive.complete);
    assert!(!massive.observations[0].source_url.contains("apiKey"));
}

#[test]
fn future_macro_vintages_and_invalid_offline_sources_are_rejected() {
    let s = source("fred_treasury_10y");
    assert!(fetch(&s,json!({"count":1,"observations":[{"date":"2026-10-01","realtime_start":"2026-11-01","value":"4.5"}]})).is_err());
    let mut s = source("yfinance_offline");
    s.max_records = 501;
    assert!(offline_page(&s, now(), b"[]").is_err());
    let s = source("sec_filings");
    let p=fetch(&s,json!({"cik":"320193","filings":{"recent":{"accessionNumber":["0001-26-1"],"form":["10-Q"],"filingDate":["2026-10-29"],"primaryDocument":["report.htm"]},"files":[]}})).unwrap();
    assert!(p.complete);
    assert!(matches!(
        p.observations[0].payload,
        FinancialPayload::Filing { .. }
    ));
}
