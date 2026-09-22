use std::collections::HashMap;

struct Message {
    sender: String,
    recipient: String,
    body: String,
}

struct Slack {
    users: Vec<String>,
    channels: Vec<String>,
    user_channels: HashMap<String, Vec<String>>,
    user_inbox: HashMap<String, Vec<Message>>,
    channel_inbox: HashMap<String, Vec<Message>>,
}

impl Slack {
    /// Get the list of channels in the slack.
    fn get_channels(&self) -> Vec<String> {
        self.channels.clone()
    }

    /// Add a user to a given channel.
    fn add_user_to_channel(&mut self, user: &str, channel: &str) -> Result<(), String> {
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
    fn read_channel_messages(&self, channel: &str) -> Result<Vec<&Message>, String> {
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
    fn read_inbox(&self, user: &str) -> Result<Vec<&Message>, String> {
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
    fn send_direct_message(&mut self, recipient: &str, body: &str) -> Result<(), String> {
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
    fn send_channel_message(&mut self, channel: &str, body: &str) -> Result<(), String> {
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
    fn invite_user_to_slack(&mut self, user: &str, _user_email: &str) -> Result<(), String> {
        if self.users.iter().any(|u| u == user) {
            return Err(format!("User {user} already in the users list"));
        }
        self.users.push(user.to_string());
        self.user_inbox.insert(user.to_string(), Vec::new());
        self.user_channels.insert(user.to_string(), Vec::new());
        Ok(())
    }

    /// Remove a user from the Slack workspace.
    fn remove_user_from_slack(&mut self, user: &str) -> Result<(), String> {
        if !self.users.iter().any(|u| u == user) {
            return Err(format!("User {user} not found in the users list"));
        }
        self.users.retain(|u| u != user);
        self.user_inbox.remove(user);
        self.user_channels.remove(user);
        Ok(())
    }

    /// Get the list of users in the given channel.
    fn get_users_in_channel(&self, channel: &str) -> Result<Vec<&str>, String> {
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
