//! typescript-config: install, remove, reorder and update language packs, and the settings that
//! go with them. The command line, the setup screen and the dashboard all call into here.

pub mod catalog;
pub mod cli;
pub mod env;
pub mod error;
pub mod fetch;
pub mod hash;
pub mod install;
pub mod languages;
pub mod setup;
pub mod source;

#[cfg(test)]
mod testutil;
