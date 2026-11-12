import Pathos

open Pathos

/-- Read requests from standard input and write one reply per line. -/
partial def loop (input output : IO.FS.Stream) (history : History) : IO UInt32 := do
  let line ← input.getLine
  if line.isEmpty then return 0
  match Request.parse line.trimAscii.toString with
  | none =>
    IO.eprintln s!"unrecognized request: {line.trimAscii}"
    return 1
  | some request =>
    let (next, reply) := history.respond request
    output.putStrLn reply
    loop input output next

def main : IO UInt32 := do
  loop (← IO.getStdin) (← IO.getStdout) History.initial
