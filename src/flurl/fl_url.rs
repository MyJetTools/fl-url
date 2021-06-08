use hyper::{header::*, Error};
use hyper::{Body, Client, Method, Request};
use hyper_tls::HttpsConnector;
use std::{collections::HashMap, str::FromStr};

use super::url_utils;

use super::FlUrlResponse;

pub struct FlUrl {
    url: String,

    path: Vec<String>,
    pub query: HashMap<String, String>,
    pub headers: HashMap<String, String>,
}

impl<'t> FlUrl {
    pub fn new(url: &'t str) -> FlUrl {
        FlUrl {
            url: url.to_string(),
            path: Vec::new(),
            query: HashMap::new(),
            headers: HashMap::new(),
        }
    }

    pub fn get_path(&self) -> String {
        if self.path.len() == 0 {
            return "/".to_string();
        }

        let mut result: Vec<u8> = vec![];

        fill_with_path(&mut result, &self.path);

        return String::from_utf8(result).unwrap();
    }

    pub fn append_path_segment(mut self, path: &str) -> Self {
        self.path.push(path.to_string());
        self
    }

    pub fn append_query_param(mut self, param: &str, value: &str) -> Self {
        self.query.insert(param.to_string(), value.to_string());
        self
    }

    pub fn append_query_param_string(mut self, param: &str, value: String) -> Self {
        self.query.insert(param.to_string(), value);
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
        let url = self.get_url();

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
        let url = self.get_url();

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

    pub async fn post(self, body: Option<&'static [u8]>) -> Result<FlUrlResponse, Error> {
        let url = self.get_url();

        let mut req = Request::builder().method(Method::GET).uri(url);

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

    pub async fn put(self, body: Option<&'static [u8]>) -> Result<FlUrlResponse, Error> {
        let url = self.get_url();

        let mut req = Request::builder().method(Method::PUT).uri(url);

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

        let req = req.body(body).expect("request builder");

        let https = HttpsConnector::new();
        let client = Client::builder().build::<_, hyper::Body>(https);
        let response = client.request(req).await?;

        return Ok(FlUrlResponse::new(response));
    }

    pub async fn delete(self) -> Result<FlUrlResponse, Error> {
        let url = self.get_url();

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

    fn get_url(&self) -> String {
        if self.path.len() == 0 && self.query.len() == 0 {
            return self.url.to_string();
        }

        let mut result: Vec<u8> = Vec::new();

        fill_with_url(&mut result, &self.url);
        if self.path.len() > 0 {
            fill_with_path(&mut result, &self.path);
        }

        if self.query.len() > 0 {
            fill_with_query(&mut result, &self.query)
        }

        return String::from_utf8(result).unwrap();
    }
}

fn fill_with_url(res: &mut Vec<u8>, src: &str) {
    if src.ends_with('/') {
        res.extend(src[0..src.len() - 1].as_bytes());
    } else {
        res.extend(src.as_bytes());
    }
}

fn fill_with_path(res: &mut Vec<u8>, src: &Vec<String>) {
    for segment in src {
        res.push(b'/');
        res.extend(segment.as_bytes())
    }
}

fn fill_with_query(res: &mut Vec<u8>, src: &HashMap<String, String>) {
    let mut first = true;
    for (key, value) in src {
        if first {
            res.push(b'?');
            first = false;
        } else {
            res.push(b'&');
        }

        url_utils::encode_to_url_string_and_copy(res, key);
        res.push(b'=');
        url_utils::encode_to_url_string_and_copy(res, value);
    }
}
