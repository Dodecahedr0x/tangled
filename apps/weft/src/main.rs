fn main() {
    #[cfg(any(feature = "web", feature = "desktop", feature = "mobile"))]
    dioxus::launch(weft::App);
    #[cfg(not(any(feature = "web", feature = "desktop", feature = "mobile")))]
    {
        eprintln!("weft: build with --features web, desktop, or mobile");
        std::process::exit(2);
    }
}
