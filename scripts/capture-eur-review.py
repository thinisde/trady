#!/usr/bin/env python3
"""Capture bounded, read-only evidence for an EUR SHADOW review.

Broker endpoints and the environment are fixed. Credentials never enter the
artifact, command arguments, diagnostics, or request manifest. This does not
submit orders, enable a source, or grant release/owner approval.
"""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import ssl
import stat
import subprocess
import sys
from datetime import datetime, timezone
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import HTTPRedirectHandler, HTTPSHandler, Request, build_opener

ROOT = Path(__file__).resolve().parents[1]
MAX_BYTES = 12 * 1024 * 1024
PUBLIC_SOURCES = {
    "airbus_shares": "https://www.airbus.com/en/investors/share-price-and-information",
    "asml_shares": "https://www.asml.com/en/investors/shares",
    "asml_shares_canonical": "https://investor.asml.com/share-information",
    "airbus_rss_directory": "https://www.airbus.com/en/rss-feeds",
    "airbus_press_rss": "https://www.airbus.com/en/generate-rss-feeds?type=all-press-releases",
    "airbus_press_rss_canonical": "https://www.airbus.com/en/rss-press-releases-feeds",
    "airbus_terms": "https://www.airbus.com/en/terms-of-use",
    "ecb_rss_directory": "https://www.ecb.europa.eu/home/html/rss.en.html",
    "ecb_press_rss": "https://www.ecb.europa.eu/rss/press.html",
    "ecb_social": "https://www.ecb.europa.eu/services/socialmedia/html/index.en.html",
    "ecb_disclaimer": "https://www.ecb.europa.eu/services/disclaimer/html/index.en.html",
    "ecb_disclaimer_canonical": "https://www.ecb.europa.eu/services/using-our-site/disclaimer/html/index.en.html",
    "alpha_terms": "https://www.alphavantage.co/terms_of_service/",
    "alpha_terms_pdf": "https://www.alphavantage.co/terms_of_service/",
    "alpha_limits": "https://www.alphavantage.co/support/",
    "yahoo_airbus_listing": "https://query1.finance.yahoo.com/v8/finance/chart/AIR.PA?interval=1d&range=5d",
    "yahoo_asml_listing": "https://query1.finance.yahoo.com/v8/finance/chart/ASML.AS?interval=1d&range=5d",
    "x_developer_guidelines": "https://docs.x.com/developer-guidelines",
    "x_license": "https://legal.x.com/data-terms/us/mla.html",
}


def now():
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def credentials():
    path = Path(os.environ.get("TRADY_CREDENTIALS_FILE", str(ROOT / "config/credentials.local.json")))
    if not path.is_file() or path.stat().st_size > 262144:
        raise ValueError("PRIVATE_CREDENTIAL_STORE_UNAVAILABLE")
    if stat.S_IMODE(path.stat().st_mode) & 0o077:
        raise ValueError("PRIVATE_CREDENTIAL_STORE_PERMISSIONS")
    data = json.loads(path.read_text())
    if data.get("config_version") != 1:
        raise ValueError("PRIVATE_CREDENTIAL_STORE_VERSION")
    return data.get("credentials", {})


def secret(store, name):
    value = os.environ.get(name, store.get(name, ""))
    if not isinstance(value, str) or not value or "\n" in value or "\r" in value:
        raise ValueError("REQUIRED_REVIEW_CREDENTIAL_UNAVAILABLE")
    return value


