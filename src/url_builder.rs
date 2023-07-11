use rust_extensions::StrOrString;

use crate::url_utils;

pub struct UrlBuilder {
    path_segments: Vec<String>,
    pub scheme_and_host: String,
    scheme_index: usize,
    pub query: Vec<(String, Option<String>)>,
    pub is_https: bool,
    raw_ending: Option<String>,
}

const DEFAULT_SCHEME: &str = "http";

impl UrlBuilder {
    pub fn new<'s>(host: impl Into<StrOrString<'s>>) -> Self {
        let host: StrOrString<'s> = host.into();

        let scheme_index = host.as_str().find("://");

        let (scheme_index, scheme_and_host) = if let Some(scheme_index) = scheme_index {
            (scheme_index, host.to_string())
        } else {
            (
                DEFAULT_SCHEME.len(),
                format!("{}://{}", DEFAULT_SCHEME, host.as_str()),
            )
        };

        let is_https = scheme_and_host.starts_with("https");

        Self {
            query: Vec::new(),
            path_segments: Vec::new(),
            scheme_index,
            scheme_and_host,
            is_https,
            raw_ending: None,
        }
    }

    pub fn append_raw_ending(&mut self, raw_ending: String) {
        self.raw_ending = Some(raw_ending);
    }

    pub fn append_path_segment(&mut self, path: String) {
        self.path_segments.push(path);
    }

    pub fn append_query_param(&mut self, param: String, value: Option<String>) {
        self.query.push((param.to_string(), value));
    }

    pub fn get_scheme(&self) -> &str {
        &self.scheme_and_host[..self.scheme_index]
    }

    pub fn get_host(&self) -> &str {
        remove_last_symbol_if_exists(&self.scheme_and_host[self.scheme_index + 3..], '/')
    }

    pub fn get_scheme_and_host(&self) -> &str {
        remove_last_symbol_if_exists(&self.scheme_and_host, '/')
    }

    pub fn get_path_and_query(&self) -> String {
        let mut result: Vec<u8> = Vec::new();

        fill_with_path(&mut result, &self.path_segments);

        if self.query.len() > 0 {
            fill_with_query(&mut result, &self.query)
        }

        return String::from_utf8(result).unwrap();
    }

    pub fn get_path(&self) -> String {
        if self.path_segments.len() == 0 {
            return "/".to_string();
        }

        let mut result: Vec<u8> = vec![];

        fill_with_path(&mut result, &self.path_segments);

        return String::from_utf8(result).unwrap();
    }

    pub fn to_string(&self) -> String {
        if self.path_segments.len() == 0 && self.query.len() == 0 {
            return self.scheme_and_host.to_string();
        }

        let mut result: Vec<u8> = Vec::new();

        fill_with_url(&mut result, self.scheme_and_host.as_bytes());

        if self.path_segments.len() > 0 {
            fill_with_path(&mut result, &self.path_segments);
        }

        if self.query.len() > 0 {
            fill_with_query(&mut result, &self.query)
        }

        if let Some(raw_ending) = &self.raw_ending {
            result.extend_from_slice(raw_ending.as_bytes())
        }

        return String::from_utf8(result).unwrap();
    }
}

fn fill_with_url(res: &mut Vec<u8>, scheme_and_url: &[u8]) {
    if scheme_and_url[scheme_and_url.len() - 1] == b'/' {
        res.extend_from_slice(&scheme_and_url[..scheme_and_url.len() - 1]);
        return;
    }
    res.extend_from_slice(scheme_and_url);
}

fn fill_with_path(res: &mut Vec<u8>, src: &Vec<String>) {
    if src.len() == 0 {
        res.push(b'/');
        return;
    }

    for segment in src {
        res.push(b'/');
        res.extend(segment.as_bytes())
    }
}

fn remove_last_symbol_if_exists(src: &str, last_symbol: char) -> &str {
    let last_char = last_symbol as u8;
    let src_as_bytes = src.as_bytes();
    if src_as_bytes[src_as_bytes.len() - 1] == last_char {
        let result = &src_as_bytes[..src.len() - 1];
        return std::str::from_utf8(result).unwrap();
    }

    src
}

