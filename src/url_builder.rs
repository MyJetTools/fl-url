use rust_extensions::StrOrString;

use crate::{url_utils, Scheme, UrlBuilderOwned};

pub struct UrlBuilder<'s> {
    path_segments: String,
    scheme_index: Option<usize>,
    pub query: Vec<(String, Option<String>)>,
    pub scheme: Scheme,
    raw_ending: Option<String>,
    pub host_port: StrOrString<'s>,
    has_last_slash: bool,
}

impl<'s> UrlBuilder<'s> {
    pub fn new(host_port: impl Into<StrOrString<'s>>) -> Self {
        let mut host_port: StrOrString<'s> = host_port.into();
        let has_last_slash = remove_last_symbol_if_exists(&mut host_port, '/');

        let (scheme, scheme_index) = Scheme::from_url(host_port.as_str());

        Self {
            query: Vec::new(),
            path_segments: String::new(),
            scheme,
            scheme_index,
            raw_ending: None,
            host_port,
            has_last_slash,
        }
    }

    pub fn append_raw_ending(&mut self, raw_ending: String) {
        self.raw_ending = Some(raw_ending);
    }

    pub fn append_path_segment(&mut self, path: &str) {
        if self.path_segments.len() > 0 {
            self.path_segments.push('/');
        }
        self.path_segments.push_str(path);
    }

    pub fn append_query_param(&mut self, param: String, value: Option<String>) {
        self.query.push((param.to_string(), value));
    }

    pub fn get_scheme(&self) -> Scheme {
        self.scheme.clone()
    }

    pub fn get_host_port(&self) -> &str {
        match self.scheme_index {
            Some(index) => &self.host_port.as_str()[index + 3..],
            None => self.host_port.as_str(),
        }
    }

    fn fill_schema_and_host(&self, result: &mut String) {
        result.push_str(self.scheme.scheme_as_str());

        if let Some(index) = self.scheme_index {
            result.push_str(&self.host_port.as_str()[index + 3..]);
        } else {
            result.push_str(self.host_port.as_str());
        }
    }

    pub fn get_scheme_and_host(&self) -> StrOrString<'_> {
        #[cfg(feature = "support-unix-socket")]
        if self.scheme.is_unix_socket() {
            let mut result = String::new();
            result.push_str(self.scheme.scheme_as_str());
            if let Some(index) = self.scheme_index {
                result.push_str(&self.host_port.as_str()[index + 3..]);
            } else {
                result.push_str(self.host_port.as_str());
            }
            return result.into();
        }

        if self.scheme_index.is_some() {
            return self.host_port.clone();
        }

        let mut result = String::new();
        self.fill_schema_and_host(&mut result);
        result.into()
    }

    pub fn get_path_and_query(&'s self) -> String {
        let mut result = String::new();

        fill_with_path(&mut result, &self.path_segments);

        if self.query.len() > 0 {
            fill_with_query(&mut result, &self.query)
        }

        result
    }

    pub fn get_path(&self) -> String {
        if self.path_segments.len() == 0 {
            return "/".to_string();
        }

        let mut result = String::new();

        fill_with_path(&mut result, &self.path_segments);

        result
    }

    pub fn to_string(&self) -> String {
        let mut result: String = String::new();

        self.fill_schema_and_host(&mut result);

        if self.path_segments.len() > 0 {
            fill_with_path(&mut result, &self.path_segments);
        } else {
            if self.has_last_slash && self.raw_ending.is_none() {
                result.push('/');
            }
        }

        if self.query.len() > 0 {
            fill_with_query(&mut result, &self.query)
        }

        if let Some(raw_ending) = &self.raw_ending {
            result.push_str(raw_ending)
        }

        result
    }

    pub fn into_builder_owned(&self) -> UrlBuilderOwned {
        UrlBuilderOwned::new(self.to_string())
    }
}

fn fill_with_path<'s>(res: &mut String, path: &str) {
    res.push('/');
    if path.len() == 0 {
        return;
    }

    res.push_str(path)
}

fn remove_last_symbol_if_exists<'s>(src: &mut StrOrString<'s>, last_symbol: char) -> bool {
    let last_char = last_symbol as u8;
    let src_as_bytes = src.as_str().as_bytes();
    if src_as_bytes[src_as_bytes.len() - 1] == last_char {
        src.slice_it(None, Some(src_as_bytes.len() - 1));
        return true;
    }

    false
}

