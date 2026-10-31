import TradyProof.Formula

set_option maxRecDepth 10000
set_option maxHeartbeats 0

namespace TradyProof.Formula
open CheckedDecimal

def witnessRequest : Request :=
  let now : Timestamp := 3586737600000000000
  let money := fun amount => Money.mk amount "EUR"
  let costs : Costs := ⟨money (D 1 1), D 1 3, money (D 1 1), none⟩
  { policy := ⟨1, D 98 2, 90, D 10 2, false, D 2 2, D 15 1, 5, false, false, false⟩
    envelope := ⟨D 5 3, D 1 0, D 4 0, D 1 2, D 3 2⟩
    parameters := ⟨1, "TREND", D 1 3, D 2 0, D 3 2, 3, 14, 60⟩
    max_account_risk_fraction := D 1 2
    as_of := now
    input := {
      account := ⟨"account_1", "paper_limits_test", "paper", money (D 1000 0), [], [], now⟩
      market_snapshot_id := "market_1"
      quote := ⟨"test_stock", "EUR", D 9999 2, D 100 0, now, now, true⟩
      entry_range := ⟨D 99 0, D 101 0, "EUR"⟩
      entry_costs := costs, exit_costs := costs
      rules := ⟨D 1 2, D 1 2, D 1 2, D 1 2⟩
      evidence := ⟨"bars_1", "TREND", true, D 5 1, 14, 60, 30,
        now - 45 * second, now - 30 * second, now + 300 * second,
        D 1 2, D 1 1, 100, 100, true, D 3 2, false⟩
      holding_deadline_at := now + 259200 * second
      valid_until := now + 300 * second } }

def isAccepted (result : Checked Output) : Bool :=
  match result with | .ok _ => true | .error _ => false

-- Kernel evaluation of the complete, concrete model calculation: no caller
-- witness flag, mock result, native_decide, or external executable is trusted.
theorem witness_accepted : isAccepted (evaluate witnessRequest) = true := by decide

theorem model_non_vacuous : ∃ output, evaluate witnessRequest = .ok output := by
  have h := witness_accepted
  cases heval : evaluate witnessRequest with
  | error message => simp [heval, isAccepted] at h
  | ok output => exact ⟨output, rfl⟩

def fxWitnessRequest : Request :=
  let original := witnessRequest
  let fx : Fx := ⟨"USD", "EUR", D 9 1, original.as_of⟩
  let input : Input := { original.input with
    quote := { original.input.quote with currency := "USD" }
    entry_range := { original.input.entry_range with currency := "USD" }
    entry_costs := { original.input.entry_costs with fx := some fx }
    exit_costs := { original.input.exit_costs with fx := some fx } }
  { original with input := input }

theorem fx_witness_accepted : isAccepted (evaluate fxWitnessRequest) = true := by decide

theorem fx_model_non_vacuous : ∃ output, evaluate fxWitnessRequest = .ok output := by
  have h := fx_witness_accepted
  cases heval : evaluate fxWitnessRequest with
  | error message => simp [heval, isAccepted] at h
  | ok output => exact ⟨output, rfl⟩

end TradyProof.Formula
