//! Benchmark runs and their results on disk, laid out like AgentDojo's `runs/` directory:
//!
//! ```text
//! <logdir>/<benchmark>/benchmark.json                              the run's configuration
//! <logdir>/<benchmark>/<suite>/<task>/<attack>/<injection>.json    one file per episode
//! ```
//!
//! Benign episodes use `none` for the attack and the injection, as AgentDojo does. Results are
//! written when an episode finishes, so an interrupted run can be resumed: an episode whose
//! file exists is complete.

use std::collections::HashMap;
use std::fs;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::attacks::InjectVectors;
use crate::suites::ToolResult;

/// Which build of paisho wrote a file, so results can be traced to the code that produced
/// them. Written to `benchmark.json` and to every result file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Build {
    /// The package version in `Cargo.toml`, which isn't updated for every change.
    pub paisho_version: String,
    /// The git commit paisho was built from; `None` if built outside a git checkout.
    pub paisho_commit: Option<String>,
    /// Whether paisho's files had uncommitted changes at build time, so `paisho_commit`
    /// doesn't fully describe the code; `None` if unknown.
    pub paisho_dirty: Option<bool>,
}

impl Build {
    /// This build, as recorded by `build.rs`.
    pub fn current() -> Self {
        let known = |value: &'static str| (value != "unknown").then_some(value);
        Build {
            paisho_version: env!("CARGO_PKG_VERSION").to_string(),
            paisho_commit: known(env!("PAISHO_GIT_COMMIT")).map(str::to_string),
            paisho_dirty: known(env!("PAISHO_GIT_DIRTY")).map(|dirty| dirty == "true"),
        }
    }
}

/// What a benchmark runs: every episode of each suite under each attack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    pub name: String,
    pub suites: Vec<String>,
    pub attacks: Vec<String>,
    /// How attacks address the model, such as `"GPT-4"`; `None` uses AgentDojo's generic
    /// name. Shared by every episode, as the model is.
    pub model_name: Option<String>,
    /// Which injection vectors every attacked episode fills. Benchmarks registered before
    /// this was configurable have no such field and used the default.
    #[serde(default)]
    pub inject_vectors: InjectVectors,
}

/// One episode of a benchmark, one of three kinds:
///
/// - benign: a `task` alone;
/// - attacked: a `task` with an `injection` placed by an `attack`;
/// - injection alone: an `injection` with neither, whose goal is the prompt. This checks
///   whether the agent can achieve the goal when simply asked to.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EpisodeKey {
    pub suite: String,
    pub task: Option<String>,
    pub attack: Option<String>,
    pub injection: Option<String>,
}

/// A tool call the agent made, with what the server answered.
#[derive(Debug, Serialize)]
pub struct ToolCallRecord {
    pub name: String,
    pub arguments: serde_json::Map<String, serde_json::Value>,
    /// The tool's result, or `None` if the call named no tool.
    pub result: Option<ToolResult>,
    /// Why the call couldn't be dispatched, if it couldn't.
    pub error: Option<String>,
}

/// Everything the server knows about a finished episode, as saved to its result file.
#[derive(Debug, Serialize)]
pub struct EpisodeResult {
    pub benchmark: String,
    #[serde(flatten)]
    pub key: EpisodeKey,
    /// How the attack addressed the model, if the episode was attacked.
    pub model_name: Option<String>,
    /// Which injection vectors the attack filled, if the episode was attacked.
    pub inject_vectors: Option<InjectVectors>,
    /// The text placed at each injection vector, by injection ID.
    pub injections: HashMap<String, String>,
    pub tool_calls: Vec<ToolCallRecord>,
    /// When the client read the environment's state with `get_environment`, each as the
    /// number of tool calls made before that read. `get_environment` exposes state the agent
    /// could only partly learn through tools, so each read is recorded to show which
    /// episodes had that access.
    pub environment_reads: Vec<usize>,
    /// The agent's final text answer (empty if it gave none).
    pub output: String,
    /// Whether the task was completed; `None` for an injection alone.
    pub utility: Option<bool>,
    /// Whether the injection's goal was achieved (for an attacked episode, whether the attack
    /// succeeded); `None` for a benign episode.
    pub security: Option<bool>,
    /// Seconds from `start` to `finish`.
    pub duration: f64,
    /// When the episode finished, in seconds since the Unix epoch.
    pub finished_at: f64,
    #[serde(flatten)]
    pub build: Build,
}

