use chrono::NaiveDateTime;
use indexmap::IndexMap;
use paisho_macros::tools;

use crate::environments::email_str::{EmailStr, Validation};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SharingPermission {
    R,
    Rw,
}

type CloudDriveFileID = String;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub(crate) struct CloudDriveFile {
    /// The unique identifier of the file
    pub(crate) id: CloudDriveFileID,
    /// The name of the file
    pub(crate) filename: String,
    /// The content of the file
    pub(crate) content: String,
    /// The email of the owner of the file
    pub(crate) owner: EmailStr,
    /// The last modified timestamp
    #[serde(deserialize_with = "crate::environments::deserialize_date_time")]
    pub(crate) last_modified: NaiveDateTime,
    /// The object containing emails with their sharing permissions
    #[serde(default)]
    pub(crate) shared_with: IndexMap<EmailStr, SharingPermission>,
    /// The size of the file in bytes
    #[serde(default)]
    pub(crate) size: usize,
}

/// The length of `content` as Python's `len` counts it, in characters. AgentDojo calls this
/// the size in bytes, but it is the number of characters.
fn size(content: &str) -> usize {
    content.chars().count()
}

/// The user's cloud drive.
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct CloudDrive {
    pub(crate) initial_files: Vec<CloudDriveFile>,
    pub(crate) account_email: EmailStr,
    pub(crate) files: IndexMap<CloudDriveFileID, CloudDriveFile>,
}

impl CloudDrive {
    /// A drive holding `initial_files`. Each file's `size` is computed from its content,
    /// whatever it was given as, as AgentDojo's validator does.
    ///
    /// # Errors
    ///
    /// Returns an error if two of `initial_files` share an ID.
    pub(crate) fn new(
        mut initial_files: Vec<CloudDriveFile>,
        account_email: EmailStr,
    ) -> Result<Self, String> {
        for file in &mut initial_files {
            file.size = size(&file.content);
        }
        let mut files = IndexMap::new();
        let mut duplicate_ids = Vec::new();
        for file in &initial_files {
            if files.insert(file.id.clone(), file.clone()).is_some() {
                duplicate_ids.push(file.id.clone());
            }
        }
        if !duplicate_ids.is_empty() {
            return Err(format!(
                "File IDs must be unique. Duplicates: {duplicate_ids:?}"
            ));
        }
        Ok(CloudDrive {
            initial_files,
            account_email,
            files,
        })
    }

    fn file_mut(&mut self, file_id: &str) -> Result<&mut CloudDriveFile, String> {
        self.files
            .get_mut(file_id)
            .ok_or_else(|| format!("File with ID '{file_id}' not found."))
    }

    /// The files whose name contains `filename`, ignoring case.
    pub(crate) fn files_named(&self, filename: &str) -> Vec<&CloudDriveFile> {
        let filename = filename.to_lowercase();
        self.files
            .values()
            .filter(|file| file.filename.to_lowercase().contains(&filename))
            .collect()
    }

    /// The files whose content contains `query`, ignoring case.
    pub(crate) fn files_containing(&self, query: &str) -> Vec<&CloudDriveFile> {
        let query = query.to_lowercase();
        self.files
            .values()
            .filter(|file| file.content.to_lowercase().contains(&query))
            .collect()
    }

    /// Each file's size, by ID.
    pub(crate) fn get_file_sizes(&self) -> IndexMap<&str, usize> {
        self.files
            .values()
            .map(|file| (file.id.as_str(), file.size))
            .collect()
    }
}

#[tools]
impl CloudDrive {
    /// Get a file from a cloud drive by its filename. It returns a list of files. Each file
    /// contains the file id, the content, the file type, and the filename.
    fn search_files_by_filename(
        &self,
        #[describe = "The name of the file to retrieve."] filename: &str,
    ) -> Result<Vec<&CloudDriveFile>, String> {
        Ok(self.files_named(filename))
    }

    /// Create a new file in the cloud drive.
    fn create_file(
        &mut self,
        #[describe = "The name of the file to create."] filename: &str,
        #[describe = "The content of the file to create."] content: &str,
    ) -> Result<CloudDriveFile, String> {
        let file = CloudDriveFile {
            id: crate::environments::next_id(self.files.keys())?,
            filename: filename.to_string(),
            content: content.to_string(),
            owner: self.account_email.clone(),
            last_modified: crate::environments::now(),
            shared_with: IndexMap::new(),
            size: size(content),
        };
        self.files.insert(file.id.clone(), file.clone());
        Ok(file)
    }

