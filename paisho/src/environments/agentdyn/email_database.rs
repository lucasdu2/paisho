//! AgentDyn's email database (`default_suites/v1/tools/dynamic_email_client.py`): an inbox
//! for every address, with the user logged in to one of them. Sending delivers a copy to
//! each recipient's inbox, creating it if the address has none yet.
//!
//! `search_contacts_by_name` isn't AgentDyn's: its `search_emails` description tells the
//! agent to use that tool, which AgentDyn never provides, so this adds AgentDojo's. Each
//! inbox's contacts are the addresses in its starting emails, as in AgentDojo.

use paisho_macros::tools;
use serde_json::Value;

use crate::environments::agentdojo::email::{Attachment, Email, EmailContact, Inbox};
use crate::environments::agentdyn::file_tree::FileTree;
use crate::environments::agentdyn::web_database::{AttachmentSpec, WebDatabase};
use crate::environments::email_str::{EmailStr, Validation};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EmailDatabase {
    pub(crate) inbox_list: Vec<Inbox>,
    /// The address the user is logged in to.
    #[serde(default)]
    pub(crate) current_account: Option<EmailStr>,
}

const NOT_LOGGED_IN: &str = "No email account is logged in.";

impl EmailDatabase {
    /// The inbox of `address`, or `None` if it has none.
    pub(crate) fn inbox(&self, address: &str) -> Option<&Inbox> {
        self.inbox_list
            .iter()
            .find(|inbox| inbox.account_email == address)
    }

    /// The inbox of `address`, created empty if it has none, as AgentDyn's
    /// `get_email_account_instance` does.
    pub(crate) fn inbox_mut(&mut self, address: &EmailStr) -> &mut Inbox {
        let index = match self
            .inbox_list
            .iter()
            .position(|inbox| inbox.account_email == *address)
        {
            Some(index) => index,
            None => {
                let inbox = Inbox::new(address.clone(), Vec::new(), Vec::new())
                    .expect("an inbox with no emails has no duplicate IDs");
                self.inbox_list.push(inbox);
                self.inbox_list.len() - 1
            }
        };
        &mut self.inbox_list[index]
    }

    /// The inbox the user is logged in to, if it has one.
    fn current_inbox(&self) -> Result<Option<&Inbox>, String> {
        let address = self.current_account.as_deref().ok_or(NOT_LOGGED_IN)?;
        Ok(self.inbox(address))
    }

    /// The inbox the user is logged in to, created if it has none.
    pub(crate) fn current_inbox_mut(&mut self) -> Result<&mut Inbox, String> {
        let address = self.current_account.clone().ok_or(NOT_LOGGED_IN)?;
        Ok(self.inbox_mut(&address))
    }

    /// Sends an email from `sender` and delivers it to every recipient, as AgentDyn's
    /// `send_receive_email` does: the sender's inbox keeps a sent copy and each recipient's
    /// inbox receives one. Returns the last copy received, or the sent copy if there are no
    /// recipients.
    ///
    /// As in AgentDyn, each recipient's inbox is found first, and one is created, for the
    /// normalized address, for a recipient with none. An invalid address fails there with
    /// pydantic's error for `DynamicInbox`, keeping the inboxes created before it; an invalid
    /// cc or bcc address fails when the email is sent, after the inboxes are created.
    // The arguments are those of an email, as in AgentDyn's `send_receive_email`.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn deliver(
        &mut self,
        sender: &str,
        recipients: Vec<String>,
        subject: &str,
        body: &str,
        attachments: Vec<Attachment>,
        cc: Vec<String>,
        bcc: Vec<String>,
    ) -> Result<Email, String> {
        // Callers pass the addresses of AgentDyn's web pages and of the platforms, which are
        // valid; this fails, as AgentDyn's `Email` does, if one isn't.
        let sender = Validation::run("Email", |v| v.field("sender", sender))?;
        let mut targets = Vec::new();
        for recipient in &recipients {
            let index = match self
                .inbox_list
                .iter()
                .position(|inbox| inbox.account_email == *recipient)
            {
                Some(index) => index,
                None => {
                    let address =
                        Validation::run("DynamicInbox", |v| v.field("account_email", recipient))?;
                    let inbox = Inbox::new(address, Vec::new(), Vec::new())
                        .expect("an inbox with no emails has no duplicate IDs");
                    self.inbox_list.push(inbox);
                    self.inbox_list.len() - 1
                }
            };
            targets.push(index);
        }
        let (recipients, cc, bcc) = Inbox::validate_addresses(&recipients, &cc, &bcc)?;
        let mut last = self.inbox_mut(&sender).send(
            recipients,
            subject,
            body,
            attachments.clone(),
            cc.clone(),
            bcc.clone(),
        )?;
        for index in targets {
            last = self.inbox_list[index].receive(
                &sender,
                subject,
                body,
                attachments.clone(),
                cc.clone(),
                bcc.clone(),
            )?;
        }
        Ok(last)
    }
}

