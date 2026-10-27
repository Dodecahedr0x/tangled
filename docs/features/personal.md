---
title: A node for one person
slug: personal
description: Save, search, organize, read, delegate and recover your own material without a social graph.
order: 9
---

# A node for one person

A Tangled repository is useful without followers or peers. The Personal browser app, `tangled-personal` command client and reusable Rust library provide personal workflows over the existing signed repository engine. The data belongs to ordinary protocol collections, not to one user interface. Reed, Loomed, Weft and independently built apps can use the same records.

## Start locally

Launch the Personal app:

```sh
cargo run -p tangled-personal -- serve --data .data/personal --listen 127.0.0.1:8788
```

The browser opens with eight views: Library, Capture, Notes, Reading, Connections, App access, Recovery and Publish. Save your first note or file directly in the app; no other network users are needed. Keep the private launch link printed by the service to reconnect. Reed, Loomed and Weft settings link to Personal on this computer. See the [browser app and API guide](./personal-app-api.md) for authentication, file flows and third-party integration.

For command-line clients, build with `cargo build -p tangled-personal`. Create a private note, then find it through another workflow:

```sh
printf '%s\n' '{"action":"create","rkey":"first","body":{"text":"Remember the café project"}}' |
  target/debug/tangled-personal --data .data/personal notes
printf '%s\n' '{"action":"search","query":"café"}' |
  target/debug/tangled-personal --data .data/personal memory
```

Use one data directory consistently. The client creates a persistent identity on first use, writes signed records and repository commits, and retains attachments as blobs. `info` prints the owner DID and selected directory. Requests are JSON on stdin or in a file supplied with `--input`; normal results are JSON on stdout. Errors are JSON on stderr and exit nonzero. Requests are limited to 64 MiB, with smaller feature-specific content limits. Do not put passphrases or private keys in shell arguments.

The local client obtains exclusive access against other personal clients. Do not run it against a directory being written by a separate knot process; that process does not share the personal client's lock. Stop the other writer first, or transfer a verified repository copy instead. File-based device transfer is explicit in this version.

## Workflows

| Workflow | Useful first action | Contract and limits |
| --- | --- | --- |
| Personal memory | Search material from different applications; save searches and annotate records | [Memory](./personal-memory.md) |
| Capture and reading | Save a link, article, text, screenshot or file; read saved content offline | [Capture](./personal-capture.md) |
| Notes and self inbox | Keep notes, drafts, due reminders and attachments with source evidence | [Notes](./personal-notes.md) |
| Reading feeds | Subscribe and ingest supplied RSS/Atom; keep a personal reading queue | [Feeds](./personal-feeds.md) |
| Personal agents | Issue scoped, expiring grants, query selected collections and keep cited results | [Agents](./personal-agents.md) |
| Device continuity | Create and verify encrypted recovery copies and transfer signed state | [Devices](./personal-devices.md) |
| Publishing and sharing | Deliberately export selected content for people without Tangled accounts | [Publishing](./personal-publish.md) |
| Connected-source organization | Preserve source context and add local organization to supplied activity | [Connections](./personal-connections.md) |

The first versions deliberately accept supplied content rather than claiming external account login or complete remote history. The capture helper downloads browser content as JSON. Feed ingestion accepts source XML. Backup and replica transfer use explicit files. These interfaces can be automated by another client; they do not establish a deployed connector, phone app, background reminder service or production source compatibility.

## Build another app

Call `tangled_personal::execute(feature, &mut store, input)` or individual module entry points. The executable and browser service use that same dispatcher. The versioned [local HTTP API](./personal-app-api.md) exposes public discovery and collection profiles plus authenticated JSON actions, so independently built interfaces can use the same records. `Store::open` uses the existing knot snapshot format and identity file, while `Store::memory` supplies an isolated repository for tests. `records` exposes generic `list`, `get`, `put` and `delete` actions for application-defined collections:

```sh
printf '%s\n' '{"action":"put","collection":"pub.example.fieldNotes","rkey":"site-a","body":{"text":"Oak trees by the river","vendor.example":{"survey":7}}}' |
  target/debug/tangled-personal --data .data/personal records
printf '%s\n' '{"action":"search","query":"Oak","collection":"pub.example.fieldNotes"}' |
  target/debug/tangled-personal --data .data/personal memory
```

Public extensible body schemas live in `docs/spec/schemas/`; a collection name beginning with `pub` does not imply public record visibility. Features preserve unfamiliar body fields when updating records. The generic `put` action replaces the supplied body, so a client making an edit must first read and retain extension fields. Stable references use `tangled://<did>/<collection>/<rkey>`; a CID identifies exact source evidence. Derived response `uri` and `cid` fields are not part of the signed envelope.

The adapter's schema type links resolve to permissive, content-addressed JSON schema blobs for the named collection. The published collection profiles document richer field semantics without changing the version 1 envelope. Generic protocol clients retain unfamiliar collections and blob bytes without this Rust library. Authorized CAR exports include history and referenced blobs; exports are confidential unless deliberately reduced to selected public material. The device backup additionally retains owner keys and unlinked blobs inside its encrypted envelope.

New feature schemas do not replace the existing page CRDT or chat schemas. Personal notes are ordinary causal successors; clients must not pretend that last stored text is a collaborative merge. A client writing concurrently through other hosts must handle the protocol's conflicting-head rules.

## Privacy and ownership

Personal records default to `cap`: the node requires authorization to fetch them, but the local/custodial operator can read plaintext. This is not end-to-end encryption from that operator. A user's prior signed revisions and blobs remain retained after ordinary deletion; a tombstone is not secure erasure. Publishing is an explicit action and copies cannot be recalled after someone downloads them. Encrypted backup files protect the exported contents with a separate passphrase; losing that passphrase cannot be repaired by a provider.