    /// Delete a file from the cloud drive by its filename. It returns the file that was
    /// deleted.
    fn delete_file(
        &mut self,
        #[describe = "The name of the file to delete."] file_id: &str,
    ) -> Result<CloudDriveFile, String> {
        self.files
            .shift_remove(file_id)
            .ok_or_else(|| format!("File with ID '{file_id}' not found."))
    }

    /// Get a file from a cloud drive by its ID.
    fn get_file_by_id(
        &self,
        #[describe = "The ID of the file to retrieve."] file_id: &str,
    ) -> Result<&CloudDriveFile, String> {
        self.files
            .get(file_id)
            .ok_or_else(|| format!("File with ID '{file_id}' not found."))
    }

    /// Retrieve all files in the cloud drive.
    fn list_files(&self) -> Result<Vec<&CloudDriveFile>, String> {
        Ok(self.files.values().collect())
    }

    /// Share a file with a user.
    fn share_file(
        &mut self,
        #[describe = "The ID of the file to share."] file_id: &str,
        #[describe = "The email of the user to share the file with."] email: &str,
        #[describe = "The permission level to grant the user."] permission: SharingPermission,
    ) -> Result<CloudDriveFile, String> {
        // AgentDojo shares with any string; here the address must be valid.
        let email = Validation::run("Input schema for `share_file`", |v| v.field("email", email));
        let file = self.file_mut(file_id)?;
        let email = email?;
        file.shared_with.insert(email, permission);
        Ok(file.clone())
    }

    /// Append content to a file in the cloud drive.
    fn append_to_file(
        &mut self,
        #[describe = "The ID of the file to append content to."] file_id: &str,
        #[describe = "The content to append to the file."] content: &str,
    ) -> Result<CloudDriveFile, String> {
        let file = self.file_mut(file_id)?;
        file.content.push_str(content);
        file.size = size(&file.content);
        file.last_modified = crate::environments::now();
        Ok(file.clone())
    }

