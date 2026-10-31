import TradyProof

-- Fixed decimal natural arguments keep the bridge total and avoid JSON parser
-- ambiguity. The verifier compares this output with the Rust arithmetic bridge.
def main (args : List String) : IO UInt32 := do
  match args with
  | [n, d, q, m] =>
    match n.toNat?, d.toNat?, q.toNat?, m.toNat? with
    | some numerator, some denominator, some quantity, some maximum =>
      if denominator == 0 then
        IO.eprintln "positive denominator required"
        return 2
      IO.println ("{\"floor\":" ++ toString (TradyProof.floorTicks numerator denominator) ++
        ",\"ceil\":" ++ toString (TradyProof.ceilTicks numerator denominator) ++
        ",\"cap\":" ++ toString (TradyProof.cap quantity maximum) ++ "}")
      return 0
    | _, _, _, _ =>
      IO.eprintln "natural integer arguments required"
      return 2
  | _ =>
    IO.eprintln "four natural integer arguments required"
    return 2
