//! Bounded, read-only observation of the reviewed Yahoo EUR listings.
//! This does not load credentials, change configuration or authorize an order.
use chrono::Utc;
use serde_json::json;
use std::time::Duration;
use yfinance_rs::{StreamBuilder, StreamMethod, Ticker, YfClient};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let Ok(client) = YfClient::builder()
        .timeout(Duration::from_secs(12))
        .connect_timeout(Duration::from_secs(8))
        .retry_enabled(false)
        .build()
    else {
        println!("{}", json!({"status":"CLIENT_UNAVAILABLE"}));
        return;
    };
    let started = Utc::now();
    let mut quotes = Vec::new();
    for symbol in ["AIR.PA", "ASML.AS"] {
        match Ticker::new(&client, symbol).quote_with_diagnostics().await {
            Ok(response) => {
                let quote = response.data;
                quotes.push(json!({
                    "symbol":symbol, "status":"OK", "currency":quote.currency.code(),
                    "bid":quote.bid, "ask":quote.ask, "last":quote.price,
                    "last_trade_at":quote.as_of, "received_at":Utc::now(),
                    "last_trade_age_seconds":quote.as_of.map(|at| (Utc::now()-at).num_seconds()),
                    "market_state":quote.market_state,
                    "timestamp_semantics":"regularMarketTime is a trade timestamp, not an independently timestamped order book"
                }));
            }
            Err(_) => quotes.push(json!({"symbol":symbol,"status":"UNAVAILABLE"})),
        }
    }
    let mut updates = Vec::new();
    let stream_status = match StreamBuilder::new(&client)
        .symbols(["ASML", "ASML.AS", "AIR.PA"])
        .method(StreamMethod::Websocket)
        .websocket_connect_timeout(Duration::from_secs(8))
        .start()
        .await
    {
        Ok((handle, mut receiver)) => {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
            while updates.len() < 256 {
                match tokio::time::timeout_at(deadline, receiver.recv()).await {
                    Ok(Some(update)) => updates.push(json!({
                        "update":update,"received_at":Utc::now(),
                        "sdk_exposes_bid_ask":false
                    })),
                    _ => break,
                }
            }
            handle.abort();
            if updates.is_empty() {
                "NO_UPDATES"
            } else {
                "LAST_TRADE_UPDATES"
            }
        }
        Err(_) => "UNAVAILABLE",
    };
    println!(
        "{}",
        json!({
            "schema_version":1,"provider":"yfinance-rs 0.9.1",
            "started_at":started,"received_at":Utc::now(),"quotes":quotes,
            "stream_status":stream_status,"stream_updates":updates,
            "control_symbol":"ASML is a Nasdaq control only; it is not an approved EUR broker instrument",
            "stream_limitation":"The SDK QuoteUpdate projection has no bid/ask fields; stream updates alone cannot establish book availability",
            "entry_authorized":false,"broker_mutations":0
        })
    );
}
