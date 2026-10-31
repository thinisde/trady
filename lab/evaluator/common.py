"""Strict input and numerical conventions shared by the sealed evaluator."""
import hashlib
import json
import math
import pathlib
import re
from datetime import datetime, timezone
from decimal import Decimal
from fractions import Fraction

MAX_BYTES = 32 * 1024 * 1024
MAX_ROWS = 100_000
EVENTS = ("TARGET_FIRST", "STOP_FIRST", "EXPIRY", "NON_FILL")
GATE_FIELDS = {"passed", "approved", "complete", "dependence_and_search_adjusted",
               "causal_and_uncontaminated", "risk_not_deteriorated", "economic_feasibility_passed",
               "neighborhood_checks_passed", "improvement_lower_bound"}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def canonical(value):
    return json.dumps(value, sort_keys=True, ensure_ascii=False, separators=(",", ":"), allow_nan=False).encode()


def sha(value):
    return hashlib.sha256(canonical(value)).hexdigest()


def valid_hash(value):
    return isinstance(value, str) and re.fullmatch("[a-f0-9]{64}", value) is not None


def timestamp(value):
    require(isinstance(value, str), "timestamp must be explicit")
    result = datetime.fromisoformat(value.replace("Z", "+00:00"))
    require(result.tzinfo is not None, "naive timestamp")
    return result.astimezone(timezone.utc)


def number(value):
    """Validate a bounded decimal spelling, then retain its exact rational value.

    Decimal construction from a string is exact. All subsequent financial
    arithmetic uses integer fractions, independent of the ambient Decimal
    precision, rounding mode and traps. Convert to float only for statistics.
    """
    require(isinstance(value, str) and len(value) <= 96, "financial values must be decimal strings")
    result = Decimal(value)
    require(result.is_finite() and len(result.as_tuple().digits) <= 28
            and -28 <= result.as_tuple().exponent <= 28 and result.adjusted() <= 28,
            "nonfinite or unbounded decimal")
    return Fraction(result)


def integer(value, minimum=0, maximum=MAX_ROWS):
    require(type(value) is int and minimum <= value <= maximum, "invalid bounded integer")
    return value


def ratio(numerator, denominator):
    require(denominator > 0, "undefined denominator")
    return Fraction(numerator) / Fraction(denominator)


def rational_json(value):
    """Auditable exact diagnostic, including nonterminating decimal ratios."""
    value = Fraction(value)
    return {"numerator": str(value.numerator), "denominator": str(value.denominator)}


def numeric_margin(policy):
    margin = number(policy["numeric_margin"])
    require(Fraction(1, 10**9) <= margin <= Fraction(1, 10**4),
            "explicit conservative numerical margin required")
    return margin


def decimal_string(value):
    """Serialize an exact terminating financial result without Decimal rounding."""
    value = Fraction(value)
    denominator = value.denominator
    twos = fives = 0
    while denominator % 2 == 0:
        denominator //= 2
        twos += 1
    while denominator % 5 == 0:
        denominator //= 5
        fives += 1
    require(denominator == 1, "nonterminating financial value cannot be serialized exactly")
    scale = max(twos, fives)
    units = abs(value.numerator) * (10**scale // value.denominator)
    digits = str(units).rjust(scale + 1, "0")
    result = (digits[:-scale] + "." + digits[-scale:]).rstrip("0").rstrip(".") if scale else digits
    return ("-" if value < 0 else "") + result


def finite(value):
    require(math.isfinite(value), "undefined statistic")
    return value


def unique(rows, key):
    require(isinstance(rows, list) and len(rows) <= MAX_ROWS, "unbounded or missing records")
    result = {}
    for row in rows:
        identity = row[key]
        require(isinstance(identity, str) and 0 < len(identity) <= 160, "invalid record identity")
        require(identity not in result, "duplicate or rewritten record identity")
        result[identity] = row
    return result


def no_gate_assertions(value):
    if isinstance(value, dict):
        require(not GATE_FIELDS.intersection(value), "caller-supplied acceptance flag or metric")
        for child in value.values():
            no_gate_assertions(child)
    elif isinstance(value, list):
        for child in value:
            no_gate_assertions(child)


def strict_json(data):
    require(len(data) <= MAX_BYTES, "document size limit")

    def object_pairs(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, "duplicate JSON key")
            result[key] = value
        return result

    def reject(_):
        raise ValueError("JSON decimal values must be strings; nonfinite values are forbidden")

    return json.loads(data, object_pairs_hook=object_pairs, parse_float=reject, parse_constant=reject)


def pinned_json(pin, production=True):
    path = pathlib.Path(pin["path"])
    require(path.is_absolute() and not path.is_symlink(), "absolute regular pinned artifact required")
    meta = path.stat()
    require(path.is_file() and meta.st_size <= MAX_BYTES and valid_hash(pin["sha256"]), "invalid artifact pin")
    if production:
        require(meta.st_uid == 0 and meta.st_mode & 0o022 == 0, "deployment-owned artifact required")
    with path.open("rb") as source:
        data = source.read(MAX_BYTES + 1)
    require(len(data) <= MAX_BYTES and hashlib.sha256(data).hexdigest() == pin["sha256"], "sealed artifact hash mismatch")
    return strict_json(data)


def rounded_lower(value):
    # Performance inference uses IEEE doubles, with an explicit downward margin.
    exact = Fraction(finite(value))
    units = (exact.numerator * 10**12) // exact.denominator
    return decimal_string(Fraction(units, 10**12))
