#!/usr/bin/env python3
"""Verify a bounded news projection against the linked publisher pages.

The collector intentionally stores observations as unverified. This tool follows
each article URL, checks the final host and English page metadata, compares the
headline with the page, and emits only records that pass those checks. It never
stores article bodies.
"""

from __future__ import annotations

import argparse
import copy
import html
import json
import re
from datetime import datetime, timedelta, timezone
from html.parser import HTMLParser
from pathlib import Path
from urllib.error import HTTPError, URLError
from urllib.parse import urlparse
from urllib.request import Request, urlopen

MAX_PAGE_BYTES = 2_000_000
USER_AGENT = "trady-news-verifier/1.0"
KNOWN_GAPS = {"primary_source_not_verified", "google_redirect_not_resolved_metadata_only"}

SOURCE_HOSTS = {
    "publisher_business": {"bbc.co.uk", "bbc.com"},
    "finnhub_company_news": {
        "finance.yahoo.com",
        "yahoo.com",
        "seekingalpha.com",
        "benzinga.com",
        "chartmill.com",
    },
    "google_market": {
        "citywire.com",
        "finance.yahoo.com",
        "yahoo.com",
        "seekingalpha.com",
        "benzinga.com",
    },
    "google_technology": {
        "citywire.com",
        "finance.yahoo.com",
        "yahoo.com",
        "seekingalpha.com",
        "benzinga.com",
    },
}

PUBLISHER_HOSTS = {
    "bbc": {"bbc.co.uk", "bbc.com"},
    "yahoo": {"finance.yahoo.com", "yahoo.com"},
    "seekingalpha": {"seekingalpha.com"},
    "citywire": {"citywire.com"},
    "benzinga": {"benzinga.com"},
    "chartmill": {"chartmill.com"},
}


