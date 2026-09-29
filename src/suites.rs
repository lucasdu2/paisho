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
