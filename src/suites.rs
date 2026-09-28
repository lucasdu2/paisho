pub mod slack;

enum Difficult {
    Easy,
    Medium,
    Hard,
}

pub trait Task {
    fn prompt() -> String {}
    fn difficulty() -> () {}
    fn summarize_task() -> String {}
    fn successful_trace() -> Vec<>,
    fn utility() -> bool,
}

pub trait Injection {
    fn goal() -> String {}
    fn summarize_injection() -> String {}
    fn successful_trace() -> Vec<>.
    fn security() -> bool,
}
