pub mod exit_watcher;
pub mod server;

pub use exit_watcher::{carries_exit_notification, spawn_exit_watcher, ExitAwareStdin};
pub use server::VerseConfBackend;
