use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Chats,
    NewChat,
    Settings,
    Identity,
    Refresh,
    Send,
    Search,
}

#[component]
pub fn Icon(glyph: Glyph) -> Element {
    let d: &'static [&'static str] = match glyph {
        Glyph::Chats => &[
            "M3 3.5h8.5A1.5 1.5 0 0 1 13 5v4.5A1.5 1.5 0 0 1 11.5 11H7L4 13.5V11H3A1.5 1.5 0 0 1 1.5 9.5V5A1.5 1.5 0 0 1 3 3.5z",
        ],
        Glyph::NewChat => &["M8 3.5v9", "M3.5 8h9"],
        Glyph::Settings => &["M3 5h10", "M5.5 3.2v3.6", "M3 11h10", "M10.5 9.2v3.6"],
        Glyph::Identity => &[
            "M8 7.2A2.2 2.2 0 1 0 8 2.8a2.2 2.2 0 0 0 0 4.4z",
            "M3.4 13.4c.4-2.3 2.2-3.6 4.6-3.6s4.2 1.3 4.6 3.6",
        ],
        Glyph::Refresh => &["M12.4 6.2A4.6 4.6 0 1 0 13 9", "M12.4 3.4v2.8h-2.8"],
        Glyph::Send => &["M3 8h9.2", "M8.8 4.4 13.2 8l-4.4 3.6"],
        Glyph::Search => &[
            "M7.2 7.2A3.2 3.2 0 1 1 7.2 1a3.2 3.2 0 0 1 0 6.2z",
            "M9.6 9.6 14 14",
        ],
    };
    rsx! {
        svg {
            class: "icon",
            xmlns: "http://www.w3.org/2000/svg",
            view_box: "0 0 16 16",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "1.4",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            "aria-hidden": "true",
            for p in d.iter() {
                path { d: *p }
            }
        }
    }
}
