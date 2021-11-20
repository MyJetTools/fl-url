use hyper::{header::*, Error};
use hyper::{Body, Client, Method, Request};
use hyper_tls::HttpsConnector;
use std::{collections::HashMap, str::FromStr};

use crate::FlUrlUriBuilder;

use super::FlUrlResponse;

pub struct FlUrl {
    pub url: FlUrlUriBuilder,
    pub headers: HashMap<String, String>,
}

impl<'t> FlUrl {
    pub fn new(url: &'t str) -> FlUrl {
        FlUrl {
            url: FlUrlUriBuilder::from_str(url),

            headers: HashMap::new(),
        }
    }

    pub fn append_path_segment(mut self, path: &str) -> Self {
        self.url.append_path_segment(path);
        self
    }

    pub fn append_query_param(mut self, param: &str, value: &str) -> Self {
        self.url.append_query_param(param, value.to_string());
        self
    }

    pub fn append_query_param_string(mut self, param: &str, value: String) -> Self {
        self.url.append_query_param(param, value);
        self
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.insert(name.to_string(), value.to_string());
        self
    }

    pub fn with_header_val_string(mut self, name: &str, value: String) -> Self {
        self.headers.insert(name.to_string(), value);
        self
    }

    pub async fn get(self) -> Result<FlUrlResponse, Error> {
        let url = self.url.to_string();

        let mut req = Request::builder().method(Method::GET).uri(url);

        if self.headers.len() > 0 {
            let headers = req.headers_mut().unwrap();
            for (key, value) in self.headers {
                let h = HeaderName::from_str(key.as_str()).unwrap();
                headers.insert(h, HeaderValue::from_str(value.as_str()).unwrap());
            }
        };

        let req = req.body(Body::empty()).expect("request builder");

        let https = HttpsConnector::new();
        let client = Client::builder().build::<_, hyper::Body>(https);
        let response = client.request(req).await?;

        return Ok(FlUrlResponse::new(response));
    }

    pub async fn head(self) -> Result<FlUrlResponse, Error> {
        let url = self.url.to_string();

        let mut req = Request::builder().method(Method::HEAD).uri(url);

        if self.headers.len() > 0 {
            let headers = req.headers_mut().unwrap();
            for (key, value) in self.headers {
                let h = HeaderName::from_str(key.as_str()).unwrap();
                headers.insert(h, HeaderValue::from_str(value.as_str()).unwrap());
            }
        };

        let req = req.body(Body::empty()).expect("request builder");

        let https = HttpsConnector::new();
        let client = Client::builder().build::<_, hyper::Body>(https);
        let response = client.request(req).await?;

        return Ok(FlUrlResponse::new(response));
    }

    pub async fn post(self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, Error> {
        let url = self.url.to_string();

        let mut req = Request::builder().method(Method::POST).uri(url);

        if self.headers.len() > 0 {
            let headers = req.headers_mut().unwrap();
            for (key, value) in self.headers {
                let h = HeaderName::from_str(key.as_str()).unwrap();
                headers.insert(h, HeaderValue::from_str(value.as_str()).unwrap());
            }
        };

        let body = match body {
            Some(payload) => Body::from(payload),
            None => Body::empty(),
        };

        let req = req.body(Body::from(body)).expect("request builder");

        let https = HttpsConnector::new();
        let client = Client::builder().build::<_, hyper::Body>(https);
        let response = client.request(req).await?;

        return Ok(FlUrlResponse::new(response));
    }

    pub async fn put(self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, Error> {
        let url = self.url.to_string();

        let mut req = Request::builder().method(Method::PUT).uri(url);

        if self.headers.len() > 0 {
            let headers = req.headers_mut().unwrap();
            for (key, value) in self.headers {
                let h = HeaderName::from_str(key.as_str()).unwrap();
                headers.insert(h, HeaderValue::from_str(value.as_str()).unwrap());
            }
        };

        let body = match body {
            Some(payload) => Body::from(payload.to_vec()),
            None => Body::empty(),
        };

        let req = req.body(body).expect("request builder");

        let https = HttpsConnector::new();
        let client = Client::builder().build::<_, hyper::Body>(https);
        let response = client.request(req).await?;

        return Ok(FlUrlResponse::new(response));
    }

    pub async fn delete(self) -> Result<FlUrlResponse, Error> {
        let url = self.url.to_string();

        let mut req = Request::builder().method(Method::DELETE).uri(url);

        if self.headers.len() > 0 {
            let headers = req.headers_mut().unwrap();
            for (key, value) in self.headers {
                let h = HeaderName::from_str(key.as_str()).unwrap();
                headers.insert(h, HeaderValue::from_str(value.as_str()).unwrap());
            }
        };

        let req = req.body(Body::empty()).expect("request builder");

        let https = HttpsConnector::new();
        let client = Client::builder().build::<_, hyper::Body>(https);
        let response = client.request(req).await?;

        return Ok(FlUrlResponse::new(response));
    }
}
