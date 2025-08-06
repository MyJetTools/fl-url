pub struct UrlEncodedBody {
    pub data: String,
}

impl UrlEncodedBody {
    pub fn new() -> Self {
        UrlEncodedBody {
            data: String::new(),
        }
    }
    pub fn append(mut self, key: &str, value: &str) -> Self {
        if !self.data.is_empty() {
            self.data.push('&');
        }

        url_utils::encode_to_url_string_and_copy(&mut self.data, key);
        self.data.push('=');
        url_utils::encode_to_url_string_and_copy(&mut self.data, value);
        self
    }
}
