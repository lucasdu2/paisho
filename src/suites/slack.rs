pub mod injections;
pub mod tasks;

use crate::environments::{assert_tools_provided, combine_environments, slack::Slack, web::Web};
combine_environments! {
    #[derive(Clone)]
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
