fn main() {
    #[cfg(any(feature = "web", feature = "desktop", feature = "mobile"))]
    dioxus::launch(loomed::App);
    #[cfg(not(any(feature = "web", feature = "desktop", feature = "mobile")))]
    {
        eprintln!("loomed: build with --features web, desktop, or mobile");
        std::process::exit(2);
    }
}
