//! AgentDyn's local filesystem (`default_suites/v1/tools/file_client.py`): a tree of
//! directories and files with permissions, unlike AgentDojo's flat
//! [`crate::environments::agentdojo::filesystem::Filesystem`].
//!
//! Paths are `/`-separated and absolute; empty segments are ignored, so `/a//b/` is `/a/b`.
//! AgentDyn treats an empty segment as a name that is never found.

use indexmap::IndexMap;
use paisho_macros::tools;
use serde_json::Value;

/// What may be done with a file or directory.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub(crate) struct Permissions {
    #[serde(default = "yes")]
    pub(crate) read: bool,
    #[serde(default)]
    pub(crate) write: bool,
    #[serde(default)]
    pub(crate) execute: bool,
}

fn yes() -> bool {
    true
}

impl Permissions {
    /// A new file's: read only.
    pub(crate) const FILE: Permissions = Permissions {
        read: true,
        write: false,
        execute: false,
    };

    /// A new directory's: read and write.
    pub(crate) const DIRECTORY: Permissions = Permissions {
        read: true,
        write: true,
        execute: false,
    };
}

impl Default for Permissions {
    fn default() -> Self {
        Permissions::FILE
    }
}

fn directory_permissions() -> Permissions {
    Permissions::DIRECTORY
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct File {
    pub(crate) content: String,
    #[serde(default)]
    pub(crate) permissions: Permissions,
}

#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Directory {
    #[serde(default)]
    pub(crate) children: IndexMap<String, Node>,
    #[serde(default = "directory_permissions")]
    pub(crate) permissions: Permissions,
}

impl Directory {
    /// An empty directory, readable and writable.
    pub(crate) fn new() -> Self {
        Directory {
            children: IndexMap::new(),
            permissions: Permissions::DIRECTORY,
        }
    }

    /// `name`, or if a child already has it, the first of `name` followed by `suffix(1)`,
    /// `suffix(2)` and so on that none has.
    fn free_name(&self, name: &str, suffix: impl Fn(usize) -> String) -> String {
        if !self.children.contains_key(name) {
            return name.to_string();
        }
        (1..)
            .map(|n| format!("{name}{}", suffix(n)))
            .find(|candidate| !self.children.contains_key(candidate))
            .expect("a directory has finitely many children")
    }
}

/// A file or a directory, written with a `type` of `"file"` or `"directory"`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum Node {
    File(File),
    Directory(Directory),
}

impl Node {
    fn permissions(&self) -> Permissions {
        match self {
            Node::File(file) => file.permissions,
            Node::Directory(directory) => directory.permissions,
        }
    }

    fn kind(&self) -> NodeKind {
        match self {
            Node::File(_) => NodeKind::File,
            Node::Directory(_) => NodeKind::Directory,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum NodeKind {
    File,
    Directory,
}

/// One item of a directory, as `list_directory` describes it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, schemars::JsonSchema)]
pub(crate) struct Entry {
    name: String,
    #[serde(rename = "type")]
    kind: NodeKind,
    permissions: Permissions,
}

/// The tree as the data writes it: a root that must be a directory.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct FileTreeData {
    root: Node,
}

/// The user's local files.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "FileTreeData", into = "FileTreeData")]
pub(crate) struct FileTree {
    pub(crate) root: Directory,
}

impl TryFrom<FileTreeData> for FileTree {
    type Error = String;

    fn try_from(data: FileTreeData) -> Result<Self, String> {
        match data.root {
            Node::Directory(root) => Ok(FileTree { root }),
            Node::File(_) => Err("the filesystem's root must be a directory".to_string()),
        }
    }
}

impl From<FileTree> for FileTreeData {
    fn from(tree: FileTree) -> Self {
        FileTreeData {
            root: Node::Directory(tree.root),
        }
    }
}

/// The names along `path`, leaving out empty ones.
pub(crate) fn segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|name| !name.is_empty()).collect()
}

fn not_found(path: &str) -> String {
    format!("Path {path} not found.")
}

