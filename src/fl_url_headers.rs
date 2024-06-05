use rust_extensions::StrOrString;

pub struct FlUrlHeader {
    pub name: StrOrString<'static>,
    pub value: String,
}

pub struct FlUrlHeaders {
    headers: Vec<FlUrlHeader>,
    pub has_host_header: bool,
}

impl FlUrlHeaders {
    pub fn new() -> Self {
        Self {
            headers: Vec::new(),
            has_host_header: false,
        }
    }

    fn find_index(&self, name: &str) -> Option<usize> {
        self.headers.iter().position(|header| {
            rust_extensions::str_utils::compare_strings_case_insensitive(header.name.as_str(), name)
        })
    }

    pub fn add(&mut self, name: StrOrString<'static>, value: String) {
        if rust_extensions::str_utils::compare_strings_case_insensitive(name.as_str(), "host") {
            self.has_host_header = true;
        }

        match self.find_index(name.as_str()) {
            Some(index) => self.headers[index].value = value,
            None => {
                self.headers.push(FlUrlHeader { name, value });
            }
        }
    }

    pub fn len(&self) -> usize {
        self.headers.len()
    }

    pub fn iter(&self) -> std::slice::Iter<FlUrlHeader> {
        self.headers.iter()
    }
}
