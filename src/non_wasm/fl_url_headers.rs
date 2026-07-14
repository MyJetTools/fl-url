use my_http_client::{HeaderValuePosition, MyHttpClientHeadersBuilder};

pub struct FlUrlHeaders {
    pub(crate) headers: MyHttpClientHeadersBuilder,
    pub has_connection_header: bool,
    pub len: usize,
    pub host_header_value: Option<HeaderValuePosition>,
}

impl FlUrlHeaders {
    pub fn new() -> Self {
        Self {
            headers: MyHttpClientHeadersBuilder::new(),

            has_connection_header: false,
            host_header_value: None,
            len: 0,
        }
    }

    /*
       pub fn add_json_content_type(&mut self) {
           self.headers
               .add_header(CONTENT_TYPE.as_str(), "application/json");
       }
    */
    pub fn add(&mut self, name: &str, value: &str) {
        if rust_extensions::str_utils::compare_strings_case_insensitive(name, "connection") {
            self.has_connection_header = true;
        }

        let pos = self.headers.add_header(name, value);

        if name.eq_ignore_ascii_case("host") {
            self.host_header_value = Some(pos);
        }
        self.len += 1;
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

/// Lets a `url_utils` request model (`#[derive(MyHttpInput)]`) push its header
/// fields straight into our header collection during `execute_request`.
impl url_utils::schema::client::HeaderBuilder for FlUrlHeaders {
    fn add_header(&mut self, name: &str, value: &str) {
        self.add(name, value);
    }
}
