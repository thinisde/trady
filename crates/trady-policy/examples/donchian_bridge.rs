use std::io::{self, Read};
use trady_policy::limits::{DonchianAtrSpec, DonchianBar, evaluate_signal, trade_plan};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    bars: Vec<DonchianBar>,
    entry: String,
}

fn main() {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text).unwrap();
    let request: Request = serde_json::from_str(&text).unwrap();
    let spec = DonchianAtrSpec::recommended();
    let signal = evaluate_signal(&request.bars, &spec).unwrap();
    let plan = signal.as_ref().map(|signal| {
        trade_plan(
            rust_decimal::Decimal::from_str_exact(&request.entry).unwrap(),
            signal.atr,
            &spec,
        )
        .unwrap()
    });
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "formula_id": spec.formula_id,
            "signal": signal,
            "plan": plan
        }))
        .unwrap()
    );
}
