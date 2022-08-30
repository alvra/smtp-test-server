//! This crate provides a simple SMTP server that can be used in tests.
//! In particular, the provided server was developed to be connected to
//! by a `lettre::AsyncSmtpTransport` client to check the emails sent.
//!
//! The [`Server`] can be used to receive a single email
//! or to obtain a stream of all email.
//!
//! # Examples
//!
//! ```
//! # tokio_test::block_on(async {
//! use smtp_test_server::{Server, Auth, MessageBuilderExt};
//! use lettre::{
//!     message::Message,
//!     AsyncTransport,
//!     transport::smtp::authentication::Credentials,
//! };
//!
//! let mut server = Server::start(
//!     "127.0.0.1:0".parse().unwrap(),
//!     Auth::Login {
//!         username: "my-name".to_string(),
//!         password: "secret".to_string(),
//!     },
//! ).await.unwrap();
//! let address = server.address().unwrap();
//!
//! let client = lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>
//!     ::builder_dangerous(address.ip().to_string())
//!     .port(address.port())
//!     .credentials(Credentials::new("my-name".to_string(), "secret".to_string()))
//!     .build();
//!
//! let message = Message::builder()
//!     .from("Friend <friend@example.com>".parse().unwrap())
//!     .to("MySelf <self@example.com>".parse().unwrap())
//!     .subject("Hello")
//!     .body_text_and_html(
//!         "Welcome!".to_string(),
//!         "<p>Welcome!</p>".to_string(),
//!     )
//!     .unwrap();
//!
//! tokio::spawn(async move {
//!     client.send(message).await.unwrap();
//!     println!("email sent");
//! });
//!
//! let email = server.try_receive().await.unwrap();
//! println!("email received: {email:?}");
//! # })
//! ```

#![forbid(unsafe_code)]

mod config;
mod email;
mod server;
mod smtp;

#[cfg(feature = "lettre")]
mod build;

pub use config::Config;
pub use email::{ConversionError, Email, ParseError};
pub use server::{Error, Server};
pub use smtp::{Auth, Error as SmtpError};

#[cfg(feature = "lettre")]
pub use build::MessageBuilderExt;
