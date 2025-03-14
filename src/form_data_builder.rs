use std::fmt::Display;

use http::header::CONTENT_TYPE;
use rust_extensions::StrOrString;

use crate::{FlUrl, FlUrlError, FlUrlResponse};

/*
pub struct MultipartFile {
    name: StrOrString<'static>,
    file_name: StrOrString<'static>,
    content_type: StrOrString<'static>,
    content: Vec<u8>,
    boundary: String,
}
*/

pub struct FormDataBuilder {
    fl_url: FlUrl,
    // files: Vec<MultipartFile>,
    boundary: String,
    buffer: Vec<u8>,
}

impl FormDataBuilder {
    pub fn new(fl_url: FlUrl) -> Self {
        let boundary = format!("----DataFormBoundary{}", rand_string(16));
        Self {
            fl_url,
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

    fn get_result(mut self) -> (Vec<u8>, FlUrl) {
        use std::io::Write;

        let content_type = self.get_content_type();
        write!(&mut self.buffer, "--{}--\r\n", self.boundary).unwrap();

        self.fl_url
            .headers
            .add(CONTENT_TYPE.as_str(), &content_type);

        println!("{:?}", std::str::from_utf8(self.buffer.as_slice()));

        (self.buffer, self.fl_url)
    }

    fn get_content_type(&self) -> String {
        format!("multipart/form-data; boundary={}", self.boundary)
    }

    pub async fn post(self) -> Result<FlUrlResponse, FlUrlError> {
        let (body, fl_url) = self.get_result();

        fl_url.post(body.into()).await
    }

    pub async fn put(self) -> Result<FlUrlResponse, FlUrlError> {
        let (body, fl_url) = self.get_result();
        fl_url.put(body.into()).await
    }
}

// Simple random string generator for boundary (for demonstration)
fn rand_string(len: usize) -> String {
    use rand::distr::Alphanumeric;
    rand::distr::SampleString::sample_string(&Alphanumeric, &mut rand::rng(), len)
}
