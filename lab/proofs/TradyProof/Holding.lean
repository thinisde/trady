import TradyProof.Formula

namespace TradyProof.Holding
open CheckedDecimal Formula

theorem maximum_preserves_left (old candidate : Decimal) : old ≤ maximum old candidate := by
  unfold maximum
  split
  · assumption
  · exact Int.le_refl _

structure Trailing where
  peak : Option Decimal
  floor : Option Decimal
  deriving Repr

def DoesNotDecrease (old next : Option Decimal) : Prop :=
  ∀ value, old = some value → ∃ updated, next = some updated ∧ value ≤ updated

def validTrailing (old : Trailing) : Bool :=
  match old.floor, old.peak with
  | none, _ => true
  | some _, none => false
  | some floor, some peak => decide (floor ≤ peak)

def raiseFloor (old : Option Decimal) (candidate : Decimal) :
    {next // DoesNotDecrease old next} :=
  match old with
  | none => ⟨some candidate, by intro value h; contradiction⟩
  | some previous => ⟨some (maximum previous candidate), by
      intro value h
      cases h
      exact ⟨maximum previous candidate, rfl, maximum_preserves_left previous candidate⟩⟩

-- Exactly the max/update arithmetic in TrailingState::observe. Returns no new
-- state if checked subtraction fails. Values must refer to comparable net
-- return definitions, as they do in evaluate_holding's fixed cost-basis state.
def observeTrailing (old : Trailing) (value activation distance : Decimal) :
    Checked {next : Trailing // DoesNotDecrease old.peak next.peak ∧ DoesNotDecrease old.floor next.floor} := do
  let _ ← check (validTrailing old = true)
    "trailing floor requires a known peak at least as high"
  let peak := match old.peak with | none => value | some previous => maximum previous value
  have hpeak : DoesNotDecrease old.peak (some peak) := by
    intro previous h
    cases ho : old.peak with
    | none => simp [ho] at h
    | some prior =>
      simp [ho] at h
      subst previous
      exact ⟨maximum prior value, by simp [peak, ho], maximum_preserves_left prior value⟩
  if activation ≤ peak then
    let candidate ← sub peak distance
    let floor := raiseFloor old.floor candidate
    return ⟨⟨some peak, floor.val⟩, hpeak, floor.property⟩
  else
    return ⟨⟨some peak, old.floor⟩, hpeak, by intro previous h; exact ⟨previous, h, Int.le_refl _⟩⟩

theorem trailing_floor_monotone (old : Trailing) (value activation distance : Decimal)
    (next : {next : Trailing // DoesNotDecrease old.peak next.peak ∧ DoesNotDecrease old.floor next.floor})
    (_ : observeTrailing old value activation distance = .ok next) :
    DoesNotDecrease old.floor next.val.floor := next.property.2

structure Latch where
  reason : String
  plan_version : Nat
  latched_at : Timestamp
  quote_observed_at : Option Timestamp
  deriving Repr

-- evaluate_holding returns the old state immediately when the old latch is
-- present, before observations, outages, or proposed plans can be processed.
-- With no old latch it may set a new one. The theorem is uniform over every
-- possible newly computed reason, not an assumption about external prices.
def latchExit (old : Option Latch) (reason : Option String) (version : Nat)
    (now : Timestamp) (quote : Option Timestamp) : Option Latch :=
  match old with
  | some latch => some latch
  | none => reason.map (fun reason => ⟨reason, version, now, quote⟩)

theorem latched_exit_preserved (old : Latch) (reason : Option String) (version : Nat)
    (now : Timestamp) (quote : Option Timestamp) :
    latchExit (some old) reason version now quote = some old := rfl

structure Plan where
  version : Nat
  stop_price : Decimal
  target_net_fraction : Decimal
  deadline : Timestamp
  activation_net_fraction : Decimal
  distance_fraction : Decimal
  deriving Repr

structure Update where
  active : Plan
  proposed : Plan
  expected_version : Nat
  original_deadline : Timestamp
  price_tick : Decimal
  confidence_score : Nat
  exceptional_verified : Bool
  target_increase_allowed : Bool
  order_outcome_known : Bool
  research_available : Bool
  policy : Policy
  deriving Repr

def targetCeiling (score : Nat) (active : Decimal) (exceptional : Bool) (p : Policy) :
    Checked {ceiling // score < 70 → ceiling ≤ active} := do
  let _ ← check (zero < active ∧ active ≤ D 10 2) "invalid active holding target"
  let band ← Formula.targetCeiling score exceptional p
  if hl : score < 70 then
    return ⟨minimum active p.max_target_fraction, by
      intro _
      unfold minimum
      split
      · exact Int.le_refl _
      · change units p.max_target_fraction ≤ units active
        rename_i hn
        change ¬ units active ≤ units p.max_target_fraction at hn
        omega⟩
  else return ⟨band, by intro h; exact False.elim (hl h)⟩

structure Monotone (r : Update) (next : Plan) : Prop where
  version : next.version = r.active.version + 1
  stop : r.active.stop_price ≤ next.stop_price
  deadline : next.deadline ≤ r.active.deadline ∧ next.deadline ≤ r.original_deadline
  activation : next.activation_net_fraction ≤ r.active.activation_net_fraction
  distance : next.distance_fraction ≤ r.active.distance_fraction
  target_cap : zero < next.target_net_fraction ∧ next.target_net_fraction ≤ D 10 2
  low_confidence : r.confidence_score < 70 → next.target_net_fraction ≤ r.active.target_net_fraction

-- This models checked_update's numeric/version boundary. activate_plan's old
-- observation/latch evaluation and persistence are deliberately not claimed.
def checkedUpdate (r : Update) : Checked {next // Monotone r next} := do
  let _ ← check (r.order_outcome_known = true) "unknown order outcome blocks plan activation"
  let _ ← check (r.research_available = true) "research unavailable; retain active thresholds"
  let _ ← check (r.expected_version = r.active.version) "stale expected plan version"
  let _ ← check (r.active.version < 18446744073709551615) "plan version overflow"
  let stop ← ceilTo r.proposed.stop_price r.price_tick
  let proposed : Plan := { r.proposed with stop_price := stop }
  let hp ← check (0 < proposed.version ∧ zero < proposed.stop_price ∧
      zero < proposed.target_net_fraction ∧ proposed.target_net_fraction ≤ D 10 2)
    "invalid holding plan version, stop or target"
  let _ ← check (zero ≤ proposed.activation_net_fraction ∧ zero < proposed.distance_fraction)
    "invalid trailing activation or distance"
  let hv ← check (proposed.version = r.active.version + 1) "plan version must advance exactly once"
  let hs ← check (r.active.stop_price ≤ proposed.stop_price) "long stop cannot move downward"
  let hd ← check (proposed.deadline ≤ r.active.deadline ∧ proposed.deadline ≤ r.original_deadline)
    "holding deadline cannot extend"
  let ht ← check (proposed.activation_net_fraction ≤ r.active.activation_net_fraction ∧
      proposed.distance_fraction ≤ r.active.distance_fraction)
    "trailing activation/distance cannot loosen"
  let ceiling ← targetCeiling r.confidence_score r.active.target_net_fraction r.exceptional_verified r.policy
  let hc ← check (proposed.target_net_fraction ≤ ceiling.val) "holding target exceeds permitted ceiling"
  if r.active.target_net_fraction < proposed.target_net_fraction then
    let _ ← check (r.target_increase_allowed = true ∧ 70 ≤ r.confidence_score)
      "target increase lacks current checked evidence/permissions"
    pure ()
  return ⟨proposed, {
    version := hv.down, stop := hs.down, deadline := hd.down,
    activation := ht.down.1, distance := ht.down.2, target_cap := hp.down.2.2,
    low_confidence := by
      intro hl
      have ha := hc.down
      have hb := ceiling.property hl
      change units proposed.target_net_fraction ≤ units ceiling.val at ha
      change units ceiling.val ≤ units r.active.target_net_fraction at hb
      change units proposed.target_net_fraction ≤ units r.active.target_net_fraction
      omega }⟩

def evaluate (r : Update) : Checked Plan := (checkedUpdate r).map Subtype.val

theorem accepted_update_monotone (r : Update) (next : Plan) (h : evaluate r = .ok next) : Monotone r next := by
  unfold evaluate at h
  cases hc : checkedUpdate r with
  | error error => simp [hc, Except.map] at h
  | ok certified =>
    simp [hc, Except.map] at h
    subst next
    exact certified.property

end TradyProof.Holding
