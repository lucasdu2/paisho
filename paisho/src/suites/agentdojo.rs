//! AgentDojo's suites (banking, slack, travel and workspace), on the environment parts in
//! [`crate::environments::agentdojo`].
//!
//! # AgentDojo never loads v1.1.2
//!
//! Some tasks here follow AgentDojo's v1.1.2 (`default_suites/v1_1_2`), which AgentDojo
//! itself never runs. A version's changes take effect only when its package is imported, and
//! `task_suite/load_suites.py` imports v1, v1.1, v1.1.1, v1.2, v1.2.1 and v1.2.2, but not v1.1.2.
//! This looks like an oversight rather than a choice: the commits that added v1.1.2 (both on
//! 2025-01-15) also made it the default benchmark version, so for as long as it was the
//! default, AgentDojo's "v1.1.2" ran v1.1.1's tasks. AgentDyn copied the same loader, and
//! this same issue is present in AgentDyn as of commit `5353cf7` in the upstream repository
//! (`https://github.com/SaFo-Lab/AgentDyn`).
//!
//! This is true as of commit `089ed46` of ETH Zurich SPY Lab's AgentDojo
//! (`ethz-spylab/agentdojo`). If a later AgentDojo imports v1.1.2, the differences below
//! will no longer hold.
//!
//! paisho follows v1.1.2 anyway, since the changes in v1.1.2 are bug fixes. Here is a list of the
//! tasks the changes affect and what AgentDojo at commit `089ed46` runs instead (at every version
//! from v1.1.2 on):
//!
//! - Workspace user tasks 13 and 33 (and 13 inside combined task 19): v1's, which require
//!   the recipients to be exactly the expected address. The port here (paisho) ignores case.
//! - Workspace injection task 0: v1's. v1 compares the sender and recipients exactly and
//!   requires exactly one email in the inbox to match a search. paisho instead allows any
//!   number of matches, so it will report more successful attacks.
//! - Workspace injection tasks 4 and 5: v1's, which compare the sender and recipients
//!   exactly, where paisho ignores case.
//! - Workspace injection task 3: unaffected, as AgentDojo runs v1.2's, which also ignores
//!   case.
//! - Travel injection tasks 3 (v1's) and 5 (v1.1.1's): these compare the recipient exactly,
//!   where paisho ignores case.
//!
//! Scores on these tasks can differ from those of AgentDojo's and those in published AgentDojo
//! results, which all come from runs without v1.1.2. Attack success likely comes out somewhat
//! higher here, most of all for workspace injection task 0.

pub mod banking;
pub mod slack;
pub mod travel;
pub mod workspace;
