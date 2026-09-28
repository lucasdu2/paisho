pub trait Attack {
    fn attack_name(&self) -> String;
    fn attack_summary(&self) -> String;
}

/// TemplateAttack ...
struct TemplateAttack {
    template: String,
    username: String,
    model: String,
}
