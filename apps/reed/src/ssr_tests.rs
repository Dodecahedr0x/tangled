use std::sync::Mutex;

use dioxus::prelude::*;
use reed_core::{Post, PostId, Profile, SocialEvent, SocialKind, TimelineItem, TimelineReason};

use crate::{DiscoverView, HomeView, OnboardView, ProfileView, Route, SearchView, ThreadView};

static HOME_LOCK: Mutex<()> = Mutex::new(());

fn with_reed_home<T>(f: impl FnOnce(&std::path::Path) -> T) -> T {
    let _g = HOME_LOCK.lock().expect("home lock");
    let home = std::env::temp_dir().join(format!(
        "reed-home-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    std::env::set_var("REED_HOME", &home);
    let out = f(&home);
    let _ = std::fs::remove_dir_all(&home);
    out
}

fn html(root: fn() -> Element) -> String {
    let mut vdom = VirtualDom::new(root);
    vdom.rebuild_in_place();
    dioxus_ssr::render(&vdom)
}

fn sample_post(id: &str, author: &str, text: &str) -> Post {
    Post {
        id: PostId(id.into()),
        rkey: "r1".into(),
        author: author.into(),
        handle: None,
        text: text.into(),
        langs: vec![],
        reply_to: None,
        quote_of: None,
        media: vec![],
        ts_us: 1,
        like_count: 0,
        boost_count: 0,
        liked_by_me: false,
        boosted_by_me: false,
        deleted: false,
        attested: false,
        source_url: None,
    }
}

fn empty_home() -> Element {
    rsx! {
        HomeView {
            items: Vec::<TimelineItem>::new(),
            knot_unreachable: false,
            query: "",
        }
    }
}

fn unreachable_home() -> Element {
    rsx! {
        HomeView {
            items: Vec::<TimelineItem>::new(),
            knot_unreachable: true,
            query: "",
        }
    }
}

fn unconfigured_home() -> Element {
    rsx! {
        HomeView {
            items: Vec::<TimelineItem>::new(),
            knot_unreachable: false,
            knot_unconfigured: true,
            query: "",
        }
    }
}

fn home_with_posts() -> Element {
    let mut post = sample_post("cid1", "did:sol:alice", "hello square");
    post.like_count = 3;
    post.boost_count = 1;
    rsx! {
        HomeView {
            items: vec![TimelineItem {
                post,
                reason: TimelineReason::Authored,
            }],
            knot_unreachable: false,
            query: "",
        }
    }
}

fn home_with_boost() -> Element {
    rsx! {
        HomeView {
            items: vec![TimelineItem {
                post: sample_post("cid1", "did:sol:alice", "hello square"),
                reason: TimelineReason::Boost {
                    by: "did:sol:bob".into(),
                },
            }],
            knot_unreachable: false,
            query: "",
        }
    }
}

fn empty_discover() -> Element {
    rsx! {
        DiscoverView {
            items: Vec::<TimelineItem>::new(),
            knot_unreachable: false,
        }
    }
}

fn onboard() -> Element {
    rsx! { OnboardView {} }
}

fn quiet_thread() -> Element {
    rsx! {
        ThreadView {
            cid: "cid1",
            posts: vec![
                sample_post("cid1", "did:sol:alice", "root"),
                sample_post("cid2", "did:sol:bob", "reply"),
            ],
            draft: "pong",
            self_did: "did:sol:alice",
        }
    }
}

fn search_hits() -> Element {
    rsx! {
        SearchView {
            query: "hello",
            hits: vec![sample_post("cid1", "did:sol:alice", "hello square")],
        }
    }
}

fn profile_page() -> Element {
    rsx! {
        ProfileView {
            did: "did:sol:alice",
            profile: Some(Profile {
                did: "did:sol:alice".into(),
                handle: None,
                display_name: Some("Ada".into()),
                bio: Some("wires".into()),
                avatar_cid: None,
                source_url: None,
            }),
            posts: vec![sample_post("cid1", "did:sol:alice", "hello square")],
        }
    }
}

#[test]
fn fira_sans_is_vendored() {
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
    assert!(crate::APP_CSS.contains("@font-face"), "{}", crate::APP_CSS);
    assert!(
        crate::APP_CSS.contains("FiraSans-Regular.woff2"),
        "{}",
        crate::APP_CSS
    );
    assert!(
        !crate::APP_CSS.contains("fonts.googleapis.com")
            && !crate::APP_CSS.contains("fonts.gstatic.com"),
        "must not hotlink Google Fonts"
    );
    assert!(crate::APP_CSS.contains("#12141a"));
    assert!(crate::APP_CSS.contains("#0c0e12"));
    assert!(crate::APP_CSS.contains("#c47a4a"));
}

#[test]
fn routes_match_pin() {
    assert_eq!(Route::Home {}.to_string(), "/");
    assert_eq!(Route::Discover {}.to_string(), "/d");
    assert_eq!(
        Route::Profile {
            did: "did:sol:x".into()
        }
        .to_string(),
        "/p/did:sol:x"
    );
    assert_eq!(
        Route::Thread {
            cid: "bafy1".into()
        }
        .to_string(),
        "/t/bafy1"
    );
    assert_eq!(Route::Compose {}.to_string(), "/compose");
    assert_eq!(Route::Search {}.to_string(), "/search");
    assert_eq!(Route::Settings {}.to_string(), "/settings");
    assert_eq!(Route::Onboard {}.to_string(), "/onboard");
}

#[test]
fn home_empty_ssr() {
    let s = html(empty_home);
    assert!(s.contains("shell"), "shell: {s}");
    assert!(s.contains("rail"), "rail: {s}");
    assert!(s.contains("No posts"), "{s}");
    assert!(s.contains("search") || s.contains("Search"), "{s}");
}

#[test]
fn home_unreachable_banner() {
    let s = html(unreachable_home);
    let label = crate::state::banner_text(crate::state::KnotStatus::Unreachable)
        .expect("unreachable banner");
    assert!(s.contains(label), "banner {label} missing in {s}");
    assert!(s.contains("banner"), "{s}");
}

#[test]
fn home_unconfigured_banner_is_not_unreachable() {
    let s = html(unconfigured_home);
    let down = reed_core::ReedError::KnotUnreachable.to_string();
    let none = crate::state::banner_text(crate::state::KnotStatus::Unconfigured)
        .expect("unconfigured banner");
    assert!(s.contains(none), "unconfigured banner missing in {s}");
    assert!(s.contains("banner"), "{s}");
    assert!(!s.contains(&down), "empty url must not say knot down: {s}");
}

#[test]
fn classify_knot_empty_vs_configured_vs_describe_fail() {
    use crate::state::{classify_knot, KnotStatus};
    assert_eq!(classify_knot("", None), KnotStatus::Unconfigured);
    assert_eq!(classify_knot("   ", None), KnotStatus::Unconfigured);
    assert_eq!(classify_knot("http://127.0.0.1:3000", None), KnotStatus::Ok);
    assert_eq!(
        classify_knot("http://127.0.0.1:3000", Some(true)),
        KnotStatus::Ok
    );
    assert_eq!(
        classify_knot("http://127.0.0.1:3000", Some(false)),
        KnotStatus::Unreachable
    );
}

#[test]
fn load_persists_default_knot_and_is_not_unreachable() {
    with_reed_home(|home| {
        let ui = crate::state::Ui::load();
        assert_eq!(ui.knot, crate::state::DEFAULT_KNOT_URL);
        assert_eq!(ui.knot_status, crate::state::KnotStatus::Ok);
        let written = std::fs::read_to_string(home.join("knot.txt")).unwrap();
        assert_eq!(written.trim(), crate::state::DEFAULT_KNOT_URL);
    });
}

#[test]
fn onboard_persist_status_is_not_unconfigured_when_default_url_exists() {
    with_reed_home(|_| {
        crate::state::persist_knot("");
        let mut ui = crate::state::Ui::empty();
        ui.knot_status = crate::state::KnotStatus::Unconfigured;
        crate::state::apply_onboard_keys(&mut ui, &reed_core::ReedKeys::generate());
        assert_eq!(ui.knot, crate::state::DEFAULT_KNOT_URL);
        assert_ne!(
            ui.knot_status,
            crate::state::KnotStatus::Unconfigured,
            "banner must not stay unconfigured after a URL is persisted"
        );
    });
}

#[test]
fn rail_uses_router_link() {
    let home = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/home.rs"));
    let compose = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/compose.rs"));
    let settings = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/settings.rs"));
    let onboard = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/onboard.rs"));
    let rail = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/rail.rs"));
    assert!(
        rail.contains("Link {") || rail.contains("AppLink"),
        "rail must use Router Link/AppLink"
    );
    for (name, src) in [
        ("home", home),
        ("compose", compose),
        ("settings", settings),
        ("onboard", onboard),
        ("rail", rail),
    ] {
        assert!(
            src.contains("Link {") || src.contains("AppLink") || src.contains("BandRail"),
            "{name} must use Router Link/AppLink"
        );
        assert!(
            !src.contains("href: \"/compose\"")
                && !src.contains("href: \"/settings\"")
                && !src.contains("href: \"/\""),
            "{name} still has raw href anchors: {src}"
        );
    }
}

#[test]
fn rail_ssr_nav_hrefs() {
    let s = html(home_with_posts);
    assert!(s.contains("href=\"/compose\""), "compose href: {s}");
    assert!(s.contains("href=\"/settings\""), "settings href: {s}");
    assert!(s.contains("href=\"/d\""), "discover href: {s}");
    assert!(
        s.contains("Refresh") || s.contains("refresh"),
        "visible refresh: {s}"
    );
}

#[test]
fn nav_never_targets_use_default() {
    let pages = [
        html(empty_home),
        html(home_with_posts),
        html(onboard),
        html(quiet_thread),
        html(empty_discover),
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
            "home.rs",
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/home.rs")),
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
fn onboard_ssr() {
    let s = html(onboard);
    assert!(s.contains("Generate") || s.contains("generate"), "{s}");
    assert!(s.contains("Import") || s.contains("import"), "{s}");
}

#[test]
fn onboard_has_back_nav() {
    let s = html(onboard);
    assert!(
        s.contains("href=\"/\""),
        "identity must link back home: {s}"
    );
    assert!(s.contains("Home"), "visible home back control: {s}");
}

#[test]
fn thread_messages_compose() {
    let s = html(quiet_thread);
    assert!(s.contains("root"), "{s}");
    assert!(s.contains("reply"), "{s}");
    assert!(s.contains("pong") || s.contains("compose"), "{s}");
}

#[test]
fn discover_and_search_and_profile_ssr() {
    let d = html(empty_discover);
    assert!(d.contains("Discover"), "{d}");
    let s = html(search_hits);
    assert!(s.contains("hello square"), "{s}");
    assert!(s.contains("Search"), "{s}");
    assert!(s.contains("acct:user@instance"), "follow acct field: {s}");
    assert!(s.contains("avatar"), "search tweet row: {s}");
    let p = html(profile_page);
    assert!(p.contains("Ada"), "{p}");
    assert!(p.contains("wires"), "{p}");
    assert!(p.contains("Follow"), "follow control: {p}");
    assert!(p.contains("avatar"), "profile avatar: {p}");
    assert!(p.contains("hello square"), "{p}");
}

fn federated_row() -> Element {
    let mut post = sample_post("cid1", "did:sol:alice", "hello square");
    post.attested = true;
    post.handle = Some("Gargron@mastodon.social".into());
    post.source_url = Some("https://mastodon.social/@Gargron/114".into());
    rsx! {
        crate::post_row::PostRow { post }
    }
}

fn federated_profile_page() -> Element {
    rsx! {
        ProfileView {
            did: "did:sol:alice",
            profile: Some(Profile {
                did: "did:sol:alice".into(),
                handle: Some("Gargron@mastodon.social".into()),
                display_name: Some("Eugen Rochko".into()),
                bio: Some("Founder of the fediverse.".into()),
                avatar_cid: None,
                source_url: Some("https://mastodon.social/users/Gargron".into()),
            }),
            posts: vec![],
        }
    }
}

#[test]
fn federated_badge_ssr() {
    let s = html(federated_row);
    assert!(s.contains("federated"), "attested mark: {s}");
    assert!(
        s.contains("@Gargron@mastodon.social"),
        "instance handle: {s}"
    );
    assert!(
        s.contains("href=\"https://mastodon.social/@Gargron/114\""),
        "original toot href: {s}"
    );
    assert!(s.contains("Mastodon"), "mastodon link label: {s}");
}

#[test]
fn federated_profile_ssr() {
    let s = html(federated_profile_page);
    assert!(s.contains("Eugen Rochko"), "display name: {s}");
    assert!(
        s.contains("@Gargron@mastodon.social"),
        "profile handle: {s}"
    );
    assert!(s.contains("Founder of the fediverse."), "bio text: {s}");
    assert!(
        s.contains("href=\"https://mastodon.social/users/Gargron\""),
        "actor href: {s}"
    );
    assert!(s.contains("Mastodon"), "mastodon link label: {s}");
    assert!(s.contains("avatar"), "large avatar: {s}");
    assert!(s.contains("Follow"), "follow control: {s}");
    assert!(
        !s.contains("mastodon purple") && !s.contains("#6364ff"),
        "no mastodon branding: {s}"
    );
}

#[test]
fn federated_tweet_row_uses_profile_bylines() {
    with_reed_home(|home| {
        let dir = home.join("replica");
        let mut replica = reed_core::Replica::open(&dir).unwrap();
        let png = b"\x89PNG-avatar";
        let cid = "bafkavatar1";
        replica.store_blob(cid, png).unwrap();
        let profile = serde_json::json!({
            "v": 1,
            "type": "pub.reed.profile",
            "collection": "pub.reed.profile",
            "rkey": "self",
            "author": "did:sol:alice",
            "ts": 2,
            "prev": [],
            "vis": "public",
            "body": {
                "displayName": "Eugen Rochko",
                "bio": "Founder of the fediverse.",
                "handle": "Gargron@mastodon.social",
                "url": "https://mastodon.social/users/Gargron",
                "avatar": cid
            },
            "sig": "00".repeat(32)
        });
        replica
            .apply_record(&serde_json::to_vec(&profile).unwrap())
            .unwrap();
        let s = html(federated_row);
        assert!(s.contains("Eugen Rochko"), "display name: {s}");
        assert!(
            s.contains("@Gargron@mastodon.social"),
            "instance handle: {s}"
        );
        assert!(
            s.contains("data:image/png;base64,") || s.contains("avatar-img"),
            "real pfp: {s}"
        );
    });
}

#[test]
fn tweet_row_layout_ssr() {
    let s = html(home_with_posts);
    assert!(s.contains("hello square"), "{s}");
    assert!(s.contains("avatar"), "default pfp: {s}");
    assert!(
        s.contains("AL") || s.contains("avatar-initials"),
        "initials: {s}"
    );
    assert!(
        s.contains("aria-label=\"Reply\"") || s.contains("Reply"),
        "{s}"
    );
    assert!(
        s.contains("aria-label=\"Boost\"") || s.contains("Boost"),
        "{s}"
    );
    assert!(
        s.contains("aria-label=\"Like\"") || s.contains("Like"),
        "{s}"
    );
    assert!(s.contains(">3<") || s.contains("3"), "like count: {s}");
    assert!(!s.contains("broken") && !s.contains("src=\"\""), "{s}");
    let boost = html(home_with_boost);
    assert!(boost.contains("boosted"), "boost attribution: {boost}");
}

#[test]
fn thread_parent_and_reply_layout() {
    let s = html(quiet_thread);
    assert!(s.contains("root"), "{s}");
    assert!(s.contains("reply"), "{s}");
    assert!(s.contains("is-parent"), "parent tweet: {s}");
    assert!(s.contains("is-reply"), "indented reply: {s}");
    assert!(s.contains("avatar"), "{s}");
}

#[tokio::test]
async fn probe_empty_url_vs_describe_fail() {
    use crate::state::{probe_knot, KnotStatus};
    assert_eq!(probe_knot("").await, KnotStatus::Unconfigured);
    assert_eq!(
        probe_knot("http://127.0.0.1:9").await,
        KnotStatus::Unreachable
    );
}

fn social(id: &str, actor: &str, kind: SocialKind, target: Option<&str>) -> SocialEvent {
    SocialEvent {
        id: id.into(),
        actor: actor.into(),
        kind,
        target: target.map(|t| PostId(t.into())),
    }
}

#[test]
fn notify_inbound_not_own() {
    use crate::notify::{inbound_toasts, reset_observer, take_toasts};
    reset_observer();
    let me = "did:sol:alice";
    let bob = "did:sol:bob";
    let mut own = sample_post("p1", me, "hello square");
    own.like_count = 1;
    own.boost_count = 1;
    let reply = {
        let mut p = sample_post("r1", bob, "a reply");
        p.reply_to = Some(PostId("p1".into()));
        p
    };
    let mention = sample_post("m1", bob, "ping @ada");
    let did_text = sample_post("d1", "did:sol:carol", "talking about did:sol:alice");
    let items = vec![
        TimelineItem {
            post: own.clone(),
            reason: TimelineReason::Authored,
        },
        TimelineItem {
            post: own.clone(),
            reason: TimelineReason::Boost { by: bob.into() },
        },
        TimelineItem {
            post: reply,
            reason: TimelineReason::Authored,
        },
        TimelineItem {
            post: mention,
            reason: TimelineReason::Authored,
        },
        TimelineItem {
            post: did_text,
            reason: TimelineReason::Authored,
        },
    ];
    let inbound = vec![
        social("f1", bob, SocialKind::Follow, None),
        social("l1", bob, SocialKind::Like, Some("p1")),
        social("b1", bob, SocialKind::Boost, Some("p1")),
    ];
    let toasts = inbound_toasts(me, &Default::default(), &items, &inbound, Some("ada"));
    let titles: Vec<&str> = toasts.iter().map(|(t, _)| t.as_str()).collect();
    for want in ["Reply", "Like", "Boost", "Follow", "Mention"] {
        assert!(titles.contains(&want), "missing {want}: {toasts:?}");
    }
    assert_eq!(
        titles.iter().filter(|t| **t == "Boost").count(),
        1,
        "one boost toast, not two: {toasts:?}"
    );
    assert!(
        !toasts
            .iter()
            .any(|(t, b)| t == "Follow" && b == "did:sol:carol"),
        "DID-in-text must not toast Follow: {toasts:?}"
    );
    assert!(
        toasts.iter().any(|(t, b)| t == "Follow" && b == bob),
        "follow toast must name the follower: {toasts:?}"
    );

    let own_reply = {
        let mut p = sample_post("r2", me, "replying to myself");
        p.reply_to = Some(PostId("p1".into()));
        p
    };
    let own_mention = sample_post("m2", me, "I am @ada");
    let own_items = vec![
        TimelineItem {
            post: own.clone(),
            reason: TimelineReason::Authored,
        },
        TimelineItem {
            post: own_reply,
            reason: TimelineReason::Authored,
        },
        TimelineItem {
            post: own_mention,
            reason: TimelineReason::Authored,
        },
    ];
    let own_social = vec![
        social("f2", me, SocialKind::Follow, None),
        social("l2", me, SocialKind::Like, Some("p1")),
        social("b2", me, SocialKind::Boost, Some("p1")),
    ];
    let own_toasts = inbound_toasts(
        me,
        &Default::default(),
        &own_items,
        &own_social,
        Some("ada"),
    );
    assert!(
        own_toasts.is_empty(),
        "own reply/like/boost/follow/mention must not toast: {own_toasts:?}"
    );
    let _ = take_toasts();
}

#[test]
fn notify_follow_from_replica_not_post_text() {
    use crate::notify::{after_replica_snapshot, reset_observer, take_toasts};
    use reed_core::graph::follow;
    use reed_core::social::{boost, like};
    use reed_core::{Outbox, PostApi, ReedKeys, Replica};

    reset_observer();
    let dir = std::env::temp_dir().join(format!(
        "reed-notify-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let mut replica = Replica::open(&dir).unwrap();
    let mut outbox = Outbox::new();
    let alice = ReedKeys::from_seeds([1; 32], [2; 32]);
    let bob = ReedKeys::from_seeds([3; 32], [4; 32]);
    let me = alice.did().as_str();
    let api = PostApi {
        keys: alice.clone(),
    };
    let post = api
        .compose_text(&mut replica, &mut outbox, "hello square", None, None)
        .unwrap();
    after_replica_snapshot(&me, &replica);
    assert!(take_toasts().is_empty(), "seed snapshot must not toast");

    let bob_api = PostApi { keys: bob.clone() };
    bob_api
        .compose_text(&mut replica, &mut outbox, "a reply", Some(&post), None)
        .unwrap();
    like(&bob, &mut replica, &mut outbox, &post).unwrap();
    boost(&bob, &mut replica, &mut outbox, &post).unwrap();
    follow(&bob, &mut replica, &mut outbox, &alice.did(), None).unwrap();
    after_replica_snapshot(&me, &replica);
    let toasts = take_toasts();
    for want in ["Reply", "Like", "Boost", "Follow"] {
        assert!(
            toasts.iter().any(|(t, _)| t == want),
            "missing {want}: {toasts:?}"
        );
    }
    assert_eq!(
        toasts.iter().filter(|(t, _)| t == "Boost").count(),
        1,
        "one inbound boost: {toasts:?}"
    );

    like(&alice, &mut replica, &mut outbox, &post).unwrap();
    boost(&alice, &mut replica, &mut outbox, &post).unwrap();
    follow(&alice, &mut replica, &mut outbox, &bob.did(), None).unwrap();
    api.compose_text(&mut replica, &mut outbox, "own reply", Some(&post), None)
        .unwrap();
    bob_api
        .compose_text(
            &mut replica,
            &mut outbox,
            &format!("talking about {me}"),
            None,
            None,
        )
        .unwrap();
    after_replica_snapshot(&me, &replica);
    let later = take_toasts();
    assert!(
        later
            .iter()
            .all(|(t, _)| t != "Follow" && t != "Like" && t != "Boost" && t != "Reply"),
        "own actions and DID-in-text must not toast: {later:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn desktop_notify_never_looks_up_use_default() {
    let src = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/notify.rs"));
    let prod = src.split("mod tests").next().expect("prod");
    assert!(!prod.contains("\"use_default\""));
    assert!(prod.contains("set_application"));
    assert_ne!(crate::notify::macos_notify_bundle_id(), "use_default");
}