#[tools]
impl EmailDatabase {
    /// Returns all the unread emails in the inbox. Each email has a sender, a subject, and a
    /// body. The emails are marked as read after this function is called.
    fn get_unread_emails(&mut self) -> Result<Vec<Email>, String> {
        Ok(self.current_inbox_mut()?.take_unread())
    }

    /// Returns all the received emails in the inbox. Each email has a sender, a subject, and
    /// a body.
    fn get_received_emails(&self) -> Result<Vec<&Email>, String> {
        Ok(self
            .current_inbox()?
            .map(Inbox::received)
            .unwrap_or_default())
    }

    /// Returns all the sent emails in the inbox. Each email has a recipient, a subject, and a
    /// body.
    fn get_sent_emails(&self) -> Result<Vec<&Email>, String> {
        Ok(self.current_inbox()?.map(Inbox::sent).unwrap_or_default())
    }

    /// Deletes the email with the given `email_id` from the inbox.
    fn delete_email(
        &mut self,
        #[describe = "The id of the email to delete."] email_id: &str,
    ) -> Result<String, String> {
        let inbox = self.current_inbox_mut()?;
        let Some(email) = inbox.emails.shift_remove(email_id) else {
            return Err(format!("Email with ID '{email_id}' not found."));
        };
        inbox.trash.insert(email_id.to_string(), email);
        Ok(format!("Email with id {email_id} deleted successfully."))
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
        let matches = self
            .current_inbox()?
            .map(|inbox| inbox.search(query, sender.as_deref()))
            .unwrap_or_default();
        if matches.is_empty() {
            return Err("No emails found. Try with a different query.".to_string());
        }
        Ok(matches)
    }

    /// Finds contacts in the inbox's contact list by name. It returns a list of contacts that
    /// match the given name.
    fn search_contacts_by_name(
        &self,
        #[describe = "The name of the contacts to search for."] query: &str,
    ) -> Result<Vec<&EmailContact>, String> {
        let matches = self
            .current_inbox()?
            .map(|inbox| inbox.contacts_named(query))
            .unwrap_or_default();
        if matches.is_empty() {
            return Err(format!("Contact with name '{query}' not found."));
        }
        Ok(matches)
    }
}

/// The email database with the files that attachments can come from: local files, which
/// sending uploads to the web, and files already on the web. List it in
/// `combine_environments!` under `views`, as
/// `EmailWithAttachments { email_database, filesystem, web_database }`.
pub(crate) struct EmailWithAttachments<'a> {
    pub(crate) email_database: &'a mut EmailDatabase,
    pub(crate) filesystem: &'a mut FileTree,
    pub(crate) web_database: &'a mut WebDatabase,
}

