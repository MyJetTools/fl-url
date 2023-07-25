# FLUrl

The concept is taken from the similar .Net Library (https://flurl.dev/)
Just the concept ))

It's a Hyper-based client.


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

Since HTTP1.1 and higher reuses a connection not to pay expenses  for the new connection establishment 
and as well not to pay the costs for a TLS Handshake each request - connections are reused based on schema+domain

The connection is going to be dropped and reestablished if 
* There is a hyper error
* It fits the drop connection scenario trait implementation.

The default drop connection scenario is here: https://github.com/MyJetTools/fl-url/blob/main/src/fl_drop_connection_scenario.rs

To implement a custom drop connection strategy on positive fl-url response trait DropConnectionScenario should be implemented and


```rust

pub struct MyCustomDropConnectionScenario;

impl DropConnectionScenario for MyCustomDropConnectionScenario {
    fn should_we_drop_it(&self, result: &FlUrlResponse) -> bool {
        let status_code = result.get_status_code();

        if status_code > 400 || status_code == 499 {
            return status_code != 404;
        }

        false
    }
}


    let response = "http://mywebsite.com"
        .override_drop_connection_scenario(MyCustomDropConnectionScenario)
        .append_path_segment("Row")
        .append_query_param("tableName", Some(table_name))
        .append_query_param("partitionKey", Some(partition_key))
        .get()
        .await;
```



