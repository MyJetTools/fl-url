use std::time::{Duration, SystemTime};

pub struct StopWatch {
    start_time: SystemTime,
    stop_time: SystemTime,
}

impl StopWatch {
    pub fn new() -> Self {
        let now = SystemTime::now();
        Self {
            start_time: now,
            stop_time: now,
        }
    }

    pub fn start(&mut self) {
        self.start_time = SystemTime::now()
    }

    pub fn pause(&mut self) {
        self.stop_time = SystemTime::now()
    }

    pub fn duration(&self) -> Duration {
        self.stop_time.duration_since(self.start_time).unwrap()
    }
}
