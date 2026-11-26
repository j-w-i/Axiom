//! A folder as its person looks at it, through the socket: a file read, a folder listed, files found
//! by name and by what is in them; nothing above the folder, nothing kept from everybody.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axiom_host::{Config, client::Client};
use axiom_proto::*;

mod common;
use common::*;

async fn start(f: &Fixture) -> (Client, SessionId) {
    let w = f.dir.path().join("w");
    for d in ["w", "home"] {
        std::fs::create_dir(f.dir.path().join(d)).unwrap();
    }
    let mut c = Config::new(f.paths.clone());
    c.home = Some(f.dir.path().join("home"));
    f.serve_with(c);
    let mut c = f.client().await;
    let w = Some(w.to_str().unwrap().to_owned());
    let v = c.call(Call::SessionCreate { title: "S".into(), workspace: w }).await.unwrap();
    (c, SessionId(v["session"].as_str().unwrap().into()))
}

fn put(f: &Fixture, p: &str, b: &[u8]) {
    let p = f.dir.path().join("w").join(p);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, b).unwrap();
}

async fn read(
    c: &mut Client,
    s: &SessionId,
    p: &str,
) -> Result<FileText, axiom_host::client::ClientError> {
    let call = Call::FileRead { session: s.clone(), agent: None, path: p.into() };
    c.call(call).await.map(|v| serde_json::from_value(v).unwrap())
}

async fn search(c: &mut Client, s: &SessionId, w: &str, case: bool) -> Found {
    let call = Call::FileSearch {
        session: s.clone(),
        agent: None,
        words: w.into(),
        case,
        word: false,
        regex: false,
    };
    serde_json::from_value(c.call(call).await.unwrap()).unwrap()
}

