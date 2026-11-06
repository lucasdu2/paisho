//! Loading a suite's starting data from TOML files with injection placeholders, such as
//! `{prompt_injection_channel}`, in their strings.
//!
//! AgentDojo puts the injections into its YAML text before parsing it, so an injection
//! containing YAML syntax can break or reshape the data. Here the data is parsed first and the
//! injections go into its strings, so an injection is always just text.

use std::collections::HashMap;

use toml::{Table, Value};

use crate::suites::UnknownInjections;

/// Parses a suite's TOML data. Tables keep the order the file gives, for the environments'
/// `IndexMap`s, through the `toml` crate's `preserve_order` feature.
///
/// # Panics
///
/// Panics if `text` isn't valid TOML, which each suite's tests check.
pub(crate) fn parse(text: &str) -> Table {
    text.parse().expect("the suite's data is valid TOML")
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
