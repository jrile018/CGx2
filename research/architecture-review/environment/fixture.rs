fn apply(buf: &mut [i32], budget: usize) -> Result<(), &'static str> {
    if buf.len() * 4 > budget { return Err("budget_exhausted"); }
    let staged: Vec<i32> = buf.iter().map(|x| x * 2 + 1).collect();
    buf.copy_from_slice(&staged); Ok(())
}
fn main() {
    let mut buf = vec![1, -2, 3, 4];
    let before = buf.clone();
    assert_eq!(apply(&mut buf, 15), Err("budget_exhausted"));
    assert_eq!(buf, before);
    apply(&mut buf, 16).unwrap();
    let shared: &[i32] = &buf;
    assert_eq!(shared, &[3,-3,7,9]);
    assert_eq!(shared.iter().sum::<i32>(), 16);
    println!("PASS values=[3,-3,7,9] sum=16 rejected_budget=15 accepted_budget=16 failure_unchanged=true");
}