    /// Search for files in the cloud drive by content.
    fn search_files(
        &self,
        #[describe = "The string to search for in the files."] query: &str,
    ) -> Result<Vec<&CloudDriveFile>, String> {
        let matches = self.files_containing(query);
        if matches.is_empty() {
            return Err(format!(
                "No files found with the given query ('{query}'). Try a different search term."
            ));
        }
        Ok(matches)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::suites::{ToolContent, ToolResult, call};

    const EMMA: &str = "emma.johnson@bluesparrowtech.com";

    fn file(value: Value) -> CloudDriveFile {
        serde_json::from_value(value).unwrap()
    }

    /// Two files, the first given a wrong size.
    fn drive() -> CloudDrive {
        let files = vec![
            file(json!({
                "id": "0", "filename": "feedback.xlsx", "content": "Great product!",
                "owner": EMMA, "last_modified": "2024-05-08T10:30:00", "size": 999,
            })),
            file(json!({
                "id": "1", "filename": "Team Building Ideas.docx",
                "content": "Ideas: hiking, café visit.",
                "owner": EMMA, "last_modified": "2024-05-10T09:00:00",
                "shared_with": { "alex.martin@bluesparrowtech.com": "r" },
            })),
        ];
        CloudDrive::new(files, EmailStr::parse(EMMA).unwrap()).unwrap()
    }

    fn run(drive: &mut CloudDrive, name: &str, arguments: Value) -> ToolResult {
        drive.dispatch(&call(name, arguments)).unwrap()
    }

    fn text(result: &ToolResult) -> &str {
        let ToolContent::Text { text } = &result.content[0];
        text
    }

    fn ids(result: &ToolResult) -> Vec<String> {
        let files = &result.structured_content.as_ref().unwrap()["result"];
        files
            .as_array()
            .unwrap()
            .iter()
            .map(|file| file["id"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn sizes_are_computed_in_characters() {
        let drive = drive();
        // "é" is one character but two bytes, as in Python's `len`.
        assert_eq!(
            drive.get_file_sizes(),
            IndexMap::from([("0", 14), ("1", 26)])
        );
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let files = drive().initial_files;
        let twice = vec![files[0].clone(), files[0].clone()];
        let error = CloudDrive::new(twice, EmailStr::parse(EMMA).unwrap()).unwrap_err();
        assert_eq!(error, "File IDs must be unique. Duplicates: [\"0\"]");
    }

    #[test]
    fn files_are_searched_by_name_and_content_ignoring_case() {
        let mut drive = drive();
        let by_name = run(
            &mut drive,
            "search_files_by_filename",
            json!({ "filename": "TEAM" }),
        );
        assert_eq!(ids(&by_name), ["1"]);
        let nothing = run(
            &mut drive,
            "search_files_by_filename",
            json!({ "filename": "zebra" }),
        );
        assert!(!nothing.is_error);
        assert_eq!(text(&nothing), "[]");

        let by_content = run(&mut drive, "search_files", json!({ "query": "GREAT" }));
        assert_eq!(ids(&by_content), ["0"]);
        let none = run(&mut drive, "search_files", json!({ "query": "zebra" }));
        assert!(none.is_error);
        assert_eq!(
            text(&none),
            "No files found with the given query ('zebra'). Try a different search term."
        );
    }

    #[test]
    fn created_files_get_the_next_id_and_the_account_as_owner() {
        let mut drive = drive();
        let created = run(
            &mut drive,
            "create_file",
            json!({ "filename": "notes.txt", "content": "hello" }),
        );
        assert!(!created.is_error, "{}", text(&created));
        let file = &drive.files["2"];
        assert_eq!((file.owner.as_str(), file.size), (EMMA, 5));
        assert_eq!(
            ids(&run(&mut drive, "list_files", json!({}))),
            ["0", "1", "2"]
        );
    }

    #[test]
    fn appending_updates_the_size() {
        let mut drive = drive();
        let appended = run(
            &mut drive,
            "append_to_file",
            json!({ "file_id": "0", "content": " Five stars." }),
        );
        assert!(!appended.is_error, "{}", text(&appended));
        assert_eq!(drive.files["0"].content, "Great product! Five stars.");
        assert_eq!(drive.files["0"].size, 26);
    }

    #[test]
    fn sharing_sets_a_permission() {
        let mut drive = drive();
        let shared = run(
            &mut drive,
            "share_file",
            json!({ "file_id": "0", "email": "bob@gmail.com", "permission": "rw" }),
        );
        assert!(!shared.is_error, "{}", text(&shared));
        assert_eq!(
            drive.files["0"].shared_with["bob@gmail.com"],
            SharingPermission::Rw
        );
        let bad = run(
            &mut drive,
            "share_file",
            json!({ "file_id": "0", "email": "bob@gmail.com", "permission": "write" }),
        );
        assert!(bad.is_error);
        let invalid = run(
            &mut drive,
            "share_file",
            json!({ "file_id": "0", "email": "carol", "permission": "r" }),
        );
        assert!(invalid.is_error);
        assert!(text(&invalid).contains("value is not a valid email address"));
        assert_eq!(drive.files["0"].shared_with.len(), 1);
    }

    #[test]
    fn deleted_files_are_returned_and_the_rest_keep_their_order() {
        let mut drive = drive();
        run(
            &mut drive,
            "create_file",
            json!({ "filename": "a", "content": "" }),
        );
        let deleted = run(&mut drive, "delete_file", json!({ "file_id": "0" }));
        assert_eq!(ids(&run(&mut drive, "list_files", json!({}))), ["1", "2"]);
        let deleted = &deleted.structured_content.unwrap()["result"];
        assert_eq!(deleted["filename"], "feedback.xlsx");

        let again = run(&mut drive, "get_file_by_id", json!({ "file_id": "0" }));
        assert!(again.is_error);
        assert_eq!(text(&again), "File with ID '0' not found.");
    }

    /// The output schema names `CloudDriveFile`, with permissions as `"r"` or `"rw"`.
    #[test]
    fn file_schema_lists_the_permissions() {
        let schema = CloudDrive::tool_schema("get_file_by_id").unwrap();
        let defs = &schema["output_schema"]["$defs"];
        assert_eq!(defs["SharingPermission"]["enum"], json!(["r", "rw"]));
        assert_eq!(
            defs["CloudDriveFile"]["properties"]["shared_with"]["additionalProperties"],
            json!({ "$ref": "#/$defs/SharingPermission" })
        );
    }
}
