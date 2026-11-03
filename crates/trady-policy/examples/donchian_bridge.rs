use std::io::{self, Read};
use trady_policy::limits::{
    DonchianAtrSpec, DonchianBar, evaluate_signal, resolve_entry, resolve_exit, trade_plan,
};

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    bars: Vec<DonchianBar>,
    entry: String,
    #[serde(default)]
    next_open: Option<String>,
    #[serde(default)]
    evaluate_entry: bool,
    #[serde(default)]
    exit_bars: Vec<DonchianBar>,
}

fn main() {
    let mut text = String::new();
    io::stdin().read_to_string(&mut text).unwrap();
    let request: Request = serde_json::from_str(&text).unwrap();
    let spec = DonchianAtrSpec::recommended();
    // Boundary fixtures intentionally include an under-warmed bar set.  The
    // bridge serializes that as no signal so the independent report can record
    // the fail-closed boundary without a process panic.
    let signal = evaluate_signal(&request.bars, &spec).ok().flatten();
    let plan = signal.as_ref().map(|signal| {
        trade_plan(
            rust_decimal::Decimal::from_str_exact(&request.entry).unwrap(),
            signal.atr,
            &spec,
        )
        .unwrap()
    });
    let entry_decision = signal.as_ref().and_then(|signal| {
        if request.evaluate_entry {
            Some(
                resolve_entry(
                    signal,
                    request
                        .next_open
                        .as_deref()
                        .map(|open| rust_decimal::Decimal::from_str_exact(open).unwrap()),
                )
                .unwrap(),
            )
        } else {
            request.next_open.as_deref().map(|open| {
                resolve_entry(
                    signal,
                    Some(rust_decimal::Decimal::from_str_exact(open).unwrap()),
                )
                .unwrap()
            })
        }
    });
    let exit = signal.as_ref().and_then(|_signal| {
        let plan = plan.as_ref()?;
        request
            .exit_bars
            .iter()
            .enumerate()
            .find_map(|(index, bar)| {
                resolve_exit(plan, bar, u8::try_from(index + 1).ok()?, &spec)
                    .ok()
                    .flatten()
            })
    });
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "formula_id": spec.formula_id,
            "signal": signal,
            "plan": plan,
            "entry_decision": entry_decision,
            "exit": exit,
        }))
        .unwrap()
    );
}
