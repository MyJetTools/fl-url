#[derive(Debug, Clone)]
pub enum Scheme {
    Http,
    Https,
    UnixSocket,
}

impl Scheme {
    pub fn from_url(src: &str) -> (Self, Option<usize>) {
        let index = src.find("://");

        if index.is_none() {
            return (Scheme::get_default(), index);
        }

        let index = index.unwrap();

        let scheme = &src[..index];

        if scheme.len() == 4 {
            if scheme == "http" {
                return (Scheme::Http, Some(index));
            }

            if scheme == "HTTP" {
                return (Scheme::Http, Some(index));
            }

            if scheme.to_lowercase() == "http" {
                return (Scheme::Http, Some(index));
            }
        }

        if scheme.len() == 5 {
            if scheme == "https" {
                return (Scheme::Https, Some(index));
            }

            if scheme == "HTTPS" {
                return (Scheme::Https, Some(index));
            }

            if scheme.to_lowercase() == "https" {
                return (Scheme::Https, Some(index));
            }
        }

        if scheme == "http+unix" {
            return (Scheme::UnixSocket, Some(index));
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
            Scheme::UnixSocket => "/",
        }
    }
}
