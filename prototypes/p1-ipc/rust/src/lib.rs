// In-process alternative: a Rust thread posts JSON strings to a Dart port with Dart_PostCObject.
use std::{ffi::{c_char, c_void, CString}, fmt::Write};
#[repr(C)] pub struct CObj { ty: i32, v: [usize; 5] }
type Post = extern "C" fn(i64, *mut CObj) -> bool;
#[unsafe(no_mangle)] pub extern "C" fn p1_run(post: *const c_void, port: i64, n: u64, batch: u64) {
    let post: Post = unsafe { std::mem::transmute(post) };
    std::thread::spawn(move || {
        let mut i = 0;
        while i < n {
            let mut s = String::with_capacity(8192); s.push('[');
            let end = (i + batch).min(n);
            while i < end {
                write!(s, r#"{{"session":"s{:04}","seq":{},"kind":"text.delta","item":"i{}","text":"the next \"token\" "}}"#, i % 100, i, i / 400).unwrap();
                i += 1; if i < end { s.push(',') }
            }
            s.push(']');
            let c = CString::new(s).unwrap();
            let mut o = CObj { ty: 5, v: [0; 5] }; o.v[0] = c.as_ptr() as *const c_char as usize;
            post(port, &mut o);
        }
    });
}
