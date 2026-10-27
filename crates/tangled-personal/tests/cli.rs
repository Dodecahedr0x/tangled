use serde_json::{json,Value};
use std::io::Write;
use std::process::{Command,Stdio};

fn run(dir:&std::path::Path, feature:&str, input:Value)->(bool,Value) {
    let mut child=Command::new(env!("CARGO_BIN_EXE_tangled-personal")).args(["--data",dir.to_str().unwrap(),feature]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(input.to_string().as_bytes()).unwrap();
    let out=child.wait_with_output().unwrap();
    let bytes=if out.status.success(){out.stdout}else{out.stderr};
    (out.status.success(),serde_json::from_slice(&bytes).unwrap())
}

#[test]
fn independent_commands_share_signed_records_and_search_context() {
    let dir=tempfile::tempdir().unwrap();
    let (ok,created)=run(dir.path(),"notes",json!({"action":"create","rkey":"one","body":{"text":"A café project"}}));
    assert!(ok,"{created}");
    let (ok,found)=run(dir.path(),"memory",json!({"action":"search","query":"café"}));
    assert!(ok,"{found}");
    assert_eq!(found["total"],1);
    let (ok,note)=run(dir.path(),"notes",json!({"action":"get","rkey":"one"}));
    assert!(ok);
    assert_eq!(note["cid"],created["cid"]);
}

#[test]
fn failures_are_machine_readable_and_exit_nonzero() {
    let dir=tempfile::tempdir().unwrap();
    let (ok,error)=run(dir.path(),"unknown",json!({}));
    assert!(!ok);
    assert!(error["error"].as_str().unwrap().contains("Unknown feature"));
}
