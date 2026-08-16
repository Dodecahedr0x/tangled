use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Overview,
    Topology,
    Search,
    Settings,
    Refresh,
}

#[component]
pub fn Icon(glyph: Glyph) -> Element {
    let d: &'static [&'static str] = match glyph {
        Glyph::Overview => &[
            "M3 3.5h4.5v4.5H3z",
            "M8.5 3.5H13v4.5H8.5z",
            "M3 9h4.5v4.5H3z",
            "M8.5 9H13v4.5H8.5z",
        ],
        Glyph::Topology => &[
            "M8 3.2A1.4 1.4 0 1 0 8 0.4a1.4 1.4 0 0 0 0 2.8z",
            "M3.2 12.8A1.4 1.4 0 1 0 3.2 10a1.4 1.4 0 0 0 0 2.8z",
            "M12.8 12.8A1.4 1.4 0 1 0 12.8 10a1.4 1.4 0 0 0 0 2.8z",
            "M8 3.2 3.2 10.8",
            "M8 3.2 12.8 10.8",
        ],
        Glyph::Search => &[
            "M7.2 7.2A3.2 3.2 0 1 1 7.2 1a3.2 3.2 0 0 1 0 6.2z",
            "M9.6 9.6 14 14",
        ],
        Glyph::Settings => &["M3 5h10", "M5.5 3.2v3.6", "M3 11h10", "M10.5 9.2v3.6"],
        Glyph::Refresh => &["M12.4 6.2A4.6 4.6 0 1 0 13 9", "M12.4 3.4v2.8h-2.8"],
    };
    rsx! {
        svg {
            class: "icon",
            view_box: "0 0 16 16",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "1.25",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            for p in d.iter() {
                path { d: *p }
            }
        }
    }
}
