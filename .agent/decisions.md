# Decisions

- id: A1
  phase: 1
  node: null
  route: must-ask
  claim: "New Dioxus app in apps/reed on Tangled v1 via crates/tangled; protocol client patterned on Loomed (HTTP knot client, MockKnot tests, identity generate/import, optional dioxus-solana web signer)"
  because: "user: 'In the app folder, build a cross platform tweeter clone using tangled protocol. Use dioxus. Protocol interactions should be similar to what is done in loomed'; Q1 Reed"
  affects: [brief.md]
  closes: [Q1]
  status: decided

- id: A3
  phase: 1
  node: null
  route: must-ask
  claim: "Reed is a polished real-knot client, same bar as Loomed"
  because: "Q2 option A"
  affects: [brief.md]
  closes: [Q2]
  status: decided

- id: A4
  phase: 1
  node: null
  route: must-ask
  claim: "v1 feature bar is full social: public posts, profiles, follow, home timeline, replies/threads, likes, reposts, search, notifications"
  because: "Q3 option C"
  affects: [brief.md]
  closes: [Q3]
  status: decided

- id: A5
  phase: 1
  node: null
  route: must-ask
  claim: "Visual world is the same dark metal as Loomed so Reed and Loomed feel like a suite"
  because: "Q4 option B"
  affects: [brief.md]
  closes: [Q4]
  status: decided

- id: A2
  phase: 1
  node: null
  route: must-ask
  claim: "v1 is a public social/Twitter surface (posts), not a messenger; do not reuse Loomed chat circles as the primary model"
  because: "user asked for a tweeter clone; Loomed already covers chat"
  affects: [brief.md]
  closes: []
  status: decided

- id: A6
  phase: 1
  node: null
  route: must-ask
  claim: "v1 platforms are web + desktop + iOS/Android"
  because: "Q5 option A"
  affects: [brief.md]
  closes: [Q5]
  status: decided

- id: A7
  phase: 1
  node: null
  route: must-ask
  claim: "Local replica + outbox; knot down is cached read-only with a banner and queued posts"
  because: "Q6 option A; knot-down UX follows Loomed"
  affects: [brief.md]
  closes: [Q6]
  status: decided

- id: A8
  phase: 1
  node: null
  route: must-ask
  claim: "Notifications are in-app banners plus local OS notifications; not APNs/FCM/web push"
  because: "Q7 option A"
  affects: [brief.md]
  closes: [Q7]
  status: decided

- id: A9
  phase: 1
  node: null
  route: must-ask
  claim: "Home is following graph + own posts; Discover is a second tab of public posts the knot knows"
  because: "Q8 option B"
  affects: [brief.md]
  closes: [Q8]
  status: decided

- id: A10
  phase: 1
  node: null
  route: must-ask
  claim: "Posts are text + images + video via putBlob; follow by DID or handle; search is local replica plus already-fetched Discover posts; composer uses spec 32 KiB with no extra cap"
  because: "Q9 C; Q10 A; Q11 B; Q12 B"
  affects: [brief.md]
  closes: [Q9, Q10, Q11, Q12]
  status: decided

- id: A11
  phase: 1
  node: null
  route: must-ask
  claim: "Video is putBlob of any video file plus in-app player; no transcoding; skip types the platform cannot play"
  because: "Q13 option A"
  affects: [brief.md]
  closes: [Q13]
  status: decided

- id: A12
  phase: 1
  node: null
  route: must-ask
  claim: "Likes, boosts, and mute are application collections unless spec already names them; quote is a new public post referencing the original; block is in v1"
  because: "Q14 A; Q15 C; Q20 A"
  affects: [brief.md]
  closes: [Q14, Q15, Q20]
  status: decided

- id: A13
  phase: 1
  node: null
  route: must-ask
  claim: "Profile is protocol actor/profile if one exists, otherwise a Reed app profile record (display name, bio, avatar blob)"
  because: "Q16 option A"
  affects: [brief.md]
  closes: [Q16]
  status: decided

- id: A14
  phase: 1
  node: null
  route: must-ask
  claim: "Own posts can be edited (last-ts-wins) and deleted; mentions are @handle links that notify; notify on reply, like, boost, follow, mention"
  because: "Q17 A; Q18 A; Q19 A"
  affects: [brief.md]
  closes: [Q17, Q18, Q19]
  status: decided

