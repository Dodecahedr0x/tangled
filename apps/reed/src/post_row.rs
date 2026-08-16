use dioxus::prelude::*;
use reed_core::{MediaRef, Post, PostId, TimelineItem, TimelineReason};

use crate::avatar::{blob_data_url, display_name, handle_or_did, Avatar, AvatarSize};
use crate::icons::{Glyph, Icon};
use crate::state::{self, Ui};
use crate::{AppLink, Route};

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum RowKind {
    #[default]
    Feed,
    Parent,
    Reply,
}

#[component]
pub fn TimelineCard(item: TimelineItem) -> Element {
    rsx! {
        PostRow {
            post: item.post,
            reason: Some(item.reason),
        }
    }
}

#[component]
pub fn PostRow(
    post: Post,
    #[props(default)] reason: Option<TimelineReason>,
    #[props(default)] kind: RowKind,
) -> Element {
    let ui = try_use_context::<Signal<Ui>>();
    let profile = state::profile_for(&post.author);
    let name = display_name(
        &post.author,
        profile.as_ref().and_then(|p| p.display_name.as_deref()),
        post.handle
            .as_deref()
            .or(profile.as_ref().and_then(|p| p.handle.as_deref())),
    );
    let handle = handle_or_did(
        &post.author,
        post.handle
            .as_deref()
            .or(profile.as_ref().and_then(|p| p.handle.as_deref())),
    );
    let source_url = post
        .source_url
        .clone()
        .or_else(|| profile.as_ref().and_then(|p| p.source_url.clone()))
        .filter(|u| u.starts_with("https://") || u.starts_with("http://"));
    let replies = state::reply_count(&post.id.0);
    let class = match kind {
        RowKind::Feed => "tx tweet",
        RowKind::Parent => "tx tweet is-parent",
        RowKind::Reply => "tx tweet is-reply",
    };
    let boost_by = match &reason {
        Some(TimelineReason::Boost { by }) => Some(by.clone()),
        _ => None,
    };
    let quote = post.quote_of.as_ref().and_then(state::post_for);
    let media: Vec<MediaRef> = post.media.iter().take(4).cloned().collect();
    let liked = post.liked_by_me;
    let boosted = post.boosted_by_me;
    let like_n = post.like_count;
    let boost_n = post.boost_count;
    let author = post.author.clone();
    let cid = post.id.0.clone();
    let boost_id = cid.clone();
    let like_id = cid.clone();
    let text = post.text.clone();
    let quote_missing = post.quote_of.clone();
    let ts = format_time(post.ts_us);
    let avatar_cid = profile.as_ref().and_then(|p| p.avatar_cid.clone());
    let avatar_size = match kind {
        RowKind::Parent => AvatarSize::Parent,
        _ => AvatarSize::Feed,
    };

    rsx! {
        article { class,
            if let Some(by) = boost_by {
                p { class: "tweet-boost muted",
                    Icon { glyph: Glyph::Boost }
                    {boost_label(&by)}
                }
            }
            div { class: "tweet-row",
                AppLink {
                    to: Route::Profile { did: author.clone() },
                    class: "avatar-link",
                    Avatar {
                        did: author.clone(),
                        name: Some(name.clone()),
                        avatar_cid,
                        size: avatar_size,
                    }
                }
                div { class: "tweet-body",
                    div { class: "tweet-meta",
                        AppLink {
                            to: Route::Profile { did: author.clone() },
                            class: "tweet-name",
                            {name.clone()}
                        }
                        span { class: "tweet-handle muted", {handle} }
                        if let Some(url) = source_url {
                            a {
                                class: "tweet-ap",
                                href: url,
                                target: "_blank",
                                rel: "noopener noreferrer",
                                "Mastodon"
                            }
                        }
                        if post.attested {
                            span { class: "tweet-fed", title: "Host-attested ActivityPub import", "federated" }
                        }
                        span { class: "tweet-sep muted", "·" }
                        AppLink {
                            to: Route::Thread { cid: cid.clone() },
                            class: "tweet-time muted",
                            {ts}
                        }
                    }
                    AppLink {
                        to: Route::Thread { cid: cid.clone() },
                        class: "tweet-text",
                        p { {text} }
                    }
                    if !media.is_empty() {
                        MediaGrid { items: media }
                    }
                    if let Some(q) = quote {
                        QuoteEmbed { post: q }
                    } else if let Some(qid) = quote_missing {
                        div { class: "tweet-quote",
                            p { class: "muted", "Quote of {crate::avatar::crop_did(&qid.0)}" }
                        }
                    }
                    div { class: "tweet-actions",
                        AppLink {
                            to: Route::Thread { cid: cid.clone() },
                            class: "tweet-action",
                            aria_label: "Reply",
                            Icon { glyph: Glyph::Reply }
                            if replies > 0 {
                                span { class: "count", "{replies}" }
                            }
                        }
                        button {
                            class: if boosted { "tweet-action is-on" } else { "tweet-action" },
                            r#type: "button",
                            aria_label: "Boost",
                            onclick: move |_| {
                                let _ = state::toggle_boost(&PostId(boost_id.clone()));
                                if let Some(ui) = ui {
                                    state::refresh_signal(ui);
                                }
                            },
                            Icon { glyph: Glyph::Boost }
                            if boost_n > 0 {
                                span { class: "count", "{boost_n}" }
                            }
                        }
                        button {
                            class: if liked { "tweet-action is-on" } else { "tweet-action" },
                            r#type: "button",
                            aria_label: "Like",
                            onclick: move |_| {
                                let _ = state::toggle_like(&PostId(like_id.clone()));
                                if let Some(ui) = ui {
                                    state::refresh_signal(ui);
                                }
                            },
                            Icon { glyph: Glyph::Like, filled: liked }
                            if like_n > 0 {
                                span { class: "count", "{like_n}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn QuoteEmbed(post: Post) -> Element {
    let profile = state::profile_for(&post.author);
    let name = display_name(
        &post.author,
        profile.as_ref().and_then(|p| p.display_name.as_deref()),
        post.handle.as_deref(),
    );
    let handle = handle_or_did(&post.author, post.handle.as_deref());
    let media: Vec<MediaRef> = post.media.iter().take(4).cloned().collect();
    rsx! {
        AppLink {
            to: Route::Thread { cid: post.id.0.clone() },
            class: "tweet-quote",
            div { class: "tweet-meta",
                Avatar {
                    did: post.author.clone(),
                    name: Some(name.clone()),
                    avatar_cid: profile.and_then(|p| p.avatar_cid),
                    size: AvatarSize::Small,
                }
                span { class: "tweet-name", {name} }
                span { class: "tweet-handle muted", {handle} }
            }
            p { class: "tweet-text", {post.text.clone()} }
            if !media.is_empty() {
                MediaGrid { items: media }
            }
        }
    }
}

#[component]
fn MediaGrid(items: Vec<MediaRef>) -> Element {
    let n = items.len().min(4);
    let class = format!("tweet-media count-{n}");
    rsx! {
        div { class,
            for m in items.iter().take(4) {
                MediaCell { media: m.clone() }
            }
        }
    }
}

#[component]
fn MediaCell(media: MediaRef) -> Element {
    let alt = media.alt.clone().unwrap_or_default();
    if media.mime.starts_with("video/") {
        let src = blob_data_url(&media.cid, Some(&media.mime));
        return rsx! {
            if let Some(src) = src {
                video {
                    class: "tweet-vid",
                    src,
                    controls: true,
                    preload: "metadata",
                }
            } else {
                div { class: "tweet-media-ph", aria_hidden: "true" }
            }
        };
    }
    let src = blob_data_url(&media.cid, Some(&media.mime));
    rsx! {
        if let Some(src) = src {
            img {
                class: "tweet-img",
                src,
                alt,
            }
        } else {
            div { class: "tweet-media-ph", aria_hidden: "true" }
        }
    }
}

fn boost_label(by: &str) -> String {
    let profile = state::profile_for(by);
    let name = display_name(
        by,
        profile.as_ref().and_then(|p| p.display_name.as_deref()),
        profile.as_ref().and_then(|p| p.handle.as_deref()),
    );
    format!("{name} boosted")
}

pub fn format_time(ts_us: u64) -> String {
    let then = ts_us / 1_000_000;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(then);
    let ago = now.saturating_sub(then);
    if ago < 60 {
        "now".into()
    } else if ago < 3600 {
        format!("{}m", ago / 60)
    } else if ago < 86_400 {
        format!("{}h", ago / 3600)
    } else if ago < 86_400 * 7 {
        format!("{}d", ago / 86_400)
    } else {
        let (y, m, d) = ymd(then);
        format!("{y:04}-{m:02}-{d:02}")
    }
}

fn ymd(secs: u64) -> (i32, u32, u32) {
    let z = (secs / 86_400) as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}
