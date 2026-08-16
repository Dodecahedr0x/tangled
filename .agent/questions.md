# Questions

- id: Q1
  question: "App name?"
  why: "crate path, window title, bundle id"
  answer: "Reed"

- id: Q2
  question: "What is this app for?"
  why: "decides mock vs live knot, polish bar"
  answer: "Polished app that connects to a real knot (same bar as Loomed)"

- id: Q3
  question: "v1 feature bar?"
  why: "bounds the tree: posts-only vs follows/likes/replies/reposts"
  answer: "Social + reposts + search + notifications"

- id: Q4
  question: "Visual world?"
  why: "committed visual identity; Loomed is night-radio, this is a public square"
  answer: "Same dark metal as Loomed so the two apps feel like a suite"

- id: Q5
  question: "v1 platforms?"
  why: "crate features, CI, UI chrome"
  answer: "Web + desktop + iOS/Android (same as Loomed)"

- id: Q6
  question: "Offline?"
  why: "local replica vs online-only; outbox"
  answer: "Local replica + outbox: read timeline offline, queue posts until the knot is back"

- id: Q7
  question: "What does notifications mean in v1?"
  why: "in-app vs push infra"
  answer: "In-app banners while running, plus OS notifications where Dioxus can do them locally"

- id: Q8
  question: "Home timeline source?"
  why: "client-assembled follow graph vs knot-local public firehose; crosses domain+UI"
  answer: "Following graph + a Discover tab of public posts the knot knows"

- id: Q9
  question: "Media in posts?"
  why: "blob upload, compose UI, replica size"
  answer: "Text + images + video"

- id: Q10
  question: "How do you follow someone?"
  why: "compose-to-new, profile lookup"
  answer: "Both DID and handle (recommended)"

- id: Q11
  question: "Search?"
  why: "local index vs knot API"
  answer: "Local replica search plus scanning Discover/public posts already fetched"

- id: Q12
  question: "Post length in the UI?"
  why: "spec allows 32 KiB; a Twitter clone may cap"
  answer: "Use the spec limit (32 KiB); no extra UI cap"

- id: Q13
  question: "Q9 asked for video. What does video mean in v1?"
  why: "codec, size, player; spec media CIDs are generic blobs"
  answer: "putBlob any video file + in-app player (no transcoding); skip types the platform cannot play"

- id: Q14
  question: "Likes: if the spec has no like collection, what should Reed do?"
  why: "Q3 requires likes; must not mint fake protocol"
  answer: "App collection interpreted as a like (Loomed receipts pattern); do not add a spec type unless research finds one"

- id: Q15
  question: "Reposts: native record, quote-as-new-post, or both?"
  why: "Q3 requires reposts; shapes records and timeline"
  answer: "Boost (repost record) and quote (new post) as two actions"

- id: Q16
  question: "Profiles: display name / bio / avatar from where?"
  why: "profile page and timeline bylines"
  answer: "Protocol profile/actor record if one exists; otherwise a Reed app profile record (display name, bio, avatar blob)"

- id: Q17
  question: "Edit and delete posts in v1?"
  why: "last-ts-wins vs immutable UI"
  answer: "Edit (last-ts-wins put) and delete (spec tombstone/delete if any) your own posts"

- id: Q18
  question: "Which events notify in v1?"
  why: "notification pipeline inputs"
  answer: "Reply, like, boost, follow, and mention (recommended)"

- id: Q19
  question: "Mentions (@handle) in v1?"
  why: "compose parser and notification targeting"
  answer: "Yes: @handle in text links to profiles and can notify"

- id: Q20
  question: "Mute/block in v1?"
  why: "timeline filters; app collections"
  answer: "Mute and block (app collections; hide in Home/Discover)"

- id: Q21
  question: "Max media attachments per post?"
  why: "spec media array has no maxItems; compose + core validation"
  answer: "4 attachments, any mix of image/video"

- id: Q22
  question: "Quote wire form?"
  why: "spec post has no quote field; core + UI must agree"
  answer: "Quote = public post with envelope ref=original CID and no body reply"

- id: M1
  question: "Where does the Mastodon/ActivityPub gateway live, and what is Reed?"
  why: "decides crate boundaries: knot HTTP vs Reed becoming a Mastodon client"
  answer: "Gateway lives on the knot (ActivityPub/Mastodon). Reed stays a Tangled client."

- id: M2
  question: "Read-only import, write-only export, or dual?"
  why: "inbox-only vs outbox-only vs both; shapes S2S and Reed actions"
  answer: "Dual: read Mastodon into Reed and post/follow/like/boost/reply out."

- id: M3
  question: "v1 federated surface?"
  why: "bounds the tree: timelines, social verbs, DMs"
  answer: "v1: Home/Federated-as-Discover, profiles, follow, post, reply, like, boost. No DMs."

- id: M4
  question: "How do you address a remote AP actor from Reed?"
  why: "follow/profile lookup: WebFinger acct vs actor IRI; who resolves"
  answer: "Paste acct:user@instance or an actor URL; the knot gateway resolves"

- id: M5
  question: "What is Home vs Discover once AP is ingested?"
  why: "timeline assembly: follow graph vs federated public Notes"
  answer: "Home = follows including AP actors; Discover includes federated public Notes the knot ingested"

- id: M6
  question: "How much Mastodon-specific chrome in Reed v1?"
  why: "UI blast radius: new tabs vs reuse Reed posts/follow"
  answer: "Minimal chrome: follow accepts acct:@; posts look like Reed posts"

- id: M7
  question: "Gateway process: wrap tangled-knot Router in-process, or a sidecar?"
  why: "research/reed-mastodon-extension-points.md could not verify; crate and deploy boundary"
  answer: "Wrap tangled-knot: same process, extra AP routes on the knot"

- id: M8
  question: "How should Reed surface host-attested on imported AP objects?"
  why: "spec 16 MUST surface attested; Reed has no UI field today"
  answer: "Tiny ‘federated’/attested mark on mapped posts (spec MUST show attested; still minimal)"
