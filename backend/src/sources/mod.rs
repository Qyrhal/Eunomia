//! Pluggable data sources: `demo`/`example` (reference/synthetic), `heypocket`
//! and `up_bank` (real integrations), plus the registry and sync scheduler
//! that drive them.

pub mod base;
pub mod demo;
pub mod discord;
pub mod example;
pub mod github;
pub mod gmail;
pub mod google_calendar;
pub mod heypocket;
pub mod linear;
pub mod notion;
pub mod registry;
pub mod scheduler;
pub mod slack;
pub mod spotify;
pub mod stripe;
pub mod todoist;
pub mod up_bank;
