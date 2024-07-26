#[derive(Debug, Clone)]
pub enum Scheme {
    Http,
    Https,
    #[cfg(feature = "unix-socket")]
    UnixSocket,
}

impl Scheme {
    pub fn from_url(src: &str) -> (Self, Option<usize>) {
        let index = src.find(":/");

        if index.is_none() {
            return (Scheme::get_default(), index);
        }

        let index = index.unwrap();

        let scheme = &src[..index];

        if rust_extensions::str_utils::compare_strings_case_insensitive("http", scheme) {
            return (Scheme::Http, Some(index));
        }

        if rust_extensions::str_utils::compare_strings_case_insensitive("https", scheme) {
            return (Scheme::Https, Some(index));
        }

        #[cfg(feature = "unix-socket")]
        if rust_extensions::str_utils::compare_strings_case_insensitive("http+unix", scheme) {
            return (Scheme::UnixSocket, Some(index));
        }
        #[cfg(not(feature = "unix-socket"))]
        if rust_extensions::str_utils::compare_strings_case_insensitive("http+unix", scheme) {
            panic!("Please enable feature unix-socket to use http+unix scheme");
        }

        panic!("Unknown scheme: {}", scheme);
    }

    pub fn get_default() -> Self {
        Scheme::Http
    }

    pub fn is_http(&self) -> bool {
        match self {
            Scheme::Http => true,
            _ => false,
        }
    }

    pub fn is_https(&self) -> bool {
        match self {
            Scheme::Https => true,
            _ => false,
        }
    }

    #[cfg(feature = "unix-socket")]
    pub fn is_unix_socket(&self) -> bool {
        match self {
            Scheme::UnixSocket => true,
            _ => false,
        }
    }
    pub fn scheme_as_str(&self) -> &str {
        match self {
            Scheme::Http => "http://",
            Scheme::Https => "https://",
            #[cfg(feature = "unix-socket")]
            Scheme::UnixSocket => "http+unix:/",
        }
    }
}
