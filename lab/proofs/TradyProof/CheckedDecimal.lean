import Lean
import Std

namespace TradyProof.CheckedDecimal

-- Mathematical specification of a signed 96-bit coefficient and a scale in
-- [0,28]. This is a specification, not a proved refinement of rust_decimal's
-- limb algorithms. All intermediate integers are unbounded; packing is checked.
def maxCoefficient : Nat := 79228162514264337593543950335

structure Decimal where
  coefficient : Int
  scale : Nat
  coefficient_bound : coefficient.natAbs ≤ maxCoefficient
  scale_bound : scale ≤ 28
  deriving Repr, DecidableEq

def D (coefficient : Int) (scale : Nat)
    (hc : coefficient.natAbs ≤ maxCoefficient := by decide)
    (hs : scale ≤ 28 := by decide) : Decimal :=
  ⟨coefficient, scale, hc, hs⟩

def zero : Decimal := D 0 0
def one : Decimal := D 1 0

def pack (coefficient : Int) (scale : Nat) : Option Decimal :=
  if hc : coefficient.natAbs ≤ maxCoefficient then
    if hs : scale ≤ 28 then some ⟨coefficient, scale, hc, hs⟩ else none
  else none

def units (a : Decimal) : Int := a.coefficient * (10 : Int) ^ (28 - a.scale)
instance : LE Decimal := ⟨fun a b => units a ≤ units b⟩
instance : LT Decimal := ⟨fun a b => units a < units b⟩
instance (a b : Decimal) : Decidable (a ≤ b) := inferInstanceAs (Decidable (units a ≤ units b))
instance (a b : Decimal) : Decidable (a < b) := inferInstanceAs (Decidable (units a < units b))
def equal (a b : Decimal) : Bool := units a == units b
def minimum (a b : Decimal) : Decimal := if a ≤ b then a else b
def maximum (a b : Decimal) : Decimal := if a ≤ b then b else a

def roundNat (numerator denominator : Nat) : Nat :=
  let q := numerator / denominator
  let r := numerator % denominator
  if 2 * r > denominator ∨ (2 * r = denominator ∧ q % 2 = 1) then q + 1 else q

def roundSigned (numerator denominator : Int) : Int :=
  let magnitude := Int.ofNat (roundNat numerator.natAbs denominator.natAbs)
  if (decide (numerator < 0)) != (decide (denominator < 0)) then -magnitude else magnitude

-- Each retry rounds the original coefficient, avoiding accidental double
-- rounding. Overflow at scale zero rejects rather than saturating or wrapping.
def fitLoop (coefficient : Int) (scale drop : Nat) : Nat → Option Decimal
  | 0 => none
  | fuel + 1 =>
    if drop ≤ scale then
      match pack (roundSigned coefficient ((10 : Int) ^ drop)) (scale - drop) with
      | some result => some result
      | none => fitLoop coefficient scale (drop + 1) fuel
    else none

def fit (coefficient : Int) (scale : Nat) : Option Decimal :=
  fitLoop coefficient scale (scale - 28) (scale + 1)

def align (a : Decimal) (scale : Nat) : Int := a.coefficient * (10 : Int) ^ (scale - a.scale)

def add? (a b : Decimal) : Option Decimal :=
  let scale := max a.scale b.scale
  fit (align a scale + align b scale) scale

def sub? (a b : Decimal) : Option Decimal :=
  let scale := max a.scale b.scale
  fit (align a scale - align b scale) scale

def mul? (a b : Decimal) : Option Decimal :=
  fit (a.coefficient * b.coefficient) (a.scale + b.scale)

def divLoop (numerator denominator : Int) : Nat → Option Decimal
  | 0 => pack (roundSigned numerator denominator) 0
  | scale + 1 =>
    match pack (roundSigned (numerator * (10 : Int) ^ (scale + 1)) denominator) (scale + 1) with
    | some result => some result
    | none => divLoop numerator denominator scale

def div? (a b : Decimal) : Option Decimal :=
  if b.coefficient = 0 then none
  else divLoop (a.coefficient * (10 : Int) ^ b.scale)
    (b.coefficient * (10 : Int) ^ a.scale) 28

def rem? (a b : Decimal) : Option Decimal :=
  if b.coefficient = 0 then none
  else
    let scale := max a.scale b.scale
    let r := (align a scale).natAbs % (align b scale).natAbs
    pack (if a.coefficient < 0 then -(Int.ofNat r) else Int.ofNat r) scale

abbrev Checked := Except String
def checked (message : String) (value : Option Decimal) : Checked Decimal :=
  match value with | some result => .ok result | none => .error message
