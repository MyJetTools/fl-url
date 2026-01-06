# FLUrl

A fluent, async HTTP client library for Rust, inspired by the .NET Flurl library (https://flurl.dev/).

FLUrl is a Hyper-based HTTP client that provides a fluent API for building and executing HTTP requests with connection pooling, retry logic, and comprehensive body type support.

## Features

- **Fluent API**: Chain methods to build requests naturally
- **Connection Reuse**: Automatic connection pooling and reuse for HTTP/1.1 and HTTP/2
- **Multiple HTTP Modes**: Support for HTTP/2, HTTP/1.1 with Hyper, and HTTP/1.1 without Hyper
- **Body Types**: JSON, URL-encoded, multipart/form-data, and raw data
- **SSL/TLS**: Client certificate support and invalid certificate acceptance
- **SSH Tunneling**: Optional SSH tunnel support via `with-ssh` feature
- **Unix Socket Support**: Native Unix socket support (Unix systems only)
- **Retry Logic**: Configurable retry mechanism
- **Request Compression**: Automatic gzip compression for request bodies
- **Streaming Responses**: Support for streaming response bodies
- **Debug Support**: Built-in request debugging capabilities

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
flurl = "0.6.1"
```

For SSH tunneling support:

```toml
[dependencies]
flurl = { version = "0.6.1", features = ["with-ssh"] }
```

## Basic Usage

### Simple GET Request

```rust
use flurl::FlUrl;

let response = FlUrl::new("http://mywebsite.com")
    .append_path_segment("api")
    .append_path_segment("users")
    .append_query_param("page", Some("1"))
    .append_query_param("limit", Some("10"))
    .get()
    .await?;
```

### Error Handling for URL Creation

```rust
use flurl::{FlUrl, FlUrlError};

// new() panics on invalid URL
let response = FlUrl::new("http://mywebsite.com").get().await?;

// try_new() returns Result for error handling
match FlUrl::try_new("invalid-url") {
    Ok(fl_url) => {
        // Use fl_url
    }
    Err(FlUrlError::InvalidUrl(e)) => {
        eprintln!("Invalid URL: {}", e);
    }
    Err(e) => {
        eprintln!("Error: {}", e);
    }
}
```

### Using String Literals (IntoFlUrl Trait)

```rust
use flurl::IntoFlUrl;

let response = "http://mywebsite.com"
    .append_path_segment("Row")
    .append_query_param("tableName", Some(table_name))
    .append_query_param("partitionKey", Some(partition_key))
    .get()
    .await?;
```

## HTTP Methods

### GET

```rust
let response = FlUrl::new("https://api.example.com/data")
    .get()
    .await?;
```

### GET with Debug

```rust
let mut debug_string = String::new();
let response = FlUrl::new("https://api.example.com/data")
    .get_with_debug(&mut debug_string)
    .await?;
println!("Request: {}", debug_string);
```

### POST

```rust
use flurl::body::FlUrlBody;

let body = FlUrlBody::as_json(&my_data);
let response = FlUrl::new("https://api.example.com/users")
    .post(body)
    .await?;
```

### POST with Debug

```rust
let mut debug_string = String::new();
let body = FlUrlBody::as_json(&my_data);
let response = FlUrl::new("https://api.example.com/users")
    .post_with_debug(body, &mut debug_string)
    .await?;
```

### PUT

```rust
let body = FlUrlBody::as_json(&update_data);
let response = FlUrl::new("https://api.example.com/users/123")
    .put(body)
    .await?;
```

### PATCH

```rust
let body = FlUrlBody::as_json(&patch_data);
let response = FlUrl::new("https://api.example.com/users/123")
    .patch(body)
    .await?;
```

### DELETE

```rust
let response = FlUrl::new("https://api.example.com/users/123")
    .delete()
    .await?;
```

### DELETE with Debug

```rust
let mut debug_string = String::new();
let response = FlUrl::new("https://api.example.com/users/123")
    .delete_with_debug(&mut debug_string)
    .await?;
```

### HEAD

```rust
let response = FlUrl::new("https://api.example.com/resource")
    .head()
    .await?;
```

## URL Building

### Append Path Segments

```rust
let response = FlUrl::new("https://api.example.com")
    .append_path_segment("api")
    .append_path_segment("v1")
    .append_path_segment("users")
    .get()
    .await?;
// Results in: https://api.example.com/api/v1/users
```

### Append Query Parameters

```rust
let response = FlUrl::new("https://api.example.com/search")
    .append_query_param("q", Some("rust"))
    .append_query_param("page", Some("1"))
    .append_query_param("sort", None) // Adds parameter without value
    .get()
    .await?;
// Results in: https://api.example.com/search?q=rust&page=1&sort
```

### Append Raw URL Ending

```rust
let response = FlUrl::new("https://api.example.com")
    .append_raw_ending_to_url("/custom/path?param=value")
    .get()
    .await?;
```

## Headers

### Add Custom Headers

```rust
let response = FlUrl::new("https://api.example.com/data")
    .with_header("Authorization", "Bearer token123")
    .with_header("X-Custom-Header", "value")
    .get()
    .await?;
```

## Request Bodies

### JSON Body

```rust
use flurl::body::FlUrlBody;
use serde::Serialize;

#[derive(Serialize)]
struct User {
    name: String,
    email: String,
}

let user = User {
    name: "John Doe".to_string(),
    email: "john@example.com".to_string(),
};

let response = FlUrl::new("https://api.example.com/users")
    .post(FlUrlBody::as_json(&user))
    .await?;
```

### URL-Encoded Body

```rust
use flurl::body::UrlEncodedBody;

let body = UrlEncodedBody::new()
    .append("username", "john")
    .append("password", "secret123")
    .append("remember", "true");

let response = FlUrl::new("https://api.example.com/login")
    .post(body)
    .await?;
```

### Multipart Form Data

```rust
use flurl::body::FormDataBody;

// Form fields
let form_data = FormDataBody::new()
    .append_form_data_field("username", "john")
    .append_form_data_field("email", "john@example.com");

let response = FlUrl::new("https://api.example.com/profile")
    .post(form_data)
    .await?;

// Form with file upload
let form_data = FormDataBody::new()
    .append_form_data_field("title", "My Document")
    .append_form_data_file("file", "document.pdf", "application/pdf", file_bytes);

let response = FlUrl::new("https://api.example.com/upload")
    .post(form_data)
    .await?;
```

### Raw Body

```rust
use flurl::body::FlUrlBody;

let raw_data = b"custom binary data";
let body = FlUrlBody::from_raw_data(raw_data.to_vec(), Some("application/octet-stream"));

let response = FlUrl::new("https://api.example.com/upload")
    .post(body)
    .await?;
```

## Response Handling

### Get Status Code

```rust
let mut response = FlUrl::new("https://api.example.com/data")
    .get()
    .await?;

let status_code = response.get_status_code();
println!("Status: {}", status_code);
```

### Get Body as Slice

```rust
let mut response = FlUrl::new("https://api.example.com/data")
    .get()
    .await?;

let body = response.get_body_as_slice().await?;
println!("Body length: {}", body.len());
```

### Get Body as String

```rust
let mut response = FlUrl::new("https://api.example.com/data")
    .get()
    .await?;

let body = response.get_body_as_str().await?;
println!("Body: {}", body);
```

### Get JSON Response

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct ApiResponse {
    data: Vec<String>,
}

let mut response = FlUrl::new("https://api.example.com/data")
    .get()
    .await?;

let api_response: ApiResponse = response.get_json().await?;
```

### Receive Full Body

```rust
let mut response = FlUrl::new("https://api.example.com/data")
    .get()
    .await?;

let body_bytes = response.receive_body().await?;
```

### Streaming Response

```rust
let response = FlUrl::new("https://api.example.com/large-file")
    .get()
    .await?;

let mut stream = response.get_body_as_stream();
while let Some(chunk) = stream.get_next_chunk().await? {
    // Process chunk
    println!("Received {} bytes", chunk.len());
}
```

### Get Headers

```rust
let mut response = FlUrl::new("https://api.example.com/data")
    .get()
    .await?;

// Get specific header
let content_type = response.get_header("Content-Type")?;

// Get header case-insensitive
let content_type = response.get_header_case_insensitive("content-type")?;

// Get all headers
let headers = response.get_headers();
for (key, value) in headers {
    println!("{}: {:?}", key, value);
}
```

## Connection Management

### Connection Reuse

By default, FLUrl reuses connections based on schema+domain to avoid the cost of establishing new connections and TLS handshakes.

```rust
// Connection will be reused for subsequent requests to the same domain
let response1 = FlUrl::new("https://api.example.com/endpoint1")
    .get()
    .await?;

let response2 = FlUrl::new("https://api.example.com/endpoint2")
    .get()
    .await?; // Reuses connection from response1
```

### Disable Connection Reuse

```rust
let response = FlUrl::new("https://api.example.com/data")
    .do_not_reuse_connection()
    .get()
    .await?;
```

### Custom Connection Cache

```rust
use std::sync::Arc;
use flurl::FlUrlHttpConnectionsCache;

let cache = Arc::new(FlUrlHttpConnectionsCache::new());
let response = FlUrl::new("https://api.example.com/data")
    .set_connections_cache(cache.clone())
    .get()
    .await?;
```

### Drop Connection Scenarios

Implement custom logic to determine when connections should be dropped:

```rust
use flurl::{DropConnectionScenario, FlUrlResponse};

pub struct MyCustomDropConnectionScenario;

impl DropConnectionScenario for MyCustomDropConnectionScenario {
    fn should_we_drop_it(&self, result: &FlUrlResponse) -> bool {
        let status_code = result.get_status_code();
        
        // Drop connection on server errors (5xx) except 500
        if status_code >= 500 && status_code != 500 {
            return true;
        }
        
        // Drop connection on specific client errors
        if status_code == 401 || status_code == 403 {
            return true;
        }
        
        false
    }
}

// Note: override_drop_connection_scenario method needs to be implemented
// in the FlUrl struct if not already present
```

The default drop connection scenario drops connections on:
- Status codes > 400 (except 404)
- Status code 499

**Note**: The connection is automatically dropped and reestablished if:
- There is a Hyper error
- The response matches the drop connection scenario criteria
- The connection hasn't been used for more than the configured timeout (default: 30 seconds)

## HTTP Modes

### HTTP/2

```rust
use flurl::{FlUrl, FlUrlMode};

let response = FlUrl::new("https://api.example.com/data")
    .update_mode(FlUrlMode::H2)
    .get()
    .await?;
```

### HTTP/1.1 with Hyper

```rust
use flurl::{FlUrl, FlUrlMode};

let response = FlUrl::new("https://api.example.com/data")
    .update_mode(FlUrlMode::Http1Hyper)
    .get()
    .await?;
```

### HTTP/1.1 without Hyper

```rust
use flurl::{FlUrl, FlUrlMode};

let response = FlUrl::new("https://api.example.com/data")
    .update_mode(FlUrlMode::Http1NoHyper)
    .get()
    .await?;
```

## SSL/TLS Configuration

### Accept Invalid Certificates

```rust
let response = FlUrl::new("https://self-signed.example.com")
    .accept_invalid_certificate()
    .get()
    .await?;
```

### Client Certificate

```rust
use my_tls::ClientCertificate;

let cert = ClientCertificate::from_pem_files(
    "client.crt",
    "client.key"
)?;

let response = FlUrl::new("https://api.example.com/data")
    .with_client_certificate(cert)
    .get()
    .await?;
```

## SSH Tunneling (with-ssh feature)

### Basic SSH Tunnel

```rust
// Format: ssh://user@host:port->http://target-host:port
let response = FlUrl::new("ssh://user@ssh.example.com:22->http://localhost:8080/api/data")
    .get()
    .await?;
```

### SSH with Password

```rust
let response = FlUrl::new("ssh://user@ssh.example.com:22->http://localhost:8080/api/data")
    .set_ssh_password("password123")
    .get()
    .await?;
```

### SSH with Private Key

```rust
let private_key = std::fs::read_to_string("id_rsa")?;
let response = FlUrl::new("ssh://user@ssh.example.com:22->http://localhost:8080/api/data")
    .set_ssh_private_key(private_key, None) // None = no passphrase
    .get()
    .await?;
```

### SSH with Passphrase-Protected Key

```rust
let private_key = std::fs::read_to_string("id_rsa")?;
let response = FlUrl::new("ssh://user@ssh.example.com:22->http://localhost:8080/api/data")
    .set_ssh_private_key(private_key, Some("passphrase".to_string()))
    .get()
    .await?;
```

### SSH Credentials Resolver

```rust
use std::sync::Arc;
use my_ssh::ssh_settings::SshSecurityCredentialsResolver;

struct MySshResolver;

#[async_trait::async_trait]
impl SshSecurityCredentialsResolver for MySshResolver {
    async fn update_credentials(
        &self,
        credentials: &my_ssh::SshCredentials,
    ) -> my_ssh::SshCredentials {
        // Custom logic to update credentials
        credentials.clone()
    }
}

let resolver = Arc::new(MySshResolver);
let response = FlUrl::new("ssh://user@ssh.example.com:22->http://localhost:8080/api/data")
    .set_ssh_security_credentials_resolver(resolver)
    .get()
    .await?;
```

## Unix Socket Support (Unix systems only)

```rust
let response = FlUrl::new("unix:///var/run/docker.sock")
    .append_path_segment("containers")
    .append_path_segment("json")
    .get()
    .await?;
```

## Advanced Configuration

### Timeouts

```rust
use std::time::Duration;

let response = FlUrl::new("https://api.example.com/data")
    .set_timeout(Duration::from_secs(30))
    .get()
    .await?;
```

### Connection Timeout

```rust
use std::time::Duration;

let response = FlUrl::new("https://api.example.com/data")
    .set_not_used_connection_timeout(Duration::from_secs(60))
    .get()
    .await?;
```

### Retry Logic

```rust
let response = FlUrl::new("https://api.example.com/data")
    .with_retries(3) // Retry up to 3 times on failure
    .get()
    .await?;
```

### Request Compression

```rust
let body = FlUrlBody::as_json(&large_data);
let response = FlUrl::new("https://api.example.com/data")
    .compress() // Automatically gzip compress body if > 64 bytes
    .post(body)
    .await?;
```

### Debug Request Output

```rust
let response = FlUrl::new("https://api.example.com/data")
    .print_input_request() // Prints HTTP headers to stdout
    .get()
    .await?;
```

### Request Debug String

```rust
let mut debug_string = String::new();
let body = FlUrlBody::as_json(&my_data);
let response = FlUrl::new("https://api.example.com/data")
    .post_with_debug(body, &mut debug_string)
    .await?;
println!("Request details: {}", debug_string);
```

## Error Handling

```rust
use flurl::{FlUrl, FlUrlError};

match FlUrl::new("https://api.example.com/data").get().await {
    Ok(response) => {
        // Handle success
    }
    Err(FlUrlError::Timeout) => {
        // Handle timeout
    }
    Err(FlUrlError::HyperError(e)) => {
        // Handle Hyper error
        if e.is_canceled() {
            // Request was canceled
        }
    }
    Err(FlUrlError::SerializationError(e)) => {
        // Handle JSON serialization error
    }
    Err(e) => {
        // Handle other errors
        eprintln!("Error: {}", e.to_string());
    }
}
```

## Examples

### Complete Example: API Client

```rust
use flurl::{FlUrl, body::FlUrlBody};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct CreateUser {
    name: String,
    email: String,
}

#[derive(Deserialize)]
struct User {
    id: u64,
    name: String,
    email: String,
}

async fn create_user(name: &str, email: &str) -> Result<User, Box<dyn std::error::Error>> {
    let user_data = CreateUser {
        name: name.to_string(),
        email: email.to_string(),
    };
    
    let mut response = FlUrl::new("https://api.example.com")
        .append_path_segment("users")
        .with_header("Authorization", "Bearer token123")
        .post(FlUrlBody::as_json(&user_data))
        .await?;
    
    let user: User = response.get_json().await?;
    Ok(user)
}

async fn get_user(id: u64) -> Result<User, Box<dyn std::error::Error>> {
    let mut response = FlUrl::new("https://api.example.com")
        .append_path_segment("users")
        .append_path_segment(id.to_string())
        .with_header("Authorization", "Bearer token123")
        .get()
        .await?;
    
    let user: User = response.get_json().await?;
    Ok(user)
}
```

## Additional Notes

### Connection Reuse Details

- Connections are cached and reused based on `schema + domain + port`
- Default connection reuse timeout: 120 seconds
- Default unused connection timeout: 30 seconds
- Connections are automatically cleaned up when not used
- Each connection cache is thread-safe and shared across all `FlUrl` instances (unless a custom cache is provided)

### Body Compression

- Compression is only applied if the body size is >= 64 bytes
- Uses gzip compression
- Automatically sets `Content-Encoding: gzip` header
- Compression threshold can be adjusted by modifying the source code

### HTTP Version Support

- **HTTP/2 (H2)**: Full support with multiplexing
- **HTTP/1.1 with Hyper**: Uses Hyper's HTTP/1.1 implementation
- **HTTP/1.1 without Hyper**: Uses custom HTTP/1.1 implementation (may be faster in some scenarios)

### Thread Safety

- `FlUrl` instances are not thread-safe (use `Send` but not `Sync`)
- Connection cache (`FlUrlHttpConnectionsCache`) is thread-safe
- Multiple async tasks can safely use different `FlUrl` instances concurrently

## License

See LICENSE file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.