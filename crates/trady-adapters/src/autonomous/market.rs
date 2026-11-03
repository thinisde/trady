//! Executor market reads without model, screening or research persistence capabilities.

use super::{CostsPacket, SessionPacket, TradingInputsConfig};
use crate::alpaca::{AlpacaDataConfig, AlpacaMarketData, IexQuoteCache};
use crate::news::{HttpRequest, HttpTransport, MAX_BODY_BYTES, SafeHttp};
use crate::yfinance::{YfinanceDataConfig, YfinanceMarketData};
use chrono::{DateTime, Duration, Utc};
use reqwest::Url;
use serde::de::DeserializeOwned;
use trady_application::{
    Clock,
    autonomous::{MarketSession, TradingConfig},
    contracts::parse_json_bounded,
};
use trady_domain::*;
use trady_policy::{EntryCosts, fresh};

pub struct MarketInputs<T = SafeHttp> {
    service: InputService<T>,
    clock: Box<dyn Clock>,
    alpaca: Option<AlpacaMarketData>,
    iex_stream: Option<IexQuoteCache>,
    yfinance: Option<YfinanceMarketData>,
}
impl MarketInputs {
    pub fn new(
        config: TradingInputsConfig,
        trading: &TradingConfig,
        token: Option<String>,
    ) -> Result<Self> {
        Self::with_ports(config, trading, SafeHttp, token)
    }
}
impl<T: HttpTransport> MarketInputs<T> {
    pub fn with_ports(
        config: TradingInputsConfig,
        trading: &TradingConfig,
        transport: T,
        token: Option<String>,
    ) -> Result<Self> {
        config.validate(trading)?;
        Ok(Self {
            service: InputService::new(config, transport, token)?,
            clock: Box::new(crate::SystemClock),
            alpaca: None,
            iex_stream: None,
            yfinance: None,
        })
    }
    pub fn with_clock(mut self, clock: impl Clock + 'static) -> Self {
        self.clock = Box::new(clock);
        self
    }
    /// Route executor quote reads through Alpaca Basic's IEX feed. This is
    /// deliberately explicit; there is no fallback to SIP or a broad market
    /// source.
    pub fn with_alpaca(
        mut self,
        config: Option<AlpacaDataConfig>,
        credentials: Option<(String, String)>,
    ) -> Result<Self> {
        self.alpaca = match config.filter(|value| value.enabled) {
            Some(config) => {
                let (key, secret) = credentials.ok_or_else(|| {
                    trady_domain::Error("Alpaca IEX credentials are required".into())
                })?;
                Some(AlpacaMarketData::new(config, key, secret)?)
            }
            None => None,
        };
        Ok(self)
    }
    pub fn set_iex_stream_cache(&mut self, cache: IexQuoteCache) {
        self.iex_stream = Some(cache);
    }
    /// Route mapped Euronext quote reads through Yahoo Finance.
    pub fn with_yfinance(mut self, config: Option<YfinanceDataConfig>) -> Result<Self> {
        self.yfinance = match config.filter(|value| value.enabled) {
            Some(config) => Some(YfinanceMarketData::new(config)?),
            None => None,
        };
        Ok(self)
    }
    pub fn session(&mut self, account: &AccountId) -> Result<Option<MarketSession>> {
        self.service.session(account, self.clock.as_ref())
    }
    pub fn quote(&mut self, instrument: &InstrumentId) -> Result<Quote> {
        if let Some(yfinance) = &self.yfinance
            && yfinance.supports(instrument)
        {
            return yfinance.latest_quote(instrument, self.clock.now());
        }
        if let Some(alpaca) = &self.alpaca {
            if let Some(stream) = &self.iex_stream
                && let Ok(quote) = stream.latest(
                    instrument,
                    self.clock.now(),
                    alpaca.config().quote_max_age_seconds,
                )
            {
                return Ok(quote);
            }
            alpaca
                .latest_quote(instrument, self.clock.now())
                .map(|q| q.quote)
        } else {
            self.service.quote(instrument, self.clock.as_ref())
        }
    }
    pub fn stream_quote(&self, instrument: &InstrumentId) -> Result<Quote> {
        let alpaca = self
            .alpaca
            .as_ref()
            .ok_or_else(|| Error("Alpaca IEX market data is not configured".into()))?;
        self.iex_stream
            .as_ref()
            .ok_or_else(|| Error("Alpaca IEX stream is not running".into()))?
            .latest(
                instrument,
                self.clock.now(),
                alpaca.config().quote_max_age_seconds,
            )
    }
    pub fn current_exit_costs(
        &mut self,
        instrument: &InstrumentId,
        now: DateTime<Utc>,
    ) -> Result<Option<EntryCosts>> {
        self.service.costs(instrument, now, self.clock.as_ref())
    }
}

