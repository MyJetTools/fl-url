use core::str;

use rust_extensions::remote_endpoint::RemoteEndpoint;

use crate::{url_utils, Scheme};

pub struct UrlBuilder {
    value: String,
    host_index: usize,
    port_index: usize,
    path_index: usize,
    query_index: usize,
}

impl UrlBuilder {
    pub fn new(host_port: &str) -> Self {
        let mut value = String::new();

        let mut domain_index = host_port.find("://");

        if domain_index.is_none() {
            domain_index = host_port.find(":/~");
        }

        let host_index = if let Some(domain_index) = domain_index {
            domain_index + 3
        } else {
            value.push_str("http://");
            7
        };
        value.push_str(host_port);

        let mut port_index = 0;
        let mut path_index = 0;
        let mut query_index = 0;

        let mut pos = 0;
        for c in value.chars() {
            if pos <= host_index {
                pos += 1;
                continue;
            }

            match c {
                ':' => {
                    if path_index == 0 {
                        port_index = pos;
                    }
                }
                '/' => {
                    if path_index == 0 {
                        path_index = pos;
                    }
                }
                '?' => {
                    if path_index == 0 {
                        path_index = pos;
                    }
                    if query_index == 0 {
                        query_index = pos;
                        break;
                    }
                }
                _ => {}
            }

            pos += 1;
        }

        Self {
            value,
            host_index,
            path_index,
            port_index,
            query_index,
        }
    }

    pub fn get_remote_endpoint(&self) -> RemoteEndpoint {
        if self.path_index == 0 {
            RemoteEndpoint::try_parse(&self.value[self.host_index..]).unwrap()
        } else {
            RemoteEndpoint::try_parse(&self.value[self.host_index..self.path_index]).unwrap()
        }
    }

    pub fn append_path_segment(&mut self, path: &str) {
        if !self.value.ends_with('/') {
            self.value.push('/');
        }
        if self.path_index == 0 {
            self.path_index = self.value.len() - 1;
        }

        if path.starts_with('/') {
            self.value.push_str(&path[1..]);
        } else {
            self.value.push_str(path);
        }
    }

    pub fn append_query_param(&mut self, param: &str, value: Option<&str>) {
        if self.query_index == 0 {
            self.value.push('?');
            self.query_index = self.value.len() - 1;
        } else {
            self.value.push('&');
        }
        url_utils::encode_to_url_string_and_copy(&mut self.value, param);
        if let Some(value) = value {
            self.value.push('=');
            url_utils::encode_to_url_string_and_copy(&mut self.value, value);
        }
    }

    pub fn append_raw_ending(&mut self, raw_ending: &str) {
        self.value.push_str(raw_ending);
    }

    pub fn get_scheme(&self) -> Scheme {
        Scheme::from_url(&self.value).0
    }

    pub fn get_host(&self) -> &str {
        if self.port_index > 0 {
            return &self.value[self.host_index..self.port_index];
        }

        if self.path_index > 0 {
            return &self.value[self.host_index..self.path_index];
        }

        if self.query_index > 0 {
            return &self.value[self.host_index..self.query_index];
        }

        &self.value[self.host_index..]
    }

    pub fn get_host_port(&self) -> &str {
        if self.get_scheme().is_unix_socket() {
            if self.query_index > 0 {
                return &self.value[self.host_index - 1..self.query_index];
            } else {
                return &self.value[self.host_index - 1..];
            }
        }

        if self.path_index > 0 {
            return &self.value[self.host_index..self.path_index];
        }

        if self.query_index > 0 {
            return &self.value[self.host_index..self.query_index];
        }

        &self.value[self.host_index..]
    }

    pub fn get_scheme_and_host(&self) -> &str {
        if self.get_scheme().is_unix_socket() {
            if self.query_index > 0 {
                return &self.value[..self.query_index];
            } else {
                return &self.value;
            }
        }

        if self.path_index > 0 {
            return &self.value[..self.path_index];
        }

        if self.query_index > 0 {
            return &self.value[..self.query_index];
        }

        &self.value
    }

    pub fn get_path_and_query(&self) -> &str {
        if self.get_scheme().is_unix_socket() {
            return &self.value[self.host_index - 1..];
        }

        if self.path_index == 0 && self.query_index == 0 {
            return "/";
        }

        if self.path_index > 0 {
            return &self.value[self.path_index..];
        }

        return &self.value[self.query_index..];
    }

