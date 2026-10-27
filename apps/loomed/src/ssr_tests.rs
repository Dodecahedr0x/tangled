static LOOMED_HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
use dioxus::prelude::*;
use loomed_core::{CircleRef, Conversation, Message, MessageId};

use crate::{InboxView, OnboardView, Route, ThreadView};

fn html(root: fn() -> Element) -> String {
    let mut vdom = VirtualDom::new(root);
    vdom.rebuild_in_place();
    dioxus_ssr::render(&vdom)
}

fn empty_inbox() -> Element {
    rsx! {
        InboxView {
            conversations: Vec::<Conversation>::new(),
            knot_unreachable: false,
            query: "",
        }
    }
}

fn unreachable_inbox() -> Element {
    rsx! {
        InboxView {
            conversations: Vec::<Conversation>::new(),
            knot_unreachable: true,
            query: "",
        }
    }
}

fn inbox_with_chats() -> Element {
    rsx! {
        InboxView {
            conversations: vec![
                Conversation {
                    circle: CircleRef("did:sol:alice#r1".into()),
                    title: "chat".into(),
                    last_preview: "ping".into(),
                    unread: 0,
                    members: vec!["did:sol:alice".into(), "did:sol:bob".into()],
                },
                Conversation {
                    circle: CircleRef("did:sol:alice#g1".into()),
                    title: "group".into(),
                    last_preview: "hi".into(),
                    unread: 0,
                    members: vec![
                        "did:sol:alice".into(),
                        "did:sol:bob".into(),
                        "did:sol:cara".into(),
                        "did:sol:dan".into(),
                        "did:sol:eve".into(),
                    ],
                },
            ],
            knot_unreachable: false,
            query: "",
            self_did: "did:sol:alice",
        }
    }
}

fn onboard() -> Element {
    rsx! { OnboardView {} }
}

fn quiet_msg() -> Message {
    Message {
        id: MessageId("cid1".into()),
        circle: CircleRef("did:sol:alice#r1".into()),
        author: "did:sol:bob".into(),
        text: Some("ping".into()),
        blob_cid: None,
        ts_us: 1,
        receipts: vec!["did:sol:alice".into()],
    }
}

fn long_did_msg() -> Message {
    Message {
        id: MessageId("cid-long".into()),
        circle: CircleRef("did:sol:alice#r1".into()),
        author: "did:sol:7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU".into(),
        text: Some("late".into()),
        blob_cid: None,
        ts_us: 90_000_000,
        receipts: vec![],
    }
}

fn own_msg() -> Message {
    Message {
        id: MessageId("cid-own".into()),
        circle: CircleRef("did:sol:alice#r1".into()),
        author: "did:sol:alice".into(),
        text: Some("ack".into()),
        blob_cid: None,
        ts_us: 120_000_000,
        receipts: vec![],
    }
}

fn quiet_thread() -> Element {
    rsx! {
        ThreadView {
            circle: "did:sol:alice#r1",
            messages: vec![quiet_msg(), long_did_msg()],
            draft: "pong",
        }
    }
}

fn one_to_one_thread() -> Element {
    rsx! {
        ThreadView {
            circle: "did:sol:alice#r1",
            messages: vec![quiet_msg(), own_msg()],
            draft: "pong",
            self_did: "did:sol:alice",
        }
    }
}

fn group_thread() -> Element {
    use crate::labels::PeerLabel;
    rsx! {
        ThreadView {
            circle: "did:sol:alice#g1",
            messages: vec![quiet_msg()],
            draft: "",
            self_did: "did:sol:zz",
            recipients: vec![
                PeerLabel::new("did:sol:alice", Some("alice.sol".into())),
                PeerLabel::new("did:sol:bob", Some("bob.sol".into())),
                PeerLabel::new("did:sol:cara", Some("cara.sol".into())),
                PeerLabel::new("did:sol:dan", None),
                PeerLabel::new("did:sol:eve", None),
            ],
        }
    }
}

#[test]
fn commons_keeps_local_font_assets_and_needs_no_remote_fonts() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let regular = root.join("assets/fonts/FiraSans-Regular.woff2");
    let ofl = root.join("assets/fonts/OFL.txt");
    assert!(regular.is_file(), "missing {regular:?}");
    assert!(
        regular.metadata().unwrap().len() > 10_000,
        "font too small: {regular:?}"
    );
    let ofl_text = std::fs::read_to_string(&ofl).expect("OFL.txt");
    assert!(
        ofl_text.contains("SIL OPEN FONT LICENSE"),
        "OFL text missing in {ofl:?}"
    );
    let css = crate::app_style();
    assert!(
        !css.contains("fonts.googleapis.com") && !css.contains("fonts.gstatic.com"),
        "app styles must not require external font requests"
    );
    assert!(
        css.contains("color-scheme: light"),
        "Commons is a light reading surface"
    );
}

