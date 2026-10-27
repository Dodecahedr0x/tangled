use tangled::page::{self, PageBody, PageOp, PageRecord};
fn record(cid: String, prev: Vec<String>, body: PageBody) -> PageRecord {
    PageRecord {
        cid,
        ts: 1,
        prev,
        vis: "public".into(),
        circle: None,
        epoch: None,
        body,
    }
}
#[test]
fn deep_page_traversals_complete_on_the_normal_thread_stack() {
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "deep_page_worker", "--ignored", "--nocapture"])
        .env("TANGLED_DEEP_PAGE_WORKER", "1")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "deep traversal subprocess failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
}
#[test]
#[ignore = "executed by the subprocess regression above"]
fn deep_page_worker() {
    assert!(std::env::var_os("TANGLED_DEEP_PAGE_WORKER").is_some());
    let input = vec![record(
        "deep-atom-record".into(),
        vec![],
        PageBody::Ops(vec![PageOp::Ins {
            after: None,
            run: "x".repeat(100_000),
        }]),
    )];
    assert_eq!(page::apply(&input).unwrap().text.len(), 100_000);
    assert_eq!(page::snapshot_of(&input).unwrap().atoms.len(), 100_000);
    let chain: Vec<_> = (0..10_000)
        .map(|i| {
            record(
                format!("record-{i:05}"),
                if i == 0 {
                    vec![]
                } else {
                    vec![format!("record-{:05}", i - 1)]
                },
                PageBody::Ops(vec![]),
            )
        })
        .collect();
    assert_eq!(page::apply(&chain).unwrap().text, "");
    assert_eq!(page::frontier_of(&chain), vec!["record-09999".to_string()]);
}
#[test]
fn iterative_traversal_keeps_sibling_order_and_snapshot_atoms() {
    let mut input = vec![
        record(
            "a".into(),
            vec![],
            PageBody::Ops(vec![PageOp::Ins {
                after: None,
                run: "abc".into(),
            }]),
        ),
        record(
            "b".into(),
            vec![],
            PageBody::Ops(vec![PageOp::Ins {
                after: None,
                run: "β".into(),
            }]),
        ),
    ];
    assert_eq!(page::apply(&input).unwrap().text, "βabc");
    input.reverse();
    assert_eq!(page::apply(&input).unwrap().text, "βabc");
    let snap = page::snapshot_of(&input).unwrap();
    assert_eq!(snap.atoms.len(), 4);
}
