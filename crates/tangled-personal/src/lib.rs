//! Application-independent personal workflows over signed Tangled repositories.
pub mod store;
pub use store::Store;
pub type Result<T> = std::result::Result<T, String>;
pub mod memory;
pub mod capture;
pub mod notes;
pub mod feeds;
pub mod agents;
pub mod devices;
pub mod publish;
pub mod connections;
