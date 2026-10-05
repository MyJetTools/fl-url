# Compression

## Request Compression

```rust
let body = HttpRequestBody::as_json(&large_data);
let response = FlUrl::new("https://api.example.com/data")
    .compress() // gzips the body when it is 64 bytes or longer
    .post(body)
    .await?;
```

## Body Compression

- `compress()` is only applied if the body size is >= 64 bytes; a shorter body goes out as it is
- Uses gzip compression
- Sets the `Content-Encoding: gzip` header, unless the request already carries a `Content-Encoding`
- Compression threshold can be adjusted by modifying the source code

## Response Decompression

```rust
let mut response = FlUrl::new("https://api.example.com/data")
    .accept_gzip()
    .get()
    .await?;

let body = response.get_body_as_slice().await?; // already decompressed
```

`accept_gzip()` sends `Accept-Encoding: gzip`, unless the request already carries that
header, and decompresses a `Content-Encoding: gzip` response on the buffered reads —
`get_body_as_slice`, `get_json`, `get_body_as_str`, `receive_body`. A body read with
`get_body_as_stream` is passed on as it came. Under wasm the method does nothing: the
browser negotiates and decompresses by itself.
