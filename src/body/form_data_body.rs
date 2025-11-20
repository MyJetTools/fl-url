use std::fmt::Display;

use rust_extensions::StrOrString;

pub struct FormDataBody {
    // files: Vec<MultipartFile>,
    boundary: String,
    pub(crate) buffer: Vec<u8>,
}

impl FormDataBody {
    pub fn new() -> Self {
        let boundary = format!("----DataFormBoundary{}", rand_string(16));
        Self {
            boundary,
            buffer: vec![], //files: vec![],
        }
    }

    pub fn append_form_data_field(
        mut self,
        name: impl Into<StrOrString<'static>>,
        value: impl Display,
    ) -> Self {
        use std::io::Write;

        let name = name.into();
        write!(
            &mut self.buffer,
            "--{}\r\nContent-Disposition: form-data; name=\"{}\"\r\n\r\n{}\r\n",
            self.boundary, name, value
        )
        .unwrap();

        self
    }

    pub fn append_form_data_file(
        mut self,
        name: impl Into<StrOrString<'static>>,
        file_name: impl Into<StrOrString<'static>>,
        content_type: impl Into<StrOrString<'static>>,
        content: &[u8],
    ) -> Self {
        use std::io::Write;

        let name = name.into();
        let file_name = file_name.into();
        let content_type = content_type.into();
        write!(
            &mut self.buffer,
            "--{}\r\nContent-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type:{}\r\n\r\n",
            self.boundary,
            name,
            file_name.as_str(),
            content_type.as_str()
        )
        .unwrap();

        self.buffer.extend_from_slice(content);
        self.buffer.extend_from_slice(b"\r\n--");
        self.buffer.extend_from_slice(self.boundary.as_bytes());
        self.buffer.extend_from_slice(b"\r\n");
        self
    }

    pub fn get_content_type(&self) -> String {
        format!("multipart/form-data; boundary={}", self.boundary)
    }

    /// Finalizes the form data by adding the closing boundary marker.
    /// This must be called before converting to bytes.
    pub(crate) fn finalize(mut self) -> Self {
        // Add the final closing boundary: --{boundary}--\r\n
        self.buffer.extend_from_slice(b"--");
        self.buffer.extend_from_slice(self.boundary.as_bytes());
        self.buffer.extend_from_slice(b"--\r\n");
        self
    }
}

// Simple random string generator for boundary (for demonstration)
fn rand_string(len: usize) -> String {
    use rand::distr::Alphanumeric;
    rand::distr::SampleString::sample_string(&Alphanumeric, &mut rand::rng(), len)
}
