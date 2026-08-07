pub const HTTP_DEFAULT_PORT: u16 = 80;
pub const HTTPS_DEFAULT_PORT: u16 = 443;

/// HTTP/2 has no request line — the target is carried by the `:authority` pseudo
/// header, and a unix socket path is not a valid authority (the first '/' ends
/// one). So h2 over a unix socket sends this instead, unless the caller set a
/// Host header of their own. The value never reaches the socket lookup: the
/// connector already knows which socket file to open.
#[cfg(unix)]
pub const UNIX_SOCKET_AUTHORITY: &str = "localhost";
