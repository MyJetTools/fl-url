//! Request body types. These are re-exported straight from `url_utils::body`
//! (the single, transport-agnostic definition shared with wasm transports), so
//! `flurl::body::HttpRequestBody` and `url_utils::body::HttpRequestBody` are the
//! same type.
pub use url_utils::body::{FormDataBody, HttpRequestBody, UrlEncodedBody};

/// Supplies randomness to `url_utils` request building — currently only the
/// `multipart/form-data` boundary. `url_utils` itself carries no RNG (so it stays
/// wasm-safe); fl-url, being native, plugs its `rand`-backed generator in here.
/// Used as the `TRnd` type parameter of `THttpRequestBuilder::get_body`.
pub struct FlUrlRnd;

impl url_utils::schema::client::RandomStringGenerator for FlUrlRnd {
    fn generate_random_string(len: usize) -> String {
        rand_string(len)
    }
}

/// Builds a multipart [`FormDataBody`] with a randomly generated boundary.
///
/// `url_utils::body::FormDataBody::new` takes the random boundary string
/// explicitly (so it stays wasm-safe, with no built-in RNG). On native we can
/// generate it here, preserving the old `FormDataBody::new()` ergonomics.
pub fn new_form_data() -> FormDataBody {
    FormDataBody::new(&rand_string(16))
}

fn rand_string(len: usize) -> String {
    use rand::distr::Alphanumeric;
    rand::distr::SampleString::sample_string(&Alphanumeric, &mut rand::rng(), len)
}
