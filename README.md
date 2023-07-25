# FLUrl

The concept is taken from the similar .Net Library (https://flurl.dev/)
Just the concept ))

It's a Hyper-based client


Basic Example:

```rust

    let response = "http://mywebsite.com"
        .append_path_segment("Row")
        .append_query_param("tableName", Some(table_name))
        .append_query_param("partitionKey", Some(partition_key))
        .get()
        .await;
```


## Reuse of connection

Since HTTP1.1 and higher reuses a connection not to pay expenses  for the new connection esteblishment 
and as well not to pay expenses for a TLS hanshake each request - connections are reused on the base of schema+domain

Connection is going to be dropped and reestableshed if 
* there is a hyper error
* It fits the drop connection strategy trait implementation.

Default drop connection strategy is here:


