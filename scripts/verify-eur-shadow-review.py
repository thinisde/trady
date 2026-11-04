#!/usr/bin/env python3
"""Seal or verify the dated EUR SHADOW self-review, without credentials or I/O to providers.

Sealing records existing files; it grants no runtime, broker or owner authority.
The default operation verifies the sealed local observations and current expiry.
"""
import argparse
import hashlib
from html.parser import HTMLParser
import json
from pathlib import Path
import subprocess
import sys
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BUNDLE = ROOT / "docs/evidence/eur-shadow-review-2026-11-04"
MAX_FILE_BYTES = 24 * 1024 * 1024


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def local(path):
    resolved = (ROOT / path).resolve()
    require(resolved.is_relative_to(ROOT), "PATH_OUTSIDE_REPOSITORY")
    return resolved


def read(path):
    path = local(path)
    require(path.is_file() and path.stat().st_size <= MAX_FILE_BYTES, "MISSING_OR_OVERSIZED_REVIEW_INPUT")
    return path.read_bytes()


def load(path):
    return json.loads(read(path))


def digest(path):
    return hashlib.sha256(read(path)).hexdigest()


def instant(value):
    result = datetime.fromisoformat(value.replace("Z", "+00:00"))
    require(result.utcoffset() is not None, "TIMEZONE_REQUIRED")
    return result


class Links(HTMLParser):
    def __init__(self):
        super().__init__()
        self.links = set()

    def handle_starttag(self, tag, attrs):
        if tag.lower() == "a":
            self.links.update(value.rstrip("/").lower() for name, value in attrs if name == "href" and value)


def check_member(member, catalog, exchanges, yahoo, alpha):
    """Unknown broker constraints must remain unknown, even with correct identities."""
    require(member["orderable"] is False, "REVIEW_CANNOT_MAKE_AN_INSTRUMENT_ORDERABLE")
    require(all(member[key] is None for key in ("minimum_quantity", "quantity_step", "price_tick")),
            "UNPROVEN_ORDER_PRECISION")
    require(member["fractional_support"] == "UNVERIFIED", "UNPROVEN_FRACTIONAL_SUPPORT")
    matches = [row for row in catalog if row["ticker"] == member["broker_ticker"]]
    require(len(matches) == 1, "BROKER_TICKER_MISSING_OR_AMBIGUOUS")
    row = matches[0]
    require(row["isin"] == member["isin"] and row["currencyCode"] == member["currency"] == "EUR"
            and row["type"] == "STOCK" and row["workingScheduleId"] == member["working_schedule_id"]
            and member["broker_instrument_id"] == row["ticker"], "BROKER_IDENTITY_CHANGED")
    venues = [venue for venue in exchanges if venue["id"] == member["broker_exchange_id"]
              and venue["name"] == member["exchange"]
              and any(schedule["id"] == row["workingScheduleId"] for schedule in venue["workingSchedules"])]
    require(len(venues) == 1, "BROKER_SCHEDULE_VENUE_MISMATCH")
    require(yahoo["status"] == alpha["status"] == "OK", "PROVIDER_IDENTITY_UNAVAILABLE")
    chart = yahoo["response"]["chart"]
    require(chart.get("error") is None and len(chart["result"]) == 1, "YAHOO_IDENTITY_AMBIGUOUS")
    meta = chart["result"][0]["meta"]
    require(meta["symbol"] == member["yahoo_symbol"] and meta["currency"] == "EUR"
            and meta["exchangeName"] == member["yahoo_exchange_code"]
            and meta["exchangeTimezoneName"] == member["session_timezone"]
            and meta["instrumentType"] == "EQUITY", "YAHOO_LISTING_MISMATCH")
    alpha_matches = [match for match in alpha["response"]["bestMatches"]
                     if match["1. symbol"] == member["alpha_vantage_symbol"]
                     and match["4. region"] == member["alpha_vantage_region"]
                     and match["8. currency"] == "EUR" and match["3. type"] == "Equity"]
    require(len(alpha_matches) == 1, "ALPHA_REGION_OR_CURRENCY_MISMATCH")


def semantics(bundle):
    crosswalk = load(bundle / "crosswalk.json")
    review = load(bundle / "review.json")
    require(crosswalk["review_scope"] == "SHADOW_IDENTITY_ONLY"
            and crosswalk["broker_mutations_authorized"] is False and crosswalk["live_enabled"] is False,
            "SHADOW_SCOPE_CHANGED")
    require(review["review_kind"] == "AGENT_SELF_REVIEW_OF_PRIMARY_SOURCES"
            and review["independent_reviewer_approval"] is False and review["owner_release_approval"] is False
            and review["entry_authorized"] is False and review["live_enabled"] is False
            and review["broker_mutations"] == 0 and review["first_shadow_run_passed"] is False
            and review["paper_day_passed"] is False, "REVIEW_AUTHORITY_OVERSTATED")
    capture_dir = local(crosswalk["catalog_capture"]).parent
    account = load(capture_dir / "account-binding.json")
    require(account["status"] == "OK" and account["response"] == {"id": 52211463, "currency": "EUR"}
            and crosswalk["broker_account_id"] == 52211463 and crosswalk["broker_environment"] == "demo"
            and crosswalk["base_currency"] == "EUR", "DEMO_ACCOUNT_IDENTITY_CHANGED")
    catalog_capture = load(crosswalk["catalog_capture"])
    exchange_capture = load(crosswalk["exchange_capture"])
    require(catalog_capture["status"] == exchange_capture["status"] == "OK", "CATALOG_UNAVAILABLE")
    require(len(catalog_capture["response"]) == review["catalog_instrument_count"], "CATALOG_COUNT_CHANGED")
    members = crosswalk["members"]
    require(len(members) == review["accepted_identity_count"] == 2 and review["orderable_instrument_count"] == 0,
            "REVIEWED_SCOPE_CHANGED")
    require(len({member["isin"] for member in members}) == 2, "AMBIGUOUS_ISIN")
    for member in members:
        check_member(member, catalog_capture["response"], exchange_capture["response"],
                     load(member["yahoo_capture"]), load(member["alpha_vantage_capture"]))
    readonly = load("config/yfinance.eur-shadow.reviewed.json")
    require(readonly["base_currency"] == "EUR" and readonly["quote_max_age_seconds"] == 10
            and readonly["symbols"] == {member["internal_instrument_id"]: member["yahoo_symbol"] for member in members},
            "READONLY_MAPPING_OR_FRESHNESS_CHANGED")
    for member in members:
        binding = readonly["bindings"][member["internal_instrument_id"]]
        require(binding == {key: member[key] for key in ("broker_instrument_id", "isin", "exchange_mic")},
                "READONLY_BROKER_BINDING_CHANGED")
    research = load("config/research.eur.json")
    sources = {source["id"]: source for source in research["sources"]}
    require(research["x_budget"] == load("config/research.json")["x_budget"], "SHARED_X_BUDGET_CHANGED")
    require(sources["alpha_eur_news_sentiment"]["credential_env"] == "TRADY_ALPHA_VANTAGE_API_KEY"
            and sources["alpha_eur_news_sentiment"]["daily_request_limit"] <= 20,
            "ALPHA_CREDENTIAL_OR_BUDGET_MISMATCH")
    for source_id in ("alpha_eur_news_sentiment", "airbus_eur_press_rss", "x_eur_airbus", "x_eur_ecb"):
        source = sources[source_id]
        require(source["enabled"] is False and source["rights"]["store_content"] is False
                and source["rights"]["llm_processing"] is False, "UNREVIEWED_SOURCE_ENABLED")
    require(sources["ecb_eur_press_rss"]["url"] == "https://www.ecb.europa.eu/rss/press.html"
            and sources["ecb_eur_press_rss"]["enabled"] is True, "ECB_REVIEW_SCOPE_CHANGED")
    for suffix in ("airbus", "ecb"):
        source = sources["x_eur_" + suffix]
        capture = load(capture_dir / ("x_" + suffix + ".json"))
        require(capture["status"] == "OK" and capture["post_content_requested"] is False
                and capture["budget_accounting"] == "EXISTING_POSTGRESQL_X_COST_LEDGER", "X_ACCESS_UNVERIFIED")
        user = capture["response"]["user"]
        require(user["id"] == source["x_account"]["user_id"]
                and user["username"].lower() == source["x_account"]["handle"].lower()
                and capture["received_at"] == source["x_account"]["verified_at"], "X_STABLE_ID_MISMATCH")
        ownership = load(capture_dir / ("airbus_shares.json" if suffix == "airbus" else "ecb_social.json"))
        require(ownership["status"] == "OK"
                and ownership["endpoint"] == source["x_account"]["ownership_reference"], "X_OWNERSHIP_UNAVAILABLE")
        links = Links()
        links.feed(ownership["response"])
        require("https://x.com/" + user["username"].lower() in links.links, "X_OFFICIAL_LINK_MISSING")
    observations = capture_dir.parent
    ecb = load(observations / "ecb-collection.json")["reports"]
    require(len(ecb) == 1 and ecb[0]["source_id"] == "ecb_eur_press_rss" and ecb[0]["status"] == "COMPLETE"
            and ecb[0]["changed_items"] == 15 and "rss_snapshot_not_full_history" in ecb[0]["gaps"],
            "ECB_POSTGRESQL_COLLECTION_NOT_PROVEN")
    items = load(observations / "ecb-model-read.json")["items"]
    require(len(items) == 5 and all(item["source_id"] == "ecb_eur_press_rss"
            and item["rights"]["llm_processing"] is True and item.get("text") is None for item in items),
            "ECB_MODEL_READBACK_SCOPE_CHANGED")
    airbus = load(observations / "airbus-collection.json")["reports"]
    require(len(airbus) == 1 and airbus[0]["status"] == "UNAVAILABLE" and airbus[0]["changed_items"] == 0
            and "rss_parser_unhealthy" in airbus[0]["gaps"], "AIRBUS_FAILURE_NOT_PRESERVED")
    for suffix in ("airbus", "asml"):
        quote = load(observations / ("yfinance-" + suffix + "-cli.json"))
        require(quote["entry_authorized"] is False and quote["quote"]["tradable"] is False
                and quote["quote"]["fresh"] is False and quote["quote"]["bid"] is None
                and quote["quote"]["ask"] is None, "QUOTE_LIMITATIONS_NOT_PRESERVED")
    budget = load(observations / "x-budget-after.json")["accounting"]
    require(budget["policy_matches"] is True and budget["support_microusd"] == 20000
            and budget["effective_microusd"] == 20000 and budget["remaining_microusd"] == 1880000,
            "METERED_IDENTITY_COST_MISMATCH")
    start, end = instant(crosswalk["effective_at"]), instant(crosswalk["expires_at"])
    require(end > start and (end - start).total_seconds() <= 86400, "REVIEW_EXPIRY_TOO_LONG")
    return start <= datetime.now(timezone.utc) < end


