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
    fields: Vec<(StrOrString<'static>, String)>,
    // files: Vec<MultipartFile>,
    boundary: String,
}

impl FormDataBuilder {
    pub fn new(fl_url: FlUrl) -> Self {
        let boundary = format!("----DataFormBoundary{}", rand_string(16));
        Self {
            fl_url,
            boundary,
            fields: vec![],
            //files: vec![],
        }
    }

    pub fn append_form_data_field(
        mut self,
        name: impl Into<StrOrString<'static>>,
        value: impl Display,
    ) -> Self {
        let value = value.to_string();
        self.fields.push((name.into(), value));

        self
    }

    fn form_data_to_bytes(&self) -> std::io::Result<Vec<u8>> {
        use std::io::Write;

        let mut buffer: Vec<u8> = Vec::new();

        // Write text fields
        for (name, value) in &self.fields {
            let _ = write!(
                &mut buffer,
                "--{}\r\nContent-Disposition: form-data; name=\"{}\"\r\n\r\n{}\r\n",
                self.boundary, name, value
            )?;
        }

        // Write closing boundary
        write!(&mut buffer, "--{}--\r\n", self.boundary)?;

        Ok(buffer)
    }

    fn get_content_type(&self) -> String {
        format!("multipart/form-data; boundary={}", self.boundary)
    }

    pub async fn post(mut self) -> Result<FlUrlResponse, FlUrlError> {
        let body = self.form_data_to_bytes();

        if let Err(err) = body {
            panic!(
                "Somehow we could not serialize from data for request: '{}'. Err: {}",
                self.fl_url.url.to_string(),
                err
            );
        }

        let body = body.unwrap();

        self.fl_url
            .headers
            .add(CONTENT_TYPE.as_str(), &self.get_content_type());

        self.fl_url.post(body.into()).await
    }

    pub async fn put(mut self) -> Result<FlUrlResponse, FlUrlError> {
        let body = self.form_data_to_bytes();

        if let Err(err) = body {
            panic!(
                "Somehow we could not serialize from data for request: '{}'. Err: {}",
                self.fl_url.url.to_string(),
                err
            );
        }

        let body = body.unwrap();

        self.fl_url
            .headers
            .add(CONTENT_TYPE.as_str(), &self.get_content_type());

        self.fl_url.put(body.into()).await
    }
}

// Simple random string generator for boundary (for demonstration)
fn rand_string(len: usize) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let mut s = String::new();
    for i in 0..len {
        let c = ((seed >> (i % 64)) & 0xF) as u8;
        s.push((b'A' + (c % 26)) as char);
    }
    s
}
