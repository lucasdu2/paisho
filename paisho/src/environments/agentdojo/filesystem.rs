//! AgentDojo's file reader (`default_suites/v1/tools/file_reader.py`).

use indexmap::IndexMap;
use paisho_macros::tools;

/// Files by path.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Filesystem {
    pub(crate) files: IndexMap<String, String>,
}

#[tools]
impl Filesystem {
    /// Reads the contents of the file at the given path.
    // As in AgentDojo, a missing file reads as empty. (A plain comment, as a doc comment
    // here would become part of the tool's description.)
    fn read_file(
        &self,
        #[describe = "The path to the file to read."] file_path: &str,
    ) -> Result<String, String> {
        Ok(self.files.get(file_path).cloned().unwrap_or_default())
    }
}
