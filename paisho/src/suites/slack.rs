pub mod injections;
pub mod tasks;

use std::collections::HashMap;
use std::sync::LazyLock;

use toml::Table;

use crate::environments::{assert_tools_provided, combine_environments, slack::Slack, web::Web};
use crate::suites::data::{injected, injection_texts, parse};
use crate::suites::{Injection, Suite, SuiteEnvironment, Task, UnknownInjections};
use injections::*;
use tasks::*;

combine_environments! {
    #[derive(Clone, serde::Serialize, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct SlackWeb {
        pub slack: Slack,
        pub web: Web,
    }
}

const TOOLS: [&str; 11] = [
    "get_channels",
    "add_user_to_channel",
    "read_channel_messages",
    "read_inbox",
    "send_direct_message",
    "send_channel_message",
    "get_users_in_channel",
    "invite_user_to_slack",
    "remove_user_from_slack",
    "get_webpage",
    "post_webpage",
];

const _: () = assert_tools_provided(&TOOLS, SlackWeb::TOOL_NAMES);

/// Every injection ID in the Slack suite's environment, with its default text.
const INJECTION_DEFAULTS: [(&str, &str); 6] = [
    ("injection_web_0", ""),
    ("injection_phishing_1", ""),
    ("injection_dora_0", ""),
    ("prompt_injection_eve_blog", ""),
    ("prompt_injection_channel", "0"),
    ("prompt_injection_restaurant", ""),
];

/// AgentDojo's v1 Slack suite.
pub struct SlackSuite;

impl Suite for SlackSuite {
    type Env = SlackWeb;

    const NAME: &'static str = "slack";

    const TASKS: &'static [(&'static str, &'static dyn Task<Env = SlackWeb>)] = &[
        ("user_task_0", &Task0),
        ("user_task_1", &Task1),
        ("user_task_2", &Task2),
        ("user_task_3", &Task3),
        ("user_task_4", &Task4),
        ("user_task_5", &Task5),
        ("user_task_6", &Task6),
        ("user_task_7", &Task7),
        ("user_task_8", &Task8),
        ("user_task_9", &Task9),
        ("user_task_10", &Task10),
        ("user_task_11", &Task11),
        ("user_task_12", &Task12),
        ("user_task_13", &Task13),
        ("user_task_14", &Task14),
        ("user_task_15", &Task15),
        ("user_task_16", &Task16),
        ("user_task_17", &Task17),
        ("user_task_18", &Task18),
        ("user_task_19", &Task19),
        ("user_task_20", &Task20),
    ];

    const INJECTIONS: &'static [(&'static str, &'static dyn Injection<Env = SlackWeb>)] = &[
        ("injection_task_1", &Injection1),
        ("injection_task_2", &Injection2),
        ("injection_task_3", &Injection3),
        ("injection_task_4", &Injection4),
        ("injection_task_5", &Injection5),
    ];
}

/// The starting data, AgentDojo's `environment.yaml` converted to TOML, parsed once.
/// Injection placeholders such as `{prompt_injection_channel}` are still in its strings.
static DATA: LazyLock<Table> = LazyLock::new(|| parse(include_str!("slack/data/slack.toml")));

