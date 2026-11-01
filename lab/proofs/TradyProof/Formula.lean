import TradyProof.CheckedDecimal

set_option maxRecDepth 4096
set_option maxHeartbeats 800000

namespace TradyProof.Formula
open CheckedDecimal

-- Timestamp transport encodes Chrono's (epoch seconds, subsecond nanoseconds)
-- lexicographic pair with a 2e9 stride. Fractions >= 1e9 retain a represented
-- leap second instead of aliasing it with the next ordinary second.
-- The fixture driver performs syntax conversion, never a pass/fail decision.
abbrev Timestamp := Int
def second : Int := 2000000000
def nanosPerSecond : Int := 1000000000
def minTimestamp : Int := -16669202457600000000000
def maxOrdinaryTimestamp : Int := 16420533753598999999999
-- Chrono can parse the final day's second 60, beyond DateTime::MAX_UTC.
def maxTimestamp : Int := 16420533753599999999999
def validTimestamp (time : Timestamp) : Bool :=
  decide (minTimestamp / second ≤ time / second ∧ time / second ≤ maxTimestamp / second ∧
    (time % second < nanosPerSecond ∨ (time / second) % 60 = 59))
def durationNanos (later earlier : Timestamp) : Int :=
  let laterSeconds := later / second
  let earlierSeconds := earlier / second
  let laterFraction := later % second
  let earlierFraction := earlier % second
  let leap := if earlierSeconds % 86400 < laterSeconds % 86400 ∧ nanosPerSecond ≤ earlierFraction then 1
    else if laterSeconds % 86400 < earlierSeconds % 86400 ∧ nanosPerSecond ≤ laterFraction then -1 else 0
  (laterSeconds - earlierSeconds + leap) * nanosPerSecond + laterFraction - earlierFraction
-- All formula expiry durations are positive whole seconds. Chrono leaves a
-- represented leap second when adding them and removes the extra fraction.
def addSeconds (observed : Timestamp) (age : Nat) : Timestamp :=
  (observed / second + age) * second + observed % second % nanosPerSecond
def fresh (observed now : Timestamp) (seconds : Nat) : Prop :=
  observed ≤ now ∧ durationNanos now observed ≤ seconds * nanosPerSecond
instance (observed now : Timestamp) (seconds : Nat) : Decidable (fresh observed now seconds) :=
  inferInstanceAs (Decidable (_ ∧ _))

structure Money where
  amount : Decimal
  currency : String
  deriving Lean.FromJson, Lean.ToJson, Repr
structure Position where
  instrument_id : String
  quantity : Decimal
  deriving Lean.FromJson, Repr
structure BrokerOrder where
  broker_order_id : String
  client_intent_id : Option String
  instrument_id : String
  side : String
  quantity : Decimal
  filled_quantity : Decimal
  status : String
  deriving Lean.FromJson, Repr
structure Account where
  snapshot_id : String
  account_id : String
  mode : String
  spendable_cash : Money
  positions : List Position
  orders : List BrokerOrder
  observed_at : Timestamp
  deriving Lean.FromJson, Repr
structure Quote where
  instrument_id : String
  currency : String
  bid : Decimal
  ask : Decimal
  observed_at : Timestamp
  received_at : Timestamp
  tradable : Bool
  deriving Lean.FromJson, Repr
structure EntryRange where
  minimum : Decimal
  maximum : Decimal
  currency : String
  deriving Lean.FromJson, Repr
structure Fx where
  «from» : String
  to : String
  rate : Decimal
  observed_at : Timestamp
  deriving Lean.FromJson, Repr
structure Costs where
  fixed : Money
  rate : Decimal
  uncertainty : Money
  fx : Option Fx
  deriving Lean.FromJson, Repr
structure Rules where
  price_tick : Decimal
  quantity_step : Decimal
  minimum_quantity : Decimal
  currency_unit : Decimal
  deriving Lean.FromJson, Repr
structure Policy where
  version : Nat
  entry_cash_fraction : Decimal
  confidence_entry_min : Nat
  max_target_fraction : Decimal
  exceptional_targets_enabled : Bool
  max_initial_loss_fraction : Decimal
  min_reward_to_risk : Decimal
  max_holding_sessions : Nat
  leverage : Bool
  add_to_position : Bool
  rotation_enabled : Bool
  deriving Lean.FromJson, Repr, DecidableEq
structure Envelope where
  max_entry_allowance_fraction : Decimal
  min_stop_atr_multiple : Decimal
  max_stop_atr_multiple : Decimal
  min_target_net_fraction : Decimal
  max_target_net_fraction : Decimal
  deriving Lean.FromJson, Repr, DecidableEq
structure Parameters where
  version : Nat
  family : String
  entry_allowance_fraction : Decimal
  stop_atr_multiple : Decimal
  target_net_fraction : Decimal
  max_holding_sessions : Nat
  atr_lookback : Nat
  bar_seconds : Nat
  deriving Lean.FromJson, Repr, DecidableEq
