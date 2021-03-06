use crate::url_utils;

pub struct FlUrlUriBuilder {
    path: Vec<String>,
    pub scheme_and_host: String,
    scheme_index: usize,
    pub query: Vec<(String, Option<String>)>,
}

const DEFAULT_SCHEME: &str = "http";

impl FlUrlUriBuilder {
    pub fn from_str(host: &str) -> Self {
        let host = remove_last_symbol_if_exists(host, '/');

        let scheme_index = host.find("://");

        Self {
            query: Vec::new(),
            path: Vec::new(),
            scheme_index: if let Some(scheme_index) = scheme_index {
                scheme_index
            } else {
                DEFAULT_SCHEME.len()
            },
            scheme_and_host: if scheme_index.is_some() {
                host.to_string()
            } else {
                format!("{}://{}", DEFAULT_SCHEME, host)
            },
        }
    }

    pub fn append_path_segment(&mut self, path: &str) {
        self.path.push(path.to_string());
    }

    pub fn append_query_param(&mut self, param: &str, value: Option<String>) {
        self.query.push((param.to_string(), value));
    }

    pub fn get_scheme(&self) -> &str {
        &self.scheme_and_host[..self.scheme_index]
    }

    pub fn get_host(&self) -> &str {
        &self.scheme_and_host[self.scheme_index + 3..]
    }

    pub fn get_scheme_and_host(&self) -> &str {
        &self.scheme_and_host
    }

    pub fn get_path_and_query(&self) -> String {
        let mut result: Vec<u8> = Vec::new();

        fill_with_path(&mut result, &self.path);

        if self.query.len() > 0 {
            fill_with_query(&mut result, &self.query)
        }

        return String::from_utf8(result).unwrap();
    }

    pub fn get_path(&self) -> String {
        if self.path.len() == 0 {
            return "/".to_string();
        }

        let mut result: Vec<u8> = vec![];

        fill_with_path(&mut result, &self.path);

        return String::from_utf8(result).unwrap();
    }

    pub fn to_string(&self) -> String {
        if self.path.len() == 0 && self.query.len() == 0 {
            return self.scheme_and_host.to_string();
        }

        let mut result: Vec<u8> = Vec::new();

        fill_with_url(&mut result, &self.scheme_and_host);

        if self.path.len() > 0 {
            fill_with_path(&mut result, &self.path);
        }

        if self.query.len() > 0 {
            fill_with_query(&mut result, &self.query)
        }

        return String::from_utf8(result).unwrap();
    }
}

fn fill_with_url(res: &mut Vec<u8>, src: &str) {
    res.extend(src.as_bytes());
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
        return std::str::from_utf8(&src_as_bytes[..src_as_bytes.len() - 1]).unwrap();
    }

    return src;
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
    use crate::FlUrlUriBuilder;

    #[test]
    pub fn test_with_default_scheme() {
        let uri_builder = FlUrlUriBuilder::from_str("google.com");

        assert_eq!("http://google.com", uri_builder.to_string());
        assert_eq!("http://google.com", uri_builder.get_scheme_and_host());
        assert_eq!("http", uri_builder.get_scheme());
        assert_eq!("google.com", uri_builder.get_host());
        assert_eq!("/", uri_builder.get_path());
        assert_eq!("/", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_with_http_scheme() {
        let uri_builder = FlUrlUriBuilder::from_str("http://google.com");

        assert_eq!("http://google.com", uri_builder.to_string());
        assert_eq!("http://google.com", uri_builder.get_scheme_and_host());
        assert_eq!("http", uri_builder.get_scheme());
        assert_eq!("google.com", uri_builder.get_host());
        assert_eq!("/", uri_builder.get_path());
        assert_eq!("/", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_with_https_scheme() {
        let uri_builder = FlUrlUriBuilder::from_str("https://google.com");

        assert_eq!("https://google.com", uri_builder.to_string());
        assert_eq!("https://google.com", uri_builder.get_scheme_and_host());

        assert_eq!("https", uri_builder.get_scheme());
        assert_eq!("google.com", uri_builder.get_host());
        assert_eq!("/", uri_builder.get_path());
        assert_eq!("/", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_path_segmets() {
        let mut uri_builder = FlUrlUriBuilder::from_str("https://google.com");
        uri_builder.append_path_segment("first");
        uri_builder.append_path_segment("second");

        assert_eq!("https://google.com/first/second", uri_builder.to_string());
        assert_eq!("https://google.com", uri_builder.get_scheme_and_host());

        assert_eq!("https", uri_builder.get_scheme());
        assert_eq!("google.com", uri_builder.get_host());
        assert_eq!("/first/second", uri_builder.get_path());
        assert_eq!("/first/second", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_query_with_no_path() {
        let mut uri_builder = FlUrlUriBuilder::from_str("https://google.com");
        uri_builder.append_query_param("first", Some("first_value".to_string()));
        uri_builder.append_query_param("second", Some("second_value".to_string()));

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
        let mut uri_builder = FlUrlUriBuilder::from_str("https://google.com");
        uri_builder.append_path_segment("first");
        uri_builder.append_path_segment("second");

        uri_builder.append_query_param("first", Some("first_value".to_string()));
        uri_builder.append_query_param("second", Some("second_value".to_string()));

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
