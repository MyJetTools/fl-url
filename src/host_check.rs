//! What a url must name before a `FlUrl` is made of it: somewhere to connect to.
//!
//! The url usually comes from a service's settings. A value that is empty there, or
//! is not a url at all, used to be accepted and then panic once a request was built
//! from it — so it is refused here, with an error the service can report.

use my_http_utils::UrlBuilder;

use crate::FlUrlError;

/// rust-extensions hands `host:port` around as a `ShortString`, which holds this
/// many bytes and panics past them. Its `get_host_port` says so, and leaves the
/// check to whoever takes the address from outside — which is here.
const MAX_HOST_PORT_LEN: usize = 255;

/// `url` is only what the error message quotes; the checks read the builder, because
/// the builder is what the request and the connection are made from.
pub(crate) fn ensure_host_is_usable(url: &str, url_builder: &UrlBuilder) -> Result<(), FlUrlError> {
    if !names_a_host(url_builder) {
        return Err(invalid_url(url, "it names no host"));
    }

    if !host_fits(url_builder) {
        return Err(invalid_url(url, "the host is too long"));
    }

    Ok(())
}

/// `""`, `http://`, `http://:8080`, `http+unix://` name nowhere to connect to.
fn names_a_host(url_builder: &UrlBuilder) -> bool {
    let host = url_builder.get_host();

    // The "host" of a unix socket url is the path of the socket file.
    if url_builder.is_unix_socket() {
        return !host.trim_matches('/').is_empty();
    }

    // The second half is a defense against a stale checkout of my-http-utils: up to
    // `35466bd` UrlBuilder never read the first character after the scheme as a
    // delimiter, so an authority with no host in it — `http://:8080`, `http:///path`
    // — came back as a host that starts with one, and appending a path to a url with
    // nothing after its scheme made the builder slice backwards.
    !host.trim().is_empty() && !host.starts_with([':', '/', '?', '#'])
}

/// A host name ends at 253 bytes and a socket path far earlier, so a longer one is
/// not a host — more likely something else pasted into the url's place.
fn host_fits(url_builder: &UrlBuilder) -> bool {
    let host_port = url_builder.get_host_port();

    if url_builder.is_unix_socket() {
        return host_port.len() <= MAX_HOST_PORT_LEN;
    }

    // A url with no port gets the default one of its scheme appended.
    let default_port = if host_port.len() > url_builder.get_host().len() {
        ""
    } else {
        let scheme = url_builder.get_scheme();
        if scheme.is_https() || scheme.is_wss() {
            ":443"
        } else {
            ":80"
        }
    };

    host_port.len() + default_port.len() <= MAX_HOST_PORT_LEN
}

fn invalid_url(url: &str, reason: &str) -> FlUrlError {
    FlUrlError::InvalidUrl(format!("Invalid url '{}': {}", url, reason))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names_a_host(url: &str) -> bool {
        super::names_a_host(&UrlBuilder::new(url))
    }

    fn host_fits(url: &str) -> bool {
        super::host_fits(&UrlBuilder::new(url))
    }

    /// A host name of exactly `len` bytes.
    fn host(len: usize) -> String {
        "h".repeat(len)
    }

    #[test]
    fn a_url_with_a_host_is_accepted() {
        for url in [
            "localhost",
            "10.0.0.1:5123",
            "localhost:8080/templates",
            "http://localhost",
            "http://localhost:8080",
            "HTTP://localhost:8080",
            "https://domain.com/path?a=b",
            "https://domain.com?a=b",
            "http://[::1]:8080/path",
            "http://хост.рф/путь",
            "ws://localhost:8080",
        ] {
            assert!(names_a_host(url), "{url}");
            assert!(host_fits(url), "{url}");
        }
    }

    #[test]
    fn a_unix_socket_path_is_accepted() {
        for url in [
            "/var/run/docker.sock",
            "~/docker.sock",
            "http+unix://var/run/docker.sock",
            "http+unix:///var/run/docker.sock",
            "http+unix:/var/run/docker.sock",
            "/var/run/docker.sock:/containers/json",
        ] {
            assert!(names_a_host(url), "{url}");
            assert!(host_fits(url), "{url}");
        }
    }

    #[test]
    fn a_url_with_no_host_is_rejected() {
        for url in [
            "",
            " ",
            "http://",
            "https://",
            "HTTP://",
            "ws://",
            "wss://",
            "http:// ",
            "http://:8080",
            "http://:8080/path",
            "http:///path",
            "http://?a=b",
            "http://#fragment",
        ] {
            assert!(!names_a_host(url), "{url:?}");
        }
    }

    #[test]
    fn a_unix_socket_url_with_no_path_is_rejected() {
        for url in ["/", "///", "http+unix:/", "http+unix://", "http+unix:///"] {
            assert!(!names_a_host(url), "{url}");
        }
    }

    // `host:port` has to fit into 255 bytes together with the default port, which is
    // appended when the url has none.
    #[test]
    fn the_longest_host_that_fits_is_accepted() {
        assert!(host_fits(&format!("http://{}", host(252))));
        assert!(host_fits(&host(252)));
        assert!(host_fits(&format!("http://{}/path?a=b", host(252))));
        assert!(host_fits(&format!("https://{}", host(251))));
        assert!(host_fits(&format!("http://{}:1", host(253))));
        assert!(host_fits(&format!("http://{}:65535", host(249))));
        assert!(host_fits(&format!("/{}", host(254))));
    }

    #[test]
    fn a_host_one_byte_longer_is_rejected() {
        assert!(!host_fits(&format!("http://{}", host(253))));
        assert!(!host_fits(&host(253)));
        assert!(!host_fits(&format!("http://{}/path?a=b", host(253))));
        assert!(!host_fits(&format!("https://{}", host(252))));
        assert!(!host_fits(&format!("http://{}:1", host(254))));
        assert!(!host_fits(&format!("http://{}:65535", host(250))));
        assert!(!host_fits(&format!("/{}", host(255))));
    }

    #[test]
    fn the_error_quotes_the_url() {
        let err = ensure_host_is_usable("http://", &UrlBuilder::new("http://")).unwrap_err();

        match err {
            FlUrlError::InvalidUrl(message) => {
                assert_eq!(message, "Invalid url 'http://': it names no host")
            }
            other => panic!("unexpected error {other:?}"),
        }
    }
}
