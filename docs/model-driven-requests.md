# Model-Driven Requests

Instead of wiring up the path, query, headers, and body by hand, you can describe a
request with a `my_http_utils` model (any type deriving
`my_http_utils::macros::MyHttpInput`) and hand it to `execute_request`. The model
fills the URL path/query, headers, and body; the `HttpVerb` selects the method. The
base host and any static route prefix are still configured on the builder beforehand.

`my_http_utils` is re-exported as `flurl::my_http_utils`, so it does not have to be in
`Cargo.toml` — and taking it from there is what makes the model implement the very
trait `execute_request` accepts. The derive expands to paths that start with
`my_http_utils::`, so that name has to be in scope: `use flurl::my_http_utils;`.

```rust
use flurl::my_http_utils;
use flurl::{FlUrl, HttpVerb};
use my_http_utils::macros::MyHttpInput;

#[derive(MyHttpInput)]
struct CreateUser {
    #[http_path(name = "orgId", description = "")]
    org_id: String,
    #[http_query(name = "notify", description = "")]
    notify: bool,
    #[http_header(name = "X-Api-Key", description = "")]
    api_key: String,
    #[http_body(name = "name", description = "")]
    name: String,
}

let model = CreateUser {
    org_id: "org-42".to_string(),
    notify: true,
    api_key: "secret".to_string(),
    name: "John".to_string(),
};

// Base host + static route prefix set by the caller, the model fills the rest.
let response = FlUrl::new("https://api.example.com")
    .append_path_segment("api")
    .append_path_segment("users")
    .execute_request(HttpVerb::Post, model)
    .await?;
// POST https://api.example.com/api/users/org-42?notify=true
//   X-Api-Key: secret
//   { "name": "John" }
```

`Get`/`Delete`/`Head` do not carry a body, so a body produced by the model is
ignored for those verbs.

## Requests Without an Input Model

When a request carries no input model, pass `EmptyRequestModel` instead of deriving
a dedicated one. The URL and headers already set on the builder are used as-is, and
body-carrying verbs (`Post`/`Put`/`Patch`) send an empty body:

```rust
use flurl::{FlUrl, EmptyRequestModel, HttpVerb};

let response = FlUrl::new("https://api.example.com")
    .append_path_segment("health")
    .execute_request(HttpVerb::Get, EmptyRequestModel)
    .await?;
```

`EmptyRequestModel` is a shared, transport-agnostic stub that implements
`THttpRequestBuilder` as a no-op — it appends nothing to the URL, adds no headers,
and produces an empty body. It is the parameter-less stand-in for `execute_request`
on both the native and wasm backends, so you don't have to spell out a model type
(the way a bare `None` would have forced you to) just to satisfy the signature.