#[test]
fn routes_match_pin() {
    assert_eq!(Route::Inbox {}.to_string(), "/");
    assert_eq!(
        Route::Thread {
            circle: "did:sol:x#y".into()
        }
        .to_string(),
        "/c/did:sol:x%23y"
    );
    assert_eq!(Route::Compose {}.to_string(), "/new");
    assert_eq!(Route::Settings {}.to_string(), "/settings");
    assert_eq!(Route::Onboard {}.to_string(), "/onboard");
}

#[test]
fn inbox_empty_ssr() {
    let s = html(empty_inbox);
    assert!(s.contains("shell"), "night-radio shell: {s}");
    assert!(s.contains("rail"), "conversation rail: {s}");
    assert!(
        s.contains("No conversations yet") || s.contains("no traffic"),
        "{s}"
    );
    assert!(
        s.contains("search") || s.contains("tune"),
        "search field: {s}"
    );
}

#[test]
fn inbox_unreachable_banner() {
    let s = html(unreachable_inbox);
    let label = loomed_core::LoomedError::KnotUnreachable.to_string();
    assert!(s.contains(&label), "banner {label} missing in {s}");
    assert!(s.contains("banner"), "{s}");
}

#[test]
fn inbox_lists_recipients() {
    let s = html(inbox_with_chats);
    assert!(s.contains("did:sol:bob"), "1:1 recipient: {s}");
    assert!(
        s.contains("did:sol:bob, did:sol:cara + 2 others"),
        "group crop: {s}"
    );
    assert!(!s.contains(">chat<") && !s.contains(">group<"), "{s}");
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/inbox.rs"));
    assert!(
        src.contains("recipient_label"),
        "inbox must call crate::labels::recipient_label"
    );
    assert!(
        src.contains("AppLink") || src.contains("Link {"),
        "chat rows must use Router Link/AppLink"
    );
}

#[test]
fn onboard_ssr() {
    let s = html(onboard);
    assert!(s.contains("Generate identity"), "{s}");
    assert!(s.contains("import"), "{s}");
}

#[test]
fn onboard_has_back_nav() {
    let s = html(onboard);
    assert!(
        s.contains("href=\"/\""),
        "identity must link back to the inbox: {s}"
    );
    assert!(s.contains("Inbox"), "visible inbox back control: {s}");
}

#[test]
fn onboard_back_uses_router_link() {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/onboard.rs"));
    assert!(
        src.contains("AppLink") || src.contains("Link {"),
        "identity back must use Router Link/AppLink, not raw href"
    );
    assert!(
        !src.contains("href: \"/\""),
        "raw href no-ops on desktop: {src}"
    );
}

#[test]
fn thread_messages_receipts_compose() {
    let s = html(quiet_thread);
    assert!(s.contains("ping"), "{s}");
    assert!(s.contains("did:sol:alice"), "receipt: {s}");
    assert!(s.contains("pong") || s.contains("compose"), "{s}");
    assert!(s.contains("msg-bubble"), "plates: {s}");
    assert!(s.contains("msg-time"), "short time: {s}");
    assert!(
        s.contains("did:sol:…gAsU"),
        "cropped inbound DID via peer_display: {s}"
    );
    assert!(
        !s.contains(">did:sol:7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU<"),
        "full inbound DID must not be the visible name: {s}"
    );
    assert!(
        !s.contains("1970-01-01") && !s.contains("T00:00"),
        "must not show ISO datetime: {s}"
    );
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/thread.rs"));
    assert!(
        src.contains("peer_display"),
        "thread must use crate::labels::peer_display, not a local crop"
    );
}

#[test]
fn thread_own_vs_other_plates() {
    let s = html(one_to_one_thread);
    assert!(s.contains("is-own"), "own plate: {s}");
    assert!(s.contains("ack"), "{s}");
    assert!(s.contains("msg-bubble"), "{s}");
}