- id: A15
  phase: 2
  node: null
  route: verifiable
  claim: "Block uses spec tangled.graph.block; mute is pub.reed.mute via sys.schema"
  because: "research/tangled-social-collections.md; Q20 wanted both; spec already names block"
  affects: [brief.md, tree/1.md]
  closes: []
  status: decided

- id: A16
  phase: 2
  node: null
  route: verifiable
  claim: "No protocol like/repost/profile; Reed publishes sys.schema for pub.reed.like, pub.reed.boost, pub.reed.profile, pub.reed.mute (Last-ts-wins)"
  because: "research/tangled-social-collections.md; Q14 A; Q16 otherwise"
  affects: [brief.md, tree/1.md]
  closes: []
  status: decided

- id: A17
  phase: 2
  node: null
  route: verifiable
  claim: "Home/profile/Discover are client-assembled via get_head/get_repo; no feed XRPC; listRecords is tests-only"
  because: "research/knot-timeline-assembly.md"
  affects: [brief.md, tree/1.md]
  closes: []
  status: decided

- id: A18
  phase: 2
  node: null
  route: verifiable
  claim: "Copy Loomed identity/FileKeyStore/parse_peer/Outbox/Replica/Sync::http+mock; wasm persist_keys no-op; UI drain_outbox without SIWS; MockKnot::hosting in tests"
  because: "research/loomed-protocol-patterns.md"
  affects: [brief.md, tree/1.md]
  closes: []
  status: decided

- id: A19
  phase: 2
  node: null
  route: verifiable
  claim: "Post media is putBlob + pub.tangled.media (blob, mime, alt); mime unconstrained; honor describe.maxBlob; video is another blob + HTML5 player"
  because: "research/tangled-blob-media.md; Q13 A"
  affects: [brief.md, tree/1.md]
  closes: []
  status: decided

- id: A20
  phase: 2
  node: null
  route: verifiable
  claim: "Workspace members apps/reed + apps/reed/core; dioxus =0.7.10; default=[] and feature-gate launch so CI clippy/test without desktop/wasm compile"
  because: "research/dioxus-workspace-second-app.md"
  affects: [tree/1.md]
  closes: []
  status: decided

- id: A21
  phase: 3
  node: 1
  route: must-ask
  claim: "Max 4 media attachments per post, any mix of image/video"
  because: "Q21 verbatim: 4 attachments, any mix of image/video"
  affects: [tree/1.md, tree/1.1.md, tree/1.2.md]
  closes: [Q21]
  status: decided

- id: A22
  phase: 3
  node: 1
  route: must-ask
  claim: "Quote = pub.tangled.post with envelope ref = original CID and no body reply"
  because: "Q22 verbatim: Quote = public post with envelope ref=original CID and no body reply"
  affects: [tree/1.md, tree/1.1.md, tree/1.2.md]
  closes: [Q22]
  status: decided

- id: A23
  phase: 4
  node: 1.1
  route: verifiable
  claim: "Edit/delete last-ts-wins put of same rkey mints a new CID; replica aliases the old PostId to the current record"
  because: "KnotClient has no delete_record; sign_json_record CIDs the new body; UI/tests keep the compose PostId; rg -n aliases apps/reed/core/src/replica.rs"
  affects: [apps/reed/core/src/replica.rs, apps/reed/core/src/post.rs]
  check: "rg -n aliases apps/reed/core/src/replica.rs"
  status: decided

- id: A24
  phase: 4
  node: 1.1
  route: verifiable
  claim: "Unfollow/unblock/unmute/unlike/unboost are last-ts-wins puts with body.deleted=true"
  because: "no delete_record on KnotClient; same pattern as post delete; rg -n 'deleted.: true' apps/reed/core/src/graph.rs apps/reed/core/src/social.rs"
  affects: [apps/reed/core/src/graph.rs, apps/reed/core/src/social.rs]
  check: "rg -n 'deleted.: true' apps/reed/core/src/graph.rs apps/reed/core/src/social.rs"
  status: decided

- id: A25
  phase: 4
  node: 1
  route: verifiable
  claim: "Loomed crate paths stay apps/loomed/core and crates/tangled (not apps/loomed-core)"
  because: "git checkout -- apps/loomed restored a stale staged layout; workspace load requires path = core and tangled = ../../../crates/tangled; rg -n 'path = \"core\"' apps/loomed/Cargo.toml && rg -n 'crates/tangled' apps/loomed/core/Cargo.toml"
  affects: [apps/loomed/Cargo.toml, apps/loomed/core/Cargo.toml]
  check: "rg -n 'path = \"core\"' apps/loomed/Cargo.toml && rg -n 'crates/tangled' apps/loomed/core/Cargo.toml"
  status: decided

