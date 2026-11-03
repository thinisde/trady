//! The owner's fixed universe. IEX is the data venue, not the listing exchange.
use crate::{InstrumentId, Result, require};

pub const UNIVERSE_ID: &str = "IEX30";
pub const SYMBOLS: [&str; 30] = [
    "META", "MU", "NVDA", "AMD", "SNDK", "GOOGL", "BKNG", "AMZN", "AAPL", "MSFT", "INTC", "SHOP",
    "STX", "AVGO", "TSM", "CSCO", "LRCX", "VLO", "TSLA", "ORCL", "BAC", "WDC", "JPM", "GOOG",
    "LITE", "GEV", "AZO", "DELL", "WBD", "CRM",
];

pub fn symbol(instrument: &InstrumentId) -> Result<&str> {
    let raw = instrument.as_str();
    let symbol = raw.strip_suffix("_US_EQ").unwrap_or(raw);
    require(
        SYMBOLS.contains(&symbol),
        "instrument is outside the IEX30 universe",
    )?;
    Ok(symbol)
}

pub fn instruments() -> Vec<InstrumentId> {
    SYMBOLS
        .iter()
        .map(|s| InstrumentId::new(format!("{s}_US_EQ")).expect("static universe"))
        .collect()
}

pub fn aliases(symbol: &str) -> &'static [&'static str] {
    match symbol {
        "META" => &["meta platforms", "facebook", "instagram"],
        "MU" => &["micron"],
        "NVDA" => &["nvidia"],
        "AMD" => &["advanced micro devices"],
        "SNDK" => &["sandisk"],
        "GOOG" | "GOOGL" => &["alphabet", "google"],
        "BKNG" => &["booking holdings", "booking.com"],
        "AMZN" => &["amazon"],
        "AAPL" => &["apple"],
        "MSFT" => &["microsoft"],
        "INTC" => &["intel"],
        "SHOP" => &["shopify"],
        "STX" => &["seagate"],
        "AVGO" => &["broadcom"],
        "TSM" => &["tsmc", "taiwan semiconductor"],
        "CSCO" => &["cisco"],
        "LRCX" => &["lam research"],
        "VLO" => &["valero"],
        "TSLA" => &["tesla"],
        "ORCL" => &["oracle"],
        "BAC" => &["bank of america"],
        "WDC" => &["western digital"],
        "JPM" => &["jpmorgan", "jp morgan"],
        "LITE" => &["lumentum"],
        "GEV" => &["ge vernova"],
        "AZO" => &["autozone"],
        "DELL" => &["dell technologies"],
        "WBD" => &["warner bros", "warner brothers"],
        "CRM" => &["salesforce"],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn universe_is_exact_unique_and_preserves_share_classes() {
        assert_eq!(
            SYMBOLS
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            30
        );
        for instrument in instruments() {
            assert!(symbol(&instrument).is_ok());
        }
        for rejected in ["SPY", "XOM_US_EQ", "AAPL_EQ", "aapl", "*"] {
            assert!(
                InstrumentId::new(rejected)
                    .and_then(|i| symbol(&i).map(str::to_owned))
                    .is_err()
            );
        }
        assert!(SYMBOLS.contains(&"GOOG") && SYMBOLS.contains(&"GOOGL"));
    }
}
