//! Email address validation as pydantic's `EmailStr` does it in AgentDojo and AgentDyn, so
//! that a tool given a malformed address fails with the same error, and one given a valid
//! address stores it normalized, as the Python models do.
//!
//! `EmailStr` (pydantic 2.11.4) strips a display name (`Name <addr>`), strips surrounding
//! whitespace and calls the `email-validator` package (2.2.0) without deliverability checks,
//! which checks the syntax and returns the normalized address: the domain lowercased, and
//! some role mailboxes (such as `postmaster`) lowercased too. This module ports those
//! checks, with their messages, for ASCII addresses only:
//!
//! - Addresses with non-ASCII characters (internationalized local parts or domains) and
//!   Punycode domains (`xn--` labels) are rejected with a message of paisho's, where
//!   `email-validator` may accept them after Unicode and IDNA normalization.
//! - For a malformed bracketed IP address after the @-sign, the message leaves out the
//!   Python `ipaddress` module's explanation. Such addresses are rejected either way.
//!
//! The tests compare this module with pydantic on a table of inputs in `email_str/cases.json`:
//! edge cases and every address-like string in the suites' data, each with the normalized
//! address or message pydantic 2.11.4 (with `email-validator` 2.2.0) gave for it.
//!
//! Environments store addresses as [`EmailStr`], which can only hold a valid, normalized
//! address, so no tool can store an invalid one. That includes addresses AgentDojo and
//! AgentDyn store unchecked, such as calendar participants added to an event and the address
//! a file is shared with. Tools take addresses as strings and convert them with
//! [`Validation::run`], which reports every invalid one with pydantic's message, where
//! deserializing the arguments as `EmailStr` would stop at the first.

/// The most characters pydantic accepts, before anything else is checked.
const MAX_EMAIL_LENGTH: usize = 2048;
const EMAIL_MAX_LENGTH: usize = 254;
const LOCAL_PART_MAX_LENGTH: usize = 64;
const DNS_LABEL_LENGTH_LIMIT: usize = 63;
const DOMAIN_MAX_LENGTH: usize = 253;

/// Local parts `email-validator` lowercases (RFC 2142).
const CASE_INSENSITIVE_MAILBOX_NAMES: [&str; 15] = [
    "info",
    "marketing",
    "sales",
    "support",
    "abuse",
    "noc",
    "security",
    "postmaster",
    "hostmaster",
    "usenet",
    "news",
    "webmaster",
    "www",
    "uucp",
    "ftp",
];

/// Domains `email-validator` rejects, with their subdomains.
const SPECIAL_USE_DOMAIN_NAMES: [&str; 6] =
    ["arpa", "invalid", "local", "localhost", "onion", "test"];

/// A valid email address, normalized as pydantic's `EmailStr` normalizes it. The only ways to
/// make one are [`EmailStr::parse`], deserializing (which validates, so a suite's data is
/// checked when it loads) and [`Validation`]. It serializes as a plain string and compares
/// equal to strings with the same text.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize)]
#[serde(transparent)]
pub(crate) struct EmailStr(String);

