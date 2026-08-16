# What is Dodecahedr0x/dioxus-solana?

## Answer
A **Wallet Standard adapter** for Dioxus **0.7 web (wasm32)** — not a key generator, importer, or keychain. Discovers injected wallets, connects, signs (`solana:signMessage` / `signTransaction` / `signAndSendTransaction`), plus read-only RPC (spume). Secrets never enter the app. Works **only on wasm32**; native discovery returns `[]`. No iOS/Android; desktop unshipped.

**API:** `WalletProvider` + `use_wallet()` → `connect(name)`, `disconnect()`, `sign_message(bytes)`, `sign_transaction`, `sign_and_send`, `pubkey()`. Stores last wallet **name** only (`localStorage` `dioxus-solana:last-wallet`). License Cargo.toml `MIT OR Apache-2.0`; GitHub license null, no LICENSE files. crates.io `dioxus-solana` **0.1.0** (Dodecahedr0x) **and** git; workspace repo URL aursen-labs 404s.

**did:sol:** 32-byte Ed25519 pubkey → `did:sol:<b58>` possible. Cannot emit/import seeds for `Identity::from_seeds`; no X25519. `sign_message` returns only the 64-byte sig (drops `signedMessage`). Wallets **may prefix** OCMSF; Tangled `verify_unsigned` unproven. **Conflict:** generate/import needs a secret this crate never yields — web *signer*, not identity storage.

## Citations
- https://github.com/Dodecahedr0x/dioxus-solana (`master`, license null, 2026-07-25)
- README.md + Cargo.toml — Dioxus 0.7, wasm-only, MIT OR Apache-2.0, aursen-labs URL
- `crates/dioxus-solana/src/{hooks,storage,platform}.rs` — API; name-only storage; native empty
- `crates/dioxus-solana-web/src/signer.rs` — Wallet Standard; signature only
- crates.io dioxus-solana 0.1.0 (Dodecahedr0x, 2026-07-27)
- `crates/tangled/src/{identity,sign}.rs` — did:sol Ed25519+X25519; off-chain envelope
- anza-xyz/wallet-standard `signMessage.ts` — wallet may modify bytes

## Confidence: medium
Needs a live wallet sign of Tangled’s envelope vs `verify_unsigned`, and a crates.io↔git diff.

## Could not verify
- Tangled `verify_unsigned` vs wallet wrapping (no live test; crate drops `signedMessage`)
- crates.io 0.1.0 == git master (publish 2 days after last push)
- SPDX files (`LICENSE` 404)
- iOS/Android wallets (docs silent)
- `aursen-labs/dioxus-solana` (API 404)
