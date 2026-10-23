//! tacit-config: install, remove, reorder and update language packs, and the settings that
//! go with them. The command line, the setup screen and the dashboard all call into here.

pub mod app;
pub mod catalog;
pub mod cli;
pub mod diagnostics;
pub mod env;
pub mod error;
pub mod fetch;
pub mod hash;
pub mod http;
pub mod ime;
pub mod install;
pub mod languages;
pub mod learning;
mod settings;
pub mod setup;
pub mod source;
pub mod uninstall;
pub mod update;
mod web;

#[cfg(test)]
mod testutil;