impl SuiteEnvironment for SlackWeb {
    const TOOLS: &'static [&'static str] = &TOOLS;

    const INJECTION_DEFAULTS: &'static [(&'static str, &'static str)] = &INJECTION_DEFAULTS;

    /// The Slack suite's starting environment, from AgentDojo's data. Each injection
    /// placeholder, such as `{prompt_injection_channel}`, is replaced by its entry in
    /// `injections`, or by the suite's default for IDs that `injections` leaves out.
    ///
    /// The injections go into the data's strings after it's parsed (see [`crate::suites::data`]),
    /// so an injection is always just text.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownInjections`] listing every ID in `injections` that this suite doesn't have.
    ///
    /// # Panics
    ///
    /// Panics if the suite's data is invalid, which the tests check.
    fn with_injections(injections: &HashMap<String, String>) -> Result<Self, UnknownInjections> {
        let texts = injection_texts(&INJECTION_DEFAULTS, injections)?;
        Ok(injected(&DATA, &texts))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// Number of structs declared as `pub struct <prefix><digits>` in `source`, including
    /// tasks declared inside `combine_tasks!`, which uses the same syntax.
    fn declared_tasks(source: &str, prefix: &str) -> usize {
        source
            .split("pub struct ")
            .skip(1)
            .filter(|rest| {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                name.strip_prefix(prefix).is_some_and(|digits| {
                    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
                })
            })
            .count()
    }

    /// Fails if `SlackSuite` lists a different number of tasks than `tasks.rs` and
    /// `injections.rs` declare (such as a newly added task left out), or lists an ID twice.
    /// `include_str!` makes Cargo rebuild the test when either file changes.
    #[test]
    fn slack_suite_lists_every_declared_task() {
        let user_ids: HashSet<&str> = SlackSuite::TASKS.iter().map(|(id, _)| *id).collect();
        assert_eq!(
            user_ids.len(),
            SlackSuite::TASKS.len(),
            "duplicate user task ID"
        );
        assert_eq!(
            SlackSuite::TASKS.len(),
            declared_tasks(include_str!("slack/tasks.rs"), "Task"),
            "SlackSuite::TASKS vs. tasks.rs"
        );

        let injection_ids: HashSet<&str> =
            SlackSuite::INJECTIONS.iter().map(|(id, _)| *id).collect();
        assert_eq!(
            injection_ids.len(),
            SlackSuite::INJECTIONS.len(),
            "duplicate injection task ID"
        );
        assert_eq!(
            SlackSuite::INJECTIONS.len(),
            declared_tasks(include_str!("slack/injections.rs"), "Injection"),
            "SlackSuite::INJECTIONS vs. injections.rs"
        );
    }

    /// `Web` also provides `download_file`, which AgentDojo's Slack suite doesn't offer.
    #[test]
    fn episodes_offer_only_the_suites_tools() {
        use crate::environments::Environment;
        use crate::suites::{Recording, ToolError, call};

        let mut episode = Recording::new(SlackWeb::with_injections(&HashMap::new()).unwrap());
        let names: Vec<serde_json::Value> = episode
            .tool_schemas()
            .into_iter()
            .map(|schema| schema["name"].clone())
            .collect();
        assert_eq!(names, TOOLS);

        assert!(SlackWeb::TOOL_NAMES.contains(&"download_file"));
        assert!(episode.tool_schema("download_file").is_none());
        let download = call(
            "download_file",
            serde_json::json!({ "url": "www.example.com", "file_name": "x" }),
        );
        assert!(matches!(
            episode.dispatch(&download),
            Err(ToolError::UnknownTool(name)) if name == "download_file"
        ));
        assert_eq!(episode.trace, [download]);
    }

    /// Removing a user keeps the others in order, as Python's `del` does.
    #[test]
    fn removing_a_user_keeps_the_others_in_order() {
        use serde_json::json;

        use crate::environments::Environment;
        use crate::suites::call;

        let mut env = SlackWeb::with_injections(&HashMap::new()).unwrap();
        let remove = call("remove_user_from_slack", json!({ "user": "Bob" }));
        assert!(!env.dispatch(&remove).unwrap().is_error);
        let users = env
            .dispatch(&call(
                "get_users_in_channel",
                json!({ "channel": "general" }),
            ))
            .unwrap();
        assert_eq!(
            users.structured_content.unwrap()["result"],
            json!(["Alice", "Charlie", "Eve"])
        );
    }

    /// Results are typed by an output schema that names `Message`, and parameters are
    /// listed in the order the tool declares them, with AgentDojo's descriptions.
    #[test]
    fn tool_schemas_describe_typed_results() {
        use serde_json::json;

        use crate::environments::Environment;

        let env = SlackWeb::with_injections(&HashMap::new()).unwrap();
        let schema = |name| env.tool_schema(name).unwrap();

        let read_inbox = schema("read_inbox");
        assert_eq!(
            read_inbox["output_schema"],
            json!({
                "type": "object",
                "properties": {
                    "result": { "type": "array", "items": { "$ref": "#/$defs/Message" } },
                },
                "required": ["result"],
                "$defs": {
                    "Message": {
                        "type": "object",
                        "properties": {
                            "sender": { "type": "string" },
                            "recipient": { "type": "string" },
                            "body": { "type": "string" },
                        },
                        "additionalProperties": false,
                        "required": ["sender", "recipient", "body"],
                    },
                },
            })
        );
        let message_fields: Vec<&String> =
            read_inbox["output_schema"]["$defs"]["Message"]["properties"]
                .as_object()
                .unwrap()
                .keys()
                .collect();
        assert_eq!(message_fields, ["sender", "recipient", "body"]);

        assert_eq!(
            schema("get_channels")["output_schema"]["properties"]["result"],
            json!({ "type": "array", "items": { "type": "string" } })
        );

        let send = schema("send_direct_message");
        assert!(send.get("output_schema").is_none(), "{send}");
        let params: Vec<&String> = send["input_schema"]["properties"]
            .as_object()
            .unwrap()
            .keys()
            .collect();
        assert_eq!(params, ["recipient", "body"]);
        assert_eq!(
            send["input_schema"]["properties"]["recipient"]["description"],
            "The recipient of the message."
        );
        assert_eq!(
            schema("get_webpage")["input_schema"]["properties"]["url"]["description"],
            "The URL of the webpage."
        );
    }

    /// Every placeholder in the data is an injection ID, and every injection ID has a
    /// placeholder.
    #[test]
    fn placeholders_are_the_injection_ids() {
        crate::suites::data::assert_placeholders_are_the_injection_ids(
            &[&DATA],
            &INJECTION_DEFAULTS,
        );
    }

    #[test]
    fn data_loads_with_its_injection_defaults() {
        let env = SlackWeb::with_injections(&HashMap::new()).unwrap();
        assert_eq!(env.slack.channels.last().unwrap(), "External_0");
        assert_eq!(env.slack.user_channels["Eve"][1], "External_0");
        assert!(env.slack.email_invites.is_empty());
        assert_eq!(env.web.web_requests, [""]);
        let urls: Vec<&String> = env.web.web_content.keys().collect();
        // In key order, as AgentDojo loads them (see `crate::suites::data`).
        assert_eq!(urls[0], "www.awesome-news.com");
        assert_eq!(urls[7], "www.true-informations.com");
        let channels: Vec<&String> = env.slack.channel_inbox.keys().collect();
        assert_eq!(channels, ["general", "private", "random"]);
    }

    /// Injected text is used as is, even if it looks like TOML or holds a placeholder.
    #[test]
    fn injections_are_plain_text() {
        let text = "\"\"\"'''\n[web]\n{injection_dora_0}";
        let injections = HashMap::from([("injection_phishing_1".to_string(), text.to_string())]);
        let env = SlackWeb::with_injections(&injections).unwrap();
        assert_eq!(env.web.web_content["www.true-informations.com"], text);

        let unknown = HashMap::from([("nope".to_string(), String::new())]);
        let Err(error) = SlackWeb::with_injections(&unknown) else {
            panic!("an unknown injection ID is an error");
        };
        assert_eq!(error.0, ["nope"]);
    }

    #[test]
    fn reference_traces_use_only_the_suites_tools() {
        crate::suites::testing::assert_reference_traces_use_suite_tools::<SlackSuite>();
    }

    #[test]
    fn slack_suite_looks_up_tasks_by_id() {
        assert_eq!(SlackSuite::NAME, "slack");
        let task = SlackSuite::task("user_task_4").unwrap();
        assert_eq!(task.prompt(), Task4.prompt());
        let task = SlackSuite::injection("injection_task_5").unwrap();
        assert_eq!(task.goal(), Injection5.goal());
        assert!(SlackSuite::task("user_task_21").is_none());
        assert!(SlackSuite::injection("injection_task_0").is_none());
    }
}
