//! Contract tests for the protocol 0.6 additions (audit AUD-022,
//! `docs/decisions/proposed/CR-AUD-022-workspace-file-operations.md`): workspace file operations.
//! Same rule as `contract.rs`: a failure means the wire changed.

use axiom_protocol::fs::{
    self, CreateDirectoryParams, CreateFileParams, RenameParams, TrashParams, WriteFileResult,
};
use axiom_protocol::lifecycle::Empty;
use axiom_protocol::method::Method;
use axiom_protocol::{registry, PROTOCOL_VERSION};
use serde_json::{json, Value};

fn round_trip<T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(
    v: Value,
) -> T {
    let t: T = serde_json::from_value(v.clone()).expect("deserializes");
    assert_eq!(
        serde_json::to_value(&t).unwrap(),
        v,
        "re-serializes identically"
    );
    t
}

#[test]
fn protocol_version_is_0_6() {
    assert_eq!(PROTOCOL_VERSION, "0.6");
}

#[test]
fn the_methods_are_registered_with_their_wire_names() {
    let names = registry::method_names();
    for (name, wire) in [
        (fs::CreateFile::NAME, "fs/createFile"),
        (fs::CreateDirectory::NAME, "fs/createDirectory"),
        (fs::Rename::NAME, "fs/rename"),
        (fs::Trash::NAME, "fs/trash"),
    ] {
        assert_eq!(name, wire);
        assert!(names.contains(&wire), "{wire} is not registered");
    }
}

#[test]
fn params_and_results() {
    let p: CreateFileParams =
        round_trip(json!({"workspaceId":"w1","path":"src/new.rs","text":"fn main() {}\n"}));
    assert_eq!(p.text, "fn main() {}\n");
    // `text` is optional (an empty file).
    let p: CreateFileParams = round_trip(json!({"workspaceId":"w1","path":"a"}));
    assert!(p.text.is_empty());
    let _: WriteFileResult = round_trip(json!({"version":"ab"}));
    let p: CreateDirectoryParams = round_trip(json!({"workspaceId":"w1","path":"a/b"}));
    assert!(!p.parents);
    let p: CreateDirectoryParams =
        round_trip(json!({"workspaceId":"w1","path":"a/b","parents":true}));
    assert!(p.parents);
    let _: RenameParams = round_trip(json!({"workspaceId":"w1","from":"a","to":"b/a"}));
    let _: TrashParams = round_trip(json!({"workspaceId":"w1","path":"old.txt"}));
    let _: Empty = round_trip(json!({}));
}

/// AUD-012: `GitStatus.polling` is additive: absent means false, and false is not written.
#[test]
fn git_status_polling_is_additive() {
    use axiom_protocol::git::GitStatus;
    let s: GitStatus = round_trip(json!({"repo":true,"ahead":0,"behind":0,"files":[]}));
    assert!(!s.polling);
    assert!(serde_json::to_value(&s).unwrap().get("polling").is_none());
    let s: GitStatus =
        round_trip(json!({"repo":true,"ahead":0,"behind":0,"files":[],"polling":true}));
    assert!(s.polling);
}