def seal(bundle):
    require(not (bundle / "hashes.json").exists(), "SEALED_REVIEW_ALREADY_EXISTS")
    semantics(bundle)
    crosswalk = load(bundle / "crosswalk.json")
    captures = local(crosswalk["catalog_capture"]).parent.parent
    paths = sorted(set([*bundle.glob("*.json"), bundle / "README.md",
                        *captures.glob("*.json"), *captures.glob("*.log"), *captures.glob("provider-snapshots/*.json"),
                        ROOT / "scripts/capture-eur-review.py", Path(__file__).resolve(),
                        ROOT / "config/research.eur.json", ROOT / "config/research.json",
                        ROOT / "config/yfinance.eur-shadow.reviewed.json",
                        ROOT / "config/eur_equity_universe.example.json",
                        ROOT / "config/yfinance.euronext.example.json", ROOT / "config/eur-demo.example.json"]))
    commit = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True, check=True).stdout.strip()
    document = {
        "schema_version": 1, "review_reference": crosswalk["review_reference"],
        "sealed_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "source_base_commit": commit,
        "code_binding": "Base commit plus the exact file hashes below; not a release candidate.",
        "entry_authorized": False, "live_enabled": False,
        "files": [{"path": str(path.relative_to(ROOT)), "sha256": digest(path)} for path in paths],
    }
    with (bundle / "hashes.json").open("x", encoding="utf-8") as output:
        json.dump(document, output, indent=2)
        output.write("\n")


