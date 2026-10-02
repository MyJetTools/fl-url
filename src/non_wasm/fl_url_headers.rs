use my_http_client::{HeaderValuePosition, MyHttpClientHeadersBuilder};

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
        // my-http-client's header builder panics on a header it must not put on the
        // wire. A header value comes from settings as often as from code — an api key
        // read together with its trailing new line — so here it is the request that
        // fails, with an error.
        if let Err(reason) = validate_header(name, value) {
            if self.invalid_header.is_none() {
                self.invalid_header = Some(reason);
            }
            return;
        }

        if rust_extensions::str_utils::compare_strings_case_insensitive(name, "connection") {
            self.has_connection_header = true;
        }

        let pos = self.headers.add_header(name, value);

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
        let result = self.headers.get_value(host_value_pos);
        Some(result)
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

/// Refuses exactly what my-http-client's `write_header` refuses with a panic: an
/// empty name, a name that is not an HTTP token, a value with CR, LF or NUL in it.
fn validate_header(name: &str, value: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Header name must not be empty".to_string());
    }

    if let Some(byte) = name.bytes().find(|byte| !is_header_name_byte(*byte)) {
        return Err(format!(
            "Header name '{}' contains forbidden byte 0x{:02x}",
            name.escape_debug(),
            byte
        ));
    }

    // The value itself stays out of the message: that is where the secrets are.
    if let Some(byte) = find_forbidden_header_value_byte(value) {
        return Err(format!(
            "Value of header '{}' contains forbidden control byte 0x{:02x}",
            name, byte
        ));
    }

    Ok(())
}

/// CR, LF and NUL — a header value carrying one would split the header block.
pub(crate) fn find_forbidden_header_value_byte(value: &str) -> Option<u8> {
    value
        .bytes()
        .find(|byte| matches!(byte, b'\r' | b'\n' | 0))
}

fn is_header_name_byte(byte: u8) -> bool {
    matches!(
        byte,
        b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.'
        | b'^' | b'_' | b'`' | b'|' | b'~'
        | b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn panics(action: impl FnOnce() + std::panic::UnwindSafe) -> bool {
        std::panic::catch_unwind(action).is_err()
    }

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

    /// `validate_header` repeats my-http-client's rules, so the two can drift. A
    /// header it lets through must never reach the panic it stands in front of.
    #[test]
    fn nothing_that_passes_the_check_panics_in_my_http_client() {
        for code in 0..=0x2ffu32 {
            let Some(c) = char::from_u32(code) else {
                continue;
            };

            let name = format!("X{c}Name");
            let value = format!("a{c}b");

            let mut headers = FlUrlHeaders::new();
            assert!(
                !panics(move || headers.add(name.as_str(), "value")),
                "name with {c:?}"
            );

            let mut headers = FlUrlHeaders::new();
            assert!(
                !panics(move || headers.add("X-Name", value.as_str())),
                "value with {c:?}"
            );
        }
    }

    /// The other direction: the check must not refuse more than my-http-client does,
    /// or a header that used to be sent would start failing requests.
    #[test]
    fn nothing_my_http_client_accepts_is_refused() {
        for code in 0..=0x2ffu32 {
            let Some(c) = char::from_u32(code) else {
                continue;
            };

            let name = format!("X{c}Name");
            let value = format!("a{c}b");

            let accepted_name = name.clone();
            let name_is_accepted = !panics(move || {
                MyHttpClientHeadersBuilder::new().add_header(accepted_name.as_str(), "value");
            });
            assert_eq!(
                validate_header(name.as_str(), "value").is_ok(),
                name_is_accepted,
                "name with {c:?}"
            );

            let accepted_value = value.clone();
            let value_is_accepted = !panics(move || {
                MyHttpClientHeadersBuilder::new().add_header("X-Name", accepted_value.as_str());
            });
            assert_eq!(
                validate_header("X-Name", value.as_str()).is_ok(),
                value_is_accepted,
                "value with {c:?}"
            );
        }
    }
}
