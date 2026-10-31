use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    thread,
};

const TOKEN: &str = "12345:fixture_token_not_a_real_secret";

struct Response {
    status: u16,
    headers: &'static str,
    body: String,
}

fn response(value: Value) -> Response {
    Response {
        status: 200,
        headers: "",
        body: value.to_string(),
    }
}

fn server(responses: Vec<Response>) -> (String, thread::JoinHandle<Vec<(String, Value)>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let mut requests = vec![];
        for response in responses {
            let start = std::time::Instant::now();
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            start.elapsed() < Duration::from_secs(5),
                            "expected request was not received"
                        );
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("test server: {e}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut bytes = vec![];
            let (header_end, length) = loop {
                let mut buffer = [0; 4096];
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buffer[..count]);
                if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    let header = String::from_utf8_lossy(&bytes[..end]);
                    let length: usize = header
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse().unwrap())
                        })
                        .unwrap();
                    break (end + 4, length);
                }
            };
            while bytes.len() < header_end + length {
                let mut buffer = [0; 4096];
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buffer[..count]);
            }
            let request_line = String::from_utf8_lossy(&bytes[..header_end])
                .lines()
                .next()
                .unwrap()
                .to_string();
            let body = serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap();
            requests.push((request_line, body));
            write!(stream, "HTTP/1.1 {} Fixture\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n{}\r\n{}",
                response.status, response.body.len(), response.headers, response.body).unwrap();
        }
        requests
    });
    (address, handle)
}

fn client(address: String) -> TelegramClient {
    TelegramClient::build(BotToken::new(TOKEN.into()).unwrap(), 20, address, false).unwrap()
}

#[test]
fn http_contract_uses_post_offsets_private_messages_and_plain_text() {
    let (address, handle) = server(vec![
        response(json!({"ok":true,"result":{"id":12345,"is_bot":true,"username":"trady_bot"}})),
        response(json!({"ok":true,"result":[
            {"update_id":42,"message":{"from":{"id":7654321,"is_bot":false},"chat":{"id":7654321,"type":"private"},"date":1793448000,"text":"/status"}},
            {"update_id":43,"edited_message":{"text":"/status"}}
        ]})),
        response(
            json!({"ok":true,"result":{"message_id":1,"chat":{"id":7654321,"type":"private"}}}),
        ),
    ]);
    let api = client(address);
    assert_eq!(api.identity().unwrap().username, "trady_bot");
    let updates = api.updates(Some(42), true).unwrap();
    assert_eq!(updates.len(), 2);
    assert!(updates[0].message.as_ref().unwrap().private_chat);
    assert!(updates[1].message.is_none());
    api.send_message(7654321, "<literal> & [plain]").unwrap();
    let requests = handle.join().unwrap();
    assert!(requests[0].0.starts_with("POST /bot"));
    assert!(requests[1].0.contains("/getUpdates "));
    assert_eq!(requests[1].1["offset"], 42);
    assert_eq!(requests[1].1["timeout"], 20);
    assert_eq!(requests[1].1["allowed_updates"], json!(["message"]));
    assert_eq!(requests[2].1["link_preview_options"]["is_disabled"], true);
    assert_eq!(requests[2].1["protect_content"], true);
    assert!(requests[2].1.get("parse_mode").is_none());
}

#[test]
fn discovery_does_not_acknowledge_pending_updates() {
    let (address, handle) = server(vec![response(json!({"ok":true,"result":[]}))]);
    client(address).updates(None, false).unwrap();
    let requests = handle.join().unwrap();
    assert!(requests[0].1.get("offset").is_none());
    assert_eq!(requests[0].1["timeout"], 0);
}

#[test]
fn retry_after_and_errors_are_sanitized_without_provider_descriptions_or_urls() {
    let (address, handle) = server(vec![Response {
        status: 429, headers: "Retry-After: 7\r\n",
        body: json!({"ok":false,"error_code":429,"description":format!("https://api.telegram.org/bot{TOKEN}/private-body"),"parameters":{"retry_after":7}}).to_string(),
    }]);
    let error = client(address).updates(Some(1), false).unwrap_err();
    assert_eq!(error.http_status, Some(429));
    assert_eq!(error.retry_after_seconds, Some(7));
    assert!(!error.permanent);
    assert!(!format!("{error:?} {error}").contains(TOKEN));
    assert!(!format!("{error:?} {error}").contains("private-body"));
    handle.join().unwrap();
    assert!(!format!("{:?}", BotToken::new(TOKEN.into()).unwrap()).contains(TOKEN));
}

#[test]
fn redirects_are_not_followed_and_conflicts_are_terminal() {
    for status in [302, 401, 403, 409] {
        let (address, handle) = server(vec![Response {
            status,
            headers: "Location: http://127.0.0.1:1/capture\r\n",
            body: "untrusted response".into(),
        }]);
        let error = client(address).identity().unwrap_err();
        assert!(error.permanent);
        assert_eq!(error.http_status, Some(status));
        handle.join().unwrap();
    }
}

#[test]
fn malformed_success_bot_binding_and_duplicate_updates_fail_closed() {
    let cases = [
        (
            "identity",
            json!({"ok":true,"result":{"id":999,"is_bot":true,"username":"trady_bot"}}),
        ),
        (
            "updates",
            json!({"ok":true,"result":[{"update_id":1},{"update_id":1}]}),
        ),
        ("updates", json!({"ok":true,"result":[{"update_id":-1}]})),
        (
            "send",
            json!({"ok":true,"result":{"message_id":1,"chat":{"id":999,"type":"private"}}}),
        ),
        ("send", json!({"ok":true,"result":{}})),
    ];
    for (method, body) in cases {
        let (address, handle) = server(vec![response(body)]);
        let api = client(address);
        let failed = match method {
            "identity" => api.identity().is_err(),
            "updates" => api.updates(Some(0), false).is_err(),
            _ => api.send_message(12345, "hello").is_err(),
        };
        assert!(failed);
        handle.join().unwrap();
    }
}

#[test]
fn message_splitting_preserves_unicode_and_leaves_room_for_event_ids() {
    let text = "a🚀<&>\n".repeat(2000);
    let parts = split_message(&text);
    assert!(parts.len() > 1);
    assert_eq!(parts.concat(), text);
    assert!(
        parts
            .iter()
            .all(|part| part.encode_utf16().count() <= MESSAGE_UNITS)
    );
    assert!(split_message("").is_empty());
    for value in [
        "",
        "123:short",
        "https://evil.invalid/token",
        "123:long_token/with_path_characters",
    ] {
        assert!(BotToken::new(value.into()).is_err());
    }
}