impl FileTree {
    /// The node at `path`, or `None` if there isn't one. The root is at `/`.
    pub(crate) fn node(&self, path: &str) -> Option<&Node> {
        let (last, parents) = match segments(path).split_last() {
            None => return None,
            Some((last, parents)) => (*last, parents.to_vec()),
        };
        let mut directory = &self.root;
        for name in parents {
            match directory.children.get(name)? {
                Node::Directory(child) => directory = child,
                Node::File(_) => return None,
            }
        }
        directory.children.get(last)
    }

    /// Whether a file or directory is at `path`. The root always is.
    pub(crate) fn exists(&self, path: &str) -> bool {
        segments(path).is_empty() || self.node(path).is_some()
    }

    /// The file at `path`, or `None` if there is no file there.
    pub(crate) fn file(&self, path: &str) -> Option<&File> {
        match self.node(path)? {
            Node::File(file) => Some(file),
            Node::Directory(_) => None,
        }
    }

    /// The directory reached by following `names` from the root. `path` is for errors.
    pub(crate) fn directory(&self, names: &[&str], path: &str) -> Result<&Directory, String> {
        let mut directory = &self.root;
        for name in names {
            match directory.children.get(*name) {
                Some(Node::Directory(child)) => directory = child,
                Some(Node::File(_)) => return Err(format!("{path} is a file, not a directory.")),
                None => return Err(not_found(path)),
            }
        }
        Ok(directory)
    }

    /// Like [`FileTree::directory`], for changing it.
    pub(crate) fn directory_mut(
        &mut self,
        names: &[&str],
        path: &str,
    ) -> Result<&mut Directory, String> {
        let mut directory = &mut self.root;
        for name in names {
            match directory.children.get_mut(*name) {
                Some(Node::Directory(child)) => directory = child,
                Some(Node::File(_)) => return Err(format!("{path} is a file, not a directory.")),
                None => return Err(not_found(path)),
            }
        }
        Ok(directory)
    }

    /// The parent directory of `path` and the name `path` has in it.
    fn parent_mut<'p>(&mut self, path: &'p str) -> Result<(&mut Directory, &'p str), String> {
        let names = segments(path);
        let Some((name, parents)) = names.split_last() else {
            return Err("The root directory has no parent.".to_string());
        };
        let parent_path = format!("/{}", parents.join("/"));
        Ok((self.directory_mut(parents, &parent_path)?, name))
    }

    /// Adds `node` to the directory `directory`, as `name`, or `name(1)`, `name(2)` and so
    /// on if `name` is taken, as AgentDyn's `add_node` does for downloads. Returns the name.
    pub(crate) fn add(
        &mut self,
        directory: &str,
        name: &str,
        node: Node,
    ) -> Result<String, String> {
        let directory = self.directory_mut(&segments(directory), directory)?;
        let name = directory.free_name(name, |n| format!("({n})"));
        directory.children.insert(name.clone(), node);
        Ok(name)
    }
}

#[tools]
impl FileTree {
    /// List the metadata of all items inside a specific local directory.
    fn list_directory(
        &self,
        #[describe = "The path to the directory (e.g., '/system' or '/user/documents')."]
        path: &str,
    ) -> Result<Vec<Entry>, String> {
        let directory = self.directory(&segments(path), path)?;
        Ok(directory
            .children
            .iter()
            .map(|(name, node)| Entry {
                name: name.clone(),
                kind: node.kind(),
                permissions: node.permissions(),
            })
            .collect())
    }

    /// Create a new local file at the specified path.
    // As in AgentDyn, the file is read only, and a file already at the path is replaced.
    // Unlike AgentDyn, a directory at the path is not.
    fn create_file(
        &mut self,
        #[describe = "The path to the file to create."] path: &str,
        #[describe = "The initial content of the file."]
        #[serde(default)]
        content: Value,
    ) -> Result<String, String> {
        // AgentDyn writes `str(content)`; any value that isn't a string is written as JSON.
        let content = match content {
            Value::String(text) => text,
            Value::Null => String::new(),
            other => other.to_string(),
        };
        let (parent, name) = self.parent_mut(path)?;
        if let Some(Node::Directory(_)) = parent.children.get(name) {
            return Err(format!("{path} is a directory."));
        }
        parent.children.insert(
            name.to_string(),
            Node::File(File {
                content,
                permissions: Permissions::FILE,
            }),
        );
        Ok(format!("File {path} created"))
    }

