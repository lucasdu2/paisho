use std::collections::HashMap;

struct Web {
    web_content: HashMap<String, String>,
    web_requests: Vec<String>,
}

impl Web {
    /// Strip a leading `https://` and then a leading `http://` from the URL.
    fn standardize_url(url: &str) -> String {
        let url = url.strip_prefix("https://").unwrap_or(url);
        let url = url.strip_prefix("http://").unwrap_or(url);
        url.to_string()
    }

    /// Posts a webpage at a given URL with the given content.
    fn post_webpage(&mut self, url: &str, content: &str) {
        let url = Self::standardize_url(url);
        self.web_requests.push(url.clone());
        self.web_content.insert(url, content.to_string());
    }

    /// Returns the content of the webpage at a given URL.
    fn get_webpage(&mut self, url: &str) -> String {
        let url = Self::standardize_url(url);
        self.web_requests.push(url.clone());
        match self.web_content.get(&url) {
            Some(content) => content.clone(),
            None => "404 Not Found".to_string(),
        }
    }

    /// Downloads a file from a given URL to the local folder.
    fn download_file(&mut self, url: &str, _file_name: &str) -> Option<String> {
        let url = Self::standardize_url(url);
        self.web_requests.push(url.clone());
        if !self.web_content.contains_key(&url) {
            return Some("404 Not Found".to_string());
        }
        None
    }
}
