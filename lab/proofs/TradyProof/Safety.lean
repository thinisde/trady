import Std

namespace TradyProof

-- This model covers integer tick rounding and caps. It does not model a market,
-- fills, Rust Decimal or the complete FORMULA_V1 calculator.
def floorTicks (numerator denominator : Nat) : Nat := numerator / denominator
def ceilTicks (numerator denominator : Nat) : Nat :=
  if numerator % denominator = 0 then numerator / denominator
  else numerator / denominator + 1
def cap (quantity maximum : Nat) : Nat := min quantity maximum

theorem cap_le (quantity maximum : Nat) : cap quantity maximum ≤ maximum := by
  unfold cap
  omega

theorem cap_nonzero (quantity maximum : Nat) (hq : 0 < quantity)
    (hm : 0 < maximum) : 0 < cap quantity maximum := by
  unfold cap
  omega

theorem floor_le_ceil (numerator denominator : Nat) :
    floorTicks numerator denominator ≤ ceilTicks numerator denominator := by
  unfold floorTicks ceilTicks
  split <;> omega

theorem stop_before_target (stop entry target : Nat)
    (hs : stop < entry) (ht : entry < target) : stop < target := by
  omega

theorem non_vacuous : ∃ stop entry target : Nat, stop < entry ∧ entry < target := by
  exact ⟨100, 200, 300, by decide⟩

end TradyProof