fn fill_with_query(res: &mut String, src: &Vec<(String, Option<String>)>) {
    let mut first = true;
    for (key, value) in src {
        if first {
            res.push('?');
            first = false;
        } else {
            res.push('&');
        }
        url_utils::encode_to_url_string_and_copy(res, key);

        if let Some(value) = value {
            res.push('=');
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
        assert_eq!(
            "http://google.com",
            uri_builder.get_scheme_and_host().as_str()
        );
        assert_eq!(true, uri_builder.get_scheme().is_http());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!("/", uri_builder.get_path());
        assert_eq!("/", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_with_http_scheme() {
        let uri_builder = UrlBuilder::new("http://google.com");

        assert_eq!("http://google.com", uri_builder.to_string());
        assert_eq!(
            "http://google.com",
            uri_builder.get_scheme_and_host().as_str()
        );
        assert_eq!(true, uri_builder.get_scheme().is_http());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!("/", uri_builder.get_path());
        assert_eq!("/", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_with_http_scheme_and_last_slash() {
        let uri_builder = UrlBuilder::new("http://google.com/");

        assert_eq!("http://google.com/", uri_builder.to_string());
        assert_eq!(
            "http://google.com",
            uri_builder.get_scheme_and_host().as_str()
        );
        assert_eq!(true, uri_builder.get_scheme().is_http());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!("/", uri_builder.get_path());
        assert_eq!("/", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_with_https_scheme() {
        let uri_builder = UrlBuilder::new("https://google.com");

        assert_eq!("https://google.com", uri_builder.to_string());
        assert_eq!(
            "https://google.com",
            uri_builder.get_scheme_and_host().as_str()
        );

        assert_eq!(true, uri_builder.get_scheme().is_https());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!("/", uri_builder.get_path());
        assert_eq!("/", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_path_segments() {
        let mut uri_builder = UrlBuilder::new("https://google.com");
        uri_builder.append_path_segment("first");
        uri_builder.append_path_segment("second");

        assert_eq!("https://google.com/first/second", uri_builder.to_string());
        assert_eq!(
            "https://google.com",
            uri_builder.get_scheme_and_host().as_str()
        );

        assert_eq!(true, uri_builder.get_scheme().is_https());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!("/first/second", uri_builder.get_path());
        assert_eq!("/first/second", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_path_segments_with_slug_at_the_end() {
        let mut uri_builder = UrlBuilder::new("https://google.com/");
        uri_builder.append_path_segment("first");
        uri_builder.append_path_segment("second");

        assert_eq!("https://google.com/first/second", uri_builder.to_string());
        assert_eq!(
            "https://google.com",
            uri_builder.get_scheme_and_host().as_str()
        );

        assert_eq!(true, uri_builder.get_scheme().is_https());
        assert_eq!("google.com", uri_builder.get_host_port());
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
        assert_eq!(
            "https://google.com",
            uri_builder.get_scheme_and_host().as_str()
        );

        assert_eq!(true, uri_builder.get_scheme().is_https());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!("/", uri_builder.get_path());
        assert_eq!(
            "/?first=first_value&second=second_value",
            uri_builder.get_path_and_query()
        );
    }

    #[test]
    pub fn test_path_and_query() {
        let mut uri_builder = UrlBuilder::new("https://google.com");
        uri_builder.append_path_segment("first");
        uri_builder.append_path_segment("second");

        uri_builder.append_query_param("first".to_string(), Some("first_value".to_string()));
        uri_builder.append_query_param("second".to_string(), Some("second_value".to_string()));

        assert_eq!(
            "https://google.com/first/second?first=first_value&second=second_value",
            uri_builder.to_string()
        );
        assert_eq!(
            "https://google.com",
            uri_builder.get_scheme_and_host().as_str()
        );

        assert_eq!(true, uri_builder.get_scheme().is_https());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!("/first/second", uri_builder.get_path());
        assert_eq!(
            "/first/second?first=first_value&second=second_value",
            uri_builder.get_path_and_query()
        );
    }

    #[test]
    #[cfg(feature = "support-unix-socket")]
    pub fn test_unix_path_and_query() {
        let mut uri_builder = UrlBuilder::new("http+unix://google.com");
        uri_builder.append_path_segment("first".to_string());
        uri_builder.append_path_segment("second".to_string());

        uri_builder.append_query_param("first".to_string(), Some("first_value".to_string()));
        uri_builder.append_query_param("second".to_string(), Some("second_value".to_string()));

        assert_eq!(
            "./google.com/first/second?first=first_value&second=second_value",
            uri_builder.to_string()
        );
        assert_eq!("./google.com", uri_builder.get_scheme_and_host().as_str());

        assert_eq!(true, uri_builder.get_scheme().is_unix_socket());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!("/first/second", uri_builder.get_path());
        assert_eq!(
            "/first/second?first=first_value&second=second_value",
            uri_builder.get_path_and_query()
        );
    }
}
