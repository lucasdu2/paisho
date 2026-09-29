pub mod slack;

pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

pub trait Task {
    fn prompt(&self) -> String;
    fn difficulty(&self) -> Difficulty;
    fn summarize_task(&self) -> String;
    fn completed_trace(&self) -> Vec<ToolCall>;
    fn utility(&self) -> bool;
}

pub trait Injection {
    fn goal(&self) -> String;
    fn summarize_injection(&self) -> String;
    fn injected_trace(&self) -> Vec<ToolCall>;
    fn security(&self) -> bool;
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

/// The result of a tool call, shaped like an MCP `CallToolResult`.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolResult {
    pub result_type: &'static str,
    pub content: Vec<ToolContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_content: Option<serde_json::Value>,
    pub is_error: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolContent {
    Text { text: String },
}

impl ToolResult {
    /// A successful result: `value` as structured content, mirrored as JSON text.
    /// Tools returning `()` serialize to `null` and get a plain "Success" instead.
    pub fn success(value: serde_json::Value) -> Self {
        if value.is_null() {
            return ToolResult {
                result_type: "complete",
                content: vec![ToolContent::Text { text: "Success".to_string() }],
                structured_content: None,
                is_error: false,
            };
        }
        ToolResult {
            result_type: "complete",
            content: vec![ToolContent::Text { text: value.to_string() }],
            structured_content: Some(value),
            is_error: false,
        }
    }

    /// A tool execution error the model can read and recover from.
    pub fn error(message: impl Into<String>) -> Self {
        ToolResult {
            result_type: "complete",
            content: vec![ToolContent::Text { text: message.into() }],
            structured_content: None,
            is_error: true,
        }
    }
}

/// A request that could not be dispatched to any tool (an MCP protocol error).
#[derive(Debug, Clone, PartialEq)]
pub enum ToolError {
    UnknownTool(String),
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolError::UnknownTool(name) => write!(f, "Unknown tool: {name}"),
        }
    }
}