pub(super) struct InputService<T> {
    config: TradingInputsConfig,
    transport: T,
    token: Option<String>,
    retry_at: Option<DateTime<Utc>>,
}
impl<T: HttpTransport> InputService<T> {
    pub(super) fn new(
        config: TradingInputsConfig,
        transport: T,
        token: Option<String>,
    ) -> Result<Self> {
        require(
            token
                .as_ref()
                .is_none_or(|t| !t.is_empty() && !t.bytes().any(|b| b.is_ascii_control())),
            "invalid trading input credential",
        )?;
        Ok(Self {
            config,
            transport,
            token,
            retry_at: None,
        })
    }
    pub(super) fn request<D: DeserializeOwned>(
        &mut self,
        path: &[&str],
        body: Option<Vec<u8>>,
        clock: &dyn Clock,
    ) -> Result<D> {
        require(
            self.retry_at.is_none_or(|at| clock.now() >= at),
            "trading input provider backoff active",
        )?;
        let mut url = Url::parse(&self.config.base_url)
            .map_err(|_| Error("invalid trading input URL".into()))?;
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| Error("invalid trading input base path".into()))?;
            segments.pop_if_empty();
            for part in path {
                segments.push(part);
            }
        }
        let request = HttpRequest {
            url: url.clone(),
            allowed_hosts: self.config.allowed_hosts.clone(),
            bearer: self.token.clone(),
            etag: None,
            last_modified: None,
            redirects: 0,
        };
        let response = match body {
            Some(body) => self.transport.post_json(request, body),
            None => self.transport.get(request),
        }
        .map_err(|_| Error("trading input provider unavailable".into()))?;
        if matches!(response.status, 429 | 503) {
            let delay = response
                .retry_after
                .as_deref()
                .and_then(|s| s.parse::<i64>().ok())
                .filter(|s| (1..=86400).contains(s))
                .unwrap_or(60);
            self.retry_at = clock.now().checked_add_signed(Duration::seconds(delay));
            return Err(Error("trading input provider rate limited".into()));
        }
        require(
            response.status == 200
                && response.final_url.as_ref() == Some(&url)
                && response.body.len() <= MAX_BODY_BYTES,
            "invalid trading input response",
        )?;
        let text = std::str::from_utf8(&response.body)
            .map_err(|_| Error("trading input response is not UTF-8".into()))?;
        serde_json::from_value(parse_json_bounded(text, MAX_BODY_BYTES)?)
            .map_err(|_| Error("invalid trading input document".into()))
    }
    pub(super) fn session(
        &mut self,
        account: &AccountId,
        clock: &dyn Clock,
    ) -> Result<Option<MarketSession>> {
        let packet: SessionPacket = self.request(&["session", account.as_str()], None, clock)?;
        require(
            packet.account_id == *account,
            "session belongs to another account",
        )?;
        Ok(packet.session)
    }
    pub(super) fn quote(&mut self, instrument: &InstrumentId, clock: &dyn Clock) -> Result<Quote> {
        let quote: Quote = self.request(&["quote", instrument.as_str()], None, clock)?;
        require(
            quote.instrument_id == *instrument,
            "quote belongs to another instrument",
        )?;
        Ok(quote)
    }
    pub(super) fn costs(
        &mut self,
        instrument: &InstrumentId,
        now: DateTime<Utc>,
        clock: &dyn Clock,
    ) -> Result<Option<EntryCosts>> {
        let packet: CostsPacket = self.request(&["costs", instrument.as_str()], None, clock)?;
        let now = clock.now().max(now);
        require(
            packet.instrument_id == *instrument
                && fresh(packet.observed_at, now, 60)
                && packet.valid_until > now,
            "stale or incompatible exit costs",
        )?;
        Ok(Some(packet.costs))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FakeClock, news::HttpResponse};
    use serde_json::{Value, json};
    use std::{cell::RefCell, collections::VecDeque, rc::Rc};
    use trady_application::research::SourceFailure;

    struct Transport {
        replies: VecDeque<HttpResponse>,
        paths: Rc<RefCell<Vec<String>>>,
        clock: FakeClock,
        elapsed: i64,
    }
    impl HttpTransport for Transport {
        fn get(
            &mut self,
            request: HttpRequest,
        ) -> std::result::Result<HttpResponse, SourceFailure> {
            self.paths.borrow_mut().push(request.url.path().into());
            self.clock.advance(self.elapsed).expect("fixture time");
            let mut reply = self.replies.pop_front().expect("unexpected request");
            reply.final_url = Some(request.url);
            Ok(reply)
        }
    }
    fn market(
        replies: Vec<HttpResponse>,
        elapsed: i64,
    ) -> (MarketInputs<Transport>, FakeClock, Rc<RefCell<Vec<String>>>) {
        let fixture: Value =
            serde_json::from_str(include_str!("../../../../config/autonomous.example.json"))
                .expect("deployment fixture");
        let config = serde_json::from_value(fixture["inputs"].clone()).expect("inputs config");
        let trading = serde_json::from_value(fixture["trading"].clone()).expect("trading config");
        let clock = FakeClock::new(Utc::now());
        let paths = Rc::new(RefCell::new(vec![]));
        let transport = Transport {
            replies: replies.into(),
            paths: paths.clone(),
            clock: clock.clone(),
            elapsed,
        };
        let market = MarketInputs::with_ports(config, &trading, transport, None)
            .expect("market config")
            .with_clock(clock.clone());
        (market, clock, paths)
    }
    fn ok(value: Value) -> HttpResponse {
        HttpResponse {
            status: 200,
            body: serde_json::to_vec(&value).expect("fixture JSON"),
            ..Default::default()
        }
    }
    #[test]
    fn executor_market_reads_keep_account_binding_and_backoff() {
        let (mut market, clock, paths) = market(
            vec![
                ok(json!({"account_id":"paper_other", "session":null})),
                HttpResponse {
                    status: 429,
                    retry_after: Some("60".into()),
                    ..Default::default()
                },
                ok(json!({"account_id":"paper_autonomous_demo", "session":null})),
            ],
            0,
        );
        let account = AccountId::new("paper_autonomous_demo").expect("account");
        assert!(market.session(&account).is_err());
        assert!(
            market
                .quote(&InstrumentId::new("AAPL_US_EQ").expect("instrument"))
                .is_err()
        );
        assert!(market.session(&account).is_err());
        assert_eq!(paths.borrow().len(), 2);
        clock.advance(60).expect("time");
        assert!(market.session(&account).expect("retry").is_none());
        assert!(
            paths
                .borrow()
                .iter()
                .all(|p| p.contains("/session/") || p.contains("/quote/"))
        );
    }
    #[test]
    fn slow_market_response_cannot_refresh_expired_exit_costs() {
        let now = Utc::now();
        let (mut market, clock, _) = market(
            vec![ok(json!({
                "instrument_id":"AAPL_US_EQ", "observed_at":now, "valid_until":now+Duration::seconds(20),
                "costs":{"fixed":{"amount":"0","currency":"EUR"},"rate":"0","uncertainty":{"amount":"0","currency":"EUR"},"fx":null}
            }))],
            120,
        );
        let supplied = clock.now();
        assert!(
            market
                .current_exit_costs(
                    &InstrumentId::new("AAPL_US_EQ").expect("instrument"),
                    supplied
                )
                .is_err()
        );
    }
}
