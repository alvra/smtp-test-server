use std::net::{IpAddr, SocketAddr};

use tokio::sync::mpsc;
use tokio_stream::Stream;

use crate::{smtp::Response, Auth, Email};

pub const DEFAULT_PORT: u16 = 587;

/// An error while receiving email.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Smtp(#[from] crate::smtp::Error),
    #[error(transparent)]
    Parse(#[from] crate::email::ParseError),
    #[error(transparent)]
    Accept(#[from] std::io::Error),
}

/// An SMTP email server.
pub struct Server {
    auth: Auth,
    listener: tokio::net::TcpListener,
    channel_tx: mpsc::Sender<Result<Email, Error>>,
    channel_rx: mpsc::Receiver<Result<Email, Error>>,
}

impl Server {
    /// Start a new server instance.
    pub async fn start(
        address: SocketAddr,
        auth: Auth,
    ) -> Result<Self, std::io::Error> {
        use tokio::net::TcpListener;
        let listener = TcpListener::bind(address).await?;
        let (channel_tx, channel_rx) = mpsc::channel(1);
        Ok(Self {
            auth,
            listener,
            channel_tx,
            channel_rx,
        })
    }

    /// Start a new server instance
    /// with the given configuration.
    ///
    /// The `strict` argument specifies
    /// what to do if no login credentials
    /// were provided in the config.
    /// If `true`, only anonymous clients
    /// are allowed. If `false`
    /// all clients are allowed,
    /// even if they provide login credentials.
    pub async fn start_with_config(
        config: crate::Config<IpAddr>,
        strict: bool,
    ) -> Result<Self, std::io::Error> {
        let address = SocketAddr::new(
            config.address,
            config.port.unwrap_or(DEFAULT_PORT),
        );
        let auth = config
            .username_password
            .map(|(username, password)| Auth::Login { username, password })
            .unwrap_or(if strict {
                Auth::AcceptAnonOnly
            } else {
                Auth::AcceptAll
            });
        Self::start(address, auth).await
    }

    /// Return the address and port to which this server bound.
    pub fn address(&self) -> Result<SocketAddr, std::io::Error> {
        self.listener.local_addr()
    }

    /// Create a stream of emails.
    ///
    /// This stream discards any errors that occur.
    pub fn stream(&mut self) -> impl Stream<Item = Email> + '_ {
        async_stream::stream! {
            loop {
                yield self.receive().await;
            }
        }
    }

    /// Create a stream of emails.
    ///
    /// This stream includes any errors that occur.
    pub fn try_stream(
        &mut self,
    ) -> impl Stream<Item = Result<Email, Error>> + '_ {
        async_stream::stream! {
            loop {
                yield self.try_receive().await;
            }
        }
    }

    /// Receive a single email.
    ///
    /// This method discards any errors that occur.
    pub async fn receive(&mut self) -> Email {
        loop {
            match self.try_receive().await {
                Ok(email) => return email,
                Err(error) => {
                    #[cfg(feature = "tracing")]
                    {
                        use tracing::{event, Level};
                        event!(Level::TRACE, ?error);
                    }
                }
            }
        }
    }

    /// Try to receive a single email.
    pub async fn try_receive(&mut self) -> Result<Email, Error> {
        loop {
            tokio::select! {
                result = self.listener.accept() => match result {
                    Ok((socket, client_address)) => {
                        tokio::spawn(task(
                            socket, self.address()?.ip(), client_address.ip(), self.auth.clone(), self.channel_tx.clone())
                        );
                    }
                    Err(e) => return Err(Error::Accept(e))
                },
                email_result = self.channel_rx.recv() => {
                    // NOTE: since the server keeps a sender itself,
                    //       the channel never closes and
                    //       we cannot receive `None`
                    return email_result.expect("senders closed")
                }
            }
        }
    }
}

