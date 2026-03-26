// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.
// Created: 2026-03-26

//! # pact-registry
//!
//! GitHub-backed package registry for the PACT language. Provides package
//! fetching, local caching, semver resolution, and lock file management.
//!
//! ## Architecture
//!
//! Packages are GitHub repositories with a `Pact.toml` manifest at their root.
//! Versions correspond to git tags (e.g., `v0.1.0`). The registry client:
//!
//! 1. Resolves version constraints from `Pact.toml` against available tags
//! 2. Downloads and caches package tarballs in `~/.pact/cache/`
//! 3. Generates `pact.lock` for reproducible builds
//!
//! ## Usage
//!
//! ```no_run
//! use pact_registry::{Manifest, Lockfile, Resolver, Cache};
//! use std::path::Path;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let manifest = Manifest::load(Path::new("."))?;
//! let cache = Cache::new()?;
//! let resolver = Resolver::new(cache);
//! let lockfile = resolver.resolve(&manifest).await?;
//! lockfile.save(Path::new("pact.lock"))?;
//! # Ok(())
//! # }
//! ```

/// Local package cache (`~/.pact/cache/`).
pub mod cache;
/// GitHub API client for package fetching.
pub mod client;
/// Registry error types.
pub mod error;
/// Lock file (`pact.lock`) management.
pub mod lockfile;
/// Package manifest (`Pact.toml`) parsing.
pub mod manifest;
/// Version resolution and dependency solving.
pub mod resolver;

pub use cache::Cache;
pub use client::GitHubClient;
pub use error::RegistryError;
pub use lockfile::Lockfile;
pub use manifest::Manifest;
pub use resolver::Resolver;