def fetch(provider, endpoint, params=None, headers=None, secrets=()):
    started = now()
    url = endpoint + ("?" + urlencode(params) if params else "")
    request = Request(url, headers={"User-Agent": "trady-shadow-review/0.1", **(headers or {})}, method="GET")
    opener = build_opener(NoRedirect(), HTTPSHandler(context=ssl.create_default_context()))
    status, body, content_type, error, location = None, b"", None, None, None
    try:
        with opener.open(request, timeout=30) as response:
            status, content_type = response.status, response.headers.get("Content-Type")
            body = response.read(MAX_BYTES + 1)
    except HTTPError as exc:
        status, content_type = exc.code, exc.headers.get("Content-Type")
        location = exc.headers.get("Location") if not secrets else None
        body = exc.read(MAX_BYTES + 1)
        error = "HTTP_STATUS"
    except (URLError, TimeoutError, OSError):
        error = "TRANSPORT_UNAVAILABLE"
    received = now()
    if len(body) > MAX_BYTES:
        body = b""
        error = "RESPONSE_SIZE_LIMIT"
    digest = hashlib.sha256(body).hexdigest()
    binary = bool(content_type and "application/pdf" in content_type)
    text = base64.b64encode(body).decode("ascii") if binary else body.decode("utf-8", errors="replace")
    for value in secrets:
        if value:
            text = text.replace(value, "[REDACTED]")
    try:
        response = json.loads(text)
    except (ValueError, TypeError):
        response = text
    if provider == "alpha_vantage":
        if not isinstance(response, dict):
            error = "PROVIDER_RESPONSE_SCHEMA_UNAVAILABLE"
        elif any(key in response for key in ["Information", "Note", "Error Message"]):
            error = "PROVIDER_LIMIT_OR_ERROR_PAYLOAD"
        elif not response.get("bestMatches"):
            error = "PROVIDER_SYMBOL_SEARCH_UNAVAILABLE"
    if provider == "trading212_demo" and "/metadata/" in endpoint and not isinstance(response, list):
        error = "PROVIDER_CATALOG_SCHEMA_UNAVAILABLE"
    return {
        "schema_version": 1,
        "provider": provider,
        "endpoint": endpoint,
        "parameters": {key: value for key, value in (params or {}).items() if key != "apikey"},
        "requested_at": started,
        "received_at": received,
        "http_status": status,
        "content_type": content_type,
        "response_encoding": "base64" if binary else "json_or_utf8",
        "redirect_location": location,
        "response_sha256": digest,
        "status": "OK" if status == 200 and error is None else "UNAVAILABLE",
        "error_state": error,
        "rights_reference": "REVIEW_REQUIRED_METADATA_ONLY",
        "response": response,
        "broker_mutations": 0,
        "entry_authorized": False,
    }


def write_new(directory, name, document):
    path = directory / (name + ".json")
    with path.open("x", encoding="utf-8") as output:
        json.dump(document, output, ensure_ascii=False, indent=2)
        output.write("\n")
        output.flush()
        os.fsync(output.fileno())
    return path


