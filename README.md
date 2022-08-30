# SMTP Test Server

[![unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg)](https://github.com/rust-secure-code/safety-dance/)

This crate provides a simple SMTP server that can be used in tests.
In particular, the provided server was developed to be connected
to by a lettre::AsyncSmtpTransport client to check the emails sent.

The SMTP server can be used to receive a single email
or to obtain a stream of all email.


## Features

  * No unsafe code (`#[forbid(unsafe_code)]`)
  * Tested


## Example

```rust
use smtp_test_server::{Server, Auth, MessageBuilderExt};
use lettre::{
    message::Message,
    AsyncTransport,
    transport::smtp::authentication::Credentials,
};

let mut server = Server::start(
    "127.0.0.1:0".parse().unwrap(),
    Auth::Login {
        username: "my-name".to_string(),
        password: "secret".to_string(),
    },
).await.unwrap();
let address = server.address().unwrap();

let client = lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>
    ::builder_dangerous(address.ip().to_string())
    .port(address.port())
    .credentials(Credentials::new("my-name".to_string(), "secret".to_string()))
    .build();

let message = Message::builder()
    .from("Friend <friend@example.com>".parse().unwrap())
    .to("MySelf <self@example.com>".parse().unwrap())
    .subject("Hello")
    .body_text_and_html(
        "Welcome!".to_string(),
        "<p>Welcome!</p>".to_string(),
    )
    .unwrap();

tokio::spawn(async move {
    client.send(message).await.unwrap();
    println!("email sent");
});

let email = server.try_receive().await.unwrap();
println!("email received: {email:?}");
```


## Documentation

[Documentation](https://lib.rs/crates/smtp-test-server)


## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.


## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
