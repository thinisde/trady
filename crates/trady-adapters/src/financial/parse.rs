use super::*;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::Value;
use trady_application::research_jobs::digest;

fn text<'a>(v: &'a Value, key: &str) -> std::result::Result<&'a str, FinancialFailure> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| s.len() <= 512)
        .ok_or_else(|| failure("financial_required_field_missing"))
}
fn decimal(v: &Value) -> std::result::Result<Option<Decimal>, FinancialFailure> {
    if v.is_null() || v.as_str() == Some(".") {
        return Ok(None);
    }
    let s = v
        .as_str()
        .map(String::from)
        .unwrap_or_else(|| v.to_string());
    s.parse()
        .or_else(|_| Decimal::from_scientific(&s))
        .map(Some)
        .map_err(|_| failure("financial_decimal_invalid"))
}
fn number(v: &Value, key: &str) -> std::result::Result<Decimal, FinancialFailure> {
    decimal(
        v.get(key)
            .ok_or_else(|| failure("financial_required_field_missing"))?,
    )?
    .ok_or_else(|| failure("financial_required_value_missing"))
}
fn date(s: &str) -> std::result::Result<NaiveDate, FinancialFailure> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| failure("financial_date_invalid"))
}
fn date_field(v: &Value, key: &str) -> std::result::Result<NaiveDate, FinancialFailure> {
    date(text(v, key)?)
}
fn next_day(d: NaiveDate) -> std::result::Result<DateTime<Utc>, FinancialFailure> {
    d.succ_opt()
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|d| d.and_utc())
        .ok_or_else(|| failure("financial_date_invalid"))
}
fn array<'a>(v: &'a Value, key: &str) -> std::result::Result<&'a Vec<Value>, FinancialFailure> {
    v.get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| failure("financial_rows_missing"))
}
fn observation(
    source: &FinancialSource,
    payload: FinancialPayload,
    key: String,
    available: DateTime<Utc>,
    now: DateTime<Utc>,
    url: &str,
    gaps: Vec<String>,
) -> std::result::Result<FinancialObservation, FinancialFailure> {
    let content_hash = digest(&(&payload, &gaps)).map_err(|_| failure("financial_hash_failed"))?;
    let o = FinancialObservation {
        id: digest(&(&source.id, &key, &content_hash, now))
            .map_err(|_| failure("financial_hash_failed"))?,
        source_id: source.id.clone(),
        provider: source.request.provider(),
        entity_id: source.entity_id.clone(),
        external_key: key,
        lineage_id: source.lineage_id.clone(),
        registry_hash: source
            .registry_hash()
            .map_err(|_| failure("financial_hash_failed"))?,
        content_hash,
        parser_version: PARSER_VERSION.into(),
        source_url: url.into(),
        unit: source.unit.clone(),
        available_at: available,
        received_at: now,
        expires_at: now + Duration::hours(i64::from(source.rights.retention_hours)),
        payload,
        gaps,
    };
    o.validate()
        .map_err(|_| failure("financial_observation_invalid"))?;
    Ok(o)
}
fn bar(
    source: &FinancialSource,
    v: &Value,
    session: NaiveDate,
    keys: [&str; 5],
    now: DateTime<Utc>,
    url: &str,
) -> std::result::Result<FinancialObservation, FinancialFailure> {
    let payload = FinancialPayload::DailyBar {
        session,
        open: number(v, keys[0])?,
        high: number(v, keys[1])?,
        low: number(v, keys[2])?,
        close: number(v, keys[3])?,
        volume: v.get(keys[4]).map(decimal).transpose()?.flatten(),
        currency: source
            .currency
            .clone()
            .ok_or_else(|| failure("financial_currency_missing"))?,
        adjustment: source.adjustment,
    };
    observation(
        source,
        payload,
        session.to_string(),
        next_day(session)?,
        now,
        url,
        vec![],
    )
}

