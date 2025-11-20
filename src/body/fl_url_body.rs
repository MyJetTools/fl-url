use rust_extensions::StrOrString;

use crate::body::{FormDataBody, UrlEncodedBody};

pub enum FlUrlBody {
    Json(Vec<u8>),
    UrlEncoded(UrlEncodedBody),
    FormData(FormDataBody),
    Raw {
        data: Vec<u8>,
        content_type: Option<&'static str>,
    },
    Empty,
}

impl FlUrlBody {
    pub fn empty() -> Self {
        FlUrlBody::Empty
    }

    pub fn from_raw_data(data: Vec<u8>, content_type: Option<&'static str>) -> Self {
        FlUrlBody::Raw { data, content_type }
    }

    pub fn as_json(value: &impl serde::Serialize) -> Self {
        let payload = serde_json::to_vec(value).expect("Failed to serialize to JSON");
        FlUrlBody::Json(payload)
    }

    pub fn get_content_type(&self) -> Option<StrOrString<'static>> {
        match self {
            FlUrlBody::Json(_) => Some("application/json".into()),
            FlUrlBody::UrlEncoded(_) => Some("application/x-www-form-urlencoded".into()),
            FlUrlBody::FormData(body) => Some(body.get_content_type().into()),
            FlUrlBody::Raw { content_type, .. } => {
                let content_type = (*content_type)?;
                Some(content_type.into())
            }
            FlUrlBody::Empty => None,
        }
    }

    pub fn into_vec(self) -> Vec<u8> {
        match self {
            FlUrlBody::Json(data) => data,
            FlUrlBody::UrlEncoded(body) => body.data.into_bytes(),
            FlUrlBody::FormData(body) => body.finalize().buffer,
            FlUrlBody::Raw { data, .. } => data,
            FlUrlBody::Empty => Vec::new(),
        }
    }

    /*
    pub fn as_slice(&self) -> &[u8] {
        match self {
            FlUrlBody::Json(data) => data.as_slice(),
            FlUrlBody::UrlEncoded(form_data) => form_data.data.as_bytes(),
            FlUrlBody::FormData(body) => body.buffer.as_slice(),
            FlUrlBody::Raw { data, .. } => data.as_slice(),
            FlUrlBody::Empty => &[],
        }
    }
     */
}

impl Into<FlUrlBody> for UrlEncodedBody {
    fn into(self) -> FlUrlBody {
        FlUrlBody::UrlEncoded(self)
    }
}

impl Into<FlUrlBody> for FormDataBody {
    fn into(self) -> FlUrlBody {
        FlUrlBody::FormData(self)
    }
}
