#![cfg(target_arch = "wasm32")]

use dioxus::prelude::*;
use dioxus_solana::{use_wallet, WalletProvider};

/// Wallet Standard signer. Never a secret store. Connect only if pubkey matches identity.
#[component]
pub fn WalletConnect(expected_did: String) -> Element {
    let w = use_wallet();
    let expected = expected_did
        .strip_prefix("did:sol:")
        .unwrap_or(expected_did.as_str())
        .to_string();
    let pk = w.pubkey().map(|p| p.to_string());
    let matched = pk.as_ref().is_some_and(|p| p == &expected);
    rsx! {
        div { class: "wallet",
            for info in w.wallets() {
                button {
                    onclick: move |_| w.connect(info.name.clone()),
                    {info.name.clone()}
                }
            }
            if matched {
                span { class: "tick", "matched" }
            } else if pk.is_some() {
                p { class: "muted", "pubkey does not match identity" }
            }
        }
    }
}

#[component]
pub fn WithWallet(children: Element) -> Element {
    rsx! {
        WalletProvider { {children} }
    }
}
