pub mod slack;
pub mod web;

use crate::suites::{ToolCall, ToolError, ToolResult};

/// Behavior shared by every environment the agent can act in.
pub trait Environment {
    /// JSON descriptions of every tool this environment provides.
    fn tool_schemas(&self) -> Vec<serde_json::Value>;

    /// JSON description of one tool, or `None` if no tool has that name.
    fn tool_schema(&self, name: &str) -> Option<serde_json::Value>;

    /// Runs the tool named in `call` against this environment.
    fn dispatch(&mut self, call: &ToolCall) -> Result<ToolResult, ToolError>;
}

/// Loads an environment from a JSON file.
/// Placeholders such as `{prompt_injection_channel}` are left as literal text.
///
/// # Errors
///
/// Returns [`LoadError::Io`] if the file can't be read, and [`LoadError::Parse`] if it isn't
/// valid JSON or doesn't match `E` (for example, a missing field or, for types with
/// `deny_unknown_fields`, an extra one).
pub fn load_environment<E: serde::de::DeserializeOwned>(
    path: impl AsRef<std::path::Path>,
) -> Result<E, LoadError> {
    let path = path.as_ref();
    let text = std::fs::read_to_string(path).map_err(|e| LoadError::Io(path.to_owned(), e))?;
    serde_json::from_str(&text).map_err(|e| LoadError::Parse(path.to_owned(), e))
}

/// Why an environment file could not be loaded.
#[derive(Debug)]
pub enum LoadError {
    Io(std::path::PathBuf, std::io::Error),
    Parse(std::path::PathBuf, serde_json::Error),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(path, e) => write!(f, "could not read {}: {e}", path.display()),
            LoadError::Parse(path, e) => write!(f, "could not parse {}: {e}", path.display()),
        }
    }
}

impl std::error::Error for LoadError {}

/// Defines a struct whose fields are environments and implements `Environment` for it.
/// Tools from every field are exposed together, and a call goes to the field that has a
/// tool with that name. Each field's type must have a `TOOL_NAMES` constant (from
/// `#[tools]` or another `combine_environments!`); two fields providing a tool with the
/// same name is a compile error.
///
/// ```ignore
/// combine_environments! {
///     pub struct SlackWeb {
///         pub slack: Slack,
///         pub web: Web,
///     }
/// }
/// ```
macro_rules! combine_environments {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            $(
                $(#[$field_meta:meta])*
                $field_vis:vis $field:ident : $ty:ty
            ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis struct $name {
            $(
                $(#[$field_meta])*
                $field_vis $field: $ty,
            )+
        }

        impl $name {
            /// Names of every tool, known at compile time.
            pub const TOOL_NAMES: &'static [&'static str] =
                &$crate::environments::concat_tool_names::<{ 0 $(+ <$ty>::TOOL_NAMES.len())+ }>(
                    &[$(<$ty>::TOOL_NAMES),+],
                );
        }

        const _: () = $crate::environments::assert_unique_tool_names(<$name>::TOOL_NAMES);

        impl $crate::environments::Environment for $name {
            fn tool_schemas(&self) -> ::std::vec::Vec<::serde_json::Value> {
                let mut schemas = ::std::vec::Vec::new();
                $(
                    schemas.extend($crate::environments::Environment::tool_schemas(&self.$field));
                )+
                schemas
            }

            fn tool_schema(&self, name: &str) -> ::std::option::Option<::serde_json::Value> {
                $(
                    if let ::std::option::Option::Some(schema) =
                        $crate::environments::Environment::tool_schema(&self.$field, name)
                    {
                        return ::std::option::Option::Some(schema);
                    }
                )+
                ::std::option::Option::None
            }

            fn dispatch(
                &mut self,
                call: &$crate::suites::ToolCall,
            ) -> ::std::result::Result<$crate::suites::ToolResult, $crate::suites::ToolError> {
                $(
                    match $crate::environments::Environment::dispatch(&mut self.$field, call) {
                        ::std::result::Result::Err($crate::suites::ToolError::UnknownTool(_)) => {}
                        result => return result,
                    }
                )+
                ::std::result::Result::Err($crate::suites::ToolError::UnknownTool(call.name.clone()))
            }
        }
    };
}
pub(crate) use combine_environments;

/// Joins lists of tool names into one array of length `N` (the total length of `lists`).
#[doc(hidden)]
pub const fn concat_tool_names<const N: usize>(lists: &[&[&'static str]]) -> [&'static str; N] {
    let mut out = [""; N];
    let mut k = 0;
    let mut i = 0;
    while i < lists.len() {
        let mut j = 0;
        while j < lists[i].len() {
            out[k] = lists[i][j];
            k += 1;
            j += 1;
        }
        i += 1;
    }
    assert!(k == N, "tool name count does not match array length");
    out
}

/// Panics, at compile time when used in a `const`, if any tool name appears twice.
#[doc(hidden)]
pub const fn assert_unique_tool_names(names: &[&str]) {
    let mut i = 0;
    while i < names.len() {
        let mut j = i + 1;
        while j < names.len() {
            if str_eq(names[i], names[j]) {
                panic!("{}", names[i]);
            }
            j += 1;
        }
        i += 1;
    }
}

/// Panics, at compile time when used in a `const`, if any of `wanted` is not in `provided`.
pub const fn assert_tools_provided(wanted: &[&str], provided: &[&str]) {
    let mut i = 0;
    while i < wanted.len() {
        let mut j = 0;
        let mut found = false;
        while j < provided.len() {
            if str_eq(wanted[i], provided[j]) {
                found = true;
                break;
            }
            j += 1;
        }
        if !found {
            panic!("{}", wanted[i]);
        }
        i += 1;
    }
}

const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}