#[tools]
impl EmailWithAttachments<'_> {
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
                      then it is expected to have a field \"file_path\", with the path of file \
                      in the local file system, or have a field \"file_id\", with the id of \
                      file in the web. If the attachment has as \"type\" \"event\", then it is \
                      expected to be a calendar event in the field \"event_details\". A \
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
    ) -> Result<String, String> {
        let sender = self
            .email_database
            .current_account
            .clone()
            .ok_or(NOT_LOGGED_IN)?;
        let specs = attachments
            .unwrap_or_default()
            .iter()
            .map(AttachmentSpec::parse)
            .collect::<Result<Vec<_>, _>>()?;
        let attachments = self.web_database.attach(&specs, self.filesystem)?;
        self.email_database.deliver(
            &sender,
            recipients,
            subject,
            body,
            attachments,
            cc.unwrap_or_default(),
            bcc.unwrap_or_default(),
        )?;
        Ok("The email has been sent successfully.".to_string())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::environments::agentdojo::email::EmailStatus;

    fn database() -> EmailDatabase {
        serde_json::from_value(json!({
            "inbox_list": [{
                "account_email": "me@x.com",
                "initial_emails": [{
                    "id": "0", "sender": "a@x.com", "recipients": ["me@x.com"],
                    "subject": "Hi", "body": "Hello", "status": "received",
                    "timestamp": "2024-05-15T10:00",
                }],
            }],
            "current_account": "me@x.com",
        }))
        .unwrap()
    }

    #[test]
    fn delivery_creates_inboxes_and_keeps_a_sent_copy() {
        let mut db = database();
        let received = db
            .deliver(
                "me@x.com",
                vec!["b@x.com".to_string()],
                "S",
                "B",
                Vec::new(),
                Vec::new(),
                Vec::new(),
            )
            .unwrap();
        assert_eq!(received.status, EmailStatus::Received);
        assert_eq!(received.recipients, ["b@x.com"]);
        assert!(!received.read);
        let sent = db.inbox("me@x.com").unwrap().sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].id, "1");
        assert_eq!(db.inbox("b@x.com").unwrap().received()[0].id, "1");

        // A stranger sending to the user gets an inbox too.
        db.deliver(
            "c@x.com",
            vec!["me@x.com".to_string()],
            "S",
            "B",
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        assert_eq!(db.inbox("c@x.com").unwrap().sent().len(), 1);
        assert_eq!(db.inbox("me@x.com").unwrap().received().len(), 2);
    }

    /// As in AgentDyn, recipients' inboxes are created before anything is sent, so an
    /// invalid address fails after the inboxes for the addresses before it are created.
    #[test]
    fn invalid_addresses_fail_after_earlier_inboxes_are_created() {
        let mut db = database();
        let deliver = |db: &mut EmailDatabase, recipients: &[&str], cc: &[&str]| {
            let strings = |list: &[&str]| list.iter().map(|s| s.to_string()).collect();
            db.deliver(
                "me@x.com",
                strings(recipients),
                "S",
                "B",
                Vec::new(),
                strings(cc),
                Vec::new(),
            )
        };
        let error = deliver(&mut db, &["b@x.com", "bob"], &[]).unwrap_err();
        assert!(
            error.starts_with("1 validation error for DynamicInbox\naccount_email\n"),
            "{error}"
        );
        assert!(db.inbox("b@x.com").is_some());
        assert!(db.inbox("me@x.com").unwrap().sent().is_empty());

        let error = deliver(&mut db, &["C@X.com"], &["nope"]).unwrap_err();
        assert!(
            error.starts_with("1 validation error for Email\ncc.0\n"),
            "{error}"
        );
        assert!(db.inbox("C@x.com").is_some());
        assert!(db.inbox("me@x.com").unwrap().sent().is_empty());

        // A new inbox gets the normalized address, and the email reaches it.
        deliver(&mut db, &["D@X.com"], &[]).unwrap();
        assert_eq!(db.inbox("D@x.com").unwrap().received().len(), 1);
    }

    #[test]
    fn tools_need_a_logged_in_account() {
        let mut db = database();
        db.current_account = None;
        assert_eq!(db.get_received_emails().unwrap_err(), NOT_LOGGED_IN);
        assert_eq!(db.get_unread_emails().unwrap_err(), NOT_LOGGED_IN);
    }

    #[test]
    fn reading_marks_emails_read_and_search_finds_them() {
        let mut db = database();
        assert_eq!(db.get_unread_emails().unwrap().len(), 1);
        assert!(db.get_unread_emails().unwrap().is_empty());
        assert_eq!(db.search_emails("hello", None).unwrap().len(), 1);
        assert!(db.search_emails("nope", None).is_err());
        assert!(db.delete_email("0").is_ok());
        assert!(db.delete_email("0").is_err());
        assert!(db.get_received_emails().unwrap().is_empty());
    }

    /// Contacts come from the addresses in the starting emails, named after their local part.
    #[test]
    fn contacts_are_found_by_name() {
        let db = database();
        let found = db.search_contacts_by_name("A").unwrap();
        assert!(found.iter().any(|contact| contact.email == "a@x.com"));
        assert!(db.search_contacts_by_name("nobody").is_err());
    }
}