async fn task(
    mut socket: tokio::net::TcpStream,
    client_ip: IpAddr,
    server_ip: IpAddr,
    auth: Auth,
    channel: mpsc::Sender<Result<Email, Error>>,
) {
    loop {
        let result = run(&mut socket, &server_ip, &client_ip, &auth).await;
        let result = match result {
            Ok(Response::Email(email)) => channel.send(Ok(email)).await,
            Ok(Response::Continue) => Ok(()),
            Ok(Response::Quit) => return,
            Err(e) => channel.send(Err(e)).await,
        };
        if let Err(_) = result {
            // error sending on channel because it has closed
            // NOTE: just close the socket without sending a smtp `quit`
            return;
        }
    }
}

async fn run(
    socket: &mut tokio::net::TcpStream,
    client_ip: &IpAddr,
    server_ip: &IpAddr,
    auth: &Auth,
) -> Result<Response<Email>, Error> {
    let response =
        crate::smtp::receive(socket, server_ip, client_ip, auth).await?;
    match response {
        Response::Email(data) => {
            let email = Email::parse(data)?;
            Ok(Response::Email(email))
        }
        Response::Continue => Ok(Response::Continue),
        Response::Quit => Ok(Response::Quit),
    }
}

#[cfg(all(test, feature = "lettre"))]
mod tests {
    use std::{net::SocketAddr, time::Duration};

    use lettre::transport::smtp::{
        authentication::Credentials, AsyncSmtpTransportBuilder,
    };

    use super::{Auth, Server};
    use crate::MessageBuilderExt;

    type SmtpClient = lettre::AsyncSmtpTransport<lettre::Tokio1Executor>;

    const TIMEOUT: Duration = Duration::from_millis(1000);

    async fn start_server(auth: Auth) -> Server {
        Server::start("127.0.0.1:0".parse().unwrap(), auth)
            .await
            .unwrap()
    }

    fn build_client(address: SocketAddr) -> AsyncSmtpTransportBuilder {
        lettre::AsyncSmtpTransport::<lettre::Tokio1Executor>::builder_dangerous(
            address.ip().to_string(),
        )
        .port(address.port())
    }

    async fn timeout<F>(op: &str, future: F) -> F::Output
    where
        F: std::future::Future,
    {
        tokio::time::timeout(TIMEOUT, future)
            .await
            .expect(&format!("timeout {op}"))
    }

    async fn expect_timeout<F>(op: &str, future: F)
    where
        F: std::future::Future,
        F::Output: std::fmt::Debug,
    {
        match tokio::time::timeout(TIMEOUT, future).await {
            Ok(output) => {
                panic!("expected timeout {op}, unexpected output: {output:?}");
            }
            Err(_) => (),
        }
    }

    async fn check_client_connection(client: &mut SmtpClient) {
        timeout("checking connection", client.test_connection())
            .await
            .expect("error testing client");
    }

    async fn send(
        client: &mut SmtpClient,
        addr_from: (&str, &str),
        addr_to: (&str, &str),
        subject: &str,
        body_text: &str,
        body_html: &str,
    ) {
        use lettre::{message::Mailbox, AsyncTransport, Message};
        let message = Message::builder()
            .from(Mailbox::new(
                Some(addr_from.0.to_string()),
                addr_from.1.parse().unwrap(),
            ))
            .to(Mailbox::new(
                Some(addr_to.0.to_string()),
                addr_to.1.parse().unwrap(),
            ))
            .subject(subject)
            .body_text_and_html(body_text.to_string(), body_html.to_string())
            .expect("invalid email message");
        let response = timeout("sending email", client.send(message))
            .await
            .expect("error sending email message");
        if !response.is_positive() {
            panic!("received negative response")
        }
    }

    async fn run_test_ok(mut server: Server, mut client: SmtpClient) {
        tokio::join!(
            async move {
                check_client_connection(&mut client).await;
                send(
                    &mut client,
                    ("Sender", "sender@example.com"),
                    ("Recipient", "recipient@example.com"),
                    "Hello world",
                    "Welcome",
                    "<p>Welcome</p>",
                )
                .await;
            },
            async move {
                let email = timeout("receiving email", server.try_receive())
                    .await
                    .expect("error receiving email");
                assert_eq!(&email.address_from, "sender@example.com");
                assert_eq!(&email.address_to, "recipient@example.com");
                assert_eq!(
                    email.get_from(),
                    format!("Sender <sender@example.com>")
                );
                assert_eq!(
                    email.get_to(),
                    format!("Recipient <recipient@example.com>")
                );
                assert_eq!(&email.subject, "Hello world");
                assert_eq!(&email.body_text, "Welcome\r\n");
                assert_eq!(&email.body_html, "<p>Welcome</p>\r\n");
            },
        );
    }