fn fill_with_query(res: &mut Vec<u8>, src: &Vec<(String, Option<String>)>) {
    let mut first = true;
    for (key, value) in src {
        if first {
            res.push(b'?');
            first = false;
        } else {
            res.push(b'&');
        }
        url_utils::encode_to_url_string_and_copy(res, key);

        if let Some(value) = value {
            res.push(b'=');
            url_utils::encode_to_url_string_and_copy(res, value);
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::UrlBuilder;

    #[test]
    pub fn test_with_default_scheme() {
        let uri_builder = UrlBuilder::new("google.com");

        assert_eq!("http://google.com", uri_builder.to_string());
        assert_eq!("http://google.com", uri_builder.get_scheme_and_host());
        assert_eq!("http", uri_builder.get_scheme());
        assert_eq!("google.com", uri_builder.get_host());
        assert_eq!("/", uri_builder.get_path());
        assert_eq!("/", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_with_http_scheme() {
        let uri_builder = UrlBuilder::new("http://google.com");

        assert_eq!("http://google.com", uri_builder.to_string());
        assert_eq!("http://google.com", uri_builder.get_scheme_and_host());
        assert_eq!("http", uri_builder.get_scheme());
        assert_eq!("google.com", uri_builder.get_host());
        assert_eq!("/", uri_builder.get_path());
        assert_eq!("/", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_with_http_scheme_and_last_slash() {
        let uri_builder = UrlBuilder::new("http://google.com/");

        assert_eq!("http://google.com/", uri_builder.to_string());
        assert_eq!("http://google.com", uri_builder.get_scheme_and_host());
        assert_eq!("http", uri_builder.get_scheme());
        assert_eq!("google.com", uri_builder.get_host());
        assert_eq!("/", uri_builder.get_path());
        assert_eq!("/", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_with_https_scheme() {
        let uri_builder = UrlBuilder::new("https://google.com");

        assert_eq!("https://google.com", uri_builder.to_string());
        assert_eq!("https://google.com", uri_builder.get_scheme_and_host());

        assert_eq!("https", uri_builder.get_scheme());
        assert_eq!("google.com", uri_builder.get_host());
        assert_eq!("/", uri_builder.get_path());
        assert_eq!("/", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_path_segments() {
        let mut uri_builder = UrlBuilder::new("https://google.com");
        uri_builder.append_path_segment("first".to_string());
        uri_builder.append_path_segment("second".to_string());

        assert_eq!("https://google.com/first/second", uri_builder.to_string());
        assert_eq!("https://google.com", uri_builder.get_scheme_and_host());

        assert_eq!("https", uri_builder.get_scheme());
        assert_eq!("google.com", uri_builder.get_host());
        assert_eq!("/first/second", uri_builder.get_path());
        assert_eq!("/first/second", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_path_segments_with_slug_at_the_end() {
        let mut uri_builder = UrlBuilder::new("https://google.com/");
        uri_builder.append_path_segment("first".to_string());
        uri_builder.append_path_segment("second".to_string());

        assert_eq!("https://google.com/first/second", uri_builder.to_string());
        assert_eq!("https://google.com", uri_builder.get_scheme_and_host());

        assert_eq!("https", uri_builder.get_scheme());
        assert_eq!("google.com", uri_builder.get_host());
        assert_eq!("/first/second", uri_builder.get_path());
        assert_eq!("/first/second", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_query_with_no_path() {
        let mut uri_builder = UrlBuilder::new("https://google.com");
        uri_builder.append_query_param("first".to_string(), Some("first_value".to_string()));
        uri_builder.append_query_param("second".to_string(), Some("second_value".to_string()));

        assert_eq!(
            "https://google.com?first=first_value&second=second_value",
            uri_builder.to_string()
        );
        assert_eq!("https://google.com", uri_builder.get_scheme_and_host());

        assert_eq!("https", uri_builder.get_scheme());
        assert_eq!("google.com", uri_builder.get_host());
        assert_eq!("/", uri_builder.get_path());
        assert_eq!(
            "/?first=first_value&second=second_value",
            uri_builder.get_path_and_query()
        );
    }

    #[test]
    pub fn test_path_and_query() {
        let mut uri_builder = UrlBuilder::new("https://google.com");
        uri_builder.append_path_segment("first".to_string());
        uri_builder.append_path_segment("second".to_string());

        uri_builder.append_query_param("first".to_string(), Some("first_value".to_string()));
        uri_builder.append_query_param("second".to_string(), Some("second_value".to_string()));

        assert_eq!(
            "https://google.com/first/second?first=first_value&second=second_value",
            uri_builder.to_string()
        );
        assert_eq!("https://google.com", uri_builder.get_scheme_and_host());

        assert_eq!("https", uri_builder.get_scheme());
        assert_eq!("google.com", uri_builder.get_host());
        assert_eq!("/first/second", uri_builder.get_path());
        assert_eq!(
            "/first/second?first=first_value&second=second_value",
            uri_builder.get_path_and_query()
        );
    }
}
