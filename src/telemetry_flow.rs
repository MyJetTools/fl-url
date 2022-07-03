use hyper::Error;
use my_telemetry::{MyTelemetryContext, TelemetryEvent};
use rust_extensions::date_time::DateTimeAsMicroseconds;

use crate::FlUrlResponse;

pub struct TelemetryData {
    pub method: hyper::Method,
    pub url: String,
}

impl TelemetryData {
    pub fn as_string(&self) -> String {
        format!("[{}]{}", self.method, self.url)
    }
}

pub struct TelemetryFlow {
    started: DateTimeAsMicroseconds,
    telemetry_context: MyTelemetryContext,
    pub data: Option<TelemetryData>,
}

impl TelemetryFlow {
    pub fn new(telemetry_context: Option<MyTelemetryContext>) -> Option<Self> {
        let telemetry_context = telemetry_context?;
        Self {
            started: DateTimeAsMicroseconds::now(),
            telemetry_context,
            data: None,
        }
        .into()
    }

    pub async fn write_telemetry(&mut self, result: &Result<FlUrlResponse, Error>) {
        if !my_telemetry::TELEMETRY_INTERFACE.is_telemetry_set_up() {
            return;
        }

        let data = self.data.take();

        if data.is_none() {
            return;
        }

        let data = data.unwrap();

        let telemetry_event = match &result {
            Ok(result) => {
                let status_code = result.get_status_code();
                if status_code < 300 {
                    TelemetryEvent {
                        process_id: self.telemetry_context.process_id,
                        started: self.started.unix_microseconds,
                        finished: DateTimeAsMicroseconds::now().unix_microseconds,
                        data: data.as_string(),
                        success: format!("Status Code: {}", status_code).into(),
                        fail: None,
                        ip: None,
                    }
                } else {
                    TelemetryEvent {
                        process_id: self.telemetry_context.process_id,
                        started: self.started.unix_microseconds,
                        finished: DateTimeAsMicroseconds::now().unix_microseconds,
                        data: data.as_string(),
                        success: None,
                        fail: format!("Status Code: {}", status_code).into(),
                        ip: None,
                    }
                }
            }
            Err(err) => TelemetryEvent {
                process_id: self.telemetry_context.process_id,
                started: self.started.unix_microseconds,
                finished: DateTimeAsMicroseconds::now().unix_microseconds,
                data: data.as_string(),
                success: None,
                fail: format!("Err: {}", err).into(),
                ip: None,
            },
        };

        let mut write_access = my_telemetry::TELEMETRY_INTERFACE
            .telemetry_collector
            .lock()
            .await;

        write_access.write(telemetry_event)
    }
}