- id: A26
  phase: 4
  node: 1.2
  route: verifiable
  claim: "Replica::inbound_social lists inbound follow/like/boost records; notify uses those, not post text or like/boost count deltas"
  because: "home/discover only emit posts; DID-in-text follow and count-delta boosts were pin misses; rg -n 'fn inbound_social' apps/reed/core/src/replica.rs && rg -n 'SocialKind::Follow' apps/reed/src/notify.rs && ! rg -n 'text.contains\\(self_did\\)' apps/reed/src/notify.rs"
  affects: [apps/reed/core/src/replica.rs, apps/reed/src/notify.rs]
  check: "rg -n 'fn inbound_social' apps/reed/core/src/replica.rs && rg -n 'SocialKind::Follow' apps/reed/src/notify.rs && ! rg -n 'text.contains\\(self_did\\)' apps/reed/src/notify.rs"
  status: decided

- id: A27
  phase: 4
  node: 1.1
  route: verifiable
  claim: "edit_text last-ts-wins copies the stored post body and replaces only text, preserving media record CIDs, reply, and langs"
  because: "Post.media[].cid is the blob CID after hydrate; body.media stores media record CIDs; rg -n 'post_body' apps/reed/core/src/post.rs apps/reed/core/src/replica.rs"
  affects: [apps/reed/core/src/post.rs, apps/reed/core/src/replica.rs]
  check: "rg -n 'post_body' apps/reed/core/src/post.rs apps/reed/core/src/replica.rs"
  status: decided

- id: A28
  phase: 4
  node: 1.1
  route: verifiable
  claim: "poll_once uses follow.knot via KnotClient::http, or Sync::with_remote MockKnot in tests when the URL differs from the home knot"
  because: "tree/1.md optional per-follow knot; MockKnot is in-process so tests register engines by URL; rg -n 'fn with_remote|fn client_for|follow_knots' apps/reed/core/src/sync.rs"
  affects: [apps/reed/core/src/sync.rs]
  check: "rg -n 'fn with_remote|fn client_for|follow_knots' apps/reed/core/src/sync.rs"
  status: decided

- id: M-A1
  phase: 1
  node: null
  route: must-ask
  claim: "ActivityPub/Mastodon gateway lives on the knot; Reed stays a Tangled client on the existing protocol path"
  because: "M1 verbatim: Gateway lives on the knot (ActivityPub/Mastodon). Reed stays a Tangled client."
  affects: [brief-mastodon.md]
  closes: [M1]
  status: decided

- id: M-A2
  phase: 1
  node: null
  route: must-ask
  claim: "Dual federation: ingest Mastodon/AP into Tangled records Reed already reads, and emit post/follow/like/boost/reply outbound"
  because: "M2 verbatim: Dual: read Mastodon into Reed and post/follow/like/boost/reply out."
  affects: [brief-mastodon.md]
  closes: [M2]
  status: decided

- id: M-A3
  phase: 1
  node: null
  route: must-ask
  claim: "v1 surface is Home plus Federated-as-Discover, profiles, follow, post, reply, like, boost; no DMs"
  because: "M3 verbatim: v1: Home/Federated-as-Discover, profiles, follow, post, reply, like, boost. No DMs."
  affects: [brief-mastodon.md]
  closes: [M3]
  status: decided

- id: M-A4
  phase: 1
  node: null
  route: must-ask
  claim: "Remote AP actors are addressed by pasting acct:user@instance or an actor URL; the knot gateway resolves (WebFinger / actor GET)"
  because: "M4 verbatim: Paste acct:user@instance or an actor URL; the knot gateway resolves"
  affects: [brief-mastodon.md]
  closes: [M4]
  status: decided

- id: M-A5
  phase: 1
  node: null
  route: must-ask
  claim: "Home is the follow graph including AP actors; Discover includes federated public Notes the knot ingested"
  because: "M5 verbatim: Home = follows including AP actors; Discover includes federated public Notes the knot ingested"
  affects: [brief-mastodon.md]
  closes: [M5]
  status: decided

- id: M-A6
  phase: 1
  node: null
  route: must-ask
  claim: "Reed v1 chrome stays minimal: follow accepts acct:@; ingested Notes render as Reed posts"
  because: "M6 verbatim: Minimal chrome: follow accepts acct:@; posts look like Reed posts"
  affects: [brief-mastodon.md]
  closes: [M6]
  status: decided