pub(super) fn page(
    source: &FinancialSource,
    window: &DataWindow,
    now: DateTime<Utc>,
    url: &str,
    body: &[u8],
) -> std::result::Result<FinancialPage, FinancialFailure> {
    let v = financial_json(body)?;
    if v.get("error").is_some()
        || v.get("error_message").is_some()
        || v.get("Error Message").is_some()
        || v.get("Note").is_some()
        || v.get("Information").is_some()
        || v.get("quandl_error").is_some()
        || v["status"] == "error"
        || v["status"] == "ERROR"
    {
        return Err(FinancialFailure {
            status: DataStatus::Unavailable,
            code: "financial_provider_error_body",
            retry_at: Some(now + Duration::minutes(15)),
        });
    }
    let mut observations = vec![];
    let mut complete = true;
    let mut gaps = vec![];
    match &source.request {
        DataRequest::FredObservations { series_id, vintage } => {
            let rows = array(&v, "observations")?;
            let count = v["count"]
                .as_u64()
                .ok_or_else(|| failure("fred_count_missing"))?;
            complete = count == rows.len() as u64 && rows.len() <= source.max_records;
            for row in rows.iter().take(source.max_records) {
                let day = date_field(row, "date")?;
                if day < window.start || day > window.end {
                    return Err(failure("fred_period_outside_window"));
                }
                let realtime = date_field(row, "realtime_start")?;
                if realtime > now.date_naive()
                    || vintage.is_some_and(|d| realtime > d || d > now.date_naive())
                {
                    return Err(failure("fred_vintage_mismatch"));
                }
                let value = decimal(
                    row.get("value")
                        .ok_or_else(|| failure("fred_value_missing"))?,
                )?;
                let row_gaps = if value.is_none() {
                    vec!["provider_missing_value".into()]
                } else {
                    vec![]
                };
                // Runtime uses actual ingestion even for an authentic historical vintage.
                observations.push(observation(
                    source,
                    FinancialPayload::Metric {
                        metric: series_id.clone(),
                        value,
                        period_start: None,
                        period_end: day,
                        vintage: Some(realtime),
                        source_revision: realtime.to_string(),
                    },
                    day.to_string(),
                    next_day(realtime)?.min(now),
                    now,
                    url,
                    row_gaps,
                )?);
            }
        }
        DataRequest::SecConcept {
            cik,
            taxonomy,
            concept,
        } => {
            let returned = v["cik"]
                .as_u64()
                .ok_or_else(|| failure("sec_cik_missing"))?;
            if format!("{returned:010}") != *cik
                || v["taxonomy"] != *taxonomy
                || v["tag"] != *concept
            {
                return Err(failure("sec_concept_identity_mismatch"));
            }
            let rows = v["units"]
                .get(&source.unit)
                .and_then(Value::as_array)
                .ok_or_else(|| failure("sec_requested_unit_unavailable"))?;
            let mut candidates = vec![];
            for row in rows {
                let end = date_field(row, "end")?;
                let filed = date_field(row, "filed")?;
                if end < window.start || end > window.end || next_day(filed)? > now {
                    continue;
                }
                let start = row
                    .get("start")
                    .and_then(Value::as_str)
                    .map(date)
                    .transpose()?;
                let accession = text(row, "accn")?.to_owned();
                let key = format!(
                    "{}:{}:{}",
                    start.map(|d| d.to_string()).unwrap_or_default(),
                    end,
                    row["fp"].as_str().unwrap_or_default()
                );
                candidates.push((
                    filed,
                    accession,
                    key,
                    FinancialPayload::Metric {
                        metric: concept.clone(),
                        value: decimal(
                            row.get("val")
                                .ok_or_else(|| failure("sec_fact_value_missing"))?,
                        )?,
                        period_start: start,
                        period_end: end,
                        vintage: Some(filed),
                        source_revision: text(row, "accn")?.into(),
                    },
                ));
            }
            candidates.sort_by(|a, b| (&a.0, &a.1, &a.2).cmp(&(&b.0, &b.1, &b.2)));
            complete = candidates.len() <= source.max_records;
            for (filed, _, key, payload) in
                candidates.into_iter().rev().take(source.max_records).rev()
            {
                observations.push(observation(
                    source,
                    payload,
                    key,
                    next_day(filed)?,
                    now,
                    url,
                    vec![],
                )?);
            }
        }
        DataRequest::SecSubmissions { cik } => {
            if v["cik"]
                .as_str()
                .and_then(|s| s.parse::<u64>().ok())
                .or_else(|| v["cik"].as_u64())
                .map(|n| format!("{n:010}"))
                != Some(cik.clone())
            {
                return Err(failure("sec_filer_identity_mismatch"));
            }
            let recent = &v["filings"]["recent"];
            let accessions = array(recent, "accessionNumber")?;
            for key in ["form", "filingDate", "primaryDocument"] {
                if array(recent, key)?.len() != accessions.len() {
                    return Err(failure("sec_submission_columns_mismatch"));
                }
            }
            // Older SEC files are explicitly left uncollected; no arbitrary continuation URLs.
            complete = v["filings"]["files"]
                .as_array()
                .is_some_and(|a| a.is_empty());
            for i in 0..accessions.len() {
                let day = date(
                    recent["filingDate"][i]
                        .as_str()
                        .ok_or_else(|| failure("sec_filing_date_missing"))?,
                )?;
                if day < window.start || day > window.end || next_day(day)? > now {
                    continue;
                }
                if observations.len() >= source.max_records {
                    complete = false;
                    break;
                }
                let accession = accessions[i]
                    .as_str()
                    .ok_or_else(|| failure("sec_accession_missing"))?
                    .to_owned();
                observations.push(observation(
                    source,
                    FinancialPayload::Filing {
                        accession: accession.clone(),
                        form: recent["form"][i]
                            .as_str()
                            .ok_or_else(|| failure("sec_form_missing"))?
                            .into(),
                        filed_on: day,
                        primary_document: recent["primaryDocument"][i]
                            .as_str()
                            .ok_or_else(|| failure("sec_document_missing"))?
                            .into(),
                    },
                    accession,
                    next_day(day)?,
                    now,
                    url,
                    vec![],
                )?);
            }
        }
        DataRequest::TwelveDaily { symbol, exchange } => {
            if v["meta"]["symbol"] != *symbol
                || v["meta"]["exchange"] != *exchange
                || v["meta"]["interval"] != "1day"
                || v["meta"]["currency"].as_str() != source.currency.as_deref()
            {
                return Err(failure("twelve_identity_or_currency_mismatch"));
            }
            let rows = array(&v, "values")?;
            complete = rows.len() < source.max_records;
            for row in rows.iter().take(source.max_records) {
                let day = date_field(row, "datetime")?;
                if day < window.start || day > window.end {
                    return Err(failure("price_period_outside_window"));
                }
                observations.push(bar(
                    source,
                    row,
                    day,
                    ["open", "high", "low", "close", "volume"],
                    now,
                    url,
                )?);
            }
        }
        DataRequest::TiingoDaily { .. } => {
            let rows = v.as_array().ok_or_else(|| failure("tiingo_rows_missing"))?;
            complete = rows.len() <= source.max_records;
            for row in rows.iter().take(source.max_records) {
                let at = DateTime::parse_from_rfc3339(text(row, "date")?)
                    .map_err(|_| failure("tiingo_date_invalid"))?;
                let day = at.date_naive();
                if day < window.start || day > window.end {
                    return Err(failure("price_period_outside_window"));
                }
                observations.push(bar(
                    source,
                    row,
                    day,
                    ["open", "high", "low", "close", "volume"],
                    now,
                    url,
                )?);
            }
        }
        DataRequest::MassiveDaily { symbol } => {
            if v["ticker"] != *symbol || v["adjusted"] != false {
                return Err(failure("massive_price_basis_mismatch"));
            }
            let rows = array(&v, "results")?;
            complete = v.get("next_url").is_none() && rows.len() < source.max_records;
            for row in rows.iter().take(source.max_records) {
                let at = row["t"]
                    .as_i64()
                    .and_then(DateTime::from_timestamp_millis)
                    .ok_or_else(|| failure("massive_timestamp_invalid"))?;
                let day = at.date_naive();
                if day < window.start || day > window.end {
                    return Err(failure("price_period_outside_window"));
                }
                observations.push(bar(source, row, day, ["o", "h", "l", "c", "v"], now, url)?);
            }
        }
        DataRequest::FmpIncome { symbol, metric } => {
            let rows = v
                .as_array()
                .ok_or_else(|| failure("fmp_statements_missing"))?;
            complete = rows.len() < source.max_records;
            for row in rows.iter().take(source.max_records) {
                if row["symbol"] != *symbol
                    || row["reportedCurrency"].as_str() != source.currency.as_deref()
                {
                    return Err(failure("fmp_identity_or_currency_mismatch"));
                }
                let day = date_field(row, "date")?;
                if day < window.start || day > window.end {
                    continue;
                }
                let filing = text(row, "filingDate").or_else(|_| text(row, "fillingDate"))?;
                let filed = date(filing)?;
                if next_day(filed)? > now {
                    continue;
                }
                observations.push(observation(
                    source,
                    FinancialPayload::Metric {
                        metric: metric.clone(),
                        value: decimal(
                            row.get(metric)
                                .ok_or_else(|| failure("fmp_metric_missing"))?,
                        )?,
                        period_start: None,
                        period_end: day,
                        vintage: Some(filed),
                        source_revision: filing.into(),
                    },
                    day.to_string(),
                    next_day(filed)?,
                    now,
                    url,
                    vec!["fiscal_start_unavailable".into()],
                )?);
            }
        }
        DataRequest::CoinGeckoMarkets { coin_id } => {
            let rows = v
                .as_array()
                .filter(|r| r.len() == 1)
                .ok_or_else(|| failure("coingecko_asset_missing"))?;
            let row = &rows[0];
            if row["id"] != *coin_id {
                return Err(failure("coingecko_asset_mismatch"));
            }
            let at = DateTime::parse_from_rfc3339(text(row, "last_updated")?)
                .map_err(|_| failure("coingecko_timestamp_invalid"))?
                .with_timezone(&Utc);
            observations.push(observation(
                source,
                FinancialPayload::MarketContext {
                    asset_id: coin_id.clone(),
                    price: decimal(&row["current_price"])?,
                    market_cap: decimal(&row["market_cap"])?,
                    currency: source
                        .currency
                        .clone()
                        .ok_or_else(|| failure("financial_currency_missing"))?,
                },
                coin_id.clone(),
                at,
                now,
                url,
                vec!["crypto_context_only".into()],
            )?);
        }
        DataRequest::YfinanceImport => return Err(failure("yfinance_network_forbidden")),
    }
    observations.sort_by(|a, b| {
        (a.available_at, &a.external_key, &a.content_hash).cmp(&(
            b.available_at,
            &b.external_key,
            &b.content_hash,
        ))
    });
    observations
        .dedup_by(|a, b| a.external_key == b.external_key && a.content_hash == b.content_hash);
    if !complete {
        gaps.push("bounded_response_or_history_incomplete".into());
    }
    Ok(FinancialPage {
        observations,
        complete,
        gaps,
    })
}

