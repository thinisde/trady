#!/usr/bin/env python3
"""Capture raw Yahoo bid/ask availability for the two reviewed EUR listings.

Anonymous Yahoo cookies/crumb stay in memory and are never saved or printed.
This is a diagnostic only, with no broker, configuration or database mutation.
"""
from datetime import datetime, timezone
import hashlib
import http.cookiejar
import json
import os
from pathlib import Path
import ssl
import sys
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode, urlsplit, quote
from urllib.request import HTTPCookieProcessor, HTTPRedirectHandler, HTTPSHandler, Request, build_opener

LIMIT = 2 * 1024 * 1024
SYMBOLS = ["AIR.PA", "ASML.AS"]


class Redirects(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        if urlsplit(newurl).scheme != "https" or urlsplit(newurl).hostname not in ("fc.yahoo.com", "query1.finance.yahoo.com"):
            return None
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def main():
    jar = http.cookiejar.CookieJar()
    opener = build_opener(HTTPCookieProcessor(jar), Redirects(), HTTPSHandler(context=ssl.create_default_context()))
    secrets = []

    def get(endpoint, parameters=None):
        request = Request(endpoint + ("?" + urlencode(parameters) if parameters else ""),
                          headers={"User-Agent": "Mozilla/5.0 Trady-read-only-quote-diagnostic", "Accept": "application/json"})
        try:
            with opener.open(request, timeout=12) as response:
                return response.status, response.read(LIMIT + 1)
        except HTTPError as error:
            return error.code, error.read(LIMIT + 1)

    started = datetime.now(timezone.utc)
    authentication = "ANONYMOUS_COOKIE_IN_MEMORY"
    get("https://fc.yahoo.com")
    status, body = get("https://query1.finance.yahoo.com/v1/test/getcrumb")
    crumb = body.decode(errors="replace").strip()
    parameters = {"symbols": ",".join(SYMBOLS)}
    if status == 200 and 3 <= len(crumb) <= 256 and not any(c.isspace() for c in crumb):
        secrets.extend([crumb, quote(crumb, safe="")])
        parameters["crumb"] = crumb
    else:
        authentication = "ANONYMOUS_CRUMB_UNAVAILABLE"
    status, body = get("https://query1.finance.yahoo.com/v7/finance/quote", parameters)
    if len(body) > LIMIT:
        raise ValueError("Yahoo response exceeds diagnostic bound")
    original_hash = hashlib.sha256(body).hexdigest()
    secrets.extend(cookie.value for cookie in jar)
    text = body.decode(errors="replace")
    for secret in secrets:
        if secret:
            text = text.replace(secret, "[REDACTED]")
    try:
        response = json.loads(text)
    except ValueError:
        response = {"invalid_json_response_omitted": True}
    received = datetime.now(timezone.utc)
    rows = response.get("quoteResponse", {}).get("result", [])
    summary = []
    for row in rows:
        timestamp = row.get("regularMarketTime")
        summary.append({key: row.get(key) for key in ["symbol", "currency", "exchange", "fullExchangeName", "marketState",
                       "bid", "ask", "bidSize", "askSize", "regularMarketTime", "exchangeDataDelayedBy", "quoteSourceName"]}
                       | {"last_trade_age_seconds": int(received.timestamp()) - timestamp if isinstance(timestamp, (int, float)) else None,
                          "bid_field_present": "bid" in row, "ask_field_present": "ask" in row,
                          "book_timestamp_fields": {key: value for key, value in row.items()
                                                    if any(word in key.lower() for word in ["bidtime", "asktime", "quotetime"])}})
    document = {"schema_version": 1, "provider": "Yahoo Finance v7 raw HTTP", "endpoint": "https://query1.finance.yahoo.com/v7/finance/quote",
                "symbols": SYMBOLS, "authentication": authentication, "requested_at": started.isoformat(),
                "received_at": received.isoformat(), "http_status": status, "response_hash": original_hash,
                "status": "OK" if status == 200 and len(rows) == 2 else "UNAVAILABLE",
                "response": response, "summary": summary, "entry_authorized": False, "broker_mutations": 0}
    directory = Path(__file__).resolve().parents[1] / "var/eur-first-run-checks"
    directory.mkdir(parents=True, exist_ok=True)
    path = directory / ("yahoo-wire-" + received.strftime("%Y%m%dT%H%M%S%fZ") + ".json")
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, "w") as output:
        json.dump(document, output, indent=2)
        output.write("\n")
    print(json.dumps({"artifact": str(path), "http_status": status, "status": document["status"],
                      "summary": summary, "entry_authorized": False}))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, URLError):
        print("Yahoo raw-quote diagnostic unavailable; authentication material was not printed.", file=sys.stderr)
        sys.exit(1)
