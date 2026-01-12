#![warn(clippy::pedantic)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::needless_pass_by_value)] // Rocket requires owned Json<T> for routes

//! Occlusion server library.
//!
//! This module exposes the server components for use in integration tests.

#[macro_use]
extern crate rocket;

pub mod error;
pub mod fairing;
pub mod loader;
pub mod models;
pub mod routes;
pub mod source;

use source::{DataSource, SourceMetadata};
use std::sync::RwLock;

/// Shared state for the reload scheduler
pub struct ReloadState {
    pub source: DataSource,
    pub metadata: RwLock<SourceMetadata>,
}
