pub mod adapters;
pub mod application;
pub(crate) mod commands;
pub mod domain;
pub mod downloader;
mod headless;
pub(crate) mod infrastructure;
pub mod media;
pub mod persistence;
pub mod scheduler;
mod security;

pub fn run_headless() {
    headless::run_headless();
}
