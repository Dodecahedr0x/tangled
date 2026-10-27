# Tangled personal workflows

A reusable Rust library and JSON command client for useful single-owner nodes. Capture/read-later, personal memory, notes, feeds, scoped agents, encrypted recovery, publishing, and connected-source organization share signed Tangled repositories and extensible public body contracts.

```sh
cargo run -p tangled-personal -- --help
printf '%s\n' '{"action":"create","rkey":"first","body":{"text":"A note to keep"}}' |
  cargo run -p tangled-personal -- --data .data/personal notes
printf '%s\n' '{"action":"search","query":"keep"}' |
  cargo run -p tangled-personal -- --data .data/personal memory
```

Read [the feature guide](../../docs/features/personal.md) for commands, public schemas, third-party app interoperability, custody, and current limits. The library's `execute` dispatcher is the same entry point used by the executable. Generic records and CAR/blob transfer do not require one of the bundled frontends.

Use one writer for a data directory. Private records use capability-gated visibility; the local/custodial operator can read them. Device continuity currently uses explicit encrypted file transfer with full owner keys. Publishing requires explicit selection and confirmation. Source ingest and reply intents never implicitly contact or send to an external service.