#[test]
fn short_time_is_clock_not_iso() {
    let t = crate::thread::short_time(1_700_000_000 * 1_000_000);
    assert!(t.contains(':'), "{t}");
    assert!(!t.contains('T') && !t.contains('-'), "{t}");
    let (h, m) = t.split_once(':').expect(t.as_str());
    assert!(h.len() <= 2 && m.len() == 2, "{t}");
}

#[test]
fn thread_header_shows_other_person() {
    let s = html(one_to_one_thread);
    assert!(s.contains("thread-header"), "persistent header: {s}");
    let header = s
        .split("class=\"log\"")
        .next()
        .expect("message list follows header");
    assert!(
        header.contains("thread-header") && header.contains("did:sol:bob"),
        "1:1 header should name the other person: {header}"
    );
    assert!(
        crate::APP_CSS.contains(".thread-header"),
        "header styles missing"
    );
}

#[test]
fn thread_header_crops_group_recipients() {
    let s = html(group_thread);
    let header = s
        .split("class=\"log\"")
        .next()
        .expect("message list follows header");
    assert!(
        header.contains("alice.sol, bob.sol + 3 others"),
        "group crop: {header}"
    );
    assert!(header.contains("5 people"), "group count: {header}");
    let log = s.split("class=\"log").nth(1).expect("log after header");
    assert!(
        log.contains("bob.sol"),
        "handle on plate via peer_display: {log}"
    );
}