/// The on-disk form of [`BenchmarkConfig`].
#[derive(Serialize, Deserialize)]
struct ConfigFile {
    #[serde(flatten)]
    config: BenchmarkConfig,
    /// The build that registered the benchmark. Not checked when resuming, nor updated when
    /// suites or attacks are added; each result records the build that ran it.
    #[serde(flatten)]
    build: Build,
}

/// Checks that `name` can be used as a directory name: non-empty, and only ASCII letters,
/// digits, `-`, `_` and `.` (but not `.` or `..`).
pub fn validate_name(name: &str) -> Result<(), String> {
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.');
    if name.is_empty() || name == "." || name == ".." || !name.chars().all(allowed) {
        return Err(format!(
            "invalid benchmark name {name:?}: use only letters, digits, `-`, `_` and `.`"
        ));
    }
    Ok(())
}

fn config_path(logdir: &Path, name: &str) -> PathBuf {
    logdir.join(name).join("benchmark.json")
}

/// Records `config` in `logdir`, or, if the benchmark already exists, adds `config`'s suites
/// and attacks to it. Returns whether the benchmark already existed (so this resumes it).
///
/// Suites and attacks can be added because every attacked episode's result has its own file,
/// and the episodes without an attack don't depend on which attacks run, so they're shared.
/// The model name and injection vectors can't change: they change the text attacks inject,
/// but not where a result is saved, so results run with different ones would be mixed up.
/// Only `benchmark.json` and each result's own fields record them.
///
/// # Errors
///
/// Fails if the benchmark exists with a different model name or injection vectors, or on
/// I/O errors.
pub fn register(logdir: &Path, config: &BenchmarkConfig) -> Result<bool, String> {
    let path = config_path(logdir, &config.name);
    let dir = path.parent().expect("config path has a parent");
    fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;

    // Held until this returns, so of two clients registering the same benchmark at once, the
    // second sees what the first wrote and neither loses the other's suites or attacks.
    let lock_path = dir.join(".benchmark.lock");
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock_path)
        .map_err(|e| format!("could not open {}: {e}", lock_path.display()))?;
    lock.lock()
        .map_err(|e| format!("could not lock {}: {e}", lock_path.display()))?;

    if !path.exists() {
        let file = ConfigFile {
            config: config.clone(),
            build: Build::current(),
        };
        write_config(&path, &file)?;
        return Ok(false);
    }

    let mut file = load_config_file(logdir, &config.name)?;
    let existing = &file.config;
    if existing.model_name != config.model_name || existing.inject_vectors != config.inject_vectors
    {
        return Err(format!(
            "benchmark {:?} already exists with model name {:?} and injection vectors \"{}\", \
             not {:?} and \"{}\"; only suites and attacks can be added to a benchmark, so \
             choose another name",
            config.name,
            existing.model_name,
            existing.inject_vectors,
            config.model_name,
            config.inject_vectors
        ));
    }
    let add = |have: &mut Vec<String>, new: &[String]| {
        let before = have.len();
        for item in new {
            if !have.contains(item) {
                have.push(item.clone());
            }
        }
        have.len() != before
    };
    let added_suites = add(&mut file.config.suites, &config.suites);
    let added_attacks = add(&mut file.config.attacks, &config.attacks);
    if added_suites || added_attacks {
        write_config(&path, &file)?;
    }
    Ok(true)
}

/// Writes `file` to `path` under a temporary name and then renames it, so an episode
/// starting meanwhile never reads a half-written configuration.
fn write_config(path: &Path, file: &ConfigFile) -> Result<(), String> {
    let temp = path.with_extension(format!("json.tmp-{}", std::process::id()));
    let json = serde_json::to_string_pretty(file).expect("config serializes");
    fs::write(&temp, json).map_err(|e| format!("could not write {}: {e}", temp.display()))?;
    fs::rename(&temp, path).map_err(|e| format!("could not write {}: {e}", path.display()))
}

