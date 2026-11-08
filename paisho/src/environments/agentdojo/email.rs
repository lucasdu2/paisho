use std::collections::HashSet;

use indexmap::IndexMap;

use chrono::NaiveDateTime;
use paisho_macros::tools;
use serde_json::Value;

use crate::environments::agentdojo::calendar::CalendarEvent;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub(crate) enum EmailStatus {
    #[default]
    Sent,
    Received,
    Draft,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub(crate) struct EmailContact {
    /// The email of the contact
    pub(crate) email: String,
    /// The name of the contact
    pub(crate) name: String,
}

/// An email attachment: the ID of a file in the cloud drive, or a calendar event.
// Untagged, so it is written as AgentDojo writes it: a plain string or an event object.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub(crate) enum Attachment {
    File(String),
    Event(CalendarEvent),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub(crate) struct Email {
    /// The unique identifier of the email
    pub(crate) id: String,
    /// The email of the sender
    pub(crate) sender: String,
    /// The list of the emails of the recipients
    pub(crate) recipients: Vec<String>,
    /// The list of the emails of the CC recipients
    #[serde(default)]
    pub(crate) cc: Vec<String>,
    /// The list of the emails of the BCC recipients
    #[serde(default)]
    pub(crate) bcc: Vec<String>,
    /// The subject of the email
    pub(crate) subject: String,
    /// The body of the email
    pub(crate) body: String,
    /// The status of the email
    #[serde(default)]
    pub(crate) status: EmailStatus,
    /// Whether the email has been read
    #[serde(default)]
    pub(crate) read: bool,
    /// The timestamp of the email
    #[serde(
        default = "crate::environments::now",
        deserialize_with = "crate::environments::deserialize_date_time"
    )]
    #[schemars(transform = remove_default)]
    pub(crate) timestamp: NaiveDateTime,
    #[schemars(
        description = "The list of attachments of the email. If the attachment is a file, it \
                       will be the ID of the file. If the attachment is a calendar event, it \
                       will be a CalendarEvent object."
    )]
    #[serde(default)]
    pub(crate) attachments: Vec<Attachment>,
}

/// Leaves a field's default out of its schema, for a default that changes from call to call
/// (such as the current time), so that the schema stays the same.
fn remove_default(schema: &mut schemars::Schema) {
    schema.remove("default");
}

/// The user's mailbox.
/// An inbox as a suite's data gives it, before [`Inbox::new`] indexes its emails.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InboxData {
    account_email: String,
    initial_emails: Vec<Email>,
    #[serde(default)]
    contact_list: Vec<EmailContact>,
}

impl TryFrom<InboxData> for Inbox {
    type Error = String;

    fn try_from(data: InboxData) -> Result<Self, String> {
        data.into_inbox()
    }
}

impl InboxData {
    /// # Errors
    ///
    /// Returns an error if two of the emails share an ID.
    pub(crate) fn into_inbox(self) -> Result<Inbox, String> {
        Inbox::new(self.account_email, self.initial_emails, self.contact_list)
    }
}

/// Deserialized from [`InboxData`], so that a suite's data can hold inboxes directly.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "InboxData")]
pub(crate) struct Inbox {
    pub(crate) account_email: String,
    pub(crate) initial_emails: Vec<Email>,
    pub(crate) emails: IndexMap<String, Email>,
    pub(crate) contact_list: Vec<EmailContact>,
    pub(crate) trash: IndexMap<String, Email>,
}

impl Inbox {
    /// An inbox holding `initial_emails`, with `contact_list` extended by every address in
    /// them that it doesn't have yet, as AgentDojo's validators do.
    ///
    /// # Errors
    ///
    /// Returns an error if two of `initial_emails` share an ID.
    pub(crate) fn new(
        account_email: String,
        initial_emails: Vec<Email>,
        mut contact_list: Vec<EmailContact>,
    ) -> Result<Self, String> {
        let mut emails = IndexMap::new();
        let mut duplicate_ids = Vec::new();
        for email in &initial_emails {
            if emails.insert(email.id.clone(), email.clone()).is_some() {
                duplicate_ids.push(email.id.clone());
            }
        }
        if !duplicate_ids.is_empty() {
            return Err(format!(
                "Email IDs must be unique. Duplicates: {duplicate_ids:?}"
            ));
        }

        let mut existing_contacts = HashSet::new();
        for email in &initial_emails {
            let addresses = email
                .recipients
                .iter()
                .chain([&email.sender])
                .chain(&email.cc)
                .chain(&email.bcc);
            for address in addresses {
                if existing_contacts.contains(address) {
                    continue;
                }
                if !contact_list.iter().any(|contact| &contact.email == address) {
                    let name = address.split('@').next().unwrap_or_default();
                    contact_list.push(EmailContact {
                        email: address.clone(),
                        name: title_case(&name.replace('.', " ")),
                    });
                    existing_contacts.insert(address.clone());
                }
            }
        }

        Ok(Inbox {
            account_email,
            initial_emails,
            emails,
            contact_list,
            trash: IndexMap::new(),
        })
    }

