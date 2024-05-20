#[derive(Debug, Clone)]
pub struct UrlBuilderOwnedLegacy {
    scheme_and_host: String,
    path_and_query: String,
}

impl UrlBuilderOwnedLegacy {
    pub fn new(scheme_and_host: String, path_and_query: String) -> Self {
        Self {
            scheme_and_host,
            path_and_query,
        }
    }

    pub fn get_scheme_and_host(&self) -> &str {
        self.scheme_and_host.as_str()
    }

    pub fn get_host_port(&self) -> &str {
        self.scheme_and_host.as_str()
    }

    pub fn get_path_and_query(&self) -> &str {
        &self.path_and_query
    }
}
