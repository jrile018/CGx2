use std::ffi::CString;
use std::os::raw::{c_char, c_int};

unsafe extern "C" {
    fn openat(dirfd: c_int, pathname: *const c_char, flags: c_int) -> c_int;
}

fn main() {
    let path = CString::new("{PRIVATE}").unwrap();
    let fd = unsafe { openat(-100, path.as_ptr(), 0) };
    if fd >= 0 { println!("escaped"); } else { println!("denied"); }
}