    /// Emails whose sender is `address`, ignoring case.
    pub(crate) fn from_address(&self, address: &str) -> Vec<&Email> {
        let address = address.to_lowercase();
        self.emails
            .values()
            .filter(|email| email.sender.to_lowercase() == address)
            .collect()
    }

    fn emails_by_status(&self, status: EmailStatus) -> Vec<&Email> {
        self.emails
            .values()
            .filter(|email| email.status == status)
            .collect()
    }

    pub(crate) fn received(&self) -> Vec<&Email> {
        self.emails_by_status(EmailStatus::Received)
    }

    pub(crate) fn sent(&self) -> Vec<&Email> {
        self.emails_by_status(EmailStatus::Sent)
    }

    pub(crate) fn drafts(&self) -> Vec<&Email> {
        self.emails_by_status(EmailStatus::Draft)
    }

    /// Sends an email from the account, as AgentDojo's `Inbox.send_email` method does, and
    /// returns it. Other environments send email through this, as the calendar does.
    pub(crate) fn send(
        &mut self,
        recipients: Vec<String>,
        subject: &str,
        body: &str,
        attachments: Vec<Attachment>,
        cc: Vec<String>,
        bcc: Vec<String>,
    ) -> Result<Email, String> {
        let email = Email {
            id: crate::environments::next_id(self.emails.keys())?,
            sender: self.account_email.clone(),
            recipients,
            cc,
            bcc,
            subject: subject.to_string(),
            body: body.to_string(),
            status: EmailStatus::Sent,
            read: true,
            timestamp: crate::environments::now(),
            attachments,
        };
        self.emails.insert(email.id.clone(), email.clone());
        Ok(email)
    }

    /// Receives an email from `sender`, addressed to the account, as AgentDyn's
    /// `DynamicInbox.receive_email` does, and returns it. It is unread.
    pub(crate) fn receive(
        &mut self,
        sender: &str,
        subject: &str,
        body: &str,
        attachments: Vec<Attachment>,
        cc: Vec<String>,
        bcc: Vec<String>,
    ) -> Result<Email, String> {
        let email = Email {
            id: crate::environments::next_id(self.emails.keys())?,
            sender: sender.to_string(),
            recipients: vec![self.account_email.clone()],
            cc,
            bcc,
            subject: subject.to_string(),
            body: body.to_string(),
            status: EmailStatus::Received,
            read: false,
            timestamp: crate::environments::now(),
            attachments,
        };
        self.emails.insert(email.id.clone(), email.clone());
        Ok(email)
    }

    /// Emails whose subject or body contains `query`, ignoring case, only those from
    /// `sender` if given.
    pub(crate) fn search(&self, query: &str, sender: Option<&str>) -> Vec<&Email> {
        let emails = match sender {
            Some(sender) => self.from_address(sender),
            None => self.emails.values().collect(),
        };
        let query = query.to_lowercase();
        emails
            .into_iter()
            .filter(|email| {
                email.subject.to_lowercase().contains(&query)
                    || email.body.to_lowercase().contains(&query)
            })
            .collect()
    }

    /// The received email with the latest timestamp, or `None` if there are none.
    pub(crate) fn most_recently_received_email(&self) -> Option<&Email> {
        self.received()
            .into_iter()
            .max_by_key(|email| email.timestamp)
    }
}

#[tools]
impl Inbox {
    /// Returns all the unread emails in the inbox. Each email has a sender, a subject, and a
    /// body. The emails are marked as read after this function is called.
    fn get_unread_emails(&mut self) -> Result<Vec<Email>, String> {
        let mut unread = Vec::new();
        for email in self.emails.values_mut().filter(|email| !email.read) {
            email.read = true;
            unread.push(email.clone());
        }
        Ok(unread)
    }

