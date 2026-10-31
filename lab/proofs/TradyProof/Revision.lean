import TradyProof.Formula

namespace TradyProof.Revision
open CheckedDecimal Formula

-- Trusted preparation computes these digests from actual files/JSON. The
-- reference verifier compares exact identities and exact typed parameters;
-- no collision-resistance or provenance property is assumed as a Lean axiom.
structure Identity where
  candidate_id : String
  source_commit : String
  rust_build : String
  proof_sources : String
  theorem_bindings : String
  toolchain : String
  deriving DecidableEq, Repr

structure Pin where
  identity : Identity
  policy : Policy
  envelope : Envelope
  parameters : Parameters
  account_risk_fraction : Decimal
  deriving Repr

structure Bound where
  identity : Identity
  policy : Policy
  envelope : Envelope
  parameters : Parameters
  original_input : Input
  limits : Output

structure BoundTo (expected : Pin) (request : Request) (bound : Bound) : Prop where
  identity : bound.identity = expected.identity
  policy : bound.policy = expected.policy
  envelope : bound.envelope = expected.envelope
  parameters : bound.parameters = expected.parameters
  input : bound.original_input = request.input
  safety : AcceptedChecks request bound.limits

-- This is the content comparison performed by the pinned offline helper, before
-- emitting revision-bound proof/parity evidence. It is not an order authority.
def verifyCandidate (expected : Pin) (supplied : Identity) (request : Request) :
    Checked {bound // BoundTo expected request bound} := do
  let hi ← check (supplied = expected.identity) "stale or mismatched proof/source identity"
  let hp ← check (request.policy = expected.policy ∧ request.envelope = expected.envelope ∧
      request.parameters = expected.parameters ∧ equal request.max_account_risk_fraction expected.account_risk_fraction = true)
    "stale or mismatched formula/policy/parameter binding"
  let certified ← calculate request
  let bound : Bound := ⟨supplied, request.policy, request.envelope, request.parameters, request.input, certified.val⟩
  return ⟨bound, ⟨hi.down, hp.down.1, hp.down.2.1, hp.down.2.2.1, rfl, certified.property⟩⟩

theorem accepted_revision_bound (expected : Pin) (supplied : Identity) (request : Request)
    (bound : {bound // BoundTo expected request bound})
    (_ : verifyCandidate expected supplied request = .ok bound) : BoundTo expected request bound.val := bound.property

theorem mismatched_source_rejects (expected : Pin) (supplied : Identity) (request : Request)
    (h : supplied.rust_build ≠ expected.identity.rust_build) :
    verifyCandidate expected supplied request = .error "stale or mismatched proof/source identity" := by
  have hn : supplied ≠ expected.identity := by
    intro heq
    exact h (congrArg Identity.rust_build heq)
  simp [verifyCandidate, check, hn, Except.bind]
  rfl

end TradyProof.Revision