impl EmailStr {
    /// `value` normalized, or pydantic's message for why it isn't a valid address.
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        normalize(value).map(EmailStr)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for EmailStr {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for EmailStr {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for EmailStr {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for EmailStr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<EmailStr> for String {
    fn from(email: EmailStr) -> String {
        email.0
    }
}

impl PartialEq<str> for EmailStr {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for EmailStr {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<String> for EmailStr {
    fn eq(&self, other: &String) -> bool {
        &self.0 == other
    }
}

impl PartialEq<EmailStr> for str {
    fn eq(&self, other: &EmailStr) -> bool {
        self == other.0
    }
}

impl PartialEq<EmailStr> for &str {
    fn eq(&self, other: &EmailStr) -> bool {
        *self == other.0
    }
}

impl PartialEq<EmailStr> for String {
    fn eq(&self, other: &EmailStr) -> bool {
        *self == other.0
    }
}

impl<'de> serde::Deserialize<'de> for EmailStr {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        EmailStr::parse(&value).map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for EmailStr {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "EmailStr".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "string", "format": "email" })
    }
}

/// `value` as pydantic's `EmailStr` normalizes it, or the message pydantic gives for it,
/// such as `value is not a valid email address: An email address must have an @-sign.`.
fn normalize(value: &str) -> Result<String, String> {
    let reason = |reason: String| format!("value is not a valid email address: {reason}");
    if value.chars().count() > MAX_EMAIL_LENGTH {
        return Err(reason(format!(
            "Length must not exceed {MAX_EMAIL_LENGTH} characters"
        )));
    }
    let address = pretty_address(value).unwrap_or(value);
    validate(address.trim_matches(is_py_space)).map_err(reason)
}

/// Validates a model's email fields, reporting every invalid one as pydantic's
/// `ValidationError` does, such as:
///
/// ```text
/// 1 validation error for Email
/// recipients.0
///   value is not a valid email address: An email address must have an @-sign. [type=value_error, input_value='bob', input_type=str]
/// ```
///
/// Use it through [`Validation::run`].
pub(crate) struct Validation {
    errors: Vec<String>,
}

impl Validation {
    /// What `check` returns, if every address it checked with `validation.field` and
    /// `validation.list` is valid, or else the errors for the model named `model` (such as
    /// `"Email"`):
    ///
    /// ```ignore
    /// let (recipients, cc) = Validation::run("Email", |v| {
    ///     (v.list("recipients", &recipients), v.list("cc", &cc))
    /// })?;
    /// ```
    ///
    /// Inside `check`, an invalid address is returned as given; it can't escape, since `run`
    /// then fails.
    pub(crate) fn run<T>(
        model: &'static str,
        check: impl FnOnce(&mut Validation) -> T,
    ) -> Result<T, String> {
        let mut validation = Validation { errors: Vec::new() };
        let value = check(&mut validation);
        if validation.errors.is_empty() {
            return Ok(value);
        }
        let n = validation.errors.len();
        let s = if n == 1 { "" } else { "s" };
        Err(format!(
            "{n} validation error{s} for {model}\n{}",
            validation.errors.join("\n")
        ))
    }

    /// The field `field`.
    pub(crate) fn field(&mut self, field: &str, value: &str) -> EmailStr {
        self.check(field.to_string(), value)
    }

    /// The list field `field`.
    pub(crate) fn list<S: AsRef<str>>(&mut self, field: &str, values: &[S]) -> Vec<EmailStr> {
        values
            .iter()
            .enumerate()
            .map(|(i, value)| self.check(format!("{field}.{i}"), value.as_ref()))
            .collect()
    }

    fn check(&mut self, location: String, value: &str) -> EmailStr {
        EmailStr::parse(value).unwrap_or_else(|message| {
            self.errors.push(format!(
                "{location}\n  {message} [type=value_error, input_value={}, input_type=str]",
                truncated_repr(value)
            ));
            EmailStr(value.to_string())
        })
    }
}

/// Python's `repr` of a string, shortened as pydantic shortens input values.
fn truncated_repr(value: &str) -> String {
    let quote = if value.contains('\'') && !value.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut repr = String::from(quote);
    for c in value.chars() {
        match c {
            '\\' => repr.push_str("\\\\"),
            '\n' => repr.push_str("\\n"),
            '\r' => repr.push_str("\\r"),
            '\t' => repr.push_str("\\t"),
            c if c == quote => {
                repr.push('\\');
                repr.push(c);
            }
            c if c.is_ascii_control() => repr.push_str(&format!("\\x{:02x}", c as u32)),
            c => repr.push(c),
        }
    }
    repr.push(quote);
    let chars: Vec<char> = repr.chars().collect();
    if chars.len() > 50 {
        let start: String = chars[..25].iter().collect();
        let end: String = chars[chars.len() - 24..].iter().collect();
        format!("{start}...{end}")
    } else {
        repr
    }
}

/// Whether Python's `str.isspace` holds for `c`, which `\s` and `strip` use.
fn is_py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// The address in a "pretty" address such as `John Doe <john@example.com>`, matched as
/// pydantic's `pretty_email_regex` does, or `None` if `value` isn't one.
fn pretty_address(value: &str) -> Option<&str> {
    let is_name_char = |c: char| c.is_alphanumeric() || "_!#$%&'*+-/=?^`{|}~".contains(c);
    let rest = value.trim_start_matches(is_py_space);
    // The address runs to the last `>`, which only whitespace may follow.
    let trimmed = rest.trim_end_matches(is_py_space);
    let close = trimmed.strip_suffix('>').map(str::len)?;
    let address = |open: usize| {
        let address = &rest[open + 1..close];
        (open < close && !address.is_empty() && !address.contains('\n')).then_some(address)
    };
    let first = rest.chars().next()?;
    if first == '<' {
        address(0)
    } else if is_name_char(first) {
        // A name of words of name characters separated by whitespace, then the address.
        let open = rest.find(|c: char| !is_name_char(c) && !is_py_space(c))?;
        (rest[open..].starts_with('<'))
            .then(|| address(open))
            .flatten()
    } else if first == '"' {
        // A quoted name, which may contain anything, closed by the last quote that whitespace
        // and the address follow.
        rest.match_indices('"')
            .map(|(i, _)| i)
            .filter(|&i| i >= 2)
            .rev()
            .find_map(|i| {
                let after = &rest[i + 1..];
                let open = i + 1 + (after.len() - after.trim_start_matches(is_py_space).len());
                rest[open..]
                    .starts_with('<')
                    .then(|| address(open))
                    .flatten()
            })
    } else {
        None
    }
}

/// `email-validator`'s `validate_email(email, check_deliverability=False)`, returning the
/// normalized address or why it isn't valid.
fn validate(email: &str) -> Result<String, String> {
    if !email.is_ascii() {
        return Err(
            "The email address contains non-ASCII characters, which paisho doesn't \
                    support."
                .to_string(),
        );
    }
    let (display_name, local_part, domain_part, quoted) = split_email(email)?;

    let mut local = validate_local_part(&local_part, quoted)?;
    if quoted {
        return Err("Quoting the part before the @-sign is not allowed here.".to_string());
    }
    if CASE_INSENSITIVE_MAILBOX_NAMES.contains(&local.to_lowercase().as_str()) {
        local = local.to_lowercase();
    }

    let domain = if domain_part.is_empty() {
        return Err("There must be something after the @-sign.".to_string());
    } else if domain_part.starts_with('[') && domain_part.ends_with(']') {
        validate_domain_literal(&domain_part[1..domain_part.len() - 1])?;
        return Err("A bracketed IP address after the @-sign is not allowed here.".to_string());
    } else {
        validate_domain_name(&domain_part)?
    };

    // The original and normalized addresses have the same length, since normalizing only
    // changes case.
    let original_length = local_part.len() + 1 + domain_part.len();
    if original_length > EMAIL_MAX_LENGTH {
        return Err(format!(
            "The email address is too long {}.",
            length_reason(original_length, EMAIL_MAX_LENGTH)
        ));
    }
    if display_name.is_some() {
        return Err(
            "A display name and angle brackets around the email address are not permitted here."
                .to_string(),
        );
    }
    Ok(format!("{local}@{domain}"))
}

/// The display name (if the address is in angle brackets), the unquoted local part, the
/// domain part and whether the local part was quoted.
fn split_email(email: &str) -> Result<(Option<String>, String, String, bool), String> {
    let (left, right) = split_at_unquoted_special(email, &['@', '<'])?;
    let (display_name, local_part, domain_part) = if right.starts_with('<') {
        let left = left.trim_end_matches(is_py_space);
        let (display_name, display_name_quoted) = unquote(left)?;
        if !display_name_quoted {
            let bad = bad_chars(&display_name, |c| (is_atext(c) || c == ' ') && c != '.');
            if !bad.is_empty() {
                return Err(format!(
                    "The display name contains invalid characters when not quoted: {bad}."
                ));
            }
        }
        check_unsafe_chars(&display_name, true)?;
        if !right.contains('>') {
            return Err(
                "An open angle bracket at the start of the email address has to be \
                        followed by a close angle bracket at the end."
                    .to_string(),
            );
        }
        let right = right.trim_end_matches(' ');
        if !right.ends_with('>') {
            return Err("There can't be anything after the email address.".to_string());
        }
        let addr_spec = right[1..].trim_end_matches('>');
        let (local, domain) = split_at_unquoted_special(addr_spec, &['@'])?;
        (Some(display_name), local, domain)
    } else {
        (None, left, right)
    };
    let domain_part = domain_part.strip_prefix('@').unwrap_or(domain_part);
    let (local_part, quoted) = unquote(local_part)?;
    Ok((display_name, local_part, domain_part.to_string(), quoted))
}

/// `text` split before its first unquoted character in `specials`.
fn split_at_unquoted_special<'a>(
    text: &'a str,
    specials: &[char],
) -> Result<(&'a str, &'a str), String> {
    let mut inside_quote = false;
    let mut escaped = false;
    for (i, c) in text.char_indices() {
        if inside_quote {
            if c == '\\' && !escaped {
                escaped = true;
            } else if c == '"' && !escaped {
                inside_quote = false;
                escaped = false;
            } else {
                escaped = false;
            }
        } else if c == '"' {
            inside_quote = true;
        } else if specials.contains(&c) {
            return Ok((&text[..i], &text[i..]));
        }
    }
    Err("An email address must have an @-sign.".to_string())
}

/// `text` with its surrounding quotes removed and escapes undone, if it starts with a quote,
/// and whether it did.
fn unquote(text: &str) -> Result<(String, bool), String> {
    let mut quoted = false;
    let mut escaped = false;
    let mut value = String::new();
    for (i, c) in text.char_indices() {
        if quoted {
            if escaped {
                value.push(c);
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                let after = &text[i + 1..];
                if !after.is_empty() {
                    let shown: Vec<String> = after.chars().map(display_char).collect();
                    return Err(format!(
                        "Extra character(s) found after close quote: {}",
                        shown.join(", ")
                    ));
                }
                break;
            } else {
                value.push(c);
            }
        } else if i == 0 && c == '"' {
            quoted = true;
        } else {
            value.push(c);
        }
    }
    Ok((value, quoted))
}

/// The local part, normalized, or why it isn't valid.
fn validate_local_part(local: &str, quoted: bool) -> Result<String, String> {
    if local.is_empty() {
        return Err("There must be something before the @-sign.".to_string());
    }
    if local.len() > LOCAL_PART_MAX_LENGTH {
        return Err(format!(
            "The email address is too long before the @-sign {}.",
            length_reason(local.len(), LOCAL_PART_MAX_LENGTH)
        ));
    }
    if is_dot_atom(local) {
        return Ok(local.to_string());
    }
    if quoted {
        let bad = bad_chars(local, |c| (' '..='~').contains(&c));
        if !bad.is_empty() {
            return Err(format!(
                "The email address contains invalid characters in quotes before the @-sign: \
                 {bad}."
            ));
        }
        check_unsafe_chars(local, true)?;
        let escaped: String = local
            .chars()
            .flat_map(|c| match c {
                '"' | '\\' => vec!['\\', c],
                c => vec![c],
            })
            .collect();
        return Ok(format!("\"{escaped}\""));
    }
    let bad = bad_chars(local, |c| is_atext(c) || c == '.');
    if !bad.is_empty() {
        return Err(format!(
            "The email address contains invalid characters before the @-sign: {bad}."
        ));
    }
    check_dot_atom(
        local,
        "An email address cannot start with a {}.",
        "An email address cannot have a {} immediately before the @-sign.",
        false,
    )?;
    Err("The email address contains invalid characters before the @-sign.".to_string())
}

/// Checks the address in brackets after the @-sign, which is never allowed, so that a
/// malformed one is reported as `email-validator` reports it (without the explanation of
/// Python's `ipaddress`).
fn validate_domain_literal(literal: &str) -> Result<(), String> {
    if !literal.is_empty() && literal.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return match literal.parse::<std::net::Ipv4Addr>() {
            Ok(_) => Ok(()),
            Err(_) => Err(
                "The address in brackets after the @-sign is not valid: It is not \
                           an IPv4 address or is missing an address literal tag."
                    .to_string(),
            ),
        };
    }
    if let Some(address) = literal.strip_prefix("IPv6:") {
        return match address.parse::<std::net::Ipv6Addr>() {
            Ok(_) => Ok(()),
            Err(_) => {
                Err("The IPv6 address in brackets after the @-sign is not valid.".to_string())
            }
        };
    }
    if !literal.contains(':') {
        return Err(
            "The part after the @-sign in brackets is not an IPv4 address and has no \
                    address literal tag."
                .to_string(),
        );
    }
    let bad = bad_chars(literal, |c| ('!'..='~').contains(&c));
    if !bad.is_empty() {
        return Err(format!(
            "The part after the @-sign contains invalid characters in brackets: {bad}."
        ));
    }
    Err(
        "The part after the @-sign contains an invalid address literal tag in brackets."
            .to_string(),
    )
}

/// The domain, normalized, or why it isn't valid.
fn validate_domain_name(original: &str) -> Result<String, String> {
    let bad = bad_chars(original, |c| {
        c.is_ascii_alphanumeric() || c == '-' || c == '.'
    });
    if !bad.is_empty() {
        return Err(format!(
            "The part after the @-sign contains invalid characters: {bad}."
        ));
    }
    let domain = original.to_ascii_lowercase();
    check_dot_atom(
        &domain,
        "An email address cannot have a {} immediately after the @-sign.",
        "An email address cannot end with a {}.",
        true,
    )?;
    for label in domain.split('.') {
        if label.len() >= 4 && &label[2..4] == "--" && &label[..2] != "xn" {
            return Err(
                "An email address cannot have two letters followed by two dashes \
                        immediately after the @-sign or after a period, except Punycode."
                    .to_string(),
            );
        }
        if label.starts_with("xn--") {
            return Err(
                "The part after the @-sign is a Punycode (internationalized) domain, \
                        which paisho doesn't support."
                    .to_string(),
            );
        }
    }
    if domain.len() > DOMAIN_MAX_LENGTH {
        let diff = domain.len() - DOMAIN_MAX_LENGTH;
        let s = if diff > 1 { "s" } else { "" };
        return Err(if domain == original {
            format!(
                "The email address is too long after the @-sign ({diff} character{s} too many)."
            )
        } else {
            format!(
                "The email address is too long after the @-sign ({diff} byte{s} too many after \
                 IDNA encoding)."
            )
        });
    }
    for label in domain.split('.') {
        if label.len() > DNS_LABEL_LENGTH_LIMIT {
            return Err(format!(
                "After the @-sign, periods cannot be separated by so many characters {}.",
                length_reason(label.len(), DNS_LABEL_LENGTH_LIMIT)
            ));
        }
    }
    if !domain.contains('.') {
        return Err("The part after the @-sign is not valid. It should have a period.".to_string());
    }
    if !domain.ends_with(|c: char| c.is_ascii_alphabetic()) {
        return Err(
            "The part after the @-sign is not valid. It is not within a valid top-level \
                    domain."
                .to_string(),
        );
    }
    for special in SPECIAL_USE_DOMAIN_NAMES {
        if domain == special || domain.ends_with(&format!(".{special}")) {
            return Err(
                "The part after the @-sign is a special-use or reserved name that \
                        cannot be used with email."
                    .to_string(),
            );
        }
    }
    Ok(domain)
}

/// `email-validator`'s `check_dot_atom`: no period at either end or twice in a row, and for a
/// hostname, no hyphen at either end or next to a period.
fn check_dot_atom(label: &str, start: &str, end: &str, is_hostname: bool) -> Result<(), String> {
    let error = |template: &str, what: &str| Err(template.replace("{}", what));
    if label.ends_with('.') {
        return error(end, "period");
    }
    if label.starts_with('.') {
        return error(start, "period");
    }
    if label.contains("..") {
        return Err("An email address cannot have two periods in a row.".to_string());
    }
    if is_hostname {
        if label.ends_with('-') {
            return error(end, "hyphen");
        }
        if label.starts_with('-') {
            return error(start, "hyphen");
        }
        if label.contains(".-") || label.contains("-.") {
            return Err(
                "An email address cannot have a period and a hyphen next to each other."
                    .to_string(),
            );
        }
    }
    Ok(())
}

/// Rejects control characters, and spaces unless `allow_space`.
fn check_unsafe_chars(s: &str, allow_space: bool) -> Result<(), String> {
    let mut bad: Vec<char> = s
        .chars()
        .filter(|&c| c.is_ascii_control() || (c == ' ' && !allow_space))
        .collect();
    bad.sort_unstable();
    bad.dedup();
    if bad.is_empty() {
        return Ok(());
    }
    let shown: Vec<String> = bad.into_iter().map(display_char).collect();
    Err(format!(
        "The email address contains unsafe characters: {}.",
        shown.join(", ")
    ))
}

/// The characters of `s` that `ok` rejects, shown and sorted as `email-validator` lists them.
fn bad_chars(s: &str, ok: impl Fn(char) -> bool) -> String {
    let mut shown: Vec<String> = s.chars().filter(|&c| !ok(c)).map(display_char).collect();
    shown.sort();
    shown.dedup();
    shown.join(", ")
}

/// `email-validator`'s `safe_character_display` for an ASCII character.
fn display_char(c: char) -> String {
    match c {
        '\\' => "\"\\\"".to_string(),
        '\'' => "\"'\"".to_string(),
        ' ' => "SPACE".to_string(),
        c if c.is_ascii_control() => format!("U+{:04X}", c as u32),
        c => format!("'{c}'"),
    }
}

/// Whether `c` is allowed in an unquoted local part, other than a period (RFC 5322 atext).
fn is_atext(c: char) -> bool {
    c.is_ascii_alphanumeric() || "_!#$%&'*+-/=?^`{|}~".contains(c)
}

/// Whether `s` is atext separated by single periods.
fn is_dot_atom(s: &str) -> bool {
    s.split('.')
        .all(|atom| !atom.is_empty() && atom.chars().all(is_atext))
}

/// How far `length` is over `limit`, as `email-validator` says it.
fn length_reason(length: usize, limit: usize) -> String {
    let diff = length - limit;
    let s = if diff > 1 { "s" } else { "" };
    format!("({diff} character{s} too many)")
}

#[cfg(test)]
mod tests {
    use super::{EmailStr, Validation, normalize};

    /// Every case in `email_str/cases.json` as pydantic gave it; for the cases whose
    /// messages this module doesn't match, whether they're valid; and that the
    /// internationalized addresses pydantic accepts are rejected.
    #[test]
    fn matches_pydantic() {
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("email_str/cases.json")).unwrap();
        let mut failures = Vec::new();
        for case in cases["cases"].as_array().unwrap() {
            let input = case["input"].as_str().unwrap();
            let expected = match (&case["normalized"], &case["error"]) {
                (serde_json::Value::String(normalized), _) => Ok(normalized.clone()),
                (_, serde_json::Value::String(error)) => Err(error.clone()),
                _ => panic!("bad case {case}"),
            };
            let actual = normalize(input);
            if actual != expected {
                failures.push(format!("{input:?}: expected {expected:?}, got {actual:?}"));
            }
        }
        for case in cases["validity_only"].as_array().unwrap() {
            let input = case["input"].as_str().unwrap();
            if normalize(input).is_ok() != case["normalized"].is_string() {
                failures.push(format!("{input:?}: expected {case}"));
            }
        }
        for case in cases["unsupported"].as_array().unwrap() {
            let input = case["input"].as_str().unwrap();
            if normalize(input).is_ok() {
                failures.push(format!(
                    "{input:?}: expected it to be rejected as unsupported"
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn errors_are_reported_like_pydantic() {
        let cases: serde_json::Value =
            serde_json::from_str(include_str!("email_str/cases.json")).unwrap();
        let recipients = ["ok@Example.COM", "bob", &"x".repeat(80), "it's", "a\"b'c"];
        let error = Validation::run("Email", |v| {
            (v.list("recipients", &recipients), v.list("cc", &["a\nb"]))
        })
        .unwrap_err();
        assert_eq!(error, cases["validation_error"].as_str().unwrap());

        let valid = Validation::run("Email", |v| v.list("recipients", &["ok@Example.COM"]));
        assert_eq!(valid.unwrap(), ["ok@example.com"]);
    }

    /// Addresses deserialize only if valid, and compare equal to their text.
    #[test]
    fn email_strs_are_validated_when_deserialized() {
        let email: EmailStr = serde_json::from_value(serde_json::json!("A@B.COM")).unwrap();
        assert_eq!(email, "A@b.com");
        assert_eq!(serde_json::to_value(&email).unwrap(), "A@b.com");
        let invalid = serde_json::from_value::<EmailStr>(serde_json::json!("ab"));
        assert!(
            invalid
                .unwrap_err()
                .to_string()
                .contains("must have an @-sign")
        );
    }
}