    /// Sends an email with the given `body` to the given `address`. Returns a dictionary with
    /// the email details.
    fn send_email(
        &mut self,
        #[describe = "The list with the email addresses of the recipients."] recipients: Vec<
            String,
        >,
        #[describe = "The subject of the email."] subject: &str,
        #[describe = "The body of the email."] body: &str,
        #[describe = "The list of attachments to include in the email. If `null`, no \
                      attachments are included. If the attachment has as \"type\" \"file\", \
                      then it is expected to have a field \"file_id\", with the ID of the file \
                      in the cloud drive. If the attachment has as \"type\" \"event\", then it \
                      is expected to be a calendar event in the field \"event_details\". A \
                      calendar event has the following fields: `title`, `description`, \
                      `start_time` (in ISO format), `end_time` (in ISO format), `location`, \
                      and participants (a list of emails)."]
        attachments: Option<Vec<Value>>,
        #[describe = "The list of email addresses to include in the CC field. If `null`, no \
                      email addresses are included."]
        cc: Option<Vec<String>>,
        #[describe = "The list of email addresses to include in the BCC field. If `null`, no \
                      email addresses are included."]
        bcc: Option<Vec<String>>,
    ) -> Result<Email, String> {
        let attachments = attachments
            .unwrap_or_default()
            .iter()
            .map(parse_attachment)
            .collect::<Result<Vec<_>, _>>()?;
        self.send(
            recipients,
            subject,
            body,
            attachments,
            cc.unwrap_or_default(),
            bcc.unwrap_or_default(),
        )
    }

    /// Searches for emails in the inbox that contain the given query in the subject or body.
    /// If `address` is provided, only emails from that address are searched.
    fn search_emails(
        &self,
        #[describe = "The query to search for in the email subject or body. If empty, all \
                      emails are returned."]
        query: &str,
        #[describe = "The email address of the sender. If `null`, all emails are searched. \
                      Use `search_contacts_by_name` if the email address is unknown."]
        sender: Option<String>,
    ) -> Result<Vec<&Email>, String> {
        let matches = self.search(query, sender.as_deref());
        if matches.is_empty() {
            return Err("No emails found. Try with a different query.".to_string());
        }
        Ok(matches)
    }

    /// Deletes the email with the given `email_id` from the inbox.
    fn delete_email(
        &mut self,
        #[describe = "The id of the email to delete."] email_id: &str,
    ) -> Result<String, String> {
        let Some(email) = self.emails.shift_remove(email_id) else {
            return Err(format!("Email with ID '{email_id}' not found."));
        };
        self.trash.insert(email_id.to_string(), email);
        Ok(format!("Email with id {email_id} deleted successfully."))
    }

    /// Returns all the sent emails in the inbox. Each email has a recipient, a subject, and a
    /// body.
    fn get_sent_emails(&self) -> Result<Vec<&Email>, String> {
        Ok(self.sent())
    }

    /// Returns all the received emails in the inbox. Each email has a sender, a subject, and
    /// a body.
    fn get_received_emails(&self) -> Result<Vec<&Email>, String> {
        Ok(self.received())
    }

    /// Returns all the draft emails in the inbox. Each email has a recipient, a subject, and
    /// a body.
    fn get_draft_emails(&self) -> Result<Vec<&Email>, String> {
        Ok(self.drafts())
    }

    /// Finds contacts in the inbox's contact list by name. It returns a list of contacts that
    /// match the given name.
    fn search_contacts_by_name(
        &self,
        #[describe = "The name of the contacts to search for."] query: &str,
    ) -> Result<Vec<&EmailContact>, String> {
        let name = query.to_lowercase();
        let matches: Vec<&EmailContact> = self
            .contact_list
            .iter()
            .filter(|contact| contact.name.to_lowercase().contains(&name))
            .collect();
        if matches.is_empty() {
            return Err(format!("Contact with name '{query}' not found."));
        }
        Ok(matches)
    }

    /// Finds contacts in the inbox's contact list by email. It returns a list of contacts
    /// that match the given email.
    fn search_contacts_by_email(
        &self,
        #[describe = "The email of the contacts to search for."] query: &str,
    ) -> Result<Vec<&EmailContact>, String> {
        let email = query.to_lowercase();
        let matches: Vec<&EmailContact> = self
            .contact_list
            .iter()
            .filter(|contact| contact.email.to_lowercase().contains(&email))
            .collect();
        if matches.is_empty() {
            return Err(format!("Contact with email '{query}' not found."));
        }
        Ok(matches)
    }
}

