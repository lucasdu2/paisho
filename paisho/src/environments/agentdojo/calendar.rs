use chrono::{NaiveDate, NaiveDateTime};
use indexmap::IndexMap;
use paisho_macros::tools;

use crate::environments::agentdojo::email::{Attachment, Inbox};

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
pub(crate) enum EventStatus {
    #[default]
    Confirmed,
    Canceled,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub(crate) struct CalendarEvent {
    /// The unique identifier of the event
    pub(crate) id: String,
    /// The title of the event
    pub(crate) title: String,
    /// The description of the event
    pub(crate) description: String,
    /// The start time of the event
    #[serde(deserialize_with = "crate::environments::deserialize_date_time")]
    pub(crate) start_time: NaiveDateTime,
    /// The end time of the event
    #[serde(deserialize_with = "crate::environments::deserialize_date_time")]
    pub(crate) end_time: NaiveDateTime,
    /// The location of the event
    pub(crate) location: Option<String>,
    /// The list of the emails of the participants
    pub(crate) participants: Vec<String>,
    /// Whether the event is all day
    #[serde(default)]
    pub(crate) all_day: bool,
    /// The status of the event
    #[serde(default)]
    pub(crate) status: EventStatus,
}

/// The user's calendar.
/// A calendar as a suite's data gives it, before [`Calendar::new`] indexes its events.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CalendarData {
    account_email: String,
    current_day: NaiveDate,
    initial_events: Vec<CalendarEvent>,
}

impl TryFrom<CalendarData> for Calendar {
    type Error = String;

    fn try_from(data: CalendarData) -> Result<Self, String> {
        data.into_calendar()
    }
}

impl CalendarData {
    /// # Errors
    ///
    /// Returns an error if two of the events share an ID.
    pub(crate) fn into_calendar(self) -> Result<Calendar, String> {
        Calendar::new(self.current_day, self.account_email, self.initial_events)
    }
}

/// Deserialized from [`CalendarData`], so that a suite's data can hold a calendar directly.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "CalendarData")]
pub(crate) struct Calendar {
    pub(crate) current_day: NaiveDate,
    pub(crate) account_email: String,
    pub(crate) initial_events: Vec<CalendarEvent>,
    pub(crate) events: IndexMap<String, CalendarEvent>,
}

impl Calendar {
    /// A calendar holding `initial_events`.
    ///
    /// # Errors
    ///
    /// Returns an error if two of `initial_events` share an ID.
    pub(crate) fn new(
        current_day: NaiveDate,
        account_email: String,
        initial_events: Vec<CalendarEvent>,
    ) -> Result<Self, String> {
        let mut events = IndexMap::new();
        let mut duplicate_ids = Vec::new();
        for event in &initial_events {
            if events.insert(event.id.clone(), event.clone()).is_some() {
                duplicate_ids.push(event.id.clone());
            }
        }
        if !duplicate_ids.is_empty() {
            return Err(format!(
                "Event IDs must be unique. Duplicates: {duplicate_ids:?}"
            ));
        }
        Ok(Calendar {
            current_day,
            account_email,
            initial_events,
            events,
        })
    }

    /// The events starting on `day`.
    pub(crate) fn get_by_day(&self, day: NaiveDate) -> Vec<&CalendarEvent> {
        self.events
            .values()
            .filter(|event| event.start_time.date() == day)
            .collect()
    }

    /// Adds a confirmed event, with the account's address among its participants, and
    /// returns it. Participants are deduplicated, keeping the first of each.
    pub(crate) fn create_event(
        &mut self,
        title: &str,
        description: &str,
        start_time: NaiveDateTime,
        end_time: NaiveDateTime,
        location: Option<String>,
        participants: &[String],
    ) -> Result<CalendarEvent, String> {
        let mut unique: Vec<String> = Vec::new();
        for participant in participants.iter().chain([&self.account_email]) {
            if !unique.contains(participant) {
                unique.push(participant.clone());
            }
        }
        let event = CalendarEvent {
            id: crate::environments::next_id(self.events.keys())?,
            title: title.to_string(),
            description: description.to_string(),
            start_time,
            end_time,
            location,
            participants: unique,
            all_day: false,
            status: EventStatus::Confirmed,
        };
        self.events.insert(event.id.clone(), event.clone());
        Ok(event)
    }

    fn event_mut(&mut self, event_id: &str) -> Result<&mut CalendarEvent, String> {
        self.events
            .get_mut(event_id)
            .ok_or_else(|| format!("Event with ID '{event_id}' not found."))
    }

