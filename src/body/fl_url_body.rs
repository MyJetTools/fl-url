use serde::Serialize;

use crate::body::UrlEncodedBody;

pub enum FlUrlBody {
    Json(Vec<u8>),
    FormData(UrlEncodedBody),
    Raw {
        data: Vec<u8>,
        content_type: Option<&'static str>,
    },
}

impl FlUrlBody {
    pub fn from_raw_data(data: Vec<u8>, content_type: Option<&'static str>) -> Self {
        FlUrlBody::Raw { data, content_type }
    }

    pub fn new_as_json<T: Serialize>(value: T) -> Self {
        let json_data = serde_json::to_vec(&value).expect("Failed to serialize to JSON");
        FlUrlBody::Json(json_data)
    }

    pub fn get_content_type(&self) -> Option<&'static str> {
        match self {
            FlUrlBody::Json(_) => "application/json".into(),
            FlUrlBody::FormData(_) => "application/x-www-form-urlencoded".into(),
            FlUrlBody::Raw { content_type, .. } => *content_type,
        }
    }

    pub fn into_vec(self) -> Vec<u8> {
        match self {
            FlUrlBody::Json(data) => data,
            FlUrlBody::FormData(form_data) => form_data.data.into_bytes(),
            FlUrlBody::Raw { data, .. } => data,
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        match self {
            FlUrlBody::Json(data) => data.as_slice(),
            FlUrlBody::FormData(form_data) => form_data.data.as_bytes(),
            FlUrlBody::Raw { data, .. } => data.as_slice(),
        }
    }
}

impl Into<FlUrlBody> for UrlEncodedBody {
    fn into(self) -> FlUrlBody {
        FlUrlBody::FormData(self)
    }
}

impl<T: Serialize> From<T> for FlUrlBody {
    fn from(value: T) -> Self {
        let json_data = serde_json::to_vec(&value).expect("Failed to serialize to JSON");
        FlUrlBody::Json(json_data)
    }
}