/// Parses one of `send_email`'s attachments: a `"file"` is the ID of a cloud drive file in
/// `"file_id"`, and an `"event"` is a calendar event in `"event_details"`.
///
/// AgentDojo is more lenient: anything with a `"file_id"` is a file whatever its `"type"`,
/// and any other `"type"` is an event. Here an unknown type is an error, so the agent learns
/// which types exist.
fn parse_attachment(attachment: &Value) -> Result<Attachment, String> {
    let Value::Object(attachment) = attachment else {
        return Err("Attachments must be dictionaries.".to_string());
    };
    let Some(kind) = attachment.get("type") else {
        return Err("Attachment must have a 'type' field.".to_string());
    };
    match kind.as_str() {
        Some("file") => {
            let Some(file_id) = attachment.get("file_id") else {
                return Err("Attachment of type 'file' must have a 'file_id' field.".to_string());
            };
            // Like Python's `str(file_id)`: a string as is, anything else as its JSON text.
            let file_id = match file_id {
                Value::String(file_id) => file_id.clone(),
                other => other.to_string(),
            };
            Ok(Attachment::File(file_id))
        }
        Some("event") => {
            let Some(details) = attachment.get("event_details") else {
                return Err(
                    "Attachment of type 'event' must have an 'event_details' field.".to_string(),
                );
            };
            serde_json::from_value(details.clone())
                .map(Attachment::Event)
                .map_err(|e| format!("Invalid calendar event: {e}"))
        }
        _ => Err(format!(
            "Unknown attachment type {kind}; expected \"file\" or \"event\"."
        )),
    }
}