#[test]
fn send_path() {
    let _guard = LOOMED_HOME_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    use loomed_core::{Chat, FileKeyStore, KeyStore, LoomedKeys, Replica};

    use crate::paths::replica_dir;
    use crate::state::persist_keys;
    use crate::thread::transmit;

    let home = std::env::temp_dir().join(format!(
        "loomed-send-path-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    std::env::set_var("LOOMED_HOME", &home);

    let alice = LoomedKeys::generate();
    persist_keys(&alice);
    assert!(FileKeyStore {
        path: home.join("keys.json")
    }
    .load()
    .unwrap()
    .is_some());

    let bob = LoomedKeys::generate();
    let chat = Chat {
        keys: alice.clone(),
    };
    let (circle, rec) = chat.create_circle(&[bob.did()]).unwrap();
    let mut replica = Replica::open(&replica_dir()).unwrap();
    replica
        .apply_record(&serde_json::to_vec(&rec).unwrap())
        .unwrap();

    let id = transmit(&circle.0, "hello band").expect("transmit");
    let msgs = crate::state::messages_for(&circle.0);
    assert!(
        msgs.iter()
            .any(|m| m.id == id && m.text.as_deref() == Some("hello band")),
        "replica should contain sent text, got {msgs:?}"
    );
}

#[test]
fn nav_never_targets_use_default() {
    let pages = [
        html(empty_inbox),
        html(inbox_with_chats),
        html(onboard),
        html(quiet_thread),
    ];
    for s in pages {
        assert!(
            !s.contains("use_default"),
            "rendered markup must not target use_default: {s}"
        );
    }
    let src_files = [
        (
            "lib.rs",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs")),
        ),
        (
            "rail.rs",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/rail.rs")),
        ),
        (
            "inbox.rs",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/inbox.rs")),
        ),
        (
            "notify.rs",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/notify.rs")),
        ),
    ];
    for (name, src) in src_files {
        let prod = src.split("mod tests").next().unwrap_or(src);
        if name != "notify.rs" {
            assert!(
                !prod.contains("\"use_default\""),
                "{name} must not mention use_default"
            );
        }
    }
}

#[test]
fn empty_knot_probe_skips_network() {
    let _guard = LOOMED_HOME_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let home = std::env::temp_dir().join(format!(
        "loomed-probe-empty-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    std::env::set_var("LOOMED_HOME", &home);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let start = std::time::Instant::now();
    let unreachable = rt.block_on(crate::state::probe_and_poll());
    assert!(unreachable, "missing knot.txt is unreachable");
    assert!(
        start.elapsed() < std::time::Duration::from_millis(200),
        "empty knot must not hit the network: {:?}",
        start.elapsed()
    );
}

#[test]
fn first_paint_load_is_local_only() {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/state.rs"));
    let load = src
        .split("pub fn load()")
        .nth(1)
        .and_then(|s| s.split("pub fn hits").next())
        .expect("Ui::load");
    let code: String = load
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect();
    assert!(
        !code.contains("Sync::http")
            && !code.contains("probe")
            && !code.contains("poll_once")
            && !code.contains("describe"),
        "first paint must not wait on the knot: {load}"
    );
}

#[test]
fn knot_sync_runs_off_vdom_thread() {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/state.rs"));
    assert!(
        src.contains("tokio::spawn") || src.contains("std::thread::spawn"),
        "probe/poll must leave the VirtualDom thread"
    );
    let lib = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"));
    assert!(
        lib.contains("bootstrap") && lib.contains("use_future"),
        "App must start knot work after first paint, not inside load()"
    );
}

#[test]
fn inbox_search_is_wired() {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/inbox.rs"));
    assert!(
        src.contains("oninput") || src.contains("on_query"),
        "tune field must update Ui.query: {src}"
    );
}

#[test]
fn thread_compose_settings_use_router_link() {
    for (name, src) in [
        (
            "thread.rs",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/thread.rs")),
        ),
        (
            "compose.rs",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/compose.rs")),
        ),
        (
            "settings.rs",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/settings.rs")),
        ),
    ] {
        assert!(
            src.contains("AppLink") || src.contains("Link {"),
            "{name} band back must use Router Link/AppLink"
        );
        assert!(
            !src.contains("href: \"/\""),
            "{name} raw href no-ops on desktop: {src}"
        );
    }
}

#[test]
fn desktop_notify_never_looks_up_use_default() {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/notify.rs"));
    let prod = src.split("mod tests").next().expect("prod");
    assert!(!prod.contains("\"use_default\""));
    assert!(prod.contains("set_application"));
    assert_ne!(crate::notify::macos_notify_bundle_id(), "use_default");
}

#[test]
fn commons_inbox_names_privacy_and_local_custody() {
    let s = html(inbox_with_chats);
    assert!(s.contains("<h1"), "Inbox needs a real heading: {s}");
    assert!(
        s.contains("Private conversations"),
        "scope must stay visible: {s}"
    );
    assert!(
        s.contains("Keys on this device"),
        "custody must be explicit: {s}"
    );
    let thread = html(quiet_thread);
    assert!(
        thread.contains("aria-label=\"Message\""),
        "composer needs a field label: {thread}"
    );
}

#[test]
fn capture_commons_loomed_views() {
    let Ok(dir) = std::env::var("TANGLED_DESIGN_CAPTURE_DIR") else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    for (name, root) in [
        ("loomed-inbox", inbox_with_chats as fn() -> Element),
        ("loomed-thread", group_thread),
        ("loomed-identity", onboard),
        ("loomed-empty", empty_inbox),
    ] {
        std::fs::write(std::path::Path::new(&dir).join(format!("{name}.html")), format!("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{name}</title><style>{}</style><p class=\"fixture-note\">Synthetic test data · server-rendered Rust view</p>{}</html>", crate::app_style(), html(root))).unwrap();
    }
}

#[test]
fn identity_anchor_uses_current_identity_on_routes_without_a_prop() {
    fn rail_with_context() -> Element {
        use_context_provider(|| {
            Signal::new(crate::state::Ui {
                did: Some("did:sol:current-local-identity".into()),
                knot: "https://host.example".into(),
                conversations: vec![],
                knot_unreachable: false,
                query: String::new(),
                draft: String::new(),
                peer: String::new(),
                import_json: String::new(),
            })
        });
        rsx! { crate::rail::BandRail { show_band: true } }
    }
    let s = html(rail_with_context);
    assert!(
        s.contains("did:sol:current-local-identity"),
        "current identity must remain visible on every route: {s}"
    );
    assert!(
        s.contains("https://host.example"),
        "current host must remain visible: {s}"
    );
}

#[test]
fn replacing_identity_updates_shared_state_to_the_stored_signer() {
    use loomed_core::LoomedKeys;
    let _guard = LOOMED_HOME_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let home = std::env::temp_dir().join(format!("loomed-identity-switch-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    std::env::set_var("LOOMED_HOME", &home);
    let old = LoomedKeys::generate();
    crate::state::persist_keys(&old);
    let mut ui = crate::state::Ui::load();
    assert_eq!(ui.did, Some(old.did().as_str()));
    for next in [LoomedKeys::generate(), LoomedKeys::generate()] {
        let imported = LoomedKeys::from_solana_json(&next.export_json()).unwrap();
        crate::state::apply_onboard_keys(&mut ui, &imported);
        let signer = crate::state::load_keys().unwrap().did().as_str();
        assert_eq!(signer, imported.did().as_str());
        assert_eq!(
            ui.did,
            Some(signer),
            "identity anchor must follow the actual stored signer after generation/import"
        );
    }
}