/// The configuration of the benchmark `name`.
///
/// # Errors
///
/// Fails if there is no such benchmark in `logdir` or its configuration can't be read.
pub fn load_config(logdir: &Path, name: &str) -> Result<BenchmarkConfig, String> {
    load_config_file(logdir, name).map(|file| file.config)
}

fn load_config_file(logdir: &Path, name: &str) -> Result<ConfigFile, String> {
    validate_name(name)?;
    let path = config_path(logdir, name);
    let json = fs::read_to_string(&path).map_err(|e| match e.kind() {
        ErrorKind::NotFound => format!("unknown benchmark: {name}; send `start_benchmark` first"),
        _ => format!("could not read {}: {e}", path.display()),
    })?;
    serde_json::from_str(&json).map_err(|e| format!("could not parse {}: {e}", path.display()))
}

/// The result file of `key` in the benchmark `benchmark`.
///
/// An injection alone is saved where AgentDojo saves it, as if the injection were a task:
/// `<suite>/<injection>/none/none.json`.
pub fn result_path(logdir: &Path, benchmark: &str, key: &EpisodeKey) -> PathBuf {
    let suite = logdir.join(benchmark).join(&key.suite);
    match (&key.task, &key.injection) {
        (Some(task), injection) => suite
            .join(task)
            .join(key.attack.as_deref().unwrap_or("none"))
            .join(format!("{}.json", injection.as_deref().unwrap_or("none"))),
        (None, Some(injection)) => suite.join(injection).join("none").join("none.json"),
        (None, None) => unreachable!("an episode has a task, an injection, or both"),
    }
}

/// Whether `key` already has a result in the benchmark: its file exists and records an
/// evaluation (`utility` and `security`, either of which may be `null`). A file that can't be
/// read or parsed doesn't count, so the episode is rerun.
pub fn is_complete(logdir: &Path, benchmark: &str, key: &EpisodeKey) -> bool {
    fs::read_to_string(result_path(logdir, benchmark, key))
        .ok()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(&json).ok())
        .and_then(|result| result.as_object().cloned())
        .is_some_and(|result| result.contains_key("utility") && result.contains_key("security"))
}

/// Saves `result` to its result file, replacing any earlier result for the same episode.
///
/// The file is written under a temporary name and then renamed, so a reader (or a crash)
/// never sees a half-written file, and two connections finishing the same episode at once
/// leave one complete file.
pub fn save(logdir: &Path, result: &EpisodeResult) -> io::Result<PathBuf> {
    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    let path = result_path(logdir, &result.benchmark, &result.key);
    fs::create_dir_all(path.parent().expect("result path has a parent"))?;
    let temp = path.with_extension(format!(
        "json.tmp-{}-{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let json = serde_json::to_string_pretty(result).expect("results serialize to JSON");
    fs::write(&temp, json)?;
    fs::rename(&temp, &path)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::{Build, ConfigFile};
    use crate::attacks::InjectVectors;

    /// A configuration saved before injection vectors were configurable loads as the default.
    #[test]
    fn configs_without_inject_vectors_use_the_default() {
        let json = r#"{"name": "old", "suites": ["slack"], "attacks": [], "model_name": null,
                       "paisho_version": "0.1.0"}"#;
        let file: ConfigFile = serde_json::from_str(json).unwrap();
        assert_eq!(file.config.inject_vectors, InjectVectors::InjectSeenOnly);
    }

    /// `build.rs` recorded either a full commit hash and whether it was dirty, or neither.
    #[test]
    fn build_records_the_commit() {
        let build = Build::current();
        assert_eq!(build.paisho_version, env!("CARGO_PKG_VERSION"));
        match &build.paisho_commit {
            Some(commit) => {
                assert_eq!(commit.len(), 40, "{commit}");
                assert!(commit.chars().all(|c| c.is_ascii_hexdigit()), "{commit}");
                assert!(build.paisho_dirty.is_some());
            }
            None => assert_eq!(build.paisho_dirty, None),
        }
    }
}