    /// Create a new local directory at the specified path, including parent directories.
    fn create_dir(
        &mut self,
        #[describe = "The path where the directory should be created."] path: &str,
    ) -> Result<String, String> {
        let mut directory = &mut self.root;
        for name in segments(path) {
            let child = directory
                .children
                .entry(name.to_string())
                .or_insert_with(|| Node::Directory(Directory::new()));
            match child {
                Node::Directory(child) => directory = child,
                Node::File(_) => {
                    return Err(format!(
                        "Cannot create directory: '{name}' is a file, not a directory."
                    ));
                }
            }
        }
        Ok(format!("Directory {path} created."))
    }

    /// Read the contents of a local file at the given path.
    fn read_file(
        &self,
        #[describe = "The path of the local file to read."] path: &str,
    ) -> Result<String, String> {
        match self.node(path) {
            None if segments(path).is_empty() => {
                Err(format!("Path {path} is a directory, not a file"))
            }
            None => Err(not_found(path)),
            Some(Node::Directory(_)) => Err(format!("Path {path} is a directory, not a file")),
            Some(Node::File(file)) if !file.permissions.read => {
                Err(format!("Read permission denied for {path}"))
            }
            Some(Node::File(file)) => Ok(file.content.clone()),
        }
    }

    /// Delete a local file or directory and all its contents at the given path.
    fn delete_node(
        &mut self,
        #[describe = "The path to the node to delete."] path: &str,
    ) -> Result<String, String> {
        if segments(path).is_empty() {
            return Err("Cannot delete root directory".to_string());
        }
        let (parent, name) = self.parent_mut(path)?;
        match parent.children.shift_remove(name) {
            Some(_) => Ok(format!("{path} successfully deleted.")),
            None => Err(not_found(path)),
        }
    }

    /// Copies a local file or directory from the source path to the destination path.
    fn copy_node(
        &mut self,
        #[describe = "The path of the file or directory to copy."] source_path: &str,
        #[describe = "The destination directory or the new path for the copy."]
        destination_path: &str,
    ) -> Result<String, String> {
        let source_names = segments(source_path);
        let Some(source_name) = source_names.last() else {
            return Err("Cannot copy the root directory.".to_string());
        };
        let copy = self
            .node(source_path)
            .ok_or_else(|| not_found(source_path))?
            .clone();
        let into_directory = match self.node(destination_path) {
            _ if segments(destination_path).is_empty() => true,
            Some(Node::Directory(_)) => true,
            Some(Node::File(_)) => {
                return Err(format!(
                    "Target '{destination_path}' already exists and is a file."
                ));
            }
            None => false,
        };
        let (parent, name) = if into_directory {
            // Into an existing directory, keeping the source's name.
            let directory = self.directory_mut(&segments(destination_path), destination_path)?;
            (directory, *source_name)
        } else {
            // To a new path, with its last name.
            self.parent_mut(destination_path)?
        };
        let name = parent.free_name(name, |n| format!("_{n}"));
        parent.children.insert(name.clone(), copy);
        Ok(format!(
            "Successfully copied '{source_path}' to '{destination_path}' (as '{name}')."
        ))
    }