/// Like Python's `str.title`: the first letter of each run of letters is uppercased and the
/// rest lowercased.
fn title_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_word = false;
    for c in text.chars() {
        if c.is_alphabetic() {
            if in_word {
                out.extend(c.to_lowercase());
            } else {
                out.extend(c.to_uppercase());
            }
            in_word = true;
        } else {
            out.push(c);
            in_word = false;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::suites::{ToolContent, ToolResult, call};

    fn email(value: Value) -> Email {
        serde_json::from_value(value).unwrap()
    }

    /// An inbox for `emma.johnson@bluesparrowtech.com` with a received, a sent and an unread
    /// email, like the start of AgentDojo's workspace inbox.
    fn inbox() -> Inbox {
        let emails = vec![
            email(json!({
                "id": "0", "sender": "lily.white@gmail.com",
                "recipients": ["emma.johnson@bluesparrowtech.com"],
                "subject": "Birthday Party", "body": "Come to John's party!",
                "status": "received", "read": true, "timestamp": "2024-05-14T11:00:00",
            })),
            email(json!({
                "id": "1", "sender": "emma.johnson@bluesparrowtech.com",
                "recipients": ["david.smith@bluesparrowtech.com"],
                "cc": ["julie.williams@bluesparrowtech.com"],
                "subject": "Project Update", "body": "Any news on Phoenix?",
                "status": "sent", "read": true, "timestamp": "2024-05-12T09:15:00",
            })),
            email(json!({
                "id": "2", "sender": "David.Smith@bluesparrowtech.com",
                "recipients": ["emma.johnson@bluesparrowtech.com"],
                "subject": "Re: Project Update", "body": "Phoenix is on track.",
                "status": "received", "timestamp": "2024-05-15T08:00:00",
            })),
        ];
        Inbox::new(
            "emma.johnson@bluesparrowtech.com".to_string(),
            emails,
            Vec::new(),
        )
        .unwrap()
    }

    fn run(inbox: &mut Inbox, name: &str, arguments: Value) -> ToolResult {
        inbox.dispatch(&call(name, arguments)).unwrap()
    }

    fn text(result: &ToolResult) -> &str {
        let ToolContent::Text { text } = &result.content[0];
        text
    }

    /// The IDs of the emails in `result`, in order.
    fn ids(result: &ToolResult) -> Vec<String> {
        let emails = &result.structured_content.as_ref().unwrap()["result"];
        emails
            .as_array()
            .unwrap()
            .iter()
            .map(|email| email["id"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn contacts_are_made_from_every_address_in_order() {
        let inbox = inbox();
        let contacts: Vec<(&str, &str)> = inbox
            .contact_list
            .iter()
            .map(|c| (c.email.as_str(), c.name.as_str()))
            .collect();
        assert_eq!(
            contacts,
            [
                ("emma.johnson@bluesparrowtech.com", "Emma Johnson"),
                ("lily.white@gmail.com", "Lily White"),
                ("david.smith@bluesparrowtech.com", "David Smith"),
                ("julie.williams@bluesparrowtech.com", "Julie Williams"),
                // Contacts match addresses exactly, so a differently cased one is new.
                ("David.Smith@bluesparrowtech.com", "David Smith"),
            ]
        );
        assert_eq!(title_case("o'neil_x2y"), "O'Neil_X2Y");
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let emails = inbox().initial_emails;
        let twice = vec![emails[0].clone(), emails[0].clone()];
        let error = Inbox::new("a@b.com".to_string(), twice, Vec::new()).unwrap_err();
        assert_eq!(error, "Email IDs must be unique. Duplicates: [\"0\"]");
    }

    #[test]
    fn unread_emails_are_marked_read() {
        let mut inbox = inbox();
        assert_eq!(ids(&run(&mut inbox, "get_unread_emails", json!({}))), ["2"]);
        assert_eq!(text(&run(&mut inbox, "get_unread_emails", json!({}))), "[]");
    }

    #[test]
    fn search_ignores_case_and_filters_by_sender() {
        let mut inbox = inbox();
        assert_eq!(
            ids(&run(
                &mut inbox,
                "search_emails",
                json!({ "query": "PHOENIX" })
            )),
            ["1", "2"]
        );
        let from_david = json!({ "query": "phoenix", "sender": "david.smith@bluesparrowtech.com" });
        assert_eq!(ids(&run(&mut inbox, "search_emails", from_david)), ["2"]);
        let none = run(&mut inbox, "search_emails", json!({ "query": "zebra" }));
        assert!(none.is_error);
        assert_eq!(text(&none), "No emails found. Try with a different query.");
    }

    #[test]
    fn sent_emails_get_the_next_id_and_parsed_attachments() {
        let mut inbox = inbox();
        let sent = run(
            &mut inbox,
            "send_email",
            json!({
                "recipients": ["lily.white@gmail.com"],
                "subject": "Party", "body": "I'll be there.",
                "attachments": [
                    { "type": "file", "file_id": "7" },
                    { "type": "event", "event_details": {
                        "id": "9", "title": "Party", "description": "John's birthday",
                        "start_time": "2024-05-18T18:00:00", "end_time": "2024-05-18T22:00:00",
                        "location": null, "participants": ["lily.white@gmail.com"],
                    } },
                ],
            }),
        );
        assert!(!sent.is_error, "{}", text(&sent));
        let email = &inbox.emails["3"];
        assert_eq!(email.sender, "emma.johnson@bluesparrowtech.com");
        assert_eq!(email.status, EmailStatus::Sent);
        assert!(email.read && email.cc.is_empty() && email.bcc.is_empty());
        let attachments = serde_json::to_value(&email.attachments).unwrap();
        assert_eq!(attachments[0], "7");
        assert_eq!(attachments[1]["title"], "Party");
        assert_eq!(attachments[1]["status"], "confirmed");
        assert_eq!(
            ids(&run(&mut inbox, "get_sent_emails", json!({}))),
            ["1", "3"]
        );

        let missing = json!({
            "recipients": [], "subject": "", "body": "", "attachments": [{ "type": "file" }],
        });
        let missing = run(&mut inbox, "send_email", missing);
        assert_eq!(
            text(&missing),
            "Attachment of type 'file' must have a 'file_id' field."
        );

        // AgentDojo would take this as a file because of its `file_id`.
        let unknown = json!({
            "recipients": [], "subject": "", "body": "",
            "attachments": [{ "type": "image", "file_id": "7" }],
        });
        let unknown = run(&mut inbox, "send_email", unknown);
        assert!(unknown.is_error);
        assert_eq!(
            text(&unknown),
            "Unknown attachment type \"image\"; expected \"file\" or \"event\"."
        );
    }

    #[test]
    fn deleted_emails_move_to_the_trash() {
        let mut inbox = inbox();
        let deleted = run(&mut inbox, "delete_email", json!({ "email_id": "0" }));
        assert_eq!(text(&deleted), "Email with id 0 deleted successfully.");
        assert!(inbox.trash.contains_key("0"));
        let ids: Vec<&String> = inbox.emails.keys().collect();
        assert_eq!(ids, ["1", "2"]);
        let again = run(&mut inbox, "delete_email", json!({ "email_id": "0" }));
        assert_eq!(text(&again), "Email with ID '0' not found.");
    }

    #[test]
    fn contacts_are_searched_by_substring() {
        let mut inbox = inbox();
        let found = run(
            &mut inbox,
            "search_contacts_by_name",
            json!({ "query": "smith" }),
        );
        assert_eq!(
            found.structured_content.unwrap()["result"][0],
            json!({ "email": "david.smith@bluesparrowtech.com", "name": "David Smith" })
        );
        let missing = run(
            &mut inbox,
            "search_contacts_by_email",
            json!({ "query": "nobody" }),
        );
        assert_eq!(text(&missing), "Contact with email 'nobody' not found.");
    }

    #[test]
    fn most_recent_received_email_is_by_timestamp() {
        assert_eq!(inbox().most_recently_received_email().unwrap().id, "2");
    }
}
