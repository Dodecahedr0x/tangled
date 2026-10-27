use dioxus::prelude::*;

/// Cross-application links are deployment configuration: the apps have separate origins.
#[component]
pub fn SuiteBar(current: &'static str) -> Element {
    let destinations = [
        ("Reed", option_env!("TANGLED_REED_URL")),
        ("Loomed", option_env!("TANGLED_LOOMED_URL")),
        ("Weft", option_env!("TANGLED_WEFT_URL")),
        ("Account", option_env!("TANGLED_KNOT")),
    ];
    rsx! {
        header { class: "suite-bar",
            strong { "Tangled" }
            nav { aria_label: "Tangled applications",
                for (name, destination) in destinations {
                    if name == current {
                        span { class: "suite-current", aria_current: "page", "{name}" }
                    } else if let Some(url) = destination.filter(|u| u.starts_with("https://") || u.starts_with("http://")) {
                        a { href: url, "{name}" }
                    }
                }
            }
        }
    }
}