    /// Moves or renames a local file or directory from the source path to the destination
    /// path.
    // As in AgentDyn, a node with the same name already in the destination is replaced.
    fn move_node(
        &mut self,
        #[describe = "The path of the node to move."] source_path: &str,
        #[describe = "The destination path."] destination_path: &str,
    ) -> Result<String, String> {
        let source = segments(source_path);
        let Some(source_name) = source.last().copied() else {
            return Err("Cannot move the root directory.".to_string());
        };
        if self.node(source_path).is_none() {
            return Err(not_found(source_path));
        }
        let destination = segments(destination_path);
        // Where the node goes: a directory (as names from the root) and its name there.
        let (target, name): (Vec<&str>, &str) = match self.node(destination_path) {
            _ if destination.is_empty() => (Vec::new(), source_name),
            Some(Node::Directory(_)) => (destination.clone(), source_name),
            Some(Node::File(_)) => {
                return Err(format!("Destination '{destination_path}' already exists."));
            }
            None => {
                let (name, parents) = destination.split_last().expect("not the root");
                self.directory(parents, destination_path)?;
                (parents.to_vec(), *name)
            }
        };
        if destination.starts_with(&source) {
            return Err("Cannot move a directory into itself.".to_string());
        }
        let (parent, _) = self.parent_mut(source_path)?;
        let node = parent
            .children
            .shift_remove(source_name)
            .expect("the source exists");
        self.directory_mut(&target, destination_path)
            .expect("the target directory was found above and is not inside the source")
            .children
            .insert(name.to_string(), node);
        Ok(format!(
            "Successfully moved '{source_path}' to '{destination_path}'."
        ))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::suites::{ToolContent, ToolResult, call};

    fn tree() -> FileTree {
        toml::from_str::<toml::Value>(
            r#"
            [root]
            type = "directory"
            [root.children.docs]
            type = "directory"
            [root.children.docs.children."a.txt"]
            type = "file"
            content = "A"
            [root.children.docs.children."secret.txt"]
            type = "file"
            content = "S"
            permissions = { read = false, write = false, execute = false }
            [root.children."b.txt"]
            type = "file"
            content = "B"
            permissions = { read = true, write = true, execute = false }
            "#,
        )
        .unwrap()
        .try_into()
        .unwrap()
    }

    fn run(tree: &mut FileTree, name: &str, arguments: serde_json::Value) -> ToolResult {
        tree.dispatch(&call(name, arguments)).unwrap()
    }

    fn text(result: &ToolResult) -> &str {
        let ToolContent::Text { text } = &result.content[0];
        text
    }

    fn names(tree: &FileTree, path: &str) -> Vec<String> {
        let Some(Node::Directory(directory)) = tree.node(path) else {
            panic!("{path} is not a directory");
        };
        directory.children.keys().cloned().collect()
    }

    #[test]
    fn data_defaults_and_round_trip() {
        let tree = tree();
        assert_eq!(tree.root.permissions, Permissions::DIRECTORY);
        assert_eq!(
            tree.file("/docs/a.txt").unwrap().permissions,
            Permissions::FILE
        );
        let json = serde_json::to_value(&tree).unwrap();
        assert_eq!(json["root"]["type"], "directory");
        assert_eq!(json["root"]["children"]["b.txt"]["type"], "file");
        let back: FileTree = serde_json::from_value(json).unwrap();
        assert_eq!(back, tree);
        let file_root = json!({ "root": { "type": "file", "content": "" } });
        assert!(serde_json::from_value::<FileTree>(file_root).is_err());
    }

    #[test]
    fn listing_and_reading() {
        let mut tree = tree();
        let listed = run(&mut tree, "list_directory", json!({ "path": "/" }));
        assert_eq!(
            listed.structured_content.unwrap()["result"],
            json!([
                { "name": "b.txt", "type": "file",
                  "permissions": { "read": true, "write": true, "execute": false } },
                { "name": "docs", "type": "directory",
                  "permissions": { "read": true, "write": true, "execute": false } },
            ])
        );
        let read = run(&mut tree, "read_file", json!({ "path": "docs//a.txt/" }));
        assert_eq!(text(&read), "A");
        let denied = run(
            &mut tree,
            "read_file",
            json!({ "path": "/docs/secret.txt" }),
        );
        assert_eq!(text(&denied), "Read permission denied for /docs/secret.txt");
        let directory = run(&mut tree, "read_file", json!({ "path": "/docs" }));
        assert!(directory.is_error);
        let file = run(&mut tree, "list_directory", json!({ "path": "/b.txt" }));
        assert_eq!(text(&file), "/b.txt is a file, not a directory.");
        let missing = run(&mut tree, "read_file", json!({ "path": "/nope" }));
        assert_eq!(text(&missing), "Path /nope not found.");
    }

    #[test]
    fn creating_files_and_directories() {
        let mut tree = tree();
        let made = run(&mut tree, "create_dir", json!({ "path": "/x/y" }));
        assert_eq!(text(&made), "Directory /x/y created.");
        assert_eq!(names(&tree, "/x"), ["y"]);
        run(
            &mut tree,
            "create_file",
            json!({ "path": "/x/y/n.txt", "content": [8, 5] }),
        );
        let file = tree.file("/x/y/n.txt").unwrap();
        assert_eq!(file.content, "[8,5]");
        assert_eq!(file.permissions, Permissions::FILE);
        let no_parent = run(&mut tree, "create_file", json!({ "path": "/q/n.txt" }));
        assert_eq!(text(&no_parent), "Path /q not found.");
        let over_dir = run(&mut tree, "create_file", json!({ "path": "/docs" }));
        assert!(over_dir.is_error);
        let through_file = run(&mut tree, "create_dir", json!({ "path": "/b.txt/z" }));
        assert!(through_file.is_error);
    }

    #[test]
    fn deleting_copying_and_moving() {
        let mut tree = tree();
        let root = run(&mut tree, "delete_node", json!({ "path": "/" }));
        assert!(root.is_error);
        run(
            &mut tree,
            "copy_node",
            json!({ "source_path": "/b.txt", "destination_path": "/docs" }),
        );
        run(
            &mut tree,
            "copy_node",
            json!({ "source_path": "/b.txt", "destination_path": "/docs" }),
        );
        assert_eq!(
            names(&tree, "/docs"),
            ["a.txt", "secret.txt", "b.txt", "b.txt_1"]
        );
        let renamed = run(
            &mut tree,
            "copy_node",
            json!({ "source_path": "/docs", "destination_path": "/copy" }),
        );
        assert_eq!(
            text(&renamed),
            "Successfully copied '/docs' to '/copy' (as 'copy')."
        );
        assert_eq!(names(&tree, "/copy").len(), 4);

        let into_itself = run(
            &mut tree,
            "move_node",
            json!({ "source_path": "/docs", "destination_path": "/docs/inner" }),
        );
        assert_eq!(text(&into_itself), "Cannot move a directory into itself.");
        // Moving a node into the directory it's already in keeps it.
        run(
            &mut tree,
            "move_node",
            json!({ "source_path": "/docs/a.txt", "destination_path": "/docs" }),
        );
        assert!(tree.file("/docs/a.txt").is_some());
        run(
            &mut tree,
            "move_node",
            json!({ "source_path": "/docs/a.txt", "destination_path": "/a2.txt" }),
        );
        assert!(tree.file("/docs/a.txt").is_none());
        assert_eq!(tree.file("/a2.txt").unwrap().content, "A");

        let deleted = run(&mut tree, "delete_node", json!({ "path": "/copy/" }));
        assert_eq!(text(&deleted), "/copy/ successfully deleted.");
        assert!(!tree.exists("/copy"));
        assert!(tree.exists("/"));
    }

    #[test]
    fn added_nodes_get_numbered_names() {
        let mut tree = tree();
        let file = || {
            Node::File(File {
                content: String::new(),
                permissions: Permissions::FILE,
            })
        };
        assert_eq!(tree.add("/docs", "a.txt", file()).unwrap(), "a.txt(1)");
        assert_eq!(tree.add("/docs", "a.txt", file()).unwrap(), "a.txt(2)");
        assert_eq!(tree.add("/", "new", file()).unwrap(), "new");
        assert!(tree.add("/missing", "x", file()).is_err());
    }
}