structure Evidence where
  snapshot_id : String
  family : String
  signal_eligible : Bool
  atr : Decimal
  atr_lookback : Nat
  bar_seconds : Nat
  completed_bars : Nat
  last_completed_at : Timestamp
  available_at : Timestamp
  valid_until : Timestamp
  minimum_stop_distance : Decimal
  noise_distance : Decimal
  base_confidence_score : Nat
  confidence_score : Nat
  sentiment_entry_allowed : Bool
  sentiment_target_ceiling : Decimal
  exceptional_verified : Bool
  deriving Lean.FromJson, Repr
structure Input where
  account : Account
  market_snapshot_id : String
  quote : Quote
  entry_range : EntryRange
  entry_costs : Costs
  exit_costs : Costs
  rules : Rules
  evidence : Evidence
  holding_deadline_at : Timestamp
  valid_until : Timestamp
  deriving Lean.FromJson, Repr
structure Request where
  policy : Policy
  envelope : Envelope
  parameters : Parameters
  max_account_risk_fraction : Decimal
  input : Input
  as_of : Timestamp
  deriving Lean.FromJson, Repr
structure Output where
  authority : String
  account_snapshot_id : String
  market_snapshot_id : String
  evidence_snapshot_id : String
  calculated_at : Timestamp
  valid_until : Timestamp
  buy_limit : Decimal
  quantity : Decimal
  stop_trigger : Decimal
  target_sell_limit : Decimal
  target_net_fraction : Decimal
  target_rounding_overshoot : Decimal
  max_holding_sessions : Nat
  holding_deadline_at : Timestamp
  budget : Money
  acquisition_cost : Money
  proceeds_at_stop : Money
  proceeds_at_target : Money
  planned_loss_fraction : Decimal
  account_risk : Money
  reward_to_risk : Decimal
  deriving Lean.ToJson, Repr

structure PolicyCaps (p : Policy) : Prop where
  allocation : zero < p.entry_cash_fraction ∧ p.entry_cash_fraction ≤ D 98 2
  target : zero < p.max_target_fraction ∧ p.max_target_fraction ≤ D 10 2
  loss : zero < p.max_initial_loss_fraction ∧ p.max_initial_loss_fraction ≤ D 2 2
  reward : D 15 1 ≤ p.min_reward_to_risk
  holding : 1 ≤ p.max_holding_sessions ∧ p.max_holding_sessions ≤ 5

def validatePolicy (p : Policy) : Checked (PLift (PolicyCaps p)) := do
  let _ ← check (0 < p.version) "policy version must be positive"
  let ha ← check (zero < p.entry_cash_fraction ∧ p.entry_cash_fraction ≤ D 98 2)
    "entry allocation exceeds 98% or is nonpositive"
  let _ ← check (90 ≤ p.confidence_entry_min ∧ p.confidence_entry_min ≤ 100)
    "entry confidence must be 90 through 100"
  let ht ← check (zero < p.max_target_fraction ∧ p.max_target_fraction ≤ D 10 2)
    "target ceiling exceeds 10% or is nonpositive"
  let hl ← check (zero < p.max_initial_loss_fraction ∧ p.max_initial_loss_fraction ≤ D 2 2)
    "initial loss exceeds 2% or is nonpositive"
  let hr ← check (D 15 1 ≤ p.min_reward_to_risk) "reward/risk must be at least 1.5"
  let hh ← check (1 ≤ p.max_holding_sessions ∧ p.max_holding_sessions ≤ 5)
    "holding limit must be one through five sessions"
  let _ ← check (p.leverage = false ∧ p.add_to_position = false ∧ p.rotation_enabled = false)
    "leverage, additions and rotation are unsupported by the foundation"
  return ⟨⟨ha.down, ht.down, hl.down, hr.down, hh.down⟩⟩

