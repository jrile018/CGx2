#[inline(never)]
fn leak() { let data = Box::new([42u8; 4096]);
    std::hint::black_box(data.as_ptr()); std::mem::forget(data); }
fn main() { leak(); }
