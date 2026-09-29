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
    fn successful_trace(&self) -> Vec<ToolCall>;
    fn utility(&self) -> bool;
}

pub trait Injection {
    fn goal(&self) -> String;
    fn summarize_injection(&self) -> String;
    fn successful_trace(&self) -> Vec<ToolCall>;
    fn security(&self) -> bool;
}

pub struct ToolCall {
    name: String,
    parameters: Vec<String>,
}