def add (a b : Decimal) : Checked Decimal := checked "decimal addition overflow" (add? a b)
def sub (a b : Decimal) : Checked Decimal := checked "decimal subtraction overflow" (sub? a b)
def mul (a b : Decimal) : Checked Decimal := checked "decimal multiplication overflow" (mul? a b)
def div (a b : Decimal) : Checked Decimal :=
  checked "decimal division overflow or zero denominator" (div? a b)
def rem (a b : Decimal) : Checked Decimal := checked "decimal remainder overflow" (rem? a b)

def check (p : Prop) [Decidable p] (message : String) : Checked (PLift p) :=
  if h : p then .ok ⟨h⟩ else .error message

def onGrid (value step : Decimal) : Prop := ∃ r, rem? value step = some r ∧ units r = 0
instance (value step : Decimal) : Decidable (onGrid value step) :=
  match h : rem? value step with
  | none => isFalse (by intro ⟨r, hr, _⟩; simp [h] at hr)
  | some r => if hz : units r = 0 then isTrue ⟨r, h, hz⟩ else
      isFalse (by intro ⟨r', hr, hrz⟩; simp [h] at hr; subst r'; exact hz hrz)

def floorCertified (value step : Decimal) : Checked {result // result ≤ value ∧ onGrid result step} := do
  let _ ← check (zero < step ∧ zero ≤ value) "invalid rounding step or value"
  let remainder ← rem value step
  let rounded ← sub value remainder
  let h ← check (rounded ≤ value ∧ onGrid rounded step) "rounding precision exhausted"
  return ⟨rounded, h.down⟩

def floorTo (value step : Decimal) : Checked Decimal := (floorCertified value step).map Subtype.val

def ceilTo (value step : Decimal) : Checked Decimal := do
  let floor ← floorTo value step
  if equal floor value then return value
  let rounded ← add floor step
  let _ ← check (value ≤ rounded ∧ floor < rounded ∧ onGrid rounded step)
    "rounding precision exhausted"
  return rounded

def parse (text : String) : Option Decimal := do
  let negative := text.startsWith "-"
  let text := if negative then text.drop 1 else text
  let parts := text.splitOn "."
  let (digits, scale) ← match parts with
    | [whole] => some (whole, 0)
    | [whole, fraction] => if fraction.isEmpty then none else some (whole ++ fraction, fraction.length)
    | _ => none
  if digits.isEmpty ∨ !(digits.toList.all Char.isDigit) then none else do
    let magnitude ← digits.toNat?
    pack (if negative then -(Int.ofNat magnitude) else Int.ofNat magnitude) scale

-- Rendering uses exactly 28 places, then strips insignificant zeroes. It never
-- converts through a binary float and works for all representable magnitudes.
def render (a : Decimal) : String := Id.run do
  let raw := toString (units a).natAbs
  let raw := String.mk (List.replicate (29 - raw.length) '0') ++ raw
  let whole := raw.take (raw.length - 28)
  let fraction := (raw.drop (raw.length - 28)).toList.reverse.dropWhile (· == '0') |>.reverse
  let sign := if units a < 0 then "-" else ""
  return sign ++ whole ++ (if fraction.isEmpty then "" else "." ++ String.mk fraction)

instance : Lean.FromJson Decimal where
  fromJson? json := do
    let text ← json.getStr?
    match parse text with | some result => .ok result | none => .error "invalid checked decimal"
instance : Lean.ToJson Decimal where
  toJson a := Lean.Json.str (render a)

theorem representable (a : Decimal) :
    a.coefficient.natAbs ≤ maxCoefficient ∧ a.scale ≤ 28 :=
  ⟨a.coefficient_bound, a.scale_bound⟩

theorem floor_checked (value step result : Decimal) (h : floorTo value step = .ok result) :
    result ≤ value ∧ onGrid result step := by
  unfold floorTo at h
  cases hc : floorCertified value step with
  | error error => simp [hc, Except.map] at h
  | ok certified =>
    simp [hc, Except.map] at h
    subst result
    exact certified.property

theorem division_by_zero_rejects (a : Decimal) : div? a zero = none := by
  simp [div?, zero, D]

theorem ties_to_even (n d : Nat) (h : 2 * (n % d) = d) (he : (n / d) % 2 = 0) :
    roundNat n d = n / d := by
  simp only [roundNat]
  split <;> omega

theorem ties_from_odd (n d : Nat) (h : 2 * (n % d) = d) (he : (n / d) % 2 = 1) :
    roundNat n d = n / d + 1 := by
  simp [roundNat, h, he]

end TradyProof.CheckedDecimal
