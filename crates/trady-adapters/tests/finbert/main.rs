use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

use serde_json::{Value, json};
use trady_adapters::finbert::{FinBertClient, FinBertConfig};
use trady_application::sentiment::{SentimentProvider, text_sha256};
use trady_domain::sentiment::*;

fn config(endpoint: String) -> FinBertConfig {
    FinBertConfig {
        config_version: 1,
        enabled: true,
        endpoint,
        model_revision: FINBERT_REVISION.into(),
        timeout_seconds: 2,
    }
}

fn document() -> SentimentDocument {
    let text = "The company raised its earnings outlook.";
    SentimentDocument {
        item_id: "article-1".into(),
        text: text.into(),
        text_sha256: text_sha256(text),
    }
}

fn response() -> Value {
    let doc = document();
    json!({"schema_version":1,"model_id":FINBERT_MODEL,"model_revision":FINBERT_REVISION,
        "predictions":[{"item_id":doc.item_id,"text_sha256":doc.text_sha256,
            "probabilities":{"positive":"0.800000000","negative":"0.100000000","neutral":"0.100000000"},"truncated":false}]})
}

/// A one-shot HTTP fixture, not the FinBERT service. No model is loaded.
fn mock(status: &str, body: String, extra: &str) -> (FinBertClient, thread::JoinHandle<Value>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let wire = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",
        body.len()
    );
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        let mut bytes = vec![];
        let (header_end, length) = loop {
            let mut block = [0; 4096];
            let read = stream.read(&mut block).unwrap();
            assert!(read > 0);
            bytes.extend_from_slice(&block[..read]);
            if let Some(index) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                let headers = String::from_utf8(bytes[..index].to_vec()).unwrap();
                assert!(headers.starts_with("POST /v1/sentiment HTTP/1.1\r\n"));
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .map(str::to_string)
                    })
                    .unwrap()
                    .parse()
                    .unwrap();
                break (index + 4, length);
            }
        };
        while bytes.len() < header_end + length {
            let mut block = [0; 4096];
            let read = stream.read(&mut block).unwrap();
            assert!(read > 0);
            bytes.extend_from_slice(&block[..read]);
        }
        let request = serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap();
        // A bounded client may close before consuming an intentionally oversized fixture.
        let _ = stream.write_all(wire.as_bytes());
        request
    });
    (
        FinBertClient::new(config(format!("http://{address}/"))).unwrap(),
        server,
    )
}

#[test]
fn exact_wire_contract_preserves_probabilities_identity_and_hash() {
    let (client, server) = mock("200 OK", response().to_string(), "");
    let result = client.predict(&[document()]).unwrap();
    assert_eq!(result.model_revision, FINBERT_REVISION);
    assert_eq!(
        result.predictions[0].probabilities.positive.to_string(),
        "0.800000000"
    );
    let request = server.join().unwrap();
    assert_eq!(request["schema_version"], 1);
    assert_eq!(
        request["documents"][0]["text_sha256"],
        document().text_sha256
    );
}

#[test]
fn disabled_and_unsafe_configurations_fail_before_requests() {
    let mut disabled = config("http://127.0.0.1:1/".into());
    disabled.enabled = false;
    assert!(FinBertClient::new(disabled).is_err());
    for endpoint in [
        "http://public.example/",
        "http://127.0.0.1/private/",
        "http://user:secret@127.0.0.1/",
        "https://example.org/?token=secret",
    ] {
        assert!(config(endpoint.into()).validate().is_err());
    }
    for endpoint in [
        "http://127.0.0.1:8091/",
        "http://[::1]:8091/",
        "http://finbert:8000/",
        "https://sentiment.example.org/",
    ] {
        config(endpoint.into()).validate().unwrap();
    }
    let mut invalid = config("http://localhost/".into());
    invalid.timeout_seconds = 0;
    assert!(invalid.validate().is_err());
}

#[test]
fn malformed_mismatched_missing_and_oversized_responses_are_rejected() {
    let mut cases = vec!["not JSON".to_string(), "x".repeat(65537)];
    for field in [
        "model_revision",
        "model_id",
        "schema_version",
        "predictions",
    ] {
        let mut body = response();
        body[field] = match field {
            "schema_version" => json!(2),
            "predictions" => json!([]),
            _ => json!("wrong"),
        };
        cases.push(body.to_string());
    }
    for field in ["item_id", "text_sha256", "probabilities"] {
        let mut body = response();
        body["predictions"][0][field] = if field == "probabilities" {
            json!({"positive":"0.8","negative":"0.8","neutral":"0.1"})
        } else {
            json!("wrong")
        };
        cases.push(body.to_string());
    }
    let mut body = response();
    body["predictions"][0]["surprise"] = json!(true);
    cases.push(body.to_string());
    for body in cases {
        let (client, server) = mock("200 OK", body, "");
        assert!(client.predict(&[document()]).is_err());
        server.join().unwrap();
    }
}

#[test]
fn redirect_and_unavailable_responses_are_not_predictions() {
    for status in ["302 Found", "503 Service Unavailable"] {
        let (client, server) = mock(
            status,
            response().to_string(),
            "Location: http://127.0.0.1:1/\r\n",
        );
        assert!(client.predict(&[document()]).is_err());
        server.join().unwrap();
    }
}

#[test]
fn bad_request_hash_and_empty_batch_are_rejected_locally() {
    let client = FinBertClient::new(config("http://127.0.0.1:1/".into())).unwrap();
    let mut bad = document();
    bad.text_sha256 = "wrong".into();
    assert!(client.predict(&[bad]).is_err());
    assert!(client.predict(&[]).is_err());
}
