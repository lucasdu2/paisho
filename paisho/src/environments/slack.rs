use indexmap::IndexMap;

use paisho_macros::tools;

#[derive(Debug, Clone, serde::Serialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct Message {
    pub(crate) sender: String,
    pub(crate) recipient: String,
    pub(crate) body: String,
}

/// An invitation sent by `invite_user_to_slack`.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct EmailInvite {
    pub(crate) user: String,
    pub(crate) user_email: String,
}

#[derive(Clone, serde::Serialize)]
pub struct Slack {
    pub(crate) users: Vec<String>,
    pub(crate) channels: Vec<String>,
    pub(crate) user_channels: IndexMap<String, Vec<String>>,
    pub(crate) user_inbox: IndexMap<String, Vec<Message>>,
    pub(crate) channel_inbox: IndexMap<String, Vec<Message>>,
    pub(crate) email_invites: Vec<EmailInvite>,
}

#[tools]
impl Slack {
    /// Get the list of channels in the slack.
    fn get_channels(&self) -> Result<Vec<String>, String> {
        Ok(self.channels.clone())
    }

    /// Add a user to a given channel.
    fn add_user_to_channel(
        &mut self,
        #[describe = "The user to add to the channel."] user: &str,
        #[describe = "The channel to add the user to."] channel: &str,
    ) -> Result<(), String> {
        if !self.users.iter().any(|u| u == user) {
            return Err(format!("User {user} not found in the users list"));
        }
        if !self.channels.iter().any(|c| c == channel) {
            return Err(format!("Channel {channel} not found in the channels list"));
        }
        self.user_channels
            .get_mut(user)
            .expect("user_channels missing entry for existing user")
            .push(channel.to_string());
        Ok(())
    }

    /// Read the messages from the given channel.
    fn read_channel_messages(
        &self,
        #[describe = "The channel to read the messages from."] channel: &str,
    ) -> Result<Vec<&Message>, String> {
        if !self.channels.iter().any(|c| c == channel) {
            return Err("Channel does not exist!".to_string());
        }
        Ok(self
            .channel_inbox
            .get(channel)
            .map(|msgs| msgs.iter().collect())
            .unwrap_or_default())
    }

    /// Read the messages from the given user inbox.
    fn read_inbox(
        &self,
        #[describe = "The user whose inbox to read."] user: &str,
    ) -> Result<Vec<&Message>, String> {
        if !self.users.iter().any(|u| u == user) {
            return Err(format!("User {user} not found in the users list"));
        }
        Ok(self
            .user_inbox
            .get(user)
            .map(|msgs| msgs.iter().collect())
            .unwrap_or_default())
    }

    /// Send a direct message from the bot to `recipient` with the given `body`.
    fn send_direct_message(
        &mut self,
        #[describe = "The recipient of the message."] recipient: &str,
        #[describe = "The body of the message."] body: &str,
    ) -> Result<(), String> {
        let sender = "bot";
        if !self.users.iter().any(|u| u == recipient) {
            return Err(format!("Recipient {recipient} not found in the users list"));
        }
        let msg = Message {
            sender: sender.to_string(),
            recipient: recipient.to_string(),
            body: body.to_string(),
        };
        self.user_inbox
            .entry(recipient.to_string())
            .or_default()
            .push(msg);
        Ok(())
    }

    /// Send a channel message from the bot to `channel` with the given `body`.
    fn send_channel_message(
        &mut self,
        #[describe = "The channel to send the message to."] channel: &str,
        #[describe = "The body of the message."] body: &str,
    ) -> Result<(), String> {
        let sender = "bot";
        if !self.channels.iter().any(|c| c == channel) {
            return Err(format!("Channel {channel} not found in the channels list"));
        }
        let msg = Message {
            sender: sender.to_string(),
            recipient: channel.to_string(),
            body: body.to_string(),
        };
        self.channel_inbox
            .entry(channel.to_string())
            .or_default()
            .push(msg);
        Ok(())
    }

    /// Invites a user to the Slack workspace.
    fn invite_user_to_slack(
        &mut self,
        #[describe = "The user to invite."] user: &str,
        #[describe = "The user email where invite should be sent."] user_email: &str,
    ) -> Result<(), String> {
        if self.users.iter().any(|u| u == user) {
            return Err(format!("User {user} already in the users list"));
        }
        self.users.push(user.to_string());
        self.user_inbox.insert(user.to_string(), Vec::new());
        self.user_channels.insert(user.to_string(), Vec::new());
        self.email_invites.push(EmailInvite {
            user: user.to_string(),
            user_email: user_email.to_string(),
        });
        Ok(())
    }

    /// Remove a user from the Slack workspace.
    fn remove_user_from_slack(
        &mut self,
        #[describe = "The user to remove."] user: &str,
    ) -> Result<(), String> {
        if !self.users.iter().any(|u| u == user) {
            return Err(format!("User {user} not found in the users list"));
        }
        self.users.retain(|u| u != user);
        self.user_inbox.shift_remove(user);
        self.user_channels.shift_remove(user);
        Ok(())
    }

    /// Get the list of users in the given channel.
    fn get_users_in_channel(
        &self,
        #[describe = "The channel to get the users from."] channel: &str,
    ) -> Result<Vec<&str>, String> {
        if !self.channels.iter().any(|c| c == channel) {
            return Err(format!("Channel {channel} not found in the channels list"));
        }
        Ok(self
            .user_channels
            .iter()
            .filter(|(_, channels)| channels.iter().any(|c| c == channel))
            .map(|(user, _)| user.as_str())
            .collect())
    }
}
