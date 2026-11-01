import TradyProof.Formula
import Lean.Data.Json.Parser

namespace TradyProof.Parsing
open Lean Formula
open Std.Internal.Parsec Std.Internal.Parsec.String

-- Check UTF-16 escapes before Char.ofNat can replace invalid scalars with NUL.
private def codeUnit : Parser Nat := do
  let a ← Json.Parser.hexChar; let b ← Json.Parser.hexChar
  let c ← Json.Parser.hexChar; let d ← Json.Parser.hexChar
  return 4096 * a + 256 * b + 16 * c + d

private def escapedScalar : Parser Char := do
  match ← any with
  | '\\' => return '\\'
  | '"' => return '"'
  | '/' => return '/'
  | 'b' => return '\x08'
  | 'f' => return '\x0c'
  | 'n' => return '\n'
  | 'r' => return '\r'
  | 't' => return '\t'
  | 'u' =>
    let high ← codeUnit
    if 0xd800 ≤ high ∧ high ≤ 0xdbff then
      skipChar '\\'; skipChar 'u'
      let low ← codeUnit
      if 0xdc00 ≤ low ∧ low ≤ 0xdfff then
        return Char.ofNat (0x10000 + (high - 0xd800) * 1024 + low - 0xdc00)
      else fail "invalid Unicode surrogate pair"
    else if 0xdc00 ≤ high ∧ high ≤ 0xdfff then
      fail "isolated Unicode surrogate"
    else return Char.ofNat high
  | _ => fail "illegal JSON string escape"

private def string : Parser String := do
  skipChar '"'
  let chars ← many (do
    let c ← peek!
    if c == '"' then fail "end of JSON string"
    skip
    if c == '\\' then escapedScalar
    else if c.toNat < 0x20 then fail "unexpected control character in string"
    else pure c)
  skipChar '"'
  return String.mk chars.toList

-- All occurrences of these field names in the checked production schemas are
-- unsigned integers. Check their lexemes before JsonNumber normalizes 1e0/-0.
private def unsignedFields : List String :=
  ["version", "confidence_entry_min", "max_holding_sessions", "atr_lookback",
   "bar_seconds", "completed_bars", "base_confidence_score", "confidence_score"]

private def scalar (unsigned : Bool) : Parser Json := do
  let c ← peek!
  if c == '"' then
    let decoded ← string
    ws
    return Json.str decoded
  else if unsigned && (c == '-' || ('0' ≤ c && c ≤ '9')) then
    if c == '-' then fail "Natural number expected: unsigned JSON integer lexeme required"
    let n ← Json.Parser.nat
    if !(← isEof) then
      let next ← peek!
      if next == '.' || next == 'e' || next == 'E' then
        fail "Natural number expected: unsigned JSON integer lexeme required"
    ws
    return toJson n
  else Json.Parser.anyCore

-- Reject duplicate decoded keys rather than overwriting object-map entries.
private def items (item : Parser α) (closing : Char) : Parser (Array α) := do
  if (← peek!) == closing then
    skip; ws
    return #[]
  let first ← item
  let rest ← many (do skipChar ','; ws; item)
  skipChar closing; ws
  return #[first] ++ rest

-- Nesting decreases structurally; the verifier's declaration restrictions stay
-- intact. Repetition uses the same standard parser combinators as Lean itself.
private def value : Nat → Bool → Parser Json
  | 0, unsigned => do
    let c ← peek!
    if c == '{' || c == '[' then fail "JSON nesting limit exceeded"
    scalar unsigned
  | remaining + 1, unsigned => do
    let c ← peek!
    if c == '{' then
      skip; ws
      let member := do
        let key ← string
        ws
        Json.Parser.lookahead (· == ':') ":"; skip; ws
        return (key, ← value remaining (unsignedFields.contains key))
      let fields ← items member '}'
      let mut keys : List String := []
      for (key, _) in fields do
        if keys.contains key then fail "duplicate JSON key"
        keys := key :: keys
      return Json.mkObj fields.toList
    else if c == '[' then
      skip; ws
      return Json.arr (← items (value remaining false) ']')
    else scalar unsigned

def parse (text : String) : Except String Json :=
  Parser.run (do ws; let result ← value 128 false; eof; return result) text

private def ensure (ok : Bool) (message : String) : Except String Unit :=
  if ok then .ok () else .error message

private def fields (json : Json) (allowed : List String) : Except String Unit := do
  let object ← json.getObj?
  ensure (object.fold (fun valid key _ => valid && allowed.contains key) true) "unknown JSON field"

private def identifier (json : Json) : Except String Unit := do
  ensure (validIdentifier (← json.getStr?)) "invalid identifier"

private def currency (json : Json) : Except String Unit := do
  ensure (validCurrency (← json.getStr?)) "currency must be three uppercase letters"

private def choice (json : Json) (allowed : List String) : Except String Unit := do
  ensure (allowed.contains (← json.getStr?)) "unknown enum value"

private def natural (json : Json) (maximum : Nat) : Except String Unit := do
  ensure (decide ((← json.getNat?) ≤ maximum)) "integer exceeds production field width"

private def times (json : Json) (names : List String) : Except String Unit := do
  for name in names do
    let time ← (← json.getObjVal? name).getInt?
    ensure (validTimestamp time) "invalid timestamp transport"

private def money (json : Json) : Except String Unit := do
  fields json ["amount", "currency"]
  currency (← json.getObjVal? "currency")