    pub fn get_path(&self) -> &str {
        if self.path_index == 0 {
            return "/";
        }
        if self.query_index == 0 {
            return &self.value[self.path_index..];
        }

        &self.value[self.path_index..self.query_index]
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }

    pub fn to_string(&self) -> String {
        self.value.to_string()
    }
}

#[cfg(test)]
mod tests {

    use crate::UrlBuilder;

    #[test]
    pub fn test_with_default_scheme() {
        let uri_builder = UrlBuilder::new("google.com".into());

        assert_eq!(uri_builder.host_index, 7);
        assert_eq!(uri_builder.port_index, 0);
        assert_eq!(uri_builder.path_index, 0);
        assert_eq!(uri_builder.query_index, 0);

        assert_eq!("http://google.com", uri_builder.as_str());
        assert_eq!("http://google.com", uri_builder.get_scheme_and_host());
        assert_eq!("google.com", uri_builder.get_host());

        assert_eq!(true, uri_builder.get_scheme().is_http());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!("/", uri_builder.get_path());

        assert_eq!("/", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_with_http_scheme() {
        let uri_builder = UrlBuilder::new("http://google.com".into());

        assert_eq!(uri_builder.host_index, 7);
        assert_eq!(uri_builder.port_index, 0);
        assert_eq!(uri_builder.path_index, 0);
        assert_eq!(uri_builder.query_index, 0);

        assert_eq!("http://google.com", uri_builder.to_string());
        assert_eq!("http://google.com", uri_builder.get_scheme_and_host());
        assert_eq!(true, uri_builder.get_scheme().is_http());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!("/", uri_builder.get_path());
        assert_eq!("/", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_with_http_scheme_and_last_slash() {
        let uri_builder = UrlBuilder::new("http://google.com/".into());

        assert_eq!(uri_builder.host_index, 7);
        assert_eq!(uri_builder.port_index, 0);
        assert_eq!(uri_builder.path_index, 17);
        assert_eq!(uri_builder.query_index, 0);

        assert_eq!("http://google.com/", uri_builder.to_string());
        assert_eq!("http://google.com", uri_builder.get_scheme_and_host());
        assert_eq!(true, uri_builder.get_scheme().is_http());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!("/", uri_builder.get_path());
        assert_eq!("/", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_with_https_scheme() {
        let uri_builder = UrlBuilder::new("https://google.com".into());

        assert_eq!(uri_builder.host_index, 8);
        assert_eq!(uri_builder.port_index, 0);
        assert_eq!(uri_builder.path_index, 0);
        assert_eq!(uri_builder.query_index, 0);

        assert_eq!("https://google.com", uri_builder.to_string());
        assert_eq!("https://google.com", uri_builder.get_scheme_and_host());

        assert_eq!(true, uri_builder.get_scheme().is_https());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!("/", uri_builder.get_path());
        assert_eq!("/", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_path_segments() {
        let mut uri_builder = UrlBuilder::new("https://google.com".into());
        assert_eq!(uri_builder.host_index, 8);
        assert_eq!(uri_builder.port_index, 0);
        assert_eq!(uri_builder.path_index, 0);
        assert_eq!(uri_builder.query_index, 0);

        uri_builder.append_path_segment("first");
        assert_eq!(uri_builder.path_index, 18);
        uri_builder.append_path_segment("second");

        assert_eq!("https://google.com/first/second", uri_builder.as_str());
        assert_eq!("https://google.com", uri_builder.get_scheme_and_host());

        assert_eq!(true, uri_builder.get_scheme().is_https());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!("/first/second", uri_builder.get_path());
        assert_eq!("/first/second", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_path_segments_with_slug_at_the_end() {
        let mut uri_builder = UrlBuilder::new("https://google.com/".into());
        assert_eq!(uri_builder.host_index, 8);
        assert_eq!(uri_builder.port_index, 0);
        assert_eq!(uri_builder.path_index, 18);
        assert_eq!(uri_builder.query_index, 0);
        uri_builder.append_path_segment("first");
        uri_builder.append_path_segment("second");

        assert_eq!("https://google.com/first/second", uri_builder.to_string());
        assert_eq!("https://google.com", uri_builder.get_scheme_and_host());

        assert_eq!(true, uri_builder.get_scheme().is_https());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!("/first/second", uri_builder.get_path());
        assert_eq!("/first/second", uri_builder.get_path_and_query());
    }

    #[test]
    pub fn test_query_with_no_path() {
        let mut uri_builder = UrlBuilder::new("https://google.com".into());
        uri_builder.append_query_param("first", Some("first_value"));
        uri_builder.append_query_param("second", Some("second_value"));

        assert_eq!(uri_builder.host_index, 8);
        assert_eq!(uri_builder.port_index, 0);
        assert_eq!(uri_builder.path_index, 0);
        assert_eq!(uri_builder.query_index, 18);

        assert_eq!(
            "https://google.com?first=first_value&second=second_value",
            uri_builder.to_string()
        );
        assert_eq!("https://google.com", uri_builder.get_scheme_and_host());

        assert_eq!(true, uri_builder.get_scheme().is_https());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!(uri_builder.get_path(), "/",);
        assert_eq!(
            "?first=first_value&second=second_value",
            uri_builder.get_path_and_query()
        );
    }

    #[test]
    pub fn test_get_domain_different_cases() {
        let uri_builder = UrlBuilder::new("https://my-domain:5123".into());

        assert_eq!("my-domain:5123", uri_builder.get_host_port());
        assert_eq!("my-domain", uri_builder.get_host());

        let uri_builder = UrlBuilder::new("https://my-domain:5123/my-path".into());

        assert_eq!("my-domain:5123", uri_builder.get_host_port());
        assert_eq!("my-domain", uri_builder.get_host());

        let uri_builder = UrlBuilder::new("https://my-domain/my-path".into());

        assert_eq!("my-domain", uri_builder.get_host_port());
        assert_eq!("my-domain", uri_builder.get_host());
    }

    #[test]
    pub fn test_path_and_query() {
        let mut uri_builder = UrlBuilder::new("https://google.com".into());
        uri_builder.append_path_segment("first");
        uri_builder.append_path_segment("second");

        uri_builder.append_query_param("first", Some("first_value"));
        uri_builder.append_query_param("second", Some("second_value"));

        assert_eq!(
            "https://google.com/first/second?first=first_value&second=second_value",
            uri_builder.to_string()
        );
        assert_eq!("https://google.com", uri_builder.get_scheme_and_host());

        assert_eq!(true, uri_builder.get_scheme().is_https());
        assert_eq!("google.com", uri_builder.get_host_port());
        assert_eq!("/first/second", uri_builder.get_path());
        assert_eq!(
            "/first/second?first=first_value&second=second_value",
            uri_builder.get_path_and_query()
        );
    }

    #[test]

    pub fn test_unix_path_and_query() {
        let mut uri_builder = UrlBuilder::new("http+unix://var/run/test".into());

        uri_builder.append_query_param("first", Some("first_value"));
        uri_builder.append_query_param("second", Some("second_value"));

        assert_eq!(true, uri_builder.get_scheme().is_unix_socket());

        assert_eq!(
            "http+unix://var/run/test?first=first_value&second=second_value",
            uri_builder.to_string()
        );
        assert_eq!(
            "http+unix://var/run/test",
            uri_builder.get_scheme_and_host()
        );

        assert_eq!("/var/run/test", uri_builder.get_host_port());

        assert_eq!(
            "/var/run/test?first=first_value&second=second_value",
            uri_builder.get_path_and_query()
        );
    }

    #[test]
    pub fn test_unix_from_home_path() {
        let mut uri_builder = UrlBuilder::new("http+unix:/~/var/run/test".into());

        uri_builder.append_query_param("first", Some("first_value"));
        uri_builder.append_query_param("second", Some("second_value"));

        assert_eq!(true, uri_builder.get_scheme().is_unix_socket());

        assert_eq!(
            "http+unix:/~/var/run/test?first=first_value&second=second_value",
            uri_builder.to_string()
        );
        assert_eq!(
            "http+unix:/~/var/run/test",
            uri_builder.get_scheme_and_host()
        );

        assert_eq!("~/var/run/test", uri_builder.get_host_port());

        assert_eq!(
            "~/var/run/test?first=first_value&second=second_value",
            uri_builder.get_path_and_query()
        );
    }
}
