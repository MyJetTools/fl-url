# Unix Socket Support (Unix systems only)

```rust
let response = FlUrl::new("http+unix:///var/run/docker.sock")
    .append_path_segment("containers")
    .append_path_segment("json")
    .get()
    .await?;
```

The socket file can be named in any of these ways; they all reach the same file:

| form | examples |
| --- | --- |
| the path itself | `/var/run/docker.sock`, `~/docker.sock` (`~` is resolved against `$HOME`) |
| a scheme — `http+unix`, `unix` or `unix+http`, in any case — and the path | `http+unix:///var/run/docker.sock`, `unix:///var/run/docker.sock`, `unix+http://var/run/docker.sock` |

After a scheme the path is absolute however many slashes are written: `unix:/var/run/x.sock`,
`unix://var/run/x.sock` and `unix:///var/run/x.sock` are all `/var/run/x.sock`.

The http path goes in with `append_path_segment`, as above. A url that names no socket
file — `http+unix://`, `unix://`, `/` — fails the request with
`FlUrlError::InvalidUrl`.
