//! Request body types. These are re-exported straight from `my_http_utils::body`
//! (the single, transport-agnostic definition shared by both backends), so
//! `flurl::body::HttpRequestBody` and `my_http_utils::body::HttpRequestBody` are the
//! same type on native and wasm alike.
pub use my_http_utils::body::{FormDataBody, HttpRequestBody, UrlEncodedBody};

/// Supplies randomness to `my_http_utils` request building — currently only the
/// `multipart/form-data` boundary. `my_http_utils` itself carries no RNG (so it stays
/// wasm-safe); FlUrl plugs a target-appropriate generator in here. Used as the
/// `TRnd` type parameter of `THttpRequestBuilder::get_body`.
pub struct FlUrlRnd;

impl my_http_utils::schema::client::RandomStringGenerator for FlUrlRnd {
    fn generate_random_string(len: usize) -> String {
        rand_string(len)
    }
}

/// Builds a multipart [`FormDataBody`] with a randomly generated boundary.
///
/// `my_http_utils::body::FormDataBody::new` takes the random boundary string
/// explicitly (so it stays wasm-safe, with no built-in RNG); we generate it here.
pub fn new_form_data() -> FormDataBody {
    FormDataBody::new(&rand_string(16))
}

/// Native RNG: `rand`.
#[cfg(not(target_arch = "wasm32"))]
fn rand_string(len: usize) -> String {
    use rand::distr::Alphanumeric;
    rand::distr::SampleString::sample_string(&Alphanumeric, &mut rand::rng(), len)
}

/// wasm RNG: `Math.random()`. A multipart boundary only has to be absent from the
/// body of that one request, so a per-request random string is more than enough
/// (no `getrandom`/CSPRNG dependency needed).
#[cfg(target_arch = "wasm32")]
fn rand_string(len: usize) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut result = String::with_capacity(len);
    for _ in 0..len {
        let idx = ((js_sys::Math::random() * CHARS.len() as f64) as usize).min(CHARS.len() - 1);
        result.push(CHARS[idx] as char);
    }
    result
}
