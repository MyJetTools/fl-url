#[derive(Debug, Clone)]
pub struct UrlBuilderOwnedLegacy {
    value: String,
}

impl UrlBuilderOwnedLegacy {
    pub fn new(value: String) -> Self {
        Self { value }
    }

    pub fn as_str(&self) -> &str {
        self.value.as_str()
    }

    pub fn into_string(self) -> String {
        self.value
    }

    pub fn get_scheme_and_host(&self) -> &str {
        let index = self.value.find("://");

        if index.is_none() {
            panic!("Invalid UnxSocket URL: {}", self.value)
        }

        let index = index.unwrap();

        let as_bytes = self.value.as_bytes();

        for i in index + 3..as_bytes.len() {
            if as_bytes[i] == b'/' {
                return &self.value[0..i];
            }
        }

        self.value.as_str()
    }

    pub fn get_host_port(&self) -> &str {
        let index = self.value.find("://").unwrap();

        let as_bytes = self.value.as_bytes();

        for i in index + 3..as_bytes.len() {
            if as_bytes[i] == b'/' {
                return &self.value[index + 3..i];
            }
        }

        self.value.as_str()
    }

    pub fn get_path_and_query(&self) -> &str {
        let index = self.value.find("://").unwrap();

        let as_bytes = self.value.as_bytes();

        for i in index + 3..as_bytes.len() {
            if as_bytes[i] == b'/' {
                if i == as_bytes.len() - 1 {
                    return "/";
                }
                return &self.value[i + 1..];
            }
        }

        "/"
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_pure_domain() {
        let url = "https://www.google.com";
        let url = super::UrlBuilderOwnedLegacy::new(url.to_string());
        assert_eq!(url.get_scheme_and_host(), "https://www.google.com");
    }

    #[test]
    fn test_domain_with_root_slash() {
        let url = "https://www.google.com/";
        let url = super::UrlBuilderOwnedLegacy::new(url.to_string());
        assert_eq!(url.get_scheme_and_host(), "https://www.google.com");
    }

    #[test]
    fn test_domain_with_some_path() {
        let url = "https://www.google.com/MyPath";
        let url = super::UrlBuilderOwnedLegacy::new(url.to_string());
        assert_eq!(url.get_scheme_and_host(), "https://www.google.com");
    }

    #[test]
    fn test_get_host_port() {
        let url = "https://www.google.com/MyPath";
        let url = super::UrlBuilderOwnedLegacy::new(url.to_string());
        assert_eq!(url.get_host_port(), "www.google.com");
    }

    #[test]
    fn test_path_and_query() {
        let url = "https://www.google.com/MyPath";
        let url = super::UrlBuilderOwnedLegacy::new(url.to_string());
        assert_eq!(url.get_path_and_query(), "MyPath");

        let url = "https://www.google.com/";
        let url = super::UrlBuilderOwnedLegacy::new(url.to_string());
        assert_eq!(url.get_path_and_query(), "/");

        let url = "https://www.google.com";
        let url = super::UrlBuilderOwnedLegacy::new(url.to_string());
        assert_eq!(url.get_path_and_query(), "/");
    }
}
