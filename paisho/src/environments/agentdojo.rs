//! The environment parts of AgentDojo's suites (banking, slack, travel and workspace), from
//! its `default_suites/v1/tools/` clients. AgentDyn's parts (see
//! [`crate::environments::agentdyn`]) build on some of these, such as the inbox and calendar.

pub mod bank_account;
pub mod calendar;
pub mod cloud_drive;
pub mod email;
pub mod filesystem;
pub mod slack;
pub mod travel_booking;
pub mod user_account;
pub mod web;
