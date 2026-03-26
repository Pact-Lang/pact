// Copyright (c) 2025-2026 Gabriel Lars Sabadin
// Licensed under the MIT License. See LICENSE file in the project root.

//! Remote agent federation — cross-network agent discovery and dispatch for PACT.
//!
//! This crate provides the building blocks for federating PACT agents across
//! network boundaries: a registry for advertising agent capabilities, a discovery
//! client for finding agents across multiple registries, and a dispatch client
//! for invoking tools on remote agents with permission enforcement.

pub mod client;
pub mod discovery;
pub mod error;
pub mod protocol;
pub mod registry;

pub use client::FederationClient;
pub use discovery::DiscoveryClient;
pub use error::FederationError;
pub use protocol::{
    DiscoverRequest, DiscoverResponse, DispatchRequest, DispatchResponse, HealthResponse,
    RegisterRequest, RegisterResponse, RemoteAgentCard, RemoteParamInfo, RemoteToolInfo,
};
pub use registry::AgentRegistry;
