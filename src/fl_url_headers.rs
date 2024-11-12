use hyper::header::CONTENT_TYPE;
use my_http_client::MyHttpClientHeadersBuilder;

pub struct FlUrlHeaders {
    headers: MyHttpClientHeadersBuilder,
    pub has_host_header: bool,
    pub has_connection_header: bool,
    pub len: usize,
}

impl FlUrlHeaders {
    pub fn new() -> Self {
        Self {
            headers: MyHttpClientHeadersBuilder::new(),
            has_host_header: false,
            has_connection_header: false,
            len: 0,
        }
    }

    pub fn add_json_content_type(&mut self) {
        self.headers
            .add_header(CONTENT_TYPE.as_str(), "application/json");
    }

    pub fn add(&mut self, name: &str, value: &str) {
        if rust_extensions::str_utils::compare_strings_case_insensitive(name, "host") {
            self.has_host_header = true;
        }

        if rust_extensions::str_utils::compare_strings_case_insensitive(name, "connection") {
            self.has_connection_header = true;
        }

        self.headers.add_header(name, value);
        self.len += 1;
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
