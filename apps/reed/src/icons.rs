use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Home,
    Discover,
    Compose,
    Search,
    Settings,
    Identity,
    Refresh,
    Reply,
    Boost,
    Like,
}

#[component]
pub fn Icon(glyph: Glyph, #[props(default)] filled: bool) -> Element {
    let d: &'static [&'static str] = match glyph {
        Glyph::Home => &["M2 8 L8 2 L14 8", "M4 7.5 V14 H12 V7.5"],
        Glyph::Discover => &["M8 8A5 5 0 1 1 8 8", "M8 3.5 V5", "M8 11 V12.5"],
        Glyph::Compose => &["M8 3.5v9", "M3.5 8h9"],
        Glyph::Search => &[
            "M7.2 7.2A3.2 3.2 0 1 1 7.2 1a3.2 3.2 0 0 1 0 6.2z",
            "M9.6 9.6 14 14",
        ],
        Glyph::Settings => &["M3 5h10", "M5.5 3.2v3.6", "M3 11h10", "M10.5 9.2v3.6"],
        Glyph::Identity => &[
            "M8 7.2A2.2 2.2 0 1 0 8 2.8a2.2 2.2 0 0 0 0 4.4z",
            "M3.4 13.4c.4-2.3 2.2-3.6 4.6-3.6s4.2 1.3 4.6 3.6",
        ],
        Glyph::Refresh => &["M12.4 6.2A4.6 4.6 0 1 0 13 9", "M12.4 3.4v2.8h-2.8"],
        Glyph::Reply => &[
            "M3 3.5h10a1 1 0 0 1 1 1v6a1 1 0 0 1-1 1H8l-3 2.4V11.5H3a1 1 0 0 1-1-1v-6a1 1 0 0 1 1-1z",
        ],
        Glyph::Boost => &["M5 3.2 3 5.2 5 7.2", "M3 5.2h8", "M11 12.8 13 10.8 11 8.8", "M13 10.8H5"],
        Glyph::Like => &["M8 13S3 9.8 3 6.8A2.7 2.7 0 0 1 8 4.9 2.7 2.7 0 0 1 13 6.8C13 9.8 8 13 8 13z"],
    };
    rsx! {
        svg {
            class: "icon",
            xmlns: "http://www.w3.org/2000/svg",
            view_box: "0 0 16 16",
            fill: if filled { "currentColor" } else { "none" },
            stroke: "currentColor",
            stroke_width: "1.4",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            for p in d.iter() {
                path { d: *p }
            }
        }
    }
}
