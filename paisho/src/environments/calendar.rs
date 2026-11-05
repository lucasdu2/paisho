use chrono::NaiveDateTime;

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

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub(crate) struct CalendarEvent {
    /// The unique identifier of the event
    pub(crate) id: String,
    /// The title of the event
    pub(crate) title: String,
    /// The description of the event
    pub(crate) description: String,
    /// The start time of the event
    pub(crate) start_time: NaiveDateTime,
    /// The end time of the event
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

pub(crate) struct Calendar {}
