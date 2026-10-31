import TradyProof

open Lean
open TradyProof.CheckedDecimal

private def rejected (error : String) : Json :=
  Json.mkObj [("status", toJson ("REJECTED" : String)), ("error", toJson error)]

private def executeRequest (json : Json) : Except String Json := do
  let kind ← json.getObjValAs? String "kind"
  if kind = "FORMULA" then
    let request ← fromJson? (α := TradyProof.Formula.Request) json
    match TradyProof.Formula.evaluate request with
    | .ok limits => return Json.mkObj [("status", toJson ("ACCEPTED" : String)), ("limits", toJson limits)]
    | .error message => return rejected message
  else if kind = "DECIMAL" then
    let operation ← json.getObjValAs? String "operation"
    let a ← json.getObjValAs? Decimal "a"
    let b ← json.getObjValAs? Decimal "b"
    let result ← match operation with
      | "ADD" => pure (add a b)
      | "SUB" => pure (sub a b)
      | "MUL" => pure (mul a b)
      | "DIV" => pure (div a b)
      | "REM" => pure (rem a b)
      | "FLOOR" => pure (floorTo a b)
      | "CEIL" => pure (ceilTo a b)
      | _ => throw "unknown checked operation"
    match result with
    | .ok value => return Json.mkObj [("status", toJson ("DECIMAL" : String)), ("value", toJson value)]
    | .error message => return rejected message
  else throw "unknown request kind"

def main : IO UInt32 := do
  let input ← (← IO.getStdin).getLine
  let result := do executeRequest (← Json.parse input)
  match result with
  | .ok output => IO.println output.compress; return 0
  | .error message => IO.eprintln message; return 2
