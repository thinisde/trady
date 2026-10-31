import Lake
open Lake DSL

package tradyProof
lean_lib TradyProof
@[default_target]
lean_exe tradyParity where
  root := `Main
@[default_target]
lean_exe formulaParity where
  root := `FormulaMain
