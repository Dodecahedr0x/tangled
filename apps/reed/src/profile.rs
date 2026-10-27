use dioxus::prelude::*;
use reed_core::Post;
use reed_core::Profile as Actor;

use crate::avatar::{display_name, handle_or_did, Avatar, AvatarSize};
use crate::post_row::PostRow;
use crate::rail::BandRail;
use crate::state::{self, Ui};

#[component]
pub fn Profile(did: String) -> Element {
    let ui = try_use_context::<Signal<Ui>>();
    let profile = crate::state::profile_for(&did);
    let posts = crate::state::profile_posts(&did);
    let self_did = ui
        .map(|s| s().did.clone().unwrap_or_default())
        .unwrap_or_default();
    let following = if self_did.is_empty() {
        false
    } else {
        state::is_following(&self_did, &did)
    };
    rsx! {
        ProfileView { did, profile, posts, self_did, following }
    }
}

#[component]
pub fn ProfileView(
    did: String,
    profile: Option<Actor>,
    posts: Vec<Post>,
    #[props(default)] self_did: String,
    #[props(default)] following: bool,
) -> Element {
    let ui = try_use_context::<Signal<Ui>>();
    let name = display_name(
        &did,
        profile.as_ref().and_then(|p| p.display_name.as_deref()),
        profile.as_ref().and_then(|p| p.handle.as_deref()),
    );
    let handle = handle_or_did(&did, profile.as_ref().and_then(|p| p.handle.as_deref()));
    let source_url = profile
        .as_ref()
        .and_then(|p| p.source_url.clone())
        .filter(|u| u.starts_with("https://") || u.starts_with("http://"));
    let bio = profile
        .as_ref()
        .and_then(|p| p.bio.clone())
        .unwrap_or_default();
    let avatar_cid = profile.as_ref().and_then(|p| p.avatar_cid.clone());
    let is_self = !self_did.is_empty() && self_did == did;
    let subject = did.clone();
    rsx! {
        div { class: "shell",
            BandRail { self_did }
            main { class: "stage", id: "main-content",
                crate::suite::SuiteBar { current: "Reed" }
                h1 { class: "page-title",
                    span { class: "tick" }
                    "Profile"
                }
                header { class: "profile-head",
                    Avatar {
                        did: did.clone(),
                        name: Some(name.clone()),
                        avatar_cid,
                        size: AvatarSize::Large,
                    }
                    p { class: "profile-name", {name} }
                    p { class: "profile-handle muted", {handle} }
                    if !bio.is_empty() {
                        p { class: "profile-bio", {bio} }
                    }
                    if let Some(url) = source_url {
                        a {
                            class: "profile-ap",
                            href: url,
                            target: "_blank",
                            rel: "noopener noreferrer",
                            "Mastodon"
                        }
                    }
                    if !is_self {
                        button {
                            class: if following { "follow-btn is-on" } else { "follow-btn" },
                            r#type: "button",
                            onclick: move |_| {
                                let _ = state::toggle_follow(&subject);
                                if let Some(ui) = ui {
                                    state::refresh_signal(ui);
                                }
                            },
                            if following { "Unfollow" } else { "Follow" }
                        }
                    }
                }
                p { class: "profile-tab", "Posts" }
                if posts.is_empty() {
                    div { class: "empty",
                        p { "No posts" }
                    }
                } else {
                    div { class: "log",
                        for p in posts.iter() {
                            PostRow { post: p.clone() }
                        }
                    }
                }
            }
        }
    }
}
