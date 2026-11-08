//! AgentDyn's calendar (`default_suites/v1/tools/dynamic_calendar_client.py`): AgentDojo's
//! [`Calendar`], which sends its invitations, cancellations and reschedulings from the inbox
//! the user is logged in to. As with AgentDojo's, those emails stay in the sender's inbox;
//! they aren't delivered to the participants.

use chrono::NaiveDateTime;
use paisho_macros::tools;

use crate::environments::agentdojo::calendar::{Calendar, CalendarEvent, CalendarWithInbox};
use crate::environments::agentdyn::email_database::EmailDatabase;

/// The calendar with the email database whose current inbox it sends from. List it in
/// `combine_environments!` under `views`, as `CalendarWithEmail { calendar, email_database }`.
pub(crate) struct CalendarWithEmail<'a> {
    pub(crate) calendar: &'a mut Calendar,
    pub(crate) email_database: &'a mut EmailDatabase,
}

impl CalendarWithEmail<'_> {
    fn with_inbox(&mut self) -> Result<CalendarWithInbox<'_>, String> {
        Ok(CalendarWithInbox {
            calendar: self.calendar,
            inbox: self.email_database.current_inbox_mut()?,
        })
    }
}

#[tools]
impl CalendarWithEmail<'_> {
    /// Creates a new calendar event with the given details and adds it to the calendar. It
    /// also sends an email to the participants with the event details.
    fn create_calendar_event(
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
        self.with_inbox()?.create_calendar_event(
            title,
            start_time,
            end_time,
            description,
            participants,
            location,
        )
    }

    /// Cancels the event with the given `event_id`. The event will be marked as canceled and
    /// no longer appear in the calendar. It will also send an email to the participants
    /// notifying them of the cancellation.
    fn cancel_calendar_event(
        &mut self,
        #[describe = "The ID of the event to cancel."] event_id: &str,
    ) -> Result<String, String> {
        self.with_inbox()?.cancel_calendar_event(event_id)
    }

    /// Reschedules the event with the given `event_id` to the new start and end times. It
    /// will also send an email to the participants notifying them of the rescheduling.
    fn reschedule_calendar_event(
        &mut self,
        #[describe = "The ID of the event to reschedule."] event_id: &str,
        #[describe = "The new start time of the event. Must be in format YYYY-MM-DD HH:MM."]
        new_start_time: &str,
        #[describe = "The new end time of the event. Must be in format YYYY-MM-DD HH:MM. If \
                      `null`, the end time will be computed based on the new start time to \
                      keep the event duration the same."]
        new_end_time: Option<String>,
    ) -> Result<CalendarEvent, String> {
        self.with_inbox()?
            .reschedule_calendar_event(event_id, new_start_time, new_end_time)
    }
}

/// Whether some event in `calendar` satisfies `condition`.
pub(crate) fn has_event(calendar: &Calendar, condition: impl Fn(&CalendarEvent) -> bool) -> bool {
    calendar.events.values().any(condition)
}

/// `text`, `YYYY-MM-DD HH:MM`, as a date and time.
///
/// # Panics
///
/// Panics if `text` isn't in that format; it's meant for constants in task checks.
pub(crate) fn at(text: &str) -> NaiveDateTime {
    NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M").expect("a YYYY-MM-DD HH:MM time")
}
