//! Pluggable data sources: `demo`/`example` (reference/synthetic), `heypocket`
//! and `up_bank` (real integrations), plus the registry and sync scheduler
//! that drive them. Ported from the Python `sources/` package.
//!
//! `sources/management/` (Django-style management commands) has no `.py`
//! sources left in the tree to port -- only stale `__pycache__` artifacts
//! remain for `run_worker` and `seed_demo` -- so there is nothing to carry
//! over here; see the port's final report for detail.

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
