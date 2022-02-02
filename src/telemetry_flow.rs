use hyper::Error;
use std::sync::Arc;

use my_telemetry::MyTelemetry;

use crate::{stop_watch::StopWatch, FlUrlResponse};

pub struct TelemetryFlow {
    pub telemetry: Arc<dyn MyTelemetry>,
    pub sw: StopWatch,
    pub name: String,
    pub dependency_type: String,
    pub target: String,
}

impl TelemetryFlow {
    pub fn write_telemetry(mut self, result: &Result<FlUrlResponse, Error>) {
        self.sw.pause();

        match &result {
            Ok(result) => {
                if result.get_status_code() < 300 {
                    self.telemetry.track_dependency_duration(
                        self.name,
                        self.dependency_type,
                        self.target,
                        true,
                        self.sw.duration(),
                    )
                } else {
                    self.telemetry.track_dependency_duration(
                        self.name,
                        self.dependency_type,
                        self.target,
                        false,
                        self.sw.duration(),
                    )
                }
            }
            Err(_) => self.telemetry.track_dependency_duration(
                self.name,
                self.dependency_type,
                self.target,
                false,
                self.sw.duration(),
            ),
        }
    }
}