/// Preserve financial numeric lexemes locally without changing workspace JSON semantics.
/// RawValue validates the original JSON first. Fractional/exponent/large numeric
/// tokens become decimal strings; integer metadata retains its native JSON type.
fn financial_json(body: &[u8]) -> std::result::Result<Value, FinancialFailure> {
    serde_json::from_slice::<Box<serde_json::value::RawValue>>(body)
        .map_err(|_| failure("financial_json_invalid"))?;
    let mut output = Vec::with_capacity(body.len());
    let mut i = 0;
    let mut quoted = false;
    let mut escaped = false;
    while i < body.len() {
        let b = body[i];
        if quoted {
            output.push(b);
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                quoted = false;
            }
            i += 1;
            continue;
        }
        if b == b'"' {
            quoted = true;
            output.push(b);
            i += 1;
            continue;
        }
        if b == b'-' || b.is_ascii_digit() {
            let start = i;
            i += 1;
            while i < body.len() && (body[i].is_ascii_digit() || b"+-.eE".contains(&body[i])) {
                i += 1;
            }
            let token = &body[start..i];
            let s = std::str::from_utf8(token).map_err(|_| failure("financial_json_invalid"))?;
            let integer = s.parse::<i64>().is_ok() || s.parse::<u64>().is_ok();
            if !integer {
                if token.len() > 96 {
                    return Err(failure("financial_numeric_bound_exceeded"));
                }
                output.push(b'"');
                output.extend_from_slice(token);
                output.push(b'"');
            } else {
                output.extend_from_slice(token);
            }
        } else {
            output.push(b);
            i += 1;
        }
    }
    serde_json::from_slice(&output).map_err(|_| failure("financial_json_invalid"))
}

