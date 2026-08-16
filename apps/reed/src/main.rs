fn main() {
    #[cfg(any(feature = "web", feature = "desktop", feature = "mobile"))]
    dioxus::launch(reed::App);
    #[cfg(not(any(feature = "web", feature = "desktop", feature = "mobile")))]
    {
        eprintln!("reed: build with --features web, desktop, or mobile");
        std::process::exit(2);
    }
}
