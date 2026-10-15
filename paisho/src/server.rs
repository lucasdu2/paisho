//! The paisho server: runs episodes for clients that connect over a local socket.

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};

use interprocess::local_socket::{
    GenericFilePath, GenericNamespaced, ListenerOptions, Name, Stream, prelude::*,
};
use serde::{Deserialize, Serialize};

use crate::attacks::{
    Attack, DEFAULT_MODEL_NAME, ImportantInstructions, InjectVectors, ToolKnowledge, inject_at,
    injection_vectors,
};
use crate::environments::Environment;
use crate::suites::slack::SlackSuite;
use crate::suites::{
    Injection, Recording, Suite, SuiteEnvironment, Task, ToolCall, ToolError, ToolResult,
};

/// Name of the local socket the server listens on, unless given as the first argument.
pub const DEFAULT_SOCKET_NAME: &str = "paisho.sock";

// =============================================================================
// Protocol: newline-delimited JSON. The client sends one `Request` per line and
// the server answers each with one `Response` line.
// =============================================================================

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    /// Starts a new episode, replacing any episode in progress on this connection.
    /// With `injection_task` and `attack`, the environment is attacked; otherwise not.
    Start {
        /// The suite's AgentDojo name, such as `"slack"`.
        suite: String,
        user_task: String,
        injection_task: Option<String>,
        attack: Option<String>,
        /// How the attack addresses the model; defaults to AgentDojo's generic name.
        model_name: Option<String>,
    },
    /// Lists the tools of the current episode's environment.
    ListTools,
    /// Calls a tool, like the `params` of an MCP `tools/call` request.
    CallTool {
        name: String,
        #[serde(default)]
        arguments: serde_json::Map<String, serde_json::Value>,
    },
    /// Ends the current episode and evaluates it.
    Finish {
        /// The agent's final text answer, if it gave one.
        output: Option<String>,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Response {
    /// The episode has started; `prompt` is the user task to give the agent.
    Started {
        prompt: String,
        tools: Vec<serde_json::Value>,
    },
    Tools {
        tools: Vec<serde_json::Value>,
    },
    ToolResult {
        result: ToolResult,
    },
    /// `security` is `None` when the episode had no injection task.
    Finished {
        utility: bool,
        security: Option<bool>,
    },
    Error {
        message: String,
    },
}

// =============================================================================
// Episodes
// =============================================================================

/// One run of an agent on one user task of suite `S`, optionally attacked toward an
/// injection task.
struct Episode<S: Suite> {
    user_task: &'static dyn Task<Env = S::Env>,
    injection_task: Option<&'static dyn Injection<Env = S::Env>>,
    pre_env: S::Env,
    recording: Recording<S::Env>,
}

impl<S: Suite> Episode<S> {
    fn start(
        user_task: &str,
        injection_task: Option<&str>,
        attack: Option<&str>,
        model_name: Option<&str>,
    ) -> Result<Self, String> {
        let suite = S::NAME;
        let user_task = S::user_task(user_task)
            .ok_or_else(|| format!("unknown user task in suite {suite}: {user_task}"))?;
        let injection_task = injection_task
            .map(|id| {
                S::injection_task(id)
                    .ok_or_else(|| format!("unknown injection task in suite {suite}: {id}"))
            })
            .transpose()?;

        let injections = match (injection_task, attack) {
            (Some(injection_task), Some(attack)) => {
                let model_name = model_name.unwrap_or(DEFAULT_MODEL_NAME);
                // Recomputed for every episode. Cache these per user task and per injection
                // task if episodes become frequent enough for it to matter.
                let vectors = injection_vectors(user_task, InjectVectors::InjectSeenOnly);
                let texts = attack_texts(attack, model_name, injection_task)?;
                inject_at(&vectors, &texts)
            }
            (None, None) => HashMap::new(),
            (Some(_), None) => return Err("an injection task needs an attack".to_string()),
            (None, Some(_)) => return Err("an attack needs an injection task".to_string()),
        };

        let pre_env = S::Env::with_injections(&injections).map_err(|e| e.to_string())?;
        Ok(Episode {
            user_task,
            injection_task,
            recording: Recording::new(pre_env.clone()),
            pre_env,
        })
    }
}

/// What the server needs from an episode of any suite, so that a connection can hold an
/// episode whose suite is chosen at runtime.
trait RunningEpisode {
    fn prompt(&self) -> String;
    fn tool_schemas(&self) -> Vec<serde_json::Value>;
    fn dispatch(&mut self, call: &ToolCall) -> Result<ToolResult, ToolError>;
    /// Evaluates the episode, given the agent's final text answer.
    fn finish(self: Box<Self>, output: &str) -> Response;
}

impl<S: Suite> RunningEpisode for Episode<S> {
    fn prompt(&self) -> String {
        self.user_task.prompt()
    }

    fn tool_schemas(&self) -> Vec<serde_json::Value> {
        self.recording.tool_schemas()
    }

    fn dispatch(&mut self, call: &ToolCall) -> Result<ToolResult, ToolError> {
        self.recording.dispatch(call)
    }

    fn finish(self: Box<Self>, output: &str) -> Response {
        let post_env = &self.recording.env;
        let trace = &self.recording.trace;
        Response::Finished {
            utility: self
                .user_task
                .utility(output, &self.pre_env, post_env, trace),
            security: self
                .injection_task
                .map(|task| task.security(output, &self.pre_env, post_env, trace)),
        }
    }
}

/// Starts an episode of the suite named `suite`.
fn start_episode(
    suite: &str,
    user_task: &str,
    injection_task: Option<&str>,
    attack: Option<&str>,
    model_name: Option<&str>,
) -> Result<Box<dyn RunningEpisode>, String> {
    fn start<S: Suite>(
        user_task: &str,
        injection_task: Option<&str>,
        attack: Option<&str>,
        model_name: Option<&str>,
    ) -> Result<Box<dyn RunningEpisode>, String> {
        let episode = Episode::<S>::start(user_task, injection_task, attack, model_name)?;
        Ok(Box::new(episode))
    }

    match suite {
        SlackSuite::NAME => start::<SlackSuite>(user_task, injection_task, attack, model_name),
        _ => Err(format!("unknown suite: {suite}")),
    }
}

/// The text of `attack` for `injection_task` at every injection vector.
fn attack_texts<E: SuiteEnvironment>(
    attack: &str,
    model_name: &str,
    injection_task: &dyn Injection<Env = E>,
) -> Result<HashMap<String, String>, String> {
    match attack {
        "important_instructions" => {
            Ok(ImportantInstructions::new(model_name).injection_texts(injection_task))
        }
        "tool_knowledge" => Ok(ToolKnowledge::new(model_name).injection_texts(injection_task)),
        _ => Err(format!("unknown attack: {attack}")),
    }
}

// =============================================================================
// Server
// =============================================================================

/// Handles one request, updating the connection's current episode.
fn handle(request: Request, episode: &mut Option<Box<dyn RunningEpisode>>) -> Response {
    match request {
        Request::Start {
            suite,
            user_task,
            injection_task,
            attack,
            model_name,
        } => match start_episode(
            &suite,
            &user_task,
            injection_task.as_deref(),
            attack.as_deref(),
            model_name.as_deref(),
        ) {
            Ok(started) => {
                let response = Response::Started {
                    prompt: started.prompt(),
                    tools: started.tool_schemas(),
                };
                *episode = Some(started);
                response
            }
            Err(message) => Response::Error { message },
        },
        Request::ListTools => match episode {
            Some(episode) => Response::Tools {
                tools: episode.tool_schemas(),
            },
            None => no_episode(),
        },
        Request::CallTool { name, arguments } => match episode {
            Some(episode) => match episode.dispatch(&ToolCall { name, arguments }) {
                Ok(result) => Response::ToolResult { result },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            },
            None => no_episode(),
        },
        // AgentDojo evaluates a missing final answer as an empty one.
        Request::Finish { output } => match episode.take() {
            Some(episode) => episode.finish(output.as_deref().unwrap_or("")),
            None => no_episode(),
        },
    }
}

fn no_episode() -> Response {
    Response::Error {
        message: "no episode in progress; send a `start` request first".to_string(),
    }
}

/// Serves one client until it disconnects. Each connection has its own episode, and an
/// episode left unfinished when the client disconnects is discarded.
fn serve_connection(conn: Stream) -> io::Result<()> {
    let mut conn = BufReader::new(conn);
    let mut episode = None;
    let mut line = String::new();
    loop {
        line.clear();
        if conn.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let response = match serde_json::from_str::<Request>(&line) {
            Ok(request) => handle(request, &mut episode),
            Err(e) => Response::Error {
                message: format!("malformed request: {e}"),
            },
        };
        let mut out = serde_json::to_string(&response).expect("responses serialize to JSON");
        out.push('\n');
        conn.get_mut().write_all(out.as_bytes())?;
    }
}

/// The socket name: namespaced where supported (an abstract socket on Linux),
/// otherwise a file in the temporary directory.
fn socket_name(name: &str) -> io::Result<Name<'static>> {
    if GenericNamespaced::is_supported() {
        name.to_string().to_ns_name::<GenericNamespaced>()
    } else {
        std::env::temp_dir()
            .join(name)
            .to_fs_name::<GenericFilePath>()
    }
}

/// Listens on the local socket `name` and serves clients until the process is stopped.
pub fn run(name: &str) -> io::Result<()> {
    let listener = ListenerOptions::new()
        .name(socket_name(name)?)
        .try_overwrite(true)
        .create_sync()?;
    eprintln!("paisho listening on {name}");

    // One client at a time; a second client waits until the first disconnects.
    for conn in listener.incoming() {
        let result = conn.and_then(serve_connection);
        if let Err(e) = result {
            eprintln!("connection failed: {e}");
        }
    }
    Ok(())
}
