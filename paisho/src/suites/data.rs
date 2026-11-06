//! Loading a suite's starting data from TOML files with injection placeholders, such as
//! `{prompt_injection_channel}`, in their strings.
//!
//! AgentDojo puts the injections into its YAML text before parsing it, so an injection
//! containing YAML syntax can break or reshape the data. Here the data is parsed first and the
//! injections go into its strings, so an injection is always just text.
//!
//! AgentDojo also writes its parsed YAML back out with PyYAML's `yaml.dump` before putting in
//! the injections (`read_suite_file` in `task_suite.py`), which sorts every mapping by key. So
//! its environments see each mapping, such as Slack's `channel_inbox`, in key order, not the
//! file's order. [`parse`] sorts tables the same way.

use std::collections::HashMap;

use toml::{Table, Value};

use crate::suites::UnknownInjections;

/// Parses a suite's TOML data, with every table sorted by key as AgentDojo's data is (see
/// the module documentation). The environments' `IndexMap`s keep that order.
///
/// # Panics
///
/// Panics if `text` isn't valid TOML, which each suite's tests check.
pub(crate) fn parse(text: &str) -> Table {
    let mut table = text.parse().expect("the suite's data is valid TOML");
    sort_tables(&mut table);
    table
}

/// Sorts `table` and every table in it by key, as PyYAML's `yaml.dump` does. Rust compares
/// strings by their bytes, which for UTF-8 is the code point order Python compares them by.
///
/// `toml`'s tables are already sorted unless its `preserve_order` feature is on, but any
/// crate in the build could turn that feature on, so this doesn't rely on it.
fn sort_tables(table: &mut Table) {
    let mut entries: Vec<(String, Value)> = std::mem::take(table).into_iter().collect();
    entries.sort_by(|(a, _), (b, _)| a.cmp(b));
    for (key, mut value) in entries {
        sort_value(&mut value);
        table.insert(key, value);
    }
}

fn sort_value(value: &mut Value) {
    match value {
        Value::Table(table) => sort_tables(table),
        Value::Array(items) => items.iter_mut().for_each(sort_value),
        Value::String(_)
        | Value::Integer(_)
        | Value::Float(_)
        | Value::Boolean(_)
        | Value::Datetime(_) => {}
    }
}

/// The text for each of a suite's injection IDs: its entry in `injections`, or its default
/// from `defaults` if `injections` leaves it out.
///
/// # Errors
///
/// Returns [`UnknownInjections`] listing, sorted, every ID in `injections` that isn't in
/// `defaults`.
pub(crate) fn injection_texts<'a>(
    defaults: &[(&'static str, &'static str)],
    injections: &'a HashMap<String, String>,
) -> Result<HashMap<&'a str, &'a str>, UnknownInjections> {
    let mut texts: HashMap<&str, &str> = defaults.iter().copied().collect();
    let mut unknown = Vec::new();
    for (id, text) in injections {
        match texts.get_mut(id.as_str()) {
            Some(slot) => *slot = text.as_str(),
            None => unknown.push(id.clone()),
        }
    }
    if unknown.is_empty() {
        Ok(texts)
    } else {
        // Sorted so the error doesn't depend on HashMap iteration order.
        unknown.sort();
        Err(UnknownInjections(unknown))
    }
}

/// `data` with every placeholder in its strings filled from `texts`, as a `T`.
///
/// Dates and times can be TOML datetimes, such as `start_time = 2024-05-15T10:00`.
/// Converting from a [`Value`] hands each one to the struct as a string, as written, which
/// the environments' date deserializers expect. `toml::from_str` straight into the structs
/// would hand them over as maps instead and fail.
///
/// # Panics
///
/// Panics if the data doesn't have `T`'s shape, which each suite's tests check.
pub(crate) fn injected<T: serde::de::DeserializeOwned>(
    data: &Table,
    texts: &HashMap<&str, &str>,
) -> T {
    let mut data = Value::Table(data.clone());
    for_each_string(&mut data, &mut |text| *text = fill(text, texts));
    data.try_into()
        .unwrap_or_else(|e| panic!("the suite's data doesn't have the environment's shape: {e}"))
}

/// Calls `f` on every string in `value`, including those nested in arrays and tables. Table
/// keys aren't included.
fn for_each_string(value: &mut Value, f: &mut impl FnMut(&mut String)) {
    match value {
        Value::String(text) => f(text),
        Value::Array(items) => items.iter_mut().for_each(|item| for_each_string(item, f)),
        Value::Table(table) => table
            .iter_mut()
            .for_each(|(_, item)| for_each_string(item, f)),
        Value::Integer(_) | Value::Float(_) | Value::Boolean(_) | Value::Datetime(_) => {}
    }
}

/// `template` with each `{id}` whose ID is in `texts` replaced by its text. Inserted text is
/// not searched for placeholders, and other braces are left as they are.
fn fill(template: &str, texts: &HashMap<&str, &str>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after
            .find('}')
            .and_then(|end| Some((end, texts.get(&after[..end])?)))
        {
            Some((end, text)) => {
                out.push_str(text);
                rest = &after[end + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Checks that every placeholder in the strings of `data` is an ID in `defaults`, and that
/// every ID has a placeholder. A placeholder may appear more than once, as the Slack suite's
/// external channel name does.
///
/// # Panics
///
/// Panics if either fails.
#[cfg(test)]
pub(crate) fn assert_placeholders_are_the_injection_ids(
    data: &[&Table],
    defaults: &[(&'static str, &'static str)],
) {
    use std::collections::HashSet;

    let mut found = HashSet::new();
    for table in data {
        for_each_string(&mut Value::Table((*table).clone()), &mut |text| {
            let mut rest = text.as_str();
            while let Some(start) = rest.find('{') {
                let after = &rest[start + 1..];
                let end = after
                    .find('}')
                    .expect("every brace in the data's strings closes");
                found.insert(after[..end].to_string());
                rest = &after[end + 1..];
            }
        });
    }
    let found: HashSet<&str> = found.iter().map(String::as_str).collect();
    let ids: HashSet<&str> = defaults.iter().map(|(id, _)| *id).collect();
    assert_eq!(found, ids);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_are_sorted_by_key_at_every_depth() {
        let table = parse("b = 1\na = 2\n[[c]]\nz = 1\ny = { q = 1, p = 2 }\n");
        let keys = |table: &Table| table.keys().cloned().collect::<Vec<_>>();
        assert_eq!(keys(&table), ["a", "b", "c"]);
        let item = table["c"][0].as_table().unwrap();
        assert_eq!(keys(item), ["y", "z"]);
        assert_eq!(keys(item["y"].as_table().unwrap()), ["p", "q"]);
    }

    #[test]
    fn fill_replaces_known_placeholders_once() {
        let texts = HashMap::from([("a", "{b}"), ("b", "B")]);
        assert_eq!(fill("x {a} {b} {c} {", &texts), "x {b} B {c} {");
    }

    #[test]
    fn unknown_injections_are_sorted() {
        let injections = HashMap::from([
            ("z".to_string(), String::new()),
            ("a".to_string(), String::new()),
            ("known".to_string(), "text".to_string()),
        ]);
        let error = injection_texts(&[("known", "")], &injections).unwrap_err();
        assert_eq!(error.0, ["a", "z"]);
        let none = HashMap::new();
        let texts = injection_texts(&[("known", "default")], &none).unwrap();
        assert_eq!(texts["known"], "default");
    }
}