class MetadataParser(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.page_title = ""
        self.html_language = ""
        self.meta: dict[str, str] = {}
        self._in_title = False

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        values = {key.lower(): value or "" for key, value in attrs}
        if tag.lower() == "html":
            self.html_language = values.get("lang", "")
        elif tag.lower() == "title":
            self._in_title = True
        elif tag.lower() == "meta":
            key = (values.get("property") or values.get("name") or "").lower()
            content = values.get("content", "")
            if key and content:
                self.meta[key] = content

    def handle_endtag(self, tag: str) -> None:
        if tag.lower() == "title":
            self._in_title = False

    def handle_data(self, data: str) -> None:
        if self._in_title:
            self.page_title += data


def tokens(value: str) -> list[str]:
    return re.findall(r"[a-z0-9]+", html.unescape(value).lower())


def host_allowed(host: str, allowed: set[str]) -> bool:
    host = host.lower().removeprefix("www.")
    return any(host == item or host.endswith("." + item) for item in allowed)


def publisher_allowed(publisher: str, host: str) -> bool:
    lower = publisher.lower()
    for key, allowed in PUBLISHER_HOSTS.items():
        if key in lower:
            return host_allowed(host, allowed)
    return True


def title_matches(headline: str, parser: MetadataParser, body: str) -> bool:
    expected = [token for token in tokens(headline) if len(token) >= 3][:8]
    if len(expected) < 3:
        return False
    page_text = " ".join(
        [parser.page_title, parser.meta.get("og:title", ""), parser.meta.get("twitter:title", ""), body[:400_000]]
    )
    observed = set(tokens(page_text))
    return sum(token in observed for token in expected) >= min(4, len(expected))


def verify_item(item: dict) -> tuple[dict, dict]:
    url = item.get("canonical_url", "")
    parsed = urlparse(url)
    if parsed.scheme != "https" or not parsed.hostname:
        return item, {"verified": False, "error": "canonical_url_not_https"}
    try:
        request = Request(url, headers={"User-Agent": USER_AGENT})
        with urlopen(request, timeout=15) as response:
            body_bytes = response.read(MAX_PAGE_BYTES + 1)
            final_url = response.geturl()
            status = getattr(response, "status", 200)
            content_type = response.headers.get("content-type", "")
        if len(body_bytes) > MAX_PAGE_BYTES:
            raise ValueError("publisher_page_too_large")
        final = urlparse(final_url)
        host = (final.hostname or "").lower().removeprefix("www.")
        if final.scheme != "https" or not host:
            raise ValueError("final_url_not_https")
        if not host_allowed(host, SOURCE_HOSTS.get(item.get("source_id", ""), set())):
            raise ValueError("final_host_not_allowlisted")
        if not publisher_allowed(item.get("publisher", ""), host):
            raise ValueError("publisher_host_mismatch")
        body = body_bytes.decode("utf-8", "replace")
        parser = MetadataParser()
        parser.feed(body)
        language = parser.html_language or parser.meta.get("og:locale", "")
        if not language.lower().startswith("en"):
            raise ValueError("publisher_language_not_english")
        if not title_matches(item.get("title", ""), parser, body):
            raise ValueError("publisher_title_mismatch")
        verified = copy.deepcopy(item)
        verified["canonical_url"] = final_url
        verified["language"] = item.get("language") or language.lower()
        verified["gaps"] = [gap for gap in item.get("gaps", []) if gap not in KNOWN_GAPS]
        return verified, {
            "verified": True,
            "final_url": final_url,
            "final_host": host,
            "http_status": status,
            "content_type": content_type,
            "language": language.lower(),
        }
    except (HTTPError, URLError, TimeoutError, ValueError) as error:
        return item, {"verified": False, "error": str(error)[:200]}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--max-items", type=int, default=16)
    args = parser.parse_args()
    source = json.loads(Path(args.input).read_text())
    cutoff = datetime.now(timezone.utc)
    examined = []
    skipped_stale = 0
    for item in source.get("items", [])[: args.max_items]:
        observed_text = item.get("event_at") or item.get("published_at")
        expires_text = item.get("expires_at")
        if not observed_text or not expires_text:
            skipped_stale += 1
            continue
        observed = datetime.fromisoformat(observed_text.replace("Z", "+00:00"))
        expires = datetime.fromisoformat(expires_text.replace("Z", "+00:00"))
        if observed > cutoff or cutoff - observed > timedelta(hours=24) or expires <= cutoff:
            skipped_stale += 1
            continue
        examined.append(item)
    verified_items: list[dict] = []
    verification_items: dict[str, dict] = {}
    for item in examined:
        verified, report = verify_item(item)
        verification_items[item.get("item_id", "")] = report
        if report.get("verified"):
            verified_items.append(verified)
    verified_items.sort(key=lambda item: item.get("event_at") or item.get("published_at") or "", reverse=True)
    unique_items: list[dict] = []
    seen_lineages: set[str] = set()
    for item in verified_items:
        lineage = item.get("lineage_id", item.get("item_id", ""))
        if lineage in seen_lineages:
            continue
        seen_lineages.add(lineage)
        unique_items.append(item)
    verified_items = unique_items
    now = datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")
    complete = bool(examined) and all(
        report.get("verified") for report in verification_items.values()
    )
    output = {
        "schema_version": 1,
        "generated_at": now,
        "source_read": source.get("source_read", "trady news read --for-model"),
        "evidence_verified": complete,
        "untrusted_source_data": not complete,
        "verification": {
            "status": "verified" if complete else "partial",
            "method": "http_primary_source_check_v1",
            "reviewed_at": now,
            "items": verification_items,
        },
        "items": verified_items,
        "entry_authorized": False,
    }
    Path(args.output).write_text(json.dumps(output, indent=2, sort_keys=True) + "\n")
    print(json.dumps({
        "examined_items": len(examined),
        "skipped_stale_items": skipped_stale,
        "verified_items": len(verified_items),
        "evidence_verified": complete,
        "output": args.output,
    }, indent=2))
    return 0 if verified_items else 1


if __name__ == "__main__":
    raise SystemExit(main())
