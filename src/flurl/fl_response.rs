use hyper::{Body, Error, Response, StatusCode};
pub struct FlUrlResponse {
    pub response: Response<Body>,
}

impl FlUrlResponse {
    pub fn new(response: Response<Body>) -> Self {
        Self { response }
    }

    pub async fn get_body(self) -> Result<Vec<u8>, Error> {
        let body = self.response.into_body();
        let full_body = hyper::body::to_bytes(body).await?;

        Ok(full_body.iter().cloned().collect::<Vec<u8>>())
    }

    pub async fn get_body_as_ut8string(self) -> Result<String, Error> {
        let body = self.get_body().await?;
        let result = String::from_utf8(body).unwrap();
        Ok(result)
    }

    pub fn get_status_code(&self) -> u16 {
        self.response.status().as_u16()
    }
}
