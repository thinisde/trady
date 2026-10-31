use super::*;
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, TrySendError},
    },
    thread,
    time::{Duration, Instant},
};

const MAX_HEADER_BYTES: usize = 16_384;
pub struct HttpServiceResponse {
    pub status: u16,
    pub body: Vec<u8>,
}
impl HttpServiceResponse {
    fn error(status: u16, code: &str) -> Self {
        Self {
            status,
            body: format!("{{\"error\":\"{code}\"}}").into_bytes(),
        }
    }
    fn json<T: Serialize>(value: &T, max: usize) -> Result<Self> {
        let body = serde_json::to_vec(value)
            .map_err(|_| Error("input response encoding failed".into()))?;
        require(
            body.len() <= max,
            "input response exceeds configured body bound",
        )?;
        Ok(Self { status: 200, body })
    }
}
impl InputsService {
    /// Exact existing Rust routes. Imports/reviews are local operator operations,
    /// never writes available to the bearer-token data client.
    pub fn respond(
        &self,
        method: &str,
        path: &str,
        body: &[u8],
        now: DateTime<Utc>,
    ) -> HttpServiceResponse {
        let parts: Vec<_> = path.strip_prefix('/').unwrap_or("").split('/').collect();
        if parts.len() != 2 || !valid_id(parts[1]) || path.len() > 512 {
            return HttpServiceResponse::error(404, "UNKNOWN_ROUTE");
        }
        if method != "GET" && method != "POST" {
            return HttpServiceResponse::error(405, "METHOD_NOT_ALLOWED");
        }
        let max = self.config().maximum_response_bytes;
        let result = (|| match (method, parts[0]) {
            ("GET", "session") => {
                HttpServiceResponse::json(&self.session(&AccountId::new(parts[1])?, now)?, max)
            }
            ("GET", "quote") => {
                HttpServiceResponse::json(&self.quote(&InstrumentId::new(parts[1])?, now)?, max)
            }
            ("GET", "costs") => {
                HttpServiceResponse::json(&self.costs(&InstrumentId::new(parts[1])?, now)?, max)
            }
            ("GET", "research") => {
                HttpServiceResponse::json(&self.research(&AccountId::new(parts[1])?, now)?, max)
            }
            ("GET", "universe") if parts[1] == "SP500" => {
                HttpServiceResponse::json(&self.universe(now)?, max)
            }
            ("POST", "predictions") => {
                let text = std::str::from_utf8(body)
                    .map_err(|_| Error("prediction request is not UTF-8".into()))?;
                let parsed = trady_application::contracts::parse_json_bounded(
                    text,
                    self.config().maximum_body_bytes,
                )?;
                let request: PredictionRequest = serde_json::from_value(parsed)
                    .map_err(|_| Error("invalid prediction request schema".into()))?;
                require(
                    request.account_id.as_str() == parts[1],
                    "prediction route/account mismatch",
                )?;
                HttpServiceResponse::json(&self.predictions(&request, now)?, max)
            }
            _ => Ok(HttpServiceResponse::error(404, "UNKNOWN_ROUTE")),
        })();
        // Do not disclose source text, account data, paths, tokens or upstream bodies.
        result.unwrap_or_else(|_| HttpServiceResponse::error(503, "VERIFIED_INPUT_UNAVAILABLE"))
    }
}

