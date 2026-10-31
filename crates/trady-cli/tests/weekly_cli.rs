use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};
fn arithmetic(value: Value) -> std::process::Output {
    bridge("arithmetic-bridge", value)
}
fn bridge(operation: &str, value: Value) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_trady"))
        .args(["weekly", operation, "--file", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&value).unwrap().as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn weekly_formula_bridge_has_exact_decimal_replies_and_strict_requests() {
    let result = bridge(
        "formula-bridge",
        json!({"kind":"DECIMAL","operation":"ADD","a":"0.10","b":"0.20"}),
    );
    assert!(result.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&result.stdout).unwrap(),
        json!({"status":"DECIMAL","value":"0.3"})
    );
    let result = bridge(
        "formula-bridge",
        json!({"kind":"DECIMAL","operation":"DIV","a":"1","b":"0"}),
    );
    assert!(result.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&result.stdout).unwrap()["status"],
        "REJECTED"
    );
    assert!(
        !bridge(
            "formula-bridge",
            json!({"kind":"DECIMAL","operation":"ADD","a":"1","b":"1","accepted":true})
        )
        .status
        .success()
    );
}
#[test]
fn weekly_arithmetic_bridge_checks_rounding_boundaries_and_zero_divisors() {
    for (n, d, q, m, floor, ceil, cap) in [
        (0, 1, 0, 0, 0, 0, 0),
        (100, 3, 500, 2, 33, 34, 2),
        (10000, 100, 3, 3, 100, 100, 3),
    ] {
        let output = arithmetic(json!({"numerator":n,"denominator":d,"quantity":q,"maximum":m}));
        assert!(output.status.success());
        assert_eq!(
            serde_json::from_slice::<Value>(&output.stdout).unwrap(),
            json!({"floor":floor,"ceil":ceil,"cap":cap})
        );
    }
    assert!(
        !arithmetic(json!({"numerator":1,"denominator":0,"quantity":1,"maximum":1}))
            .status
            .success()
    );
    assert!(
        !arithmetic(
            json!({"numerator":1,"denominator":1,"quantity":1,"maximum":1,"shell":"forbidden"})
        )
        .status
        .success()
    );
}