    async fn run_test_auth_fail(
        mut server: Server,
        client: SmtpClient,
    ) -> Result<bool, lettre::transport::smtp::Error> {
        tokio::join!(
            async move {
                timeout("checking connection", client.test_connection()).await
            },
            async move {
                expect_timeout("receiving email", server.try_receive()).await
            },
        )
        .0
    }

    #[tokio::test]
    async fn test_send_anon_only() {
        let server = start_server(Auth::AcceptAnonOnly).await;
        let address = server.address().unwrap();
        let client: SmtpClient = build_client(address).build();
        run_test_ok(server, client).await
    }

    #[tokio::test]
    async fn test_send_anon_accepted() {
        let server = start_server(Auth::AcceptAll).await;
        let address = server.address().unwrap();
        let client: SmtpClient = build_client(address).build();
        run_test_ok(server, client).await
    }

    #[tokio::test]
    async fn test_send_login_accepted() {
        let server = start_server(Auth::AcceptAll).await;
        let address = server.address().unwrap();
        let client: SmtpClient = build_client(address)
            .credentials(Credentials::new(
                "user".to_string(),
                "pwd".to_string(),
            ))
            .build();
        run_test_ok(server, client).await
    }

    #[tokio::test]
    async fn test_send_login_ok() {
        let server = start_server(Auth::Login {
            username: "user".to_string(),
            password: "pwd".to_string(),
        })
        .await;
        let address = server.address().unwrap();
        let client: SmtpClient = build_client(address)
            .credentials(Credentials::new(
                "user".to_string(),
                "pwd".to_string(),
            ))
            .build();
        run_test_ok(server, client).await
    }

    #[tokio::test]
    async fn test_login_fail() {
        let server = start_server(Auth::Login {
            username: "user".to_string(),
            password: "pwd".to_string(),
        })
        .await;
        let address = server.address().unwrap();
        let client: SmtpClient = build_client(address)
            .credentials(Credentials::new(
                "user".to_string(),
                "xxx".to_string(),
            ))
            .build();
        match run_test_auth_fail(server, client).await {
            Ok(is_ok) => panic!("expected auth fail, received: {is_ok:?}"),
            Err(error) => {
                use std::error::Error;
                let source = format!(
                    "{}",
                    error.source().expect("missing error source")
                );
                assert_eq!(source, "Authentication failed");
            }
        }
    }

    #[tokio::test]
    async fn test_anon_only_auth_fail() {
        let server = start_server(Auth::AcceptAnonOnly).await;
        let address = server.address().unwrap();
        let client: SmtpClient = build_client(address)
            .credentials(Credentials::new(
                "user".to_string(),
                "pwd".to_string(),
            ))
            .build();
        match run_test_auth_fail(server, client).await {
            Ok(is_ok) => panic!("expected auth fail, received: {is_ok:?}"),
            Err(error) => {
                use std::error::Error;
                let source = format!(
                    "{}",
                    error.source().expect("missing error source")
                );
                assert_eq!(source, "Authentication failed");
            }
        }
    }

    #[tokio::test]
    async fn test_auth_anon_fail() {
        let server = start_server(Auth::Login {
            username: "user".to_string(),
            password: "pwd".to_string(),
        })
        .await;
        let address = server.address().unwrap();
        let client: SmtpClient = build_client(address).build();
        match run_test_auth_fail(server, client).await {
            Ok(is_ok) => assert_eq!(is_ok, false),
            Err(error) => panic!("unexpected error: {error:?}"),
        }
    }
}
