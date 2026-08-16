# What do the Tangled spec and `crates/tangled` expose for a messaging client?

## Answer
Call this crate; no typed `Chat`. `pub.tangled.chat` `{circle` required, `text`?, `blob`?}`. Envelope `circle` (`did:sol:<owner>#<rkey>`) ⇒ `vis: sealed`; last-ts-wins, new rkey/msg. Sign JSON (`sign_json_record`). `Visibility::{Public,Cap,Sealed}`. Cap/sealed: `Authorization: Cap …`. Fetch via `circle/read` on `tangled://circle/<owner>/<rkey>`. `sys.circle` public/cap only.

1:1 `alg: hpke-record` (`seal_record`/`open_record`); e2e uses `pub.tangled.post`. Circle chat `alg: epoch-aead`. `CircleState::{create,add_member,remove_member}`: HPKE ≤16 never-MLS, then dual→mls forever. Wraps `sys.circle.wrap`; MLS `sys.circle.mls` opaque `Vec<u8>`. Hide HPKE/MLS in UI. `Identity::generate`/`from_seeds`: Ed25519 + separate X25519, `did:sol:<b58>`.

`KnotClient::{mock,mock_shared,http,with_cap_header}`: put/get record, getHead, getRepo(Auth), blobs, describe, identity.resolve, cap.verify, notify, queryBacklinks. `MockKnot`=`KnotEngine`. Mock WS fail; `list_records` mock-only.

Flow: circle+wrap → grant → sealed chat on author repo → `get_record` → unwrap → `epoch_aead_open`. MLS RFC 9420 unverified.

## Citations
- `spec/schemas/pub.tangled.chat.json:5-14`; `spec/06-data-model.md:45-50,132,150` — schema/sealed/LWW
- `spec/08-visibility.md:13-40,57`; `spec/10-circles.md:9,35-63,88-107`; `spec/05-cryptography.md:81-94` — vis/dual-stack/HPKE
- `crates/tangled/src/lib.rs:6-42`; `record.rs:15-36,323-336`; `http.rs:280-547`; `knot.rs:77-78` — exports/KnotClient/MockKnot
- `circle.rs:13-118,296-308`; `hpke.rs:203-296`; `identity.rs:99-116`; `constants.rs:5-43` — circle/HPKE/identity
- `tests/e2e.rs:144-218,391-463`; `README.md:24,102-106`; `spec/12-http-api.md:43-107` — e2e/limits/no listRecords

## Confidence: high
Repo spec+crate; drop if OpenAPI or chat plaintext encoding changes.

## Could not verify
- Live knot `listRecords`: OpenAPI omits; no live knot (`brief.md`).
- Chat AEAD plaintext = schema JSON: spec 06; `e2e.rs` seals raw `hello dual-stack`.
- 1:1 DM collection: chat requires `circle`; HPKE e2e uses `pub.tangled.post` (Q2 unanswered).
