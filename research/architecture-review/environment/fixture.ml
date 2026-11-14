let apply buf budget =
  if Array.length buf * 4 > budget then Error "budget_exhausted"
  else (let staged = Array.map (fun x -> x * 2 + 1) buf in
        Array.blit staged 0 buf 0 (Array.length buf); Ok ())
let () =
  let buf = [|1; -2; 3; 4|] in
  let alias = buf in
  let before = Array.copy buf in
  assert (apply buf 15 = Error "budget_exhausted");
  assert (buf = before);
  assert (apply buf 16 = Ok ());
  assert (alias = [|3; -3; 7; 9|]);
  assert (Array.fold_left (+) 0 alias = 16);
  print_endline "PASS values=[3,-3,7,9] sum=16 rejected_budget=15 accepted_budget=16 failure_unchanged=true"
