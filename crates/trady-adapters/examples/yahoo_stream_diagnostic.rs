//! Read-only browser-Origin handshake and wire capture. No browser cookies.
use chrono::Utc;
use serde_json::json;
use std::{
    net::{TcpStream, ToSocketAddrs},
    time::{Duration, Instant},
};
use tungstenite::{Message, client::IntoClientRequest, client_tls, stream::MaybeTlsStream};

fn capture() -> Result<serde_json::Value, &'static str> {
    let started = Utc::now();
    let symbols = match std::env::args().nth(1).as_deref() {
        None => vec!["ASML", "ASML.AS", "AIR.PA"],
        Some("--asml-amsterdam") => vec!["ASML.AS"],
        Some(_) => return Err("UNSUPPORTED_DIAGNOSTIC_ARGUMENT"),
    };
    let addresses = ("streamer.finance.yahoo.com", 443)
        .to_socket_addrs()
        .map_err(|_| "DNS_UNAVAILABLE")?;
    let stream = addresses
        .take(3)
        .find_map(|address| TcpStream::connect_timeout(&address, Duration::from_secs(3)).ok())
        .ok_or("CONNECTION_UNAVAILABLE")?;
    stream
        .set_read_timeout(Some(Duration::from_secs(8)))
        .map_err(|_| "TIMEOUT_UNAVAILABLE")?;
    stream
        .set_write_timeout(Some(Duration::from_secs(8)))
        .map_err(|_| "TIMEOUT_UNAVAILABLE")?;
    let mut request = "wss://streamer.finance.yahoo.com/?version=2"
        .into_client_request()
        .map_err(|_| "INVALID_REQUEST")?;
    request.headers_mut().insert(
        "Origin",
        "https://finance.yahoo.com"
            .parse()
            .map_err(|_| "INVALID_ORIGIN")?,
    );
    request.headers_mut().insert(
        "User-Agent",
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:157.0) Gecko/20100101 Firefox/157.0"
            .parse()
            .map_err(|_| "INVALID_AGENT")?,
    );
    for (name, value) in [
        ("Accept", "*/*"),
        ("Accept-Language", "en-US,en;q=0.9"),
        ("Pragma", "no-cache"),
        ("Cache-Control", "no-cache"),
        ("Sec-Fetch-Dest", "empty"),
        ("Sec-Fetch-Mode", "websocket"),
        ("Sec-Fetch-Site", "same-site"),
    ] {
        request
            .headers_mut()
            .insert(name, value.parse().map_err(|_| "INVALID_PUBLIC_HEADER")?);
    }
    let (mut socket, response) =
        client_tls(request, stream).map_err(|_| "HANDSHAKE_UNAVAILABLE")?;
    socket
        .send(Message::text(json!({"subscribe":symbols}).to_string()))
        .map_err(|_| "SUBSCRIPTION_UNAVAILABLE")?;
    match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => stream.set_read_timeout(Some(Duration::from_secs(2))),
        MaybeTlsStream::NativeTls(stream) => stream
            .get_mut()
            .set_read_timeout(Some(Duration::from_secs(2))),
        _ => return Err("UNSUPPORTED_TRANSPORT"),
    }
    .map_err(|_| "TIMEOUT_UNAVAILABLE")?;
    let mut frames = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut status = "NO_UPDATES";
    while Instant::now() < deadline && frames.len() < 256 {
        match socket.read() {
            Ok(Message::Text(text)) if text.len() <= 100_000 => {
                let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
                    status = "INVALID_MESSAGE";
                    break;
                };
                if value["type"] == "pricing" {
                    frames.push(json!({"received_at":Utc::now(),"wire_response":value}));
                    status = "PRICING_FRAMES_RECEIVED";
                }
            }
            Ok(Message::Ping(_)) => {
                let _ = socket.flush();
            }
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                ) => {}
            Err(_) => {
                status = "STREAM_DISCONNECTED";
                break;
            }
        }
    }
    let _ = socket.close(None);
    Ok(
        json!({"schema_version":1,"provider":"Yahoo Finance browser-Origin websocket",
        "endpoint":"wss://streamer.finance.yahoo.com/?version=2",
        "origin":"https://finance.yahoo.com","symbols":symbols,
        "started_at":started,"received_at":Utc::now(),"http_status":response.status().as_u16(),
        "status":status,"frames":frames,"cookie_authentication":false,
        "entry_authorized":false,"broker_mutations":0}),
    )
}

fn main() {
    let result = capture().unwrap_or_else(|code| {
        json!({"status":"UNAVAILABLE","error_code":code,
        "entry_authorized":false,"broker_mutations":0})
    });
    println!("{result}");
}