private def costs (json : Json) : Except String Unit := do
  fields json ["fixed", "rate", "uncertainty", "fx"]
  money (← json.getObjVal? "fixed")
  money (← json.getObjVal? "uncertainty")
  let fx := json.getObjValD "fx"
  if !fx.isNull then
    fields fx ["from", "to", "rate", "observed_at"]
    currency (← fx.getObjVal? "from")
    currency (← fx.getObjVal? "to")
    times fx ["observed_at"]

private def account (json : Json) : Except String Unit := do
  fields json ["snapshot_id", "account_id", "mode", "spendable_cash", "positions", "orders", "observed_at"]
  identifier (← json.getObjVal? "account_id")
  choice (← json.getObjVal? "mode") ["paper", "shadow", "live"]
  money (← json.getObjVal? "spendable_cash")
  times json ["observed_at"]
  for position in (← (← json.getObjVal? "positions").getArr?) do
    fields position ["instrument_id", "quantity"]
    identifier (← position.getObjVal? "instrument_id")
  for order in (← (← json.getObjVal? "orders").getArr?) do
    fields order ["broker_order_id", "client_intent_id", "instrument_id", "side", "quantity", "filled_quantity", "status"]
    identifier (← order.getObjVal? "instrument_id")
    let intent := order.getObjValD "client_intent_id"
    if !intent.isNull then identifier intent
    choice (← order.getObjVal? "side") ["BUY", "SELL"]
    choice (← order.getObjVal? "status") ["OPEN", "PARTIALLY_FILLED", "FILLED", "CANCELLED", "REJECTED", "UNKNOWN"]

-- Derived FromJson supplies the remaining required-field/type/decimal checks.
-- These guards cover the additional newtype, enum, width and unknown-field
-- constraints carried by production's Serde types.
def formula (json : Json) : Except String Formula.Request := do
  fields json ["kind", "policy", "envelope", "parameters", "max_account_risk_fraction", "input", "as_of"]
  choice (← json.getObjVal? "kind") ["FORMULA"]
  times json ["as_of"]
  let policy ← json.getObjVal? "policy"
  fields policy ["version", "entry_cash_fraction", "confidence_entry_min", "max_target_fraction",
    "exceptional_targets_enabled", "max_initial_loss_fraction", "min_reward_to_risk", "max_holding_sessions",
    "leverage", "add_to_position", "rotation_enabled"]
  natural (← policy.getObjVal? "version") 4294967295
  natural (← policy.getObjVal? "confidence_entry_min") 255
  natural (← policy.getObjVal? "max_holding_sessions") 255
  let envelope ← json.getObjVal? "envelope"
  fields envelope ["max_entry_allowance_fraction", "min_stop_atr_multiple", "max_stop_atr_multiple",
    "min_target_net_fraction", "max_target_net_fraction"]
  let parameters ← json.getObjVal? "parameters"
  fields parameters ["version", "family", "entry_allowance_fraction", "stop_atr_multiple", "target_net_fraction",
    "max_holding_sessions", "atr_lookback", "bar_seconds"]
  choice (← parameters.getObjVal? "family") ["TREND", "REVERSAL"]
  for (name, maximum) in [("version", 4294967295), ("max_holding_sessions", 255),
      ("atr_lookback", 65535), ("bar_seconds", 4294967295)] do
    natural (← parameters.getObjVal? name) maximum
  let input ← json.getObjVal? "input"
  fields input ["account", "market_snapshot_id", "quote", "entry_range", "entry_costs", "exit_costs", "rules",
    "evidence", "holding_deadline_at", "valid_until"]
  account (← input.getObjVal? "account")
  times input ["holding_deadline_at", "valid_until"]
  let quote ← input.getObjVal? "quote"
  fields quote ["instrument_id", "currency", "bid", "ask", "observed_at", "received_at", "tradable"]
  identifier (← quote.getObjVal? "instrument_id")
  currency (← quote.getObjVal? "currency")
  times quote ["observed_at", "received_at"]
  let range ← input.getObjVal? "entry_range"
  fields range ["minimum", "maximum", "currency"]
  currency (← range.getObjVal? "currency")
  costs (← input.getObjVal? "entry_costs")
  costs (← input.getObjVal? "exit_costs")
  fields (← input.getObjVal? "rules") ["price_tick", "quantity_step", "minimum_quantity", "currency_unit"]
  let evidence ← input.getObjVal? "evidence"
  fields evidence ["snapshot_id", "family", "signal_eligible", "atr", "atr_lookback", "bar_seconds", "completed_bars",
    "last_completed_at", "available_at", "valid_until", "minimum_stop_distance", "noise_distance",
    "base_confidence_score", "confidence_score", "sentiment_entry_allowed", "sentiment_target_ceiling", "exceptional_verified"]
  choice (← evidence.getObjVal? "family") ["TREND", "REVERSAL"]
  for (name, maximum) in [("atr_lookback", 65535), ("bar_seconds", 4294967295), ("completed_bars", 4294967295),
      ("base_confidence_score", 255), ("confidence_score", 255)] do
    natural (← evidence.getObjVal? name) maximum
  times evidence ["last_completed_at", "available_at", "valid_until"]
  fromJson? json

def decimal (json : Json) : Except String Unit := do
  fields json ["kind", "operation", "a", "b"]
  choice (← json.getObjVal? "kind") ["DECIMAL"]
  choice (← json.getObjVal? "operation") ["ADD", "SUB", "MUL", "DIV", "REM", "FLOOR", "CEIL"]

end TradyProof.Parsing
