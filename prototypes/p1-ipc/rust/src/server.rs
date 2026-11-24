// Emits synthetic token-delta events over a Unix socket. argv: <socket> <format> <events> <batch>
// format: ndjson | ljson (u32 LE length + JSON array of `batch` events) | bin (u32 LE length + fixed binary)
// After the event run it echoes each 1-byte message back, for round-trip latency.
use std::{env, io::{BufWriter, Read, Write}, os::unix::net::UnixListener};
fn json(out: &mut Vec<u8>, i: u64) {
    write!(out, r#"{{"session":"s{:04}","seq":{},"kind":"text.delta","item":"i{}","text":"the next \"token\" "}}"#, i % 100, i, i / 400).unwrap();
}
fn bin(out: &mut Vec<u8>, i: u64) {
    let t = b"the next \"token\" ";
    out.extend((i as u32 % 100).to_le_bytes()); out.extend(i.to_le_bytes());
    out.extend(((i / 400) as u32).to_le_bytes()); out.extend((t.len() as u32).to_le_bytes()); out.extend(t);
}
fn main() {
    let a: Vec<String> = env::args().collect();
    let (fmt, n, batch): (&str, u64, u64) = (&a[2], a[3].parse().unwrap(), a[4].parse().unwrap());
    let _ = std::fs::remove_file(&a[1]);
    let l = UnixListener::bind(&a[1]).unwrap();
    println!("ready"); 
    let (mut s, _) = l.accept().unwrap();
    let mut w = BufWriter::with_capacity(1 << 16, s.try_clone().unwrap());
    let mut i = 0; let mut b = Vec::with_capacity(1 << 16);
    while i < n {
        b.clear();
        let end = (i + batch).min(n);
        match fmt {
            "ndjson" => while i < end { json(&mut b, i); b.push(b'\n'); i += 1 },
            "ljson" => { b.push(b'['); while i < end { json(&mut b, i); i += 1; if i < end { b.push(b',') } } b.push(b']');
                w.write_all(&(b.len() as u32).to_le_bytes()).unwrap(); }
            _ => { while i < end { bin(&mut b, i); i += 1 } w.write_all(&(b.len() as u32).to_le_bytes()).unwrap(); }
        }
        w.write_all(&b).unwrap();
    }
    w.flush().unwrap();
    let mut one = [0u8; 1];
    while s.read_exact(&mut one).is_ok() { s.write_all(&one).unwrap(); }
}
