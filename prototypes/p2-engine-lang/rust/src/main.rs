use std::{env, io::Write, sync::atomic::{AtomicU64, Ordering::Relaxed}, time::Instant};
fn main() {
    let a: Vec<String> = env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("idle") => { println!("ready"); std::thread::sleep(std::time::Duration::from_secs(3)); }
        Some("search") => {
            let t = Instant::now();
            let m = grep_regex::RegexMatcher::new(&a[3]).unwrap();
            let (files, hits) = (AtomicU64::new(0), AtomicU64::new(0));
            ignore::WalkBuilder::new(&a[2]).threads(8).build_parallel().run(|| {
                let m = m.clone(); let (files, hits) = (&files, &hits);
                let mut s = grep_searcher::SearcherBuilder::new().line_number(true).binary_detection(grep_searcher::BinaryDetection::quit(0)).build();
                Box::new(move |e| {
                    if let Ok(e) = e && e.file_type().is_some_and(|t| t.is_file()) {
                        files.fetch_add(1, Relaxed);
                        let _ = s.search_path(&m, e.path(), grep_searcher::sinks::Lossy(|_, _| { hits.fetch_add(1, Relaxed); Ok(true) }));
                    }
                    ignore::WalkState::Continue
                })
            });
            println!("rust search: {} files, {} hits, {} ms", files.into_inner(), hits.into_inner(), t.elapsed().as_millis());
        }
        Some("journal") => journal(),
        _ => {}
    }
}
fn pct(mut v: Vec<u128>) -> String { v.sort(); let n = v.len(); format!("p50 {:.1} us  p99 {:.1} us  max {:.1} us", v[n/2] as f64/1e3, v[n*99/100] as f64/1e3, v[n-1] as f64/1e3) }
fn journal() {
    let ev = r#"{"session":"s0001","seq":1,"kind":"text.delta","item":"i1","text":"the next \"token\" "}"#;
    let d = env::temp_dir().join("p2j"); let _ = std::fs::remove_dir_all(&d); std::fs::create_dir_all(&d).unwrap();
    for (mode, batch) in [("NORMAL", 1), ("NORMAL", 20), ("FULL", 20)] {
        let p = d.join(format!("{mode}{batch}.db")); let mut c = rusqlite::Connection::open(&p).unwrap();
        c.pragma_update(None, "journal_mode", "WAL").unwrap(); c.pragma_update(None, "synchronous", mode).unwrap();
        c.execute_batch("create table ev(session text, seq integer, body text, primary key(session, seq)) without rowid").unwrap();
        let mut v = vec![]; let mut i = 0i64;
        for _ in 0..1000 { let t = Instant::now(); let tx = c.transaction().unwrap(); { let mut st = tx.prepare_cached("insert into ev values('s0001',?,?)").unwrap(); for _ in 0..batch { st.execute(rusqlite::params![i, ev]).unwrap(); i += 1; } } tx.commit().unwrap(); v.push(t.elapsed().as_nanos()); }
        println!("sqlite WAL sync={mode} tx of {batch}: {}", pct(v));
    }
    for sync in [false, true] {
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(d.join(format!("j{sync}"))).unwrap(); let mut v = vec![];
        for _ in 0..1000 { let t = Instant::now(); f.write_all(ev.repeat(20).as_bytes()).unwrap(); if sync { f.sync_data().unwrap() } v.push(t.elapsed().as_nanos()); }
        println!("jsonl append of 20 events, fsync={sync}: {}", pct(v));
    }
}
