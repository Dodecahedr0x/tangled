# Which Tangled v1 collections exist for a Twitter-like app, and what is missing?

## Answer
Use frozen v1 collections. Do not invent protocol types. Likes/reposts/profiles are unspecified — add only via `sys.schema` (default Last-ts-wins).

**Specified.** `pub.tangled.post` body: required `text` ≤32KiB; optional `langs`, `reply` (parent record CID), `media` (blob or media-record CIDs). Envelope `vis` (`public`/`cap`/`sealed`), not a body field. Replies live in the replier’s repo; `ref` wins if both set. rkey: TID. Last-ts-wins. `tangled.graph.follow`: rkey = DID-safe (`:`→`.`); body `v`,`subject`, optional `knot`. Also `tangled.graph.block`. `pub.tangled.media`: `blob`,`mime`, optional `alt`. `tangled.label`: rkey TID; `src`,`uri`,`val`,`neg`,`ts`. Identity is `sys.id`/`self` (keys, endpoints, `alsoKnownAs`) — no displayName/bio/avatar. Comments: `pub.tangled.annotation` + `ref`.

**Missing.** No like, repost, or profile in `spec/schemas/` (16 files) or interop. Path: publish `sys.schema` (`name`,`fields`); knots sync any collection.

## Citations
- `spec/06-data-model.md:19-43,99-158` — envelope, rkey, collections, Last-ts-wins, `sys.schema`
- `spec/schemas/pub.tangled.post.json:6-18` — post body
- `spec/schemas/tangled.graph.follow.json:6-11` — follow body
- `spec/20-use-cases.md:13-25` — microblog
- `spec/16-interoperability.md:118-122` — interop; no like/repost
- `spec/04-identifiers.md:41-76` — `sys.id` + handles, not a profile
- `spec/15-labels.md:13-23` — `tangled.label`
- `rg pub.tangled.(like|repost|profile)` → no matches

## Confidence: high
Would drop if a later spec added like/repost/profile schemas.

## Could not verify
- App-level profile convention: none in spec or `crates/tangled`.
