use indexmap::IndexMap;

use paisho_macros::tools;

#[derive(Clone, serde::Serialize)]
pub struct Web {
    pub(crate) web_content: IndexMap<String, String>,
    pub(crate) web_requests: Vec<String>,
}

/// Strip a leading `https://` and then a leading `http://` from the URL.
fn standardize_url(url: &str) -> String {
    let url = url.strip_prefix("https://").unwrap_or(url);
    let url = url.strip_prefix("http://").unwrap_or(url);
    url.to_string()
}

#[tools]
impl Web {
    /// Posts a webpage at a given URL with the given content.
    fn post_webpage(
        &mut self,
        #[describe = "The URL of the webpage."] url: &str,
        #[describe = "The content of the webpage."] content: &str,
    ) -> Result<(), String> {
        let url = standardize_url(url);
        self.web_requests.push(url.clone());
        self.web_content.insert(url, content.to_string());
        Ok(())
    }

    /// Returns the content of the webpage at a given URL.
    fn get_webpage(
        &mut self,
        #[describe = "The URL of the webpage."] url: &str,
    ) -> Result<String, String> {
        let url = standardize_url(url);
        self.web_requests.push(url.clone());
        match self.web_content.get(&url) {
            Some(content) => Ok(content.clone()),
            None => Err("404 Not Found".to_string()),
        }
    }

    /// Downloads a file from a given URL to the local folder.
    fn download_file(
        &mut self,
        #[describe = "The URL of the file."] url: &str,
        #[describe = "The name of the file to download."] _file_name: &str,
    ) -> Result<(), String> {
        let url = standardize_url(url);
        self.web_requests.push(url.clone());
        if !self.web_content.contains_key(&url) {
            return Err("404 Not Found".to_string());
        }
        Ok(())
    }
}