def validateParameters (p : Policy) (e : Envelope) (v : Parameters) : Checked
    (PLift (1 ≤ v.max_holding_sessions ∧ v.max_holding_sessions ≤ p.max_holding_sessions ∧
      zero < v.target_net_fraction ∧ v.target_net_fraction ≤ p.max_target_fraction)) := do
  let _ ← validatePolicy p
  let he ← check
    (zero ≤ e.max_entry_allowance_fraction ∧ e.max_entry_allowance_fraction ≤ D 1 2 ∧
      zero < e.min_stop_atr_multiple ∧ e.min_stop_atr_multiple ≤ e.max_stop_atr_multiple ∧
      e.max_stop_atr_multiple ≤ D 10 0 ∧ zero < e.min_target_net_fraction ∧
      e.min_target_net_fraction ≤ e.max_target_net_fraction ∧
      e.max_target_net_fraction ≤ p.max_target_fraction)
    "invalid owner-approved formula envelope"
  let hv ← check
    (v.version = 1 ∧ zero ≤ v.entry_allowance_fraction ∧
      v.entry_allowance_fraction ≤ e.max_entry_allowance_fraction ∧
      e.min_stop_atr_multiple ≤ v.stop_atr_multiple ∧ v.stop_atr_multiple ≤ e.max_stop_atr_multiple ∧
      e.min_target_net_fraction ≤ v.target_net_fraction ∧ v.target_net_fraction ≤ e.max_target_net_fraction ∧
      1 ≤ v.max_holding_sessions ∧ v.max_holding_sessions ≤ p.max_holding_sessions ∧
      2 ≤ v.atr_lookback ∧ v.atr_lookback ≤ 252 ∧ 60 ≤ v.bar_seconds ∧ v.bar_seconds ≤ 86400)
    "formula parameters exceed the approved domain"
  return ⟨by
    refine ⟨hv.down.2.2.2.2.2.2.2.1, hv.down.2.2.2.2.2.2.2.2.1, ?_, ?_⟩
    · have ha := he.down.2.2.2.2.2.1
      have hb := hv.down.2.2.2.2.2.1
      change units zero < units e.min_target_net_fraction at ha
      change units e.min_target_net_fraction ≤ units v.target_net_fraction at hb
      change units zero < units v.target_net_fraction
      omega
    · have ha := hv.down.2.2.2.2.2.2.1
      have hb := he.down.2.2.2.2.2.2.2
      change units v.target_net_fraction ≤ units e.max_target_net_fraction at ha
      change units e.max_target_net_fraction ≤ units p.max_target_fraction at hb
      change units v.target_net_fraction ≤ units p.max_target_fraction
      omega⟩

-- Production's identifier and currency newtypes admit ASCII only. After this
-- check, character length and UTF-8 byte length coincide.
def validIdentifier (value : String) : Bool :=
  decide (0 < value.length ∧ value.length ≤ 96) && value.toList.all (fun c =>
    decide (('A' ≤ c ∧ c ≤ 'Z') ∨ ('a' ≤ c ∧ c ≤ 'z') ∨
      ('0' ≤ c ∧ c ≤ '9') ∨ c = '_' ∨ c = '-'))

def validCurrency (value : String) : Bool :=
  decide (value.length = 3) && value.toList.all (fun c => decide ('A' ≤ c ∧ c ≤ 'Z'))

def validAccountInput (a : Account) : Bool :=
  validIdentifier a.account_id && ["paper", "shadow", "live"].contains a.mode &&
  validCurrency a.spendable_cash.currency &&
  a.positions.all (fun p => validIdentifier p.instrument_id) &&
  a.orders.all (fun o => validIdentifier o.instrument_id &&
    (match o.client_intent_id with | none => true | some id => validIdentifier id) &&
    ["BUY", "SELL"].contains o.side &&
    ["OPEN", "PARTIALLY_FILLED", "FILLED", "CANCELLED", "REJECTED", "UNKNOWN"].contains o.status)

def validateAccount (a : Account) : Checked (PLift (validAccountInput a = true)) := do
  let hinput ← check (validAccountInput a = true) "invalid account input"
  let _ ← check (a.snapshot_id.isEmpty = false) "snapshot ID is required"
  let _ ← check (zero ≤ a.spendable_cash.amount) "negative spendable cash"
  let mut instruments : List String := []
  for p in a.positions do
    let _ ← check (zero < p.quantity) "position must be positive; shorting is unsupported"
    let _ ← check (p.instrument_id ∉ instruments) "duplicate position"
    instruments := p.instrument_id :: instruments
  let mut orders : List String := []
  for o in a.orders do
    let _ ← check (o.broker_order_id.isEmpty = false ∧ o.broker_order_id ∉ orders)
      "invalid or duplicate broker order"
    orders := o.broker_order_id :: orders
    let _ ← check (zero < o.quantity ∧ zero ≤ o.filled_quantity ∧ o.filled_quantity ≤ o.quantity)
      "invalid broker quantity"
    let _ ← check (o.status ≠ "FILLED" ∨ equal o.quantity o.filled_quantity = true)
      "filled order has remaining quantity"
    let _ ← check (o.status ≠ "PARTIALLY_FILLED" ∨
      (zero < o.filled_quantity ∧ o.filled_quantity < o.quantity))
      "partial order must have positive filled and remaining quantities"
  return hinput

def targetCeiling (score : Nat) (exceptional : Bool) (p : Policy) : Checked Decimal := do
  let _ ← validatePolicy p
  let _ ← check (score ≤ 100) "confidence score exceeds 100"
  let band := if score ≤ 69 then zero else if score ≤ 79 then D 1 2
    else if score ≤ 89 then D 2 2
    else if 95 ≤ score ∧ exceptional = true ∧ p.exceptional_targets_enabled = true then D 10 2
    else D 3 2
  return minimum band p.max_target_fraction

def fxRate (fromCurrency toCurrency : String) (observation : Option Fx) (now : Timestamp) : Checked Decimal := do
  if fromCurrency = toCurrency then return one
  let fx ← match observation with
    | some value => pure value
    | none => throw "FX observation required"
  let _ ← check (fx.from = fromCurrency ∧ fx.to = toCurrency ∧ zero < fx.rate ∧ fresh fx.observed_at now 60)
    "invalid or stale FX"
  return fx.rate

