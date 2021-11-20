use std::sync::Arc;

use hyper::Error;
use my_telemetry::MyTelemetry;

use crate::stop_watch::StopWatch;
use crate::FlUrl;

use super::FlUrlResponse;

pub struct FlUrlWithTelemetry<TMyTelemetry: MyTelemetry> {
    pub fl_url: FlUrl,
    pub telemetry: Option<Arc<TMyTelemetry>>,
    pub dependency_type: String,
}

impl<'s, TMyTelemetry: MyTelemetry> FlUrlWithTelemetry<TMyTelemetry> {
    pub fn new(url: &str, telemetry: Option<Arc<TMyTelemetry>>, dependency_type: String) -> Self {
        Self {
            fl_url: FlUrl::new(url),
            telemetry,
            dependency_type,
        }
    }

    pub fn from_fl_url(
        fl_url: FlUrl,
        telemetry: Option<Arc<TMyTelemetry>>,
        dependency_type: String,
    ) -> Self {
        Self {
            fl_url,
            telemetry,
            dependency_type,
        }
    }

    pub fn append_path_segment(mut self, path: &str) -> Self {
        self.fl_url = self.fl_url.append_path_segment(path);
        self
    }

    pub fn append_query_param(mut self, param: &str, value: &str) -> Self {
        self.fl_url = self.fl_url.append_query_param(param, value);
        self
    }

    pub fn append_query_param_string(mut self, param: &str, value: String) -> Self {
        self.fl_url = self.fl_url.append_query_param_string(param, value);
        self
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.fl_url = self.fl_url.with_header(name, value);
        self
    }

    pub fn with_header_val_string(mut self, name: &str, value: String) -> Self {
        self.fl_url = self.fl_url.with_header_val_string(name, value);
        self
    }

    fn get_telemetry(&mut self, verb: &str) -> TelemetryData<TMyTelemetry> {
        let mut result = None;
        std::mem::swap(&mut result, &mut self.telemetry);

        let mut sw = StopWatch::new();
        sw.start();

        TelemetryData {
            telemetry: result,
            sw,
            target: self.fl_url.url.get_host().to_string(),
            dependency_type: self.dependency_type.to_string(),
            name: format!("{} {}", verb, self.fl_url.url.get_path_and_query()),
        }
    }

    pub async fn get(mut self) -> Result<FlUrlResponse, Error> {
        let telemetry = self.get_telemetry("GET");

        let result = self.fl_url.get().await;

        telemetry.write_telemetry(&result);

        return result;
    }

    pub async fn head(mut self) -> Result<FlUrlResponse, Error> {
        let telemetry = self.get_telemetry("HEAD");

        let result = self.fl_url.head().await;
        telemetry.write_telemetry(&result);
        result
    }

    pub async fn post(mut self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, Error> {
        let telemetry = self.get_telemetry("POST");
        let result = self.fl_url.post(body).await;
        telemetry.write_telemetry(&result);
        result
    }

    pub async fn put(mut self, body: Option<Vec<u8>>) -> Result<FlUrlResponse, Error> {
        let telemetry = self.get_telemetry("PUT");
        let result = self.fl_url.put(body).await;

        telemetry.write_telemetry(&result);
        result
    }

    pub async fn delete(mut self) -> Result<FlUrlResponse, Error> {
        let telemetry = self.get_telemetry("DELETE");
        let result = self.fl_url.delete().await;
        telemetry.write_telemetry(&result);
        result
    }
}

struct TelemetryData<TMyTelemetry: MyTelemetry> {
    pub telemetry: Option<Arc<TMyTelemetry>>,
    pub sw: StopWatch,
    pub name: String,
    pub dependency_type: String,
    pub target: String,
}

impl<TMyTelemetry: MyTelemetry> TelemetryData<TMyTelemetry> {
    fn write_telemetry(mut self, result: &Result<FlUrlResponse, Error>) {
        self.sw.pause();

        if let Some(telemetry) = self.telemetry {
            match &result {
                Ok(result) => {
                    if result.get_status_code() < 300 {
                        telemetry.track_dependency_duration(
                            self.name,
                            self.dependency_type,
                            self.target,
                            true,
                            self.sw.duration(),
                        )
                    } else {
                        telemetry.track_dependency_duration(
                            self.name,
                            self.dependency_type,
                            self.target,
                            false,
                            self.sw.duration(),
                        )
                    }
                }
                Err(_) => telemetry.track_dependency_duration(
                    self.name,
                    self.dependency_type,
                    self.target,
                    false,
                    self.sw.duration(),
                ),
            }
        }
    }
}
