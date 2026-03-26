// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Registry error types.

use thiserror::Error;

/// Errors that can occur during registry operations.
#[derive(Debug, Error)]
pub enum RegistryError {
    /// An I/O error occurred.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A TOML parsing error occurred.
    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    /// A TOML serialization error occurred.
    #[error("TOML serialization error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),

    /// A semver parsing error occurred.
    #[error("invalid version: {0}")]
    SemverParse(#[from] semver::Error),

    /// An HTTP error occurred while fetching a package.
    #[error("HTTP error: {0}")]
    Http(String),

    /// The GitHub API returned an error.
    #[error("GitHub API error (status {status}): {body}")]
    GitHubApi {
        /// HTTP status code.
        status: u16,
        /// Response body.
        body: String,
    },

    /// No version matching the constraint was found.
    #[error("no version of '{package}' matching '{constraint}'")]
    NoMatchingVersion {
        /// Package name.
        package: String,
        /// Version constraint that could not be satisfied.
        constraint: String,
    },

    /// The manifest file was not found.
    #[error("Pact.toml not found in '{0}'")]
    ManifestNotFound(String),

    /// A package was not found in the lock file.
    #[error("package '{0}' not found in pact.lock")]
    NotInLockfile(String),

    /// The cache directory could not be determined.
    #[error("could not determine cache directory")]
    NoCacheDir,

    /// A package was not found in the local cache.
    #[error("package '{name}@{version}' not cached; run 'pact install'")]
    NotCached {
        /// Package name.
        name: String,
        /// Package version.
        version: String,
    },

    /// JSON deserialization error.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}