structure Sized where
  quantity : Decimal
  budget : Decimal
  deriving Repr

def retain {α : Type} (computation : Checked α) : Checked {value // computation = .ok value} :=
  match computation with
  | .ok value => .ok ⟨value, rfl⟩
  | .error message => .error message

structure SizingChecks (cash : Money) (quote : Quote) (range : EntryRange) (rules : Rules)
    (p : Policy) (s : Sized) : Prop where
  entry_bounds : range.minimum ≤ quote.ask ∧ quote.ask ≤ range.maximum
  allocation : ∃ allocated, mul cash.amount p.entry_cash_fraction = .ok allocated ∧ s.budget ≤ allocated
  currency_grid : onGrid s.budget rules.currency_unit

def sizeEntry (cash : Money) (quote : Quote) (range : EntryRange) (costs : Costs)
    (rules : Rules) (now : Timestamp) (p : Policy) :
    Checked {s // SizingChecks cash quote range rules p s} := do
  let _ ← validatePolicy p
  let _ ← check (zero ≤ cash.amount) "negative cash"
  let _ ← check (quote.tradable = true ∧ zero < quote.bid ∧ quote.bid ≤ quote.ask) "unusable bid/ask"
  let _ ← check (quote.observed_at ≤ quote.received_at ∧ fresh quote.observed_at now 10 ∧
      fresh quote.received_at now 10) "stale or future quote"
  let _ ← check (range.currency = quote.currency ∧ zero < range.minimum ∧ range.minimum ≤ range.maximum)
    "invalid entry range or currency"
  let hrange ← check (range.minimum ≤ quote.ask ∧ quote.ask ≤ range.maximum) "ask outside inclusive entry range"
  let _ ← check (zero < rules.price_tick ∧ zero < rules.quantity_step ∧
    zero < rules.minimum_quantity ∧ zero < rules.currency_unit) "invalid instrument increments"
  let _ ← check (costs.fixed.currency = cash.currency ∧ costs.uncertainty.currency = cash.currency)
    "cost currency mismatch"
  let _ ← check (zero ≤ costs.fixed.amount ∧ zero ≤ costs.uncertainty.amount ∧ zero ≤ costs.rate)
    "negative costs"
  let fx ← fxRate quote.currency cash.currency costs.fx now
  let limitPrice ← ceilTo quote.ask rules.price_tick
  let _ ← check (range.minimum ≤ limitPrice ∧ limitPrice ≤ range.maximum) "no valid entry tick inside range"
  let allocated ← retain (mul cash.amount p.entry_cash_fraction)
  let roundedBudget ← floorCertified allocated.val rules.currency_unit
  let budget := roundedBudget.val
  let fixed ← add costs.fixed.amount costs.uncertainty.amount
  let _ ← check (fixed < budget) "cash cannot cover costs"
  let unitCost ← mul (← mul limitPrice fx) (← add one costs.rate)
  let quantity ← floorTo (← div (← sub budget fixed) unitCost) rules.quantity_step
  let _ ← check (rules.minimum_quantity ≤ quantity) "below minimum quantity"
  let estimatedCost ← add (← mul quantity unitCost) fixed
  let _ ← check (estimatedCost ≤ budget) "rounded cost exceeds allocation"
  let _ ← sub cash.amount estimatedCost
  return ⟨⟨quantity, budget⟩, ⟨hrange.down,
    ⟨allocated.val, allocated.property, roundedBudget.property.1⟩, roundedBudget.property.2⟩⟩

def validateExit (quantity cost : Decimal) (currency accountCurrency : String)
    (costs : Costs) (now : Timestamp) : Checked Decimal := do
  let _ ← check (zero < quantity ∧ zero < cost)
    "cost basis requires positive filled quantity and acquisition cost"
  let _ ← check (costs.fixed.currency = accountCurrency ∧ costs.uncertainty.currency = accountCurrency ∧
      zero ≤ costs.fixed.amount ∧ zero ≤ costs.uncertainty.amount ∧ zero ≤ costs.rate ∧ costs.rate < one)
    "invalid exit charges or account currency"
  fxRate currency accountCurrency costs.fx now

def proceeds (quantity cost price : Decimal) (currency accountCurrency : String)
    (costs : Costs) (now : Timestamp) : Checked Decimal := do
  let _ ← check (zero < price) "liquidation price must be positive"
  let fx ← validateExit quantity cost currency accountCurrency costs now
  let gross ← mul (← mul quantity price) fx
  sub (← mul gross (← sub one costs.rate)) (← add costs.fixed.amount costs.uncertainty.amount)

structure TargetChecks (quantity cost target : Decimal) (currency accountCurrency : String)
    (costs : Costs) (tick : Decimal) (now : Timestamp) (price : Decimal) : Prop where
  positive : zero < price
  grid : onGrid price tick
  net_wealth : ∃ required, (do mul cost (← add one target)) = .ok required ∧
    ∃ finalProceeds, proceeds quantity cost price currency accountCurrency costs now = .ok finalProceeds ∧
      required ≤ finalProceeds

def targetPrice (quantity cost target : Decimal) (currency accountCurrency : String)
    (costs : Costs) (tick : Decimal) (now : Timestamp) :
    Checked {price // TargetChecks quantity cost target currency accountCurrency costs tick now price} := do
  let _ ← check (zero < target ∧ target ≤ D 10 2) "net target must be positive and at most 10%"
  let fx ← validateExit quantity cost currency accountCurrency costs now
  let requiredComputed ← retain (do mul cost (← add one target))
  let required := requiredComputed.val
  let fixed ← add costs.fixed.amount costs.uncertainty.amount
  let unitProceeds ← mul (← mul quantity fx) (← sub one costs.rate)
  let _ ← check (zero < unitProceeds) "exit proceeds precision exhausted"
  let price ← ceilTo (← div (← add required fixed) unitProceeds) tick
  let initialProceeds ← proceeds quantity cost price currency accountCurrency costs now
  let price ← if initialProceeds < required then add price tick else pure price
  let finalComputed ← retain (proceeds quantity cost price currency accountCurrency costs now)
  let finalProceeds := finalComputed.val
  let h ← check (zero < price ∧ onGrid price tick ∧ required ≤ finalProceeds)
    "target price cannot conservatively satisfy net proceeds"
  return ⟨price, ⟨h.down.1, h.down.2.1,
    ⟨required, requiredComputed.property, finalProceeds, finalComputed.property, h.down.2.2⟩⟩⟩

def correctQuantity (quantity step acquisitionUnit entryFixed lossUnit fixedLoss budget cap : Decimal) :
    Nat → Checked Decimal
  | 0 => pure quantity
  | fuel + 1 => do
    let cost ← add (← mul quantity acquisitionUnit) entryFixed
    let loss ← add (← mul quantity lossUnit) fixedLoss
    if cost ≤ budget ∧ loss ≤ cap then return quantity
    correctQuantity (← sub quantity step) step acquisitionUnit entryFixed lossUnit fixedLoss budget cap fuel

-- Facts certified here are the actual production acceptance checks. They do not
-- silently stand in for the still-missing refinement of the Rust implementation.
structure AcceptedChecks (r : Request) (o : Output) : Prop where
  policy : PolicyCaps r.policy
  account_input : validAccountInput r.input.account = true
  quantity_positive : zero < o.quantity
  quantity_minimum : r.input.rules.minimum_quantity ≤ o.quantity
  affordable : o.acquisition_cost.amount ≤ o.budget.amount
  cash_budget : o.budget.amount ≤ r.input.account.spendable_cash.amount
  cash_cost : o.acquisition_cost.amount ≤ r.input.account.spendable_cash.amount
  entry_bounds : r.input.entry_range.minimum ≤ o.buy_limit ∧ o.buy_limit ≤ r.input.entry_range.maximum
  marketable : r.input.quote.ask ≤ o.buy_limit
  allocation : ∃ allocated, mul r.input.account.spendable_cash.amount r.policy.entry_cash_fraction = .ok allocated ∧
    o.budget.amount ≤ allocated
  currency_grid : onGrid o.budget.amount r.input.rules.currency_unit
  risk_positive : zero < o.account_risk.amount
  account_risk_cap : ∃ cap, mul r.input.account.spendable_cash.amount r.max_account_risk_fraction = .ok cap ∧
    o.account_risk.amount ≤ cap
  position_risk_cap : ∃ cap, mul o.acquisition_cost.amount r.policy.max_initial_loss_fraction = .ok cap ∧
    o.account_risk.amount ≤ cap
  planned_risk : zero < o.planned_loss_fraction ∧ o.planned_loss_fraction ≤ r.policy.max_initial_loss_fraction
  loss_definition : sub o.acquisition_cost.amount o.proceeds_at_stop.amount = .ok o.account_risk.amount
  fraction_definition : div o.account_risk.amount o.acquisition_cost.amount = .ok o.planned_loss_fraction
  stop_positive : zero < o.stop_trigger
  stop_before_entry : o.stop_trigger < o.buy_limit
  target_after_entry : o.buy_limit < o.target_sell_limit
  reward_risk : r.policy.min_reward_to_risk ≤ o.reward_to_risk
  target_net_wealth : ∃ required,
    (do mul o.acquisition_cost.amount (← add one o.target_net_fraction)) = .ok required ∧
      required ≤ o.proceeds_at_target.amount
  reward_definition : ∃ reward, sub o.proceeds_at_target.amount o.acquisition_cost.amount = .ok reward ∧
    div reward o.account_risk.amount = .ok o.reward_to_risk ∧
      ∃ requiredReward, mul o.account_risk.amount r.policy.min_reward_to_risk = .ok requiredReward ∧ requiredReward ≤ reward
  grid : onGrid o.buy_limit r.input.rules.price_tick ∧ onGrid o.stop_trigger r.input.rules.price_tick ∧
    onGrid o.target_sell_limit r.input.rules.price_tick ∧ onGrid o.quantity r.input.rules.quantity_step
  target_cap : zero < o.target_net_fraction ∧ o.target_net_fraction ≤ r.policy.max_target_fraction
  holding_cap : 1 ≤ o.max_holding_sessions ∧ o.max_holding_sessions ≤ r.policy.max_holding_sessions
  deadline : o.holding_deadline_at = r.input.holding_deadline_at
  lifetime : r.as_of < o.valid_until
  snapshot_binding : o.account_snapshot_id = r.input.account.snapshot_id ∧
    o.market_snapshot_id = r.input.market_snapshot_id ∧ o.evidence_snapshot_id = r.input.evidence.snapshot_id

def calculate (r : Request) : Checked {o // AcceptedChecks r o} := do
  let p := r.policy
  let v := r.parameters
  let i := r.input
  let e := i.evidence
  let now := r.as_of
  let hv ← validateParameters p r.envelope v
  let hp ← validatePolicy p
  let haccount ← validateAccount i.account
  let _ ← check (zero < r.max_account_risk_fraction ∧ r.max_account_risk_fraction ≤ p.max_initial_loss_fraction)
    "owner account-risk budget is missing or exceeds policy"
  let _ ← check (i.account.snapshot_id.isEmpty = false ∧ i.market_snapshot_id.isEmpty = false ∧
      fresh i.account.observed_at now 60 ∧ now < i.valid_until ∧ i.valid_until ≤ i.holding_deadline_at)
    "missing, stale or expired formula inputs"
  let _ ← check (e.snapshot_id.isEmpty = false ∧ e.family = v.family ∧ e.signal_eligible = true ∧ zero < e.atr ∧
      e.atr_lookback = v.atr_lookback ∧ e.bar_seconds = v.bar_seconds ∧ v.atr_lookback < e.completed_bars ∧
      e.last_completed_at ≤ e.available_at ∧ fresh e.last_completed_at now v.bar_seconds ∧
      e.available_at ≤ now ∧ now < e.valid_until ∧ zero ≤ e.minimum_stop_distance ∧ zero ≤ e.noise_distance)
    "formula requires current, warmed, causal volatility and an eligible signal"
  let ceiling ← targetCeiling e.confidence_score e.exceptional_verified p
  let _ ← check (e.base_confidence_score ≤ 100 ∧ e.confidence_score ≤ e.base_confidence_score ∧
      e.sentiment_entry_allowed = true ∧ zero ≤ e.sentiment_target_ceiling ∧
      e.sentiment_target_ceiling ≤ ceiling ∧ v.target_net_fraction ≤ e.sentiment_target_ceiling)
    "formula target or entry violates the confidence/sentiment gate"
  let buy ← floorTo (minimum i.entry_range.maximum
    (← mul i.quote.ask (← add one v.entry_allowance_fraction))) i.rules.price_tick
  let hmarket ← check (i.quote.ask ≤ buy) "no marketable formula tick inside range"
  let _ ← sizeEntry i.account.spendable_cash i.quote i.entry_range i.entry_costs i.rules now p
  let sized ← sizeEntry i.account.spendable_cash { i.quote with ask := buy } i.entry_range i.entry_costs i.rules now p
  let upper := sized.val
  let stop ← ceilTo (← sub buy (← mul v.stop_atr_multiple e.atr)) i.rules.price_tick
  let stopDistance ← sub i.quote.bid stop
  let hs ← check (zero < stop ∧ stop < buy ∧ e.noise_distance < stopDistance ∧ e.minimum_stop_distance ≤ stopDistance)
    "formula stop is nonpositive or inside spread, noise or venue minimum"
  let cash := i.account.spendable_cash
  let entryFx ← fxRate i.quote.currency cash.currency i.entry_costs.fx now
  let exitFx ← fxRate i.quote.currency cash.currency i.exit_costs.fx now
  let _ ← check (i.exit_costs.fixed.currency = cash.currency ∧ i.exit_costs.uncertainty.currency = cash.currency ∧
      zero ≤ i.exit_costs.fixed.amount ∧ zero ≤ i.exit_costs.uncertainty.amount ∧
      zero ≤ i.exit_costs.rate ∧ i.exit_costs.rate < one) "invalid formula liquidation costs"
  let acquisitionUnit ← mul (← mul buy entryFx) (← add one i.entry_costs.rate)
  let entryFixed ← add i.entry_costs.fixed.amount i.entry_costs.uncertainty.amount
  let exitFixed ← add i.exit_costs.fixed.amount i.exit_costs.uncertainty.amount
  let lossUnit ← sub acquisitionUnit (← mul (← mul stop exitFx) (← sub one i.exit_costs.rate))
  let fixedLoss ← add entryFixed exitFixed
  let cap ← retain (mul cash.amount r.max_account_risk_fraction)
  let _ ← check (zero < lossUnit ∧ fixedLoss < cap.val) "no feasible positive modeled downside"
  let quantity ← floorTo (minimum upper.quantity (← div (← sub cap.val fixedLoss) lossUnit)) i.rules.quantity_step
  let quantity ← correctQuantity quantity i.rules.quantity_step acquisitionUnit entryFixed lossUnit fixedLoss upper.budget cap.val 2
  let hq ← check (zero < quantity ∧ i.rules.minimum_quantity ≤ quantity) "no supported quantity within account risk"
  let next ← add quantity i.rules.quantity_step
  -- Rust || is short circuiting: arithmetic in an unused arm must not reject.
  let maximal ← if upper.quantity < next then pure true else do
    let nextCost ← add (← mul next acquisitionUnit) entryFixed
    if upper.budget < nextCost then pure true else do
      let nextLoss ← add (← mul next lossUnit) fixedLoss
      pure (decide (cap.val < nextLoss))
  let _ ← check (maximal = true) "quantity solver precision cannot certify the largest lot"
  let cost ← add (← mul quantity acquisitionUnit) entryFixed
  let stopProceeds ← proceeds quantity cost stop i.quote.currency cash.currency i.exit_costs now
  let lossComputed ← retain (sub cost stopProceeds)
  let loss := lossComputed.val
  -- The multiplication is evaluated only if the first three checks pass.
  let hfirst ← check (cost ≤ upper.budget ∧ upper.budget ≤ cash.amount ∧ cost ≤ cash.amount ∧
      zero < loss ∧ loss ≤ cap.val)
    "final formula quantity fails allocation or account-risk budget"
  let positionCap ← retain (mul cost p.max_initial_loss_fraction)
  let hposition ← check (loss ≤ positionCap.val) "final formula quantity fails allocation or account-risk budget"
  let fractionComputed ← retain (div loss cost)
  let planned := fractionComputed.val
  let riskCeiling ← targetCeiling e.confidence_score e.exceptional_verified p
  let _ ← check (p.confidence_entry_min ≤ e.confidence_score) "entry confidence below policy floor"
  let _ ← check (zero < v.target_net_fraction ∧ v.target_net_fraction ≤ riskCeiling)
    "target exceeds confidence or policy ceiling"
  let hrisk ← check (zero < planned ∧ planned ≤ p.max_initial_loss_fraction) "invalid initial loss"
  let requiredTarget ← mul planned p.min_reward_to_risk
  let _ ← check (requiredTarget ≤ v.target_net_fraction) "insufficient reward/risk"
  let certifiedTarget ← targetPrice quantity cost v.target_net_fraction i.quote.currency cash.currency i.exit_costs i.rules.price_tick now
  let target := certifiedTarget.val
  let targetComputed ← retain (proceeds quantity cost target i.quote.currency cash.currency i.exit_costs now)
  let targetProceeds := targetComputed.val
  let rewardComputed ← retain (sub targetProceeds cost)
  let reward := rewardComputed.val
  let ratioComputed ← retain (div reward loss)
  let ratio := ratioComputed.val
  let htorder ← check (buy < target) "formula target ordering or reward/risk failed"
  let requiredComputed ← retain (mul loss p.min_reward_to_risk)
  let requiredReward := requiredComputed.val
  let ht ← check (requiredReward ≤ reward ∧ p.min_reward_to_risk ≤ ratio)
    "formula target ordering or reward/risk failed"
  let hg ← check (onGrid buy i.rules.price_tick ∧ onGrid stop i.rules.price_tick ∧
      onGrid target i.rules.price_tick ∧ onGrid quantity i.rules.quantity_step)
    "final formula prices or quantity lost grid precision"
  let mut validUntil := min i.valid_until e.valid_until
  for (observed, age) in [(i.account.observed_at, 60), (i.quote.observed_at, 10),
      (i.quote.received_at, 10), (e.last_completed_at, v.bar_seconds)] do
    let expiry := addSeconds observed age
    let _ ← check (minTimestamp ≤ expiry ∧ expiry ≤ maxOrdinaryTimestamp) "formula validity timestamp overflow"
    validUntil := min validUntil expiry
  if i.quote.currency ≠ cash.currency then
    for fx in [i.entry_costs.fx, i.exit_costs.fx] do
      if let some fx := fx then
        let expiry := addSeconds fx.observed_at 60
        let _ ← check (minTimestamp ≤ expiry ∧ expiry ≤ maxOrdinaryTimestamp) "formula FX validity timestamp overflow"
        validUntil := min validUntil expiry
  let hlifetime ← check (now < validUntil) "no remaining valid formula input lifetime"
  let overshoot ← sub (← div reward cost) v.target_net_fraction
  let output : Output := {
    authority := "FORMULA_V1", account_snapshot_id := i.account.snapshot_id,
    market_snapshot_id := i.market_snapshot_id, evidence_snapshot_id := e.snapshot_id,
    calculated_at := now, valid_until := validUntil, buy_limit := buy, quantity := quantity,
    stop_trigger := stop, target_sell_limit := target, target_net_fraction := v.target_net_fraction,
    target_rounding_overshoot := overshoot, max_holding_sessions := v.max_holding_sessions,
    holding_deadline_at := i.holding_deadline_at, budget := ⟨upper.budget, cash.currency⟩,
    acquisition_cost := ⟨cost, cash.currency⟩, proceeds_at_stop := ⟨stopProceeds, cash.currency⟩,
    proceeds_at_target := ⟨targetProceeds, cash.currency⟩, planned_loss_fraction := planned,
    account_risk := ⟨loss, cash.currency⟩, reward_to_risk := ratio }
  return ⟨output, {
    policy := hp.down, account_input := haccount.down,
    quantity_positive := hq.down.1, quantity_minimum := hq.down.2,
    affordable := hfirst.down.1, entry_bounds := sized.property.entry_bounds,
    cash_budget := hfirst.down.2.1, cash_cost := hfirst.down.2.2.1,
    marketable := hmarket.down, allocation := sized.property.allocation,
    currency_grid := sized.property.currency_grid, risk_positive := hfirst.down.2.2.2.1,
    account_risk_cap := ⟨cap.val, cap.property, hfirst.down.2.2.2.2⟩,
    position_risk_cap := ⟨positionCap.val, positionCap.property, hposition.down⟩,
    planned_risk := hrisk.down, loss_definition := lossComputed.property,
    fraction_definition := fractionComputed.property, stop_positive := hs.down.1, stop_before_entry := hs.down.2.1,
    target_after_entry := htorder.down, reward_risk := ht.down.2,
    target_net_wealth := by
      obtain ⟨required, hrequired, achieved, hachieved, hbound⟩ := certifiedTarget.property.net_wealth
      have heq : achieved = targetProceeds := Except.ok.inj (hachieved.symm.trans targetComputed.property)
      have hfinal : required ≤ targetProceeds := by rw [← heq]; exact hbound
      exact ⟨required, hrequired, hfinal⟩,
    reward_definition := ⟨reward, rewardComputed.property, ratioComputed.property,
      requiredReward, requiredComputed.property, ht.down.1⟩,
    grid := hg.down,
    target_cap := ⟨hv.down.2.2.1, hv.down.2.2.2⟩,
    holding_cap := ⟨hv.down.1, hv.down.2.1⟩,
    deadline := rfl, lifetime := hlifetime.down, snapshot_binding := ⟨rfl, rfl, rfl⟩ }⟩

def evaluate (r : Request) : Checked Output := (calculate r).map Subtype.val

theorem total_calculation (r : Request) :
    (∃ output, evaluate r = .ok output) ∨ (∃ rejection, evaluate r = .error rejection) := by
  cases h : evaluate r with
  | ok output => exact Or.inl ⟨output, rfl⟩
  | error rejection => exact Or.inr ⟨rejection, rfl⟩

theorem accepted_checks (r : Request) (o : Output) (h : evaluate r = .ok o) : AcceptedChecks r o := by
  unfold evaluate at h
  cases hc : calculate r with
  | error error => simp [hc, Except.map] at h
  | ok certified =>
    simp [hc, Except.map] at h
    subst o
    exact certified.property

theorem accepted_account_input (r : Request) (o : Output) (h : evaluate r = .ok o) :
    validAccountInput r.input.account = true := (accepted_checks r o h).account_input

theorem invalid_account_input_cannot_accept (r : Request)
    (h : validAccountInput r.input.account = false) : ¬ ∃ o, evaluate r = .ok o := by
  intro ⟨o, accepted⟩
  have valid := accepted_account_input r o accepted
  rw [h] at valid
  contradiction

theorem accepted_stop_before_target (r : Request) (o : Output) (h : evaluate r = .ok o) :
    o.stop_trigger < o.target_sell_limit := by
  have hs := accepted_checks r o h
  have h₁ := hs.stop_before_entry
  have h₂ := hs.target_after_entry
  change units o.stop_trigger < units o.target_sell_limit
  change units o.stop_trigger < units o.buy_limit at h₁
  change units o.buy_limit < units o.target_sell_limit at h₂
  omega

theorem accepted_target_holding_caps (r : Request) (o : Output) (h : evaluate r = .ok o) :
    o.target_net_fraction ≤ D 10 2 ∧ 1 ≤ o.max_holding_sessions ∧ o.max_holding_sessions ≤ 5 := by
  have hs := accepted_checks r o h
  have ht := hs.target_cap.2
  have hp := hs.policy.target.2
  have hh := hs.holding_cap
  have ph := hs.policy.holding
  change units o.target_net_fraction ≤ units (D 10 2) ∧ _
  change units o.target_net_fraction ≤ units r.policy.max_target_fraction at ht
  change units r.policy.max_target_fraction ≤ units (D 10 2) at hp
  omega

end TradyProof.Formula