pub struct HttpServer {
    listener: TcpListener,
    service: Arc<InputsService>,
    token: Arc<String>,
}
impl HttpServer {
    pub fn bind(service: InputsService, token: String) -> Result<Self> {
        require(
            (32..=256).contains(&token.len()) && token.bytes().all(|b| b.is_ascii_graphic()),
            "input service requires a private 32 through 256 character bearer token",
        )?;
        service.config().validate()?;
        let listener = TcpListener::bind(service.config().listen)
            .map_err(|_| Error("input service listener unavailable".into()))?;
        listener
            .set_nonblocking(true)
            .map_err(|_| Error("input listener setup failed".into()))?;
        Ok(Self {
            listener,
            service: Arc::new(service),
            token: Arc::new(token),
        })
    }
    pub fn local_addr(&self) -> Result<std::net::SocketAddr> {
        self.listener
            .local_addr()
            .map_err(|_| Error("input listener address unavailable".into()))
    }
    /// Fixed worker count and bounded queue; no per-connection thread growth.
    pub fn serve_until(&self, stop: Arc<AtomicBool>) -> Result<()> {
        let workers = usize::from(self.service.config().workers);
        let (sender, receiver) = mpsc::sync_channel::<TcpStream>(workers * 2);
        let receiver = Arc::new(Mutex::new(receiver));
        let mut threads = Vec::new();
        for _ in 0..workers {
            let receiver = receiver.clone();
            let service = self.service.clone();
            let token = self.token.clone();
            threads.push(thread::spawn(move || {
                loop {
                    let stream = match receiver.lock() {
                        Ok(r) => r.recv(),
                        Err(_) => return,
                    };
                    match stream {
                        Ok(stream) => handle(stream, &service, &token),
                        Err(_) => return,
                    }
                }
            }));
        }
        let mut failed = false;
        while !stop.load(Ordering::Relaxed) {
            match self.listener.accept() {
                Ok((stream, peer)) => {
                    if !peer.ip().is_loopback() {
                        continue;
                    }
                    match sender.try_send(stream) {
                        Ok(()) => {}
                        Err(TrySendError::Full(mut stream)) => {
                            let _ = stream.set_write_timeout(Some(Duration::from_millis(100)));
                            let _ = send(
                                &mut stream,
                                &HttpServiceResponse::error(503, "SERVICE_BUSY"),
                                Duration::from_millis(100),
                            );
                        }
                        Err(TrySendError::Disconnected(_)) => {
                            failed = true;
                            break;
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(10))
                }
                Err(_) => {
                    failed = true;
                    break;
                }
            }
        }
        drop(sender);
        for thread in threads {
            if thread.join().is_err() {
                failed = true;
            }
        }
        require(!failed, "input service worker or listener failed")
    }
}

fn constant_equal(a: &[u8], b: &[u8]) -> bool {
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        diff |= usize::from(a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0));
    }
    diff == 0
}
fn handle(mut stream: TcpStream, service: &InputsService, token: &str) {
    let timeout = Duration::from_millis(u64::from(service.config().request_timeout_ms));
    let _ = stream.set_write_timeout(Some(timeout));
    let response = receive(&mut stream, service, token, timeout)
        .unwrap_or_else(|| HttpServiceResponse::error(400, "INVALID_HTTP_REQUEST"));
    let _ = send(&mut stream, &response, timeout);
}
fn read_until_deadline(
    stream: &mut TcpStream,
    bytes: &mut [u8],
    deadline: Instant,
) -> Option<usize> {
    let left = deadline.checked_duration_since(Instant::now())?;
    if left.is_zero() {
        return None;
    }
    stream.set_read_timeout(Some(left)).ok()?;
    stream.read(bytes).ok().filter(|n| *n > 0)
}
fn receive(
    stream: &mut TcpStream,
    service: &InputsService,
    token: &str,
    timeout: Duration,
) -> Option<HttpServiceResponse> {
    let deadline = Instant::now().checked_add(timeout)?;
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 1024];
    let header_end = loop {
        if let Some(i) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        if bytes.len() >= MAX_HEADER_BYTES {
            return Some(HttpServiceResponse::error(431, "HEADERS_TOO_LARGE"));
        }
        let n = read_until_deadline(stream, &mut buffer, deadline)?;
        bytes.extend_from_slice(&buffer[..n]);
    };
    if header_end > MAX_HEADER_BYTES {
        return Some(HttpServiceResponse::error(431, "HEADERS_TOO_LARGE"));
    }
    let text = std::str::from_utf8(&bytes[..header_end]).ok()?;
    if !text.is_ascii() {
        return None;
    }
    let mut lines = text[..text.len() - 4].split("\r\n");
    let mut request = lines.next()?.split(' ');
    let method = request.next()?;
    let path = request.next()?;
    if request.next() != Some("HTTP/1.1")
        || request.next().is_some()
        || !path.starts_with('/')
        || path.contains(['?', '#', '%', '\\'])
    {
        return None;
    }
    let mut headers = BTreeMap::new();
    for line in lines {
        let (name, value) = line.split_once(':')?;
        if name.is_empty()
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || value.bytes().any(|b| b.is_ascii_control())
            || headers
                .insert(name.to_ascii_lowercase(), value.trim())
                .is_some()
        {
            return None;
        }
    }
    if !headers.contains_key("host")
        || headers.contains_key("transfer-encoding")
        || headers.contains_key("expect")
    {
        return None;
    }
    let credential = headers
        .get("authorization")
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    if !constant_equal(credential.as_bytes(), token.as_bytes()) {
        return Some(HttpServiceResponse::error(401, "AUTHENTICATION_REQUIRED"));
    }
    let length = if let Some(value) = headers.get("content-length") {
        if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        value.parse::<usize>().ok()?
    } else {
        if method == "POST" {
            return None;
        }
        0
    };
    if length > service.config().maximum_body_bytes {
        return Some(HttpServiceResponse::error(413, "BODY_TOO_LARGE"));
    }
    if method == "POST" && headers.get("content-type") != Some(&"application/json")
        || method == "GET" && length != 0
    {
        return None;
    }
    let mut body = bytes[header_end..].to_vec();
    if body.len() > length {
        return None;
    }
    while body.len() < length {
        let remaining = (length - body.len()).min(buffer.len());
        let n = read_until_deadline(stream, &mut buffer[..remaining], deadline)?;
        body.extend_from_slice(&buffer[..n]);
    }
    Some(service.respond(method, path, &body, Utc::now()))
}
fn send(
    stream: &mut TcpStream,
    response: &HttpServiceResponse,
    timeout: Duration,
) -> std::io::Result<()> {
    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        431 => "Request Header Fields Too Large",
        _ => "Service Unavailable",
    };
    let auth = if response.status == 401 {
        "WWW-Authenticate: Bearer\r\n"
    } else {
        ""
    };
    let retry = if response.status == 503 {
        "Retry-After: 1\r\n"
    } else {
        ""
    };
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\n{}{}\r\n",
        response.status,
        reason,
        response.body.len(),
        auth,
        retry
    );
    let deadline = Instant::now() + timeout;
    for part in [header.as_bytes(), &response.body] {
        let mut written = 0;
        while written < part.len() {
            let left = deadline
                .checked_duration_since(Instant::now())
                .filter(|d| !d.is_zero())
                .ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::TimedOut, "response deadline")
                })?;
            stream.set_write_timeout(Some(left))?;
            let n = stream.write(&part[written..])?;
            if n == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "response closed",
                ));
            }
            written += n;
        }
    }
    Ok(())
}