def verify(bundle):
    manifest = load(bundle / "hashes.json")
    require(manifest["entry_authorized"] is False and manifest["live_enabled"] is False, "MANIFEST_AUTHORITY_CHANGED")
    paths = set()
    for item in manifest["files"]:
        require(item["path"] not in paths, "DUPLICATE_REVIEW_FILE")
        paths.add(item["path"])
        require(digest(item["path"]) == item["sha256"], "REVIEW_FILE_HASH_MISMATCH: " + item["path"])
    mandatory = [bundle / "review.json", bundle / "crosswalk.json", bundle / "README.md",
                 ROOT / "scripts/capture-eur-review.py", Path(__file__).resolve(),
                 ROOT / "config/research.eur.json", ROOT / "config/research.json",
                 ROOT / "config/yfinance.eur-shadow.reviewed.json"]
    require(all(str(path.relative_to(ROOT)) in paths for path in mandatory), "INCOMPLETE_HASH_MANIFEST")
    crosswalk = load(bundle / "crosswalk.json")
    capture_dir = local(crosswalk["catalog_capture"]).parent
    required_captures = {crosswalk["catalog_capture"], crosswalk["exchange_capture"],
                         str((capture_dir / "account-binding.json").relative_to(ROOT)),
                         str((capture_dir / "x_airbus.json").relative_to(ROOT)),
                         str((capture_dir / "x_ecb.json").relative_to(ROOT)),
                         str((capture_dir / "airbus_shares.json").relative_to(ROOT)),
                         str((capture_dir / "ecb_social.json").relative_to(ROOT))}
    for member in crosswalk["members"]:
        required_captures.update([member["yahoo_capture"], member["alpha_vantage_capture"]])
    for name in ("ecb-collection", "ecb-model-read", "airbus-collection", "yfinance-airbus-cli",
                 "yfinance-asml-cli", "x-budget-after"):
        required_captures.add(str((capture_dir.parent / (name + ".json")).relative_to(ROOT)))
    require(required_captures.issubset(paths), "UNPINNED_PROVIDER_OBSERVATION")
    return len(paths), semantics(bundle)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", type=Path, default=DEFAULT_BUNDLE)
    parser.add_argument("--seal", action="store_true")
    parser.add_argument("--integrity-only", action="store_true")
    args = parser.parse_args()
    bundle = local(args.bundle)
    if args.seal:
        seal(bundle)
    count, current = verify(bundle)
    require(current or args.integrity_only, "REVIEW_EXPIRED_REFRESH_BEFORE_SHADOW_USE")
    print(json.dumps({"status": "REVIEW_INTEGRITY_VERIFIED", "files_verified": count,
                      "current_for_shadow_identity_research": current,
                      "independent_reviewer_approval": False, "first_shadow_run_passed": False,
                      "entry_authorized": False, "live_enabled": False}))


if __name__ == "__main__":
    try:
        main()
    except (ValueError, KeyError, TypeError, OSError, subprocess.SubprocessError) as error:
        print("EUR review verification failed: " + str(error), file=sys.stderr)
        sys.exit(1)