def resolve_x(directory, handle):
    """Use the existing durable cost gate; never bypass it with a raw X request."""
    original = json.loads((ROOT / "config/research.json").read_text())
    identity = {
        "airbus": ("Airbus", "https://www.airbus.com/en/investors/share-price-and-information"),
        "ECB": ("European Central Bank", "https://www.ecb.europa.eu/services/socialmedia/html/index.en.html"),
    }[handle]
    source_id = "x_eur_review_" + handle.lower()
    config = {
        "config_version": 1, "max_pages": 1, "page_size": 5,
        "lookback_seconds": 86400, "overlap_seconds": 120,
        "x_budget": original["x_budget"],
        "sources": [{
            "id": source_id, "kind": "X_TIMELINE", "enabled": True,
            "url": "https://api.x.com/2/", "allowed_hosts": ["api.x.com"],
            "credential_env": "TRADY_X_BEARER_TOKEN", "interval_seconds": 3600,
            "daily_request_limit": 1, "query": None,
            "x_account": {"person": identity[0], "handle": handle,
                "user_id": None, "verified_at": None,
                "ownership_reference": identity[1], "priority": 0,
                "scope": ["EUR_SHADOW_IDENTITY_REVIEW"]},
            "rights": {"store_content": False, "llm_processing": False,
                "retention_hours": 24, "terms_reference": "IDENTITY_METADATA_ONLY_REVIEW"},
        }],
    }
    config_path = write_new(directory, "x-resolution-config-" + handle.lower(), config)
    started = now()
    completed = subprocess.run(
        [str(ROOT / "target/debug/trady"), "news", "resolve-x", handle,
         "--source", source_id, "--config", str(config_path.resolve())],
        cwd=ROOT, capture_output=True, timeout=45,
    )
    # The CLI owns sanitized diagnostics. Do not store its stderr or process environment.
    try:
        response = json.loads(completed.stdout) if completed.returncode == 0 else None
    except ValueError:
        response = None
    ok = isinstance(response, dict) and isinstance(response.get("user"), dict)
    document = {
        "schema_version": 1, "provider": "x_api_v2",
        "endpoint": "https://api.x.com/2/users/by/username/" + handle,
        "requested_at": started, "received_at": now(),
        "status": "OK" if ok else "UNAVAILABLE",
        "error_state": None if ok else "METERED_IDENTITY_RESOLUTION_UNAVAILABLE",
        "cli_exit_code": completed.returncode,
        "response": response,
        "response_sha256": hashlib.sha256(completed.stdout).hexdigest(),
        "response_hash_scope": "CLI_STDOUT",
        "budget_policy_sha256": hashlib.sha256(json.dumps(original["x_budget"], sort_keys=True, separators=(",", ":")).encode()).hexdigest(),
        "budget_accounting": "EXISTING_POSTGRESQL_X_COST_LEDGER",
        "post_content_requested": False, "broker_mutations": 0,
        "entry_authorized": False,
    }
    return write_new(directory, "x_" + handle.lower(), document)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", choices=["broker_catalog", "alpha_airbus", "alpha_asml", "x_airbus", "x_ecb", *PUBLIC_SOURCES], required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True, mode=0o700)
    if args.source == "broker_catalog":
        store = credentials()
        key = secret(store, "TRADY_T212_DEMO_API_KEY")
        password = secret(store, "TRADY_T212_DEMO_API_SECRET")
        auth = base64.b64encode((key + ":" + password).encode()).decode()
        headers = {"Authorization": "Basic " + auth, "Accept": "application/json"}
        account = fetch("trading212_demo", "https://demo.trading212.com/api/v0/equity/account/summary",
                        headers=headers, secrets=(key, password, auth))
        observed = account["response"]
        if isinstance(observed, dict):
            account["response"] = {"id": observed.get("id"), "currency": observed.get("currency")}
            account["response_redacted_to_identity"] = True
        if account["status"] != "OK" or not isinstance(observed, dict) or observed.get("id") != 52211463 or observed.get("currency") != "EUR":
            write_new(args.output, "account-binding-failed", account)
            raise ValueError("DEMO_ACCOUNT_IDENTITY_OR_CURRENCY_NOT_VERIFIED")
        # Account values are unnecessary for an identity review.
        paths = [write_new(args.output, "account-binding", account)]
        for name, suffix in [("instruments", "instruments"), ("exchanges", "exchanges")]:
            document = fetch("trading212_demo", "https://demo.trading212.com/api/v0/equity/metadata/" + suffix,
                             headers=headers, secrets=(key, password, auth))
            paths.append(write_new(args.output, name, document))
    elif args.source in ("x_airbus", "x_ecb"):
        paths = [resolve_x(args.output, "airbus" if args.source == "x_airbus" else "ECB")]
    elif args.source.startswith("alpha_") and args.source not in PUBLIC_SOURCES:
        store = credentials()
        key = secret(store, "TRADY_ALPHA_VANTAGE_API_KEY")
        name = "Airbus" if args.source == "alpha_airbus" else "ASML"
        document = fetch("alpha_vantage", "https://www.alphavantage.co/query",
                         {"function": "SYMBOL_SEARCH", "keywords": name, "apikey": key}, secrets=(key,))
        paths = [write_new(args.output, args.source, document)]
    else:
        document = fetch("primary_public_source", PUBLIC_SOURCES[args.source])
        paths = [write_new(args.output, args.source, document)]
    print(json.dumps({"status": "CAPTURE_WRITTEN", "files": [str(path) for path in paths],
                      "broker_mutations": 0, "activation_authorized": False}))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, KeyError, TypeError, subprocess.TimeoutExpired):
        print("EUR review capture failed; inspect sanitized artifacts or credential availability.", file=sys.stderr)
        sys.exit(1)
