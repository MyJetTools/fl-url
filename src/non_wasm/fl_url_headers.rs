use my_http_client::{HeaderValuePosition, MyHttpClientHeadersBuilder, RequestBuildError};

use crate::FlUrlError;

pub struct FlUrlHeaders {
    pub(crate) headers: MyHttpClientHeadersBuilder,
    pub has_connection_header: bool,
    pub len: usize,
    pub host_header_value: Option<HeaderValuePosition>,
    // The first header that can not be sent. `add` has no way to report it, so it is
    // kept here and fails the request instead — see `ensure_valid`.
    invalid_header: Option<String>,
}

impl FlUrlHeaders {
    pub fn new() -> Self {
        Self {
            headers: MyHttpClientHeadersBuilder::new(),

            has_connection_header: false,
            host_header_value: None,
            len: 0,
            invalid_header: None,
        }
    }

    /*
       pub fn add_json_content_type(&mut self) {
           self.headers
               .add_header(CONTENT_TYPE.as_str(), "application/json");
       }
    */
    pub fn add(&mut self, name: &str, value: &str) {
        // What must not be put on the wire is my-http-client's to decide: its header
        // builder refuses such a header and adds nothing. A header value comes from
        // settings as often as from code — an api key read together with its trailing
        // new line — so the refusal is kept, and it is the request that fails with it.
        let pos = match self.headers.add_header(name, value) {
            Ok(pos) => pos,
            Err(err) => {
                if self.invalid_header.is_none() {
                    self.invalid_header = Some(describe_refused_header(name, &err));
                }
                return;
            }
        };

        if rust_extensions::str_utils::compare_strings_case_insensitive(name, "connection") {
            self.has_connection_header = true;
        }

        if name.eq_ignore_ascii_case("host") {
            self.host_header_value = Some(pos);
        }
        self.len += 1;
    }

    /// Fails with the first header [`Self::add`] was given that can not be sent.
    /// Called right before the headers go into a request.
    pub(crate) fn ensure_valid(&self) -> Result<(), FlUrlError> {
        match self.invalid_header.as_ref() {
            Some(reason) => Err(FlUrlError::RequestBuild(reason.clone())),
            None => Ok(()),
        }
    }

    pub fn has_host_header(&self) -> bool {
        self.host_header_value.is_some()
    }

    pub fn has_header(&self, name: &str) -> bool {
        self.headers
            .iter()
            .any(|(header_name, _)| header_name.eq_ignore_ascii_case(name))
    }

    pub fn get_host_header_value(&self) -> Option<&str> {
        let host_value_pos = self.host_header_value.as_ref()?;
        self.headers.get_value(host_value_pos)
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn iter<'s>(&'s self) -> impl Iterator<Item = (&'s str, &'s str)> {
        self.headers.iter()
    }

    pub fn get_builder(&self) -> &MyHttpClientHeadersBuilder {
        &self.headers
    }
}

/// Lets a `my_http_utils` request model (`#[derive(MyHttpInput)]`) push its header
/// fields straight into our header collection during `execute_request`.
impl my_http_utils::schema::client::HeaderBuilder for FlUrlHeaders {
    fn add_header(&mut self, name: &str, value: &str) {
        self.add(name, value);
    }
}

/// What the request fails with for a header my-http-client refused. The message names
/// the header and the byte, never the value: that is where the secrets are.
fn describe_refused_header(name: &str, err: &RequestBuildError) -> String {
    match err {
        RequestBuildError::HeaderNameIsEmpty => "Header name must not be empty".to_string(),
        RequestBuildError::ForbiddenByteInHeaderName(byte) => format!(
            "Header name '{}' contains forbidden byte 0x{:02x}",
            name.escape_debug(),
            byte
        ),
        RequestBuildError::ForbiddenByteInHeaderValue(byte) => format!(
            "Value of header '{}' contains forbidden control byte 0x{:02x}",
            name, byte
        ),
        other => format!("Header '{}' can not be sent: {}", name.escape_debug(), other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_valid_header_is_kept() {
        let mut headers = FlUrlHeaders::new();
        headers.add("Authorization", "Bearer token123");
        headers.add("X-Name", "значение\twith a tab");

        assert!(headers.ensure_valid().is_ok());
        assert_eq!(headers.len(), 2);
        assert!(headers.has_header("authorization"));
    }

    #[test]
    fn a_value_with_a_new_line_fails_the_request_and_is_not_added() {
        let mut headers = FlUrlHeaders::new();
        headers.add("Authorization", "Bearer token123\n");

        assert_eq!(headers.len(), 0);
        assert!(!headers.has_header("Authorization"));

        match headers.ensure_valid() {
            Err(FlUrlError::RequestBuild(reason)) => {
                assert_eq!(
                    reason,
                    "Value of header 'Authorization' contains forbidden control byte 0x0a"
                );
            }
            other => panic!("unexpected result {other:?}"),
        }
    }

    #[test]
    fn the_message_does_not_carry_the_value() {
        let mut headers = FlUrlHeaders::new();
        headers.add("Authorization", "Bearer secret\r\n");

        let reason = format!("{:?}", headers.ensure_valid().unwrap_err());
        assert!(!reason.contains("secret"), "{reason}");
    }

    #[test]
    fn an_invalid_name_fails_the_request() {
        for name in ["", "X Name", "X-Name:", "X-Имя", "X\nName"] {
            let mut headers = FlUrlHeaders::new();
            headers.add(name, "value");

            assert!(
                matches!(headers.ensure_valid(), Err(FlUrlError::RequestBuild(_))),
                "{name:?}"
            );
        }
    }

    #[test]
    fn the_first_invalid_header_is_the_one_reported() {
        let mut headers = FlUrlHeaders::new();
        headers.add("First", "a\nb");
        headers.add("Second", "a\rb");
        headers.add("Third", "fine");

        let reason = format!("{:?}", headers.ensure_valid().unwrap_err());
        assert!(reason.contains("'First'"), "{reason}");
        assert_eq!(headers.len(), 1);
    }
}