    /// Marks the event canceled and emails its participants.
    pub(crate) fn cancel_event(&mut self, inbox: &mut Inbox, event_id: &str) -> Result<(), String> {
        let event = self.event_mut(event_id)?;
        event.status = EventStatus::Canceled;
        let event = event.clone();
        inbox.send(
            event.participants.clone(),
            &format!("Canceled: '{}'", event.title),
            "The event has been canceled.",
            vec![Attachment::Event(event)],
            Vec::new(),
            Vec::new(),
        )?;
        Ok(())
    }

    /// The events whose title or description contains `query`, ignoring case, only those
    /// starting on `date` if given.
    pub(crate) fn search_events(
        &self,
        query: &str,
        date: Option<NaiveDate>,
    ) -> Vec<&CalendarEvent> {
        let query = query.to_lowercase();
        self.events
            .values()
            .filter(|event| date.is_none_or(|date| event.start_time.date() == date))
            .filter(|event| {
                event.title.to_lowercase().contains(&query)
                    || event.description.to_lowercase().contains(&query)
            })
            .collect()
    }

    /// Whether two events starting on `day` overlap.
    pub(crate) fn day_has_conflicts(&self, day: NaiveDate) -> bool {
        let events = self.get_by_day(day);
        events.iter().enumerate().any(|(i, event)| {
            events[i + 1..].iter().any(|other| {
                (event.start_time <= other.start_time && other.start_time < event.end_time)
                    || (event.start_time < other.end_time && other.end_time <= event.end_time)
            })
        })
    }

    /// Moves the event to `new_start_time`, ending at `new_end_time` or, if that is `None`,
    /// after the same duration as before. Emails the participants and returns the event.
    pub(crate) fn reschedule_event(
        &mut self,
        inbox: &mut Inbox,
        event_id: &str,
        new_start_time: NaiveDateTime,
        new_end_time: Option<NaiveDateTime>,
    ) -> Result<CalendarEvent, String> {
        let event = self.event_mut(event_id)?;
        let duration = event.end_time - event.start_time;
        event.start_time = new_start_time;
        event.end_time = new_end_time.unwrap_or(new_start_time + duration);
        let event = event.clone();
        // The double space after "on" is AgentDojo's.
        let body = format!(
            "The event has been rescheduled. It will start on  {} at {} and end on {} at {}.",
            event.start_time.format("%Y-%m-%d"),
            event.start_time.format("%H:%M:%S"),
            event.end_time.format("%Y-%m-%d"),
            event.end_time.format("%H:%M:%S"),
        );
        // AgentDojo's subject is missing its `f` prefix, so it reads
        // "Rescheduled: '{event.title}'"; this one has the title.
        inbox.send(
            event.participants.clone(),
            &format!("Rescheduled: '{}'", event.title),
            &body,
            vec![Attachment::Event(event.clone())],
            Vec::new(),
            Vec::new(),
        )?;
        Ok(event)
    }

    /// Adds `participants` to the event and returns it. Unlike `create_event`, this doesn't
    /// deduplicate.
    pub(crate) fn add_participants(
        &mut self,
        event_id: &str,
        participants: Vec<String>,
    ) -> Result<CalendarEvent, String> {
        let event = self.event_mut(event_id)?;
        event.participants.extend(participants);
        Ok(event.clone())
    }
}

/// Parses a date given as `YYYY-MM-DD`.
fn parse_date(text: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .map_err(|_| format!("Invalid date '{text}': expected YYYY-MM-DD."))
}

/// Parses a date and time given as `YYYY-MM-DD HH:MM`.
fn parse_date_time(text: &str) -> Result<NaiveDateTime, String> {
    NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M")
        .map_err(|_| format!("Invalid date and time '{text}': expected YYYY-MM-DD HH:MM."))
}

#[tools]
impl Calendar {
    /// Returns the appointments for the given `day`. Returns a list of dictionaries with
    /// informations about each meeting.
    fn get_day_calendar_events(
        &self,
        #[describe = "The day for which to return the appointments. Must be in format \
                      YYYY-MM-DD."]
        day: &str,
    ) -> Result<Vec<&CalendarEvent>, String> {
        Ok(self.get_by_day(parse_date(day)?))
    }

    /// Searches calendar events that match the given query in the tile or the description.
    /// If provided, filters events by date.
    fn search_calendar_events(
        &self,
        #[describe = "The query string to search for in event titles and descriptions."]
        query: &str,
        #[describe = "The date for which to search events. Must be in format YYYY-MM-DD. If \
                      `null`, searches all events."]
        date: Option<String>,
    ) -> Result<Vec<&CalendarEvent>, String> {
        let date = date.as_deref().map(parse_date).transpose()?;
        let matches = self.search_events(query, date);
        if matches.is_empty() {
            return Err("No events found. Try with a different query.".to_string());
        }
        Ok(matches)
    }