#[tokio::test]
async fn a_person_reads_and_finds() {
    let f = fixture();
    let (mut c, s) = start(&f).await;
    put(&f, "src/main.rs", "fn main() {\n    println!(\"hello\");\n}\n".as_bytes());
    put(&f, "src/lib.rs", b"// Hello\n");
    put(&f, "logo.png", b"\x89PNG\r\n\x1a\n0000");
    put(&f, "bin.dat", b"a\0b");
    put(&f, ".gitignore", b"target/\n");
    put(&f, "target/out.rs", b"hello\n");
    put(&f, "big.txt", &vec![b'x'; 3 * 1024 * 1024]);
    // Of two bytes each, and one more before them: 2 MiB ends inside one.
    put(&f, "wide.txt", format!("a{}", "é".repeat(1024 * 1024 + 9)).as_bytes());
    put(&f, ".git/HEAD", b"ref: refs/heads/main\n");
    std::fs::write(f.dir.path().join("secret"), "SECRET").unwrap();
    std::os::unix::fs::symlink(f.dir.path().join("secret"), f.dir.path().join("w/out")).unwrap();
    std::os::unix::fs::symlink("src/lib.rs", f.dir.path().join("w/in")).unwrap();
    let t = read(&mut c, &s, "src/main.rs").await.unwrap();
    assert_eq!((t.kind.as_str(), t.whole, t.size), ("text", true, 37));
    assert!(t.text.unwrap().contains("println!"));
    assert_eq!(t.digest.len(), 64);
    let t = read(&mut c, &s, "logo.png").await.unwrap();
    assert_eq!((t.kind.as_str(), t.image.as_deref()), ("image", Some("png")));
    assert_eq!(t.bytes.as_deref(), Some("iVBORw0KGgowMDAw"));
    assert_eq!(read(&mut c, &s, "bin.dat").await.unwrap().kind, "other");
    let big = read(&mut c, &s, "big.txt").await.unwrap();
    assert_eq!((big.whole, big.text.unwrap().len()), (false, 2 * 1024 * 1024));
    let wide = read(&mut c, &s, "wide.txt").await.unwrap().text.unwrap();
    assert!(!wide.contains('\u{FFFD}') && wide.len() == 2 * 1024 * 1024 - 1, "{}", wide.len());
    // A link inside the folder is read where it leads; one out of it, and paths out, are not.
    assert!(read(&mut c, &s, "in").await.unwrap().text.unwrap().contains("Hello"));
    for out in ["out", "../secret", "/etc/passwd", "src/../../secret", "missing.rs", "src"] {
        assert!(read(&mut c, &s, out).await.is_err(), "{out}");
    }
    let l: FileList = serde_json::from_value(
        c.call(Call::FileList { session: s.clone(), agent: None, path: String::new() })
            .await
            .unwrap(),
    )
    .unwrap();
    let names: Vec<_> = l.entries.iter().map(|e| (e.name.as_str(), e.folder)).collect();
    assert_eq!(&names[..2], [("src", true), ("target", true)]);
    assert!(names.contains(&("logo.png", false)) && l.whole);
    assert!(!names.iter().any(|n| n.0 == ".git"), "{names:?}");
    let found: Vec<String> = serde_json::from_value(
        c.call(Call::FileFind { session: s.clone(), agent: None, words: "main".into() })
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(found, ["src/main.rs"], "not what git ignores");
    let hits = search(&mut c, &s, "hello", false).await;
    let got: Vec<_> = hits.hits.iter().map(|h| (h.path.as_str(), h.line)).collect();
    assert_eq!(got, [("src/lib.rs", 1), ("src/main.rs", 2)]);
    assert_eq!((hits.files, hits.unread, hits.whole), (2, 1, true));
    assert_eq!(search(&mut c, &s, "Hello", true).await.hits.len(), 1);
    assert!(!format!("{:?}", search(&mut c, &s, "SECRET", true).await.hits).contains("SECRET"));
    // A session without a folder has nothing to read.
    let v = c.call(Call::SessionCreate { title: "T".into(), workspace: None }).await.unwrap();
    let bare = SessionId(v["session"].as_str().unwrap().into());
    let call = Call::FileRead { session: bare, agent: None, path: "a".into() };
    assert!(c.call(call).await.is_err());
}

#[tokio::test]
async fn jobs_are_listed_and_a_stranger_is_not_found() {
    let f = fixture();
    let (mut c, s) = start(&f).await;
    let all: Vec<JobInfo> = serde_json::from_value(c.call(Call::Jobs {}).await.unwrap()).unwrap();
    assert!(all.is_empty());
    let call = Call::JobStop { session: s, agent: AgentId("a_x".into()), job: 1 };
    assert_eq!(refused(c.call(call).await).code, ErrorCode::NotFound);
}

#[tokio::test]
async fn what_signs_a_person_in_is_not_read_even_in_their_folder() {
    let f = fixture();
    let home = f.dir.path().join("home");
    std::fs::create_dir_all(home.join(".ssh")).unwrap();
    std::fs::write(home.join(".ssh/id"), "KEY").unwrap();
    std::fs::write(home.join("notes.txt"), "mine").unwrap();
    let mut c = Config::new(f.paths.clone());
    c.home = Some(home.clone());
    f.serve_with(c);
    let mut c = f.client().await;
    let w = Some(home.to_str().unwrap().to_owned());
    let v = c.call(Call::SessionCreate { title: "S".into(), workspace: w }).await.unwrap();
    let s = SessionId(v["session"].as_str().unwrap().into());
    assert_eq!(read(&mut c, &s, "notes.txt").await.unwrap().text.as_deref(), Some("mine"));
    assert!(read(&mut c, &s, ".ssh/id").await.is_err());
    let l: FileList = serde_json::from_value(
        c.call(Call::FileList { session: s.clone(), agent: None, path: String::new() })
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(l.entries.iter().all(|e| e.name != ".ssh") && l.hidden >= 1, "{l:?}");
    assert!(search(&mut c, &s, "KEY", true).await.hits.is_empty());
}

#[tokio::test]
async fn folders_are_kept_as_their_person_says() {
    let f = fixture();
    let (mut c, _) = start(&f).await;
    let w = f.dir.path().join("w").to_str().unwrap().to_owned();
    let set = |path: &str, name: Option<&str>| Call::FolderSet {
        path: path.into(),
        name: name.map(String::from),
        pinned: true,
        execution: None,
        access: Some(Access::Read),
        commands: vec![],
    };
    let one: FolderInfo =
        serde_json::from_value(c.call(set(&w, Some("Web"))).await.unwrap()).unwrap();
    assert_eq!(
        (one.name.as_deref(), one.pinned, one.access),
        (Some("Web"), true, Some(Access::Read))
    );
    let all: Vec<FolderInfo> =
        serde_json::from_value(c.call(Call::Folders {}).await.unwrap()).unwrap();
    assert_eq!(all, std::slice::from_ref(&one));
    for bad in ["w", "/nonexistent/x", &format!("{w}/missing")] {
        assert!(c.call(set(bad, None)).await.is_err(), "{bad}");
    }
    assert!(c.call(set(&w, Some("a\u{7}"))).await.is_err());
    c.call(Call::FolderRemove { id: one.id.clone() }).await.unwrap();
    let call = Call::FolderRemove { id: one.id };
    assert_eq!(refused(c.call(call).await).code, ErrorCode::NotFound);
    let all: Vec<FolderInfo> =
        serde_json::from_value(c.call(Call::Folders {}).await.unwrap()).unwrap();
    assert!(all.is_empty());
}