- id: M-A7
  phase: 2
  node: null
  route: verifiable
  claim: "Ingress reuses attested sys.foreign plus mapped pub.tangled.post; Reed Sync already get_head/get_repo/apply_record and hydrates timelines from pub.tangled.post only"
  because: "research/mastodon-ap-spec.md; research/reed-mastodon-extension-points.md (high)"
  affects: [brief-mastodon.md]
  closes: []
  status: decided

- id: M-A8
  phase: 2
  node: null
  route: verifiable
  claim: "Do not build Mastodon REST or OAuth in Reed; federation is knot S2S (M1)"
  because: "M1; research/mastodon-client-vs-s2s.md — Reed stays on Tangled HTTP; Mastodon clients use REST, not AP C2S"
  affects: [brief-mastodon.md]
  closes: []
  status: decided

- id: M-A9
  phase: 2
  node: null
  route: verifiable
  claim: "Note.content maps to post.text; inReplyTo maps to reply; attachments map to media"
  because: "research/mastodon-ap-spec.md could not verify field-level Note mapping; default adopted"
  affects: [brief-mastodon.md]
  closes: []
  status: assumed

- id: M-A10
  phase: 2
  node: null
  route: verifiable
  claim: "AP Like maps to pub.reed.like; AP Announce maps to pub.reed.boost; do not mint a spec type"
  because: "research/mastodon-ap-spec.md and research/mastodon-client-vs-s2s.md — pub.reed.* absent from spec/interop/translations.yaml; default adopted"
  affects: [brief-mastodon.md]
  closes: []
  status: assumed

- id: M-A11
  phase: 2
  node: null
  route: verifiable
  claim: "WebFinger and HTTP Message Signatures are required for S2S"
  because: "research/mastodon-ap-spec.md invent list; research/mastodon-client-vs-s2s.md (HTTP Signatures, Digest on POST, RFC9421 ≥4.5); crates/tangled/README.md:131"
  affects: [brief-mastodon.md]
  closes: []
  status: decided

- id: M-A12
  phase: 2
  node: null
  route: verifiable
  claim: "AP mapping lives in a separate crate; identity, signing, CID, and XRPC stay Tangled-only; do not extend PostApi"
  because: "research/reed-mastodon-extension-points.md (high) — PostApi is pub.tangled.post/media; tangled-knot is XRPC+WS only"
  affects: [brief-mastodon.md]
  closes: []
  status: decided

- id: M-A13
  phase: 1
  node: null
  route: must-ask
  claim: "Gateway wraps tangled-knot (one process, extra AP routes)"
  because: "M7 verbatim: Wrap tangled-knot: same process, extra AP routes on the knot"
  affects: [brief-mastodon.md]
  closes: [M7]
  status: decided

- id: M-A14
  phase: 1
  node: null
  route: must-ask
  claim: "Reed shows tiny federated/attested mark on mapped posts"
  because: "M8 verbatim: Tiny ‘federated’/attested mark on mapped posts (spec MUST show attested; still minimal)"
  affects: [brief-mastodon.md]
  closes: [M8]
  status: decided

- id: W1
  phase: 1
  node: null
  route: must-ask
  claim: "Explorer app is Weft at apps/weft (+ apps/weft/core). Public read-only network viewer; no identity onboard or wallet."
  because: "House names are fiber/weave (tangled, loomed, reed). Weft is the cross-thread. Explorer is not a client that writes."
  affects: [apps/weft]
  closes: []
  status: decided

- id: W2
  phase: 1
  node: null
  route: must-ask
  claim: "No global knot directory. GET /xrpc/tangled.node.stats is knot-local public aggregates + advertised-peer topology. Explorer crawls describe.peers from the configured knot."
  because: "spec 13: no global directory. 21.20 already has operator-marked peers; stats must not re-advertise learned peers."
  affects: [spec/12-http-api.md, crates/tangled, apps/weft]
  closes: []
  status: decided

- id: W3
  phase: 1
  node: null
  route: must-ask
  claim: "node.stats counts only live vis:public tips; omits cap/sealed, capability uses, sessions, and learned peer URLs. Hosted list is DIDs that already have public records."
  because: "Stats must not be an oracle for private activity."
  affects: [spec/18-threat-model.md, crates/tangled]
  closes: []
  status: decided