    /// Returns the current day in ISO format, e.g. '2022-01-01'. It is useful to know what
    /// the current day, year, or month is, as the assistant should not assume what the
    /// current date is.
    fn get_current_day(&self) -> Result<String, String> {
        Ok(self.current_day.format("%Y-%m-%d").to_string())
    }

    /// Adds the given `participants` to the event with the given `event_id`. It will also
    /// email the new participants notifying them of the event.
    fn add_calendar_event_participants(
        &mut self,
        #[describe = "The ID of the event to add participants to."] event_id: &str,
        #[describe = "The list of participants' email addresses to add to the event."]
        participants: Vec<String>,
    ) -> Result<CalendarEvent, String> {
        self.add_participants(event_id, participants)
    }
}

/// The calendar with the inbox it sends invitations, cancellations and reschedulings from,
/// for the calendar tools that email participants. List it in `combine_environments!` under
/// `views`, as `CalendarWithInbox { calendar, inbox }`.
pub(crate) struct CalendarWithInbox<'a> {
    pub(crate) calendar: &'a mut Calendar,
    pub(crate) inbox: &'a mut Inbox,
}

#[tools]
impl CalendarWithInbox<'_> {
    /// Creates a new calendar event with the given details and adds it to the calendar. It
    /// also sends an email to the participants with the event details.
    pub(crate) fn create_calendar_event(
        &mut self,
        #[describe = "The title of the event."] title: &str,
        #[describe = "The start time of the event. Must be in format YYYY-MM-DD HH:MM."]
        start_time: &str,
        #[describe = "The end time of the event. Must be in format YYYY-MM-DD HH:MM."]
        end_time: &str,
        #[describe = "The description of the event."]
        #[serde(default)]
        description: String,
        #[describe = "The list of participants' email addresses. If `null`, no participants \
                      are set. The calendar owner's email address is always included.."]
        participants: Option<Vec<String>>,
        #[describe = "The location of the event. If `null`, no location is set."] location: Option<
            String,
        >,
    ) -> Result<CalendarEvent, String> {
        let start_time = parse_date_time(start_time)?;
        let end_time = parse_date_time(end_time)?;
        let mut participants = participants.unwrap_or_default();
        let event = self.calendar.create_event(
            title,
            &description,
            start_time,
            end_time,
            location,
            &participants,
        )?;
        // AgentDojo adds the account to the caller's list, so the invitation goes to the
        // participants as given (duplicates and all) and to the account.
        participants.push(self.calendar.account_email.clone());
        self.inbox.send(
            participants,
            &format!("Invitation: {title}"),
            &description,
            vec![Attachment::Event(event.clone())],
            Vec::new(),
            Vec::new(),
        )?;
        Ok(event)
    }

    /// Cancels the event with the given `event_id`. The event will be marked as canceled and
    /// no longer appear in the calendar. It will also send an email to the participants
    /// notifying them of the cancellation.
    pub(crate) fn cancel_calendar_event(
        &mut self,
        #[describe = "The ID of the event to cancel."] event_id: &str,
    ) -> Result<String, String> {
        self.calendar.cancel_event(self.inbox, event_id)?;
        Ok(format!(
            "Event with ID {event_id} has been canceled and participants have been notified."
        ))
    }

    /// Reschedules the event with the given `event_id` to the new start and end times. It
    /// will also send an email to the participants notifying them of the rescheduling.
    pub(crate) fn reschedule_calendar_event(
        &mut self,
        #[describe = "The ID of the event to reschedule."] event_id: &str,
        #[describe = "The new start time of the event. Must be in format YYYY-MM-DD HH:MM."]
        new_start_time: &str,
        #[describe = "The new end time of the event. Must be in format YYYY-MM-DD HH:MM. If \
                      `null`, the end time will be computed based on the new start time to \
                      keep the event duration the same."]
        new_end_time: Option<String>,
    ) -> Result<CalendarEvent, String> {
        // In AgentDojo's order: the end time is parsed first.
        let new_end_time = new_end_time.as_deref().map(parse_date_time).transpose()?;
        let new_start_time = parse_date_time(new_start_time)?;
        self.calendar
            .reschedule_event(self.inbox, event_id, new_start_time, new_end_time)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::environments::agentdojo::email::EmailStatus;
    use crate::environments::combine_environments;
    use crate::suites::{ToolContent, ToolResult, call};

    const EMMA: &str = "emma.johnson@bluesparrowtech.com";

    fn event(value: Value) -> CalendarEvent {
        serde_json::from_value(value).unwrap()
    }

    fn time(text: &str) -> NaiveDateTime {
        parse_date_time(text).unwrap()
    }

    /// Two events on 2024-05-15 that overlap, and one on the next day.
    fn calendar() -> Calendar {
        let events = vec![
            event(json!({
                "id": "6", "title": "Team Sync", "description": "Weekly team sync.",
                "start_time": "2024-05-15T10:00:00", "end_time": "2024-05-15T11:00:00",
                "location": "Room A", "participants": [EMMA, "david.smith@bluesparrowtech.com"],
            })),
            event(json!({
                "id": "1", "title": "Lunch", "description": "Lunch with Sarah.",
                "start_time": "2024-05-15T10:30:00", "end_time": "2024-05-15T12:00:00",
                "location": null, "participants": [EMMA],
            })),
            event(json!({
                "id": "2", "title": "Dentist", "description": "Check-up.",
                "start_time": "2024-05-16T09:00:00", "end_time": "2024-05-16T09:30:00",
                "location": null, "participants": [EMMA],
            })),
        ];
        let day = parse_date("2024-05-15").unwrap();
        Calendar::new(day, EMMA.to_string(), events).unwrap()
    }

    fn inbox() -> Inbox {
        Inbox::new(EMMA.to_string(), Vec::new(), Vec::new()).unwrap()
    }

    fn run(calendar: &mut Calendar, name: &str, arguments: Value) -> ToolResult {
        calendar.dispatch(&call(name, arguments)).unwrap()
    }

    fn text(result: &ToolResult) -> &str {
        let ToolContent::Text { text } = &result.content[0];
        text
    }

    fn ids(events: &[&CalendarEvent]) -> Vec<String> {
        events.iter().map(|event| event.id.clone()).collect()
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let events = calendar().initial_events;
        let twice = vec![events[0].clone(), events[0].clone()];
        let day = parse_date("2024-05-15").unwrap();
        let error = Calendar::new(day, EMMA.to_string(), twice).unwrap_err();
        assert_eq!(error, "Event IDs must be unique. Duplicates: [\"6\"]");
    }

    #[test]
    fn events_are_found_by_day_and_query() {
        let mut calendar = calendar();
        let day = parse_date("2024-05-15").unwrap();
        assert_eq!(ids(&calendar.get_by_day(day)), ["6", "1"]);
        assert_eq!(ids(&calendar.search_events("LUNCH", None)), ["1"]);
        assert_eq!(
            ids(&calendar.search_events("check", Some(day))),
            Vec::<String>::new()
        );
        assert!(calendar.day_has_conflicts(day));
        assert!(!calendar.day_has_conflicts(parse_date("2024-05-16").unwrap()));

        let none = run(
            &mut calendar,
            "search_calendar_events",
            json!({ "query": "zebra" }),
        );
        assert_eq!(text(&none), "No events found. Try with a different query.");
        let bad = run(
            &mut calendar,
            "get_day_calendar_events",
            json!({ "day": "15/05/2024" }),
        );
        assert!(bad.is_error);
        assert_eq!(
            text(&bad),
            "Invalid date '15/05/2024': expected YYYY-MM-DD."
        );
        let today = run(&mut calendar, "get_current_day", json!({}));
        assert_eq!(text(&today), "2024-05-15");
    }

    #[test]
    fn created_events_get_the_next_id_and_the_account_as_participant() {
        let mut calendar = calendar();
        let participants = [
            "lily.white@gmail.com".to_string(),
            EMMA.to_string(),
            "lily.white@gmail.com".to_string(),
        ];
        let event = calendar
            .create_event(
                "Party",
                "",
                time("2024-05-18 18:00"),
                time("2024-05-18 22:00"),
                None,
                &participants,
            )
            .unwrap();
        assert_eq!(event.id, "7");
        assert_eq!(event.participants, ["lily.white@gmail.com", EMMA]);
        assert_eq!(event.status, EventStatus::Confirmed);
        assert!(calendar.events.contains_key("7"));
    }

    #[test]
    fn canceling_marks_the_event_and_emails_the_participants() {
        let (mut calendar, mut inbox) = (calendar(), inbox());
        calendar.cancel_event(&mut inbox, "6").unwrap();
        assert_eq!(calendar.events["6"].status, EventStatus::Canceled);
        let email = &inbox.emails["1"];
        assert_eq!(email.subject, "Canceled: 'Team Sync'");
        assert_eq!(email.recipients, calendar.events["6"].participants);
        assert_eq!(email.status, EmailStatus::Sent);
        let attachment = serde_json::to_value(&email.attachments[0]).unwrap();
        assert_eq!(attachment["status"], "canceled");

        let missing = calendar.cancel_event(&mut inbox, "99").unwrap_err();
        assert_eq!(missing, "Event with ID '99' not found.");
    }

    #[test]
    fn rescheduling_keeps_the_duration_unless_given_an_end() {
        let (mut calendar, mut inbox) = (calendar(), inbox());
        let event = calendar
            .reschedule_event(&mut inbox, "6", time("2024-05-20 14:00"), None)
            .unwrap();
        assert_eq!(event.end_time, time("2024-05-20 15:00"));
        let email = &inbox.emails["1"];
        assert_eq!(email.subject, "Rescheduled: 'Team Sync'");
        assert_eq!(
            email.body,
            "The event has been rescheduled. It will start on  2024-05-20 at 14:00:00 and end \
             on 2024-05-20 at 15:00:00."
        );

        let event = calendar
            .reschedule_event(
                &mut inbox,
                "6",
                time("2024-05-21 09:00"),
                Some(time("2024-05-21 09:15")),
            )
            .unwrap();
        assert_eq!(event.end_time, time("2024-05-21 09:15"));
    }

    #[test]
    fn participants_are_added_without_deduplicating() {
        let mut calendar = calendar();
        let added = run(
            &mut calendar,
            "add_calendar_event_participants",
            json!({ "event_id": "1", "participants": [EMMA, "sarah@gmail.com"] }),
        );
        assert!(!added.is_error, "{}", text(&added));
        assert_eq!(
            calendar.events["1"].participants,
            [EMMA, EMMA, "sarah@gmail.com"]
        );
    }

    combine_environments! {
        struct Workspace {
            inbox: Inbox,
            calendar: Calendar,
        }
        views {
            CalendarWithInbox { calendar, inbox },
        }
    }

    #[test]
    fn views_add_tools_that_use_several_environments() {
        use crate::environments::Environment;

        let mut workspace = Workspace {
            inbox: inbox(),
            calendar: calendar(),
        };
        assert!(Workspace::TOOL_NAMES.contains(&"create_calendar_event"));
        let names: Vec<Value> = workspace
            .tool_schemas()
            .into_iter()
            .map(|schema| schema["name"].clone())
            .collect();
        assert!(
            names.contains(&json!("reschedule_calendar_event")),
            "{names:?}"
        );
        let schema = workspace.tool_schema("create_calendar_event").unwrap();
        let input = &schema["input_schema"];
        assert_eq!(input["properties"]["description"]["default"], "");
        assert_eq!(
            input["required"],
            json!(["title", "start_time", "end_time"])
        );

        let created = workspace
            .dispatch(&call(
                "create_calendar_event",
                json!({
                    "title": "Party", "start_time": "2024-05-18 18:00",
                    "end_time": "2024-05-18 22:00", "participants": ["lily.white@gmail.com"],
                }),
            ))
            .unwrap();
        assert!(!created.is_error, "{}", text(&created));
        let event = &workspace.calendar.events["7"];
        assert_eq!(event.participants, ["lily.white@gmail.com", EMMA]);
        let invitation = &workspace.inbox.emails["1"];
        assert_eq!(invitation.subject, "Invitation: Party");
        assert_eq!(invitation.recipients, ["lily.white@gmail.com", EMMA]);

        let canceled = workspace
            .dispatch(&call("cancel_calendar_event", json!({ "event_id": "7" })))
            .unwrap();
        assert_eq!(
            text(&canceled),
            "Event with ID 7 has been canceled and participants have been notified."
        );
        assert_eq!(workspace.inbox.emails["2"].subject, "Canceled: 'Party'");

        let bad = workspace
            .dispatch(&call(
                "reschedule_calendar_event",
                json!({ "event_id": "7", "new_start_time": "soon", "new_end_time": "later" }),
            ))
            .unwrap();
        assert_eq!(
            text(&bad),
            "Invalid date and time 'later': expected YYYY-MM-DD HH:MM."
        );
        // Tools of a field still go to that field.
        let today = workspace
            .dispatch(&call("get_current_day", json!({})))
            .unwrap();
        assert_eq!(text(&today), "2024-05-15");
    }
}