/// Offline imports preserve download time and cannot claim historical production ingestion.
pub fn offline_page(
    source: &FinancialSource,
    now: DateTime<Utc>,
    body: &[u8],
) -> Result<FinancialPage> {
    source.validate()?;
    if source.request != DataRequest::YfinanceImport
        || !source.usable(now)
        || body.len() > MAX_BODY_BYTES
    {
        return Err(Error("offline financial import denied".into()));
    }
    let payloads: Vec<FinancialPayload> = serde_json::from_slice(body)
        .map_err(|_| Error("invalid offline financial payloads".into()))?;
    if payloads.len() > source.max_records {
        return Err(Error("offline financial import exceeds bound".into()));
    }
    let mut observations = vec![];
    for payload in payloads {
        payload.validate()?;
        let key = match &payload {
            FinancialPayload::DailyBar {
                session,
                currency,
                adjustment,
                ..
            } if source.currency.as_deref() == Some(currency)
                && source.adjustment == *adjustment =>
            {
                session.to_string()
            }
            _ => {
                return Err(Error(
                    "offline import requires daily bars with reviewed currency and adjustment"
                        .into(),
                ));
            }
        };
        observations.push(
            observation(
                source,
                payload,
                key,
                now,
                now,
                "https://ranaroussi.github.io/yfinance/",
                vec!["offline_unofficial_research_import".into()],
            )
            .map_err(|_| Error("invalid offline observation".into()))?,
        );
    }
    Ok(FinancialPage {
        observations,
        complete: false,
        gaps: vec!["offline_import_coverage_unverified".into()],
    })
}
