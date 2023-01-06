use std::net::IpAddr;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

const READ_WAIT: std::time::Duration = std::time::Duration::from_millis(10);

/// An error during an SMTP exchange.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(
        "received unexpected data; expected {expected:?}, actual {actual:?}"
    )]
    UnexpectedData { expected: String, actual: String },
    #[error("received unexpected continuation: {actual:?}")]
    UnexpectedContinuation { actual: String },
}

/// The autentication details for a SMTP server.
#[derive(Clone, Debug)]
pub enum Auth {
    /// Require clients to login with the provided credentials.
    Login { username: String, password: String },
    /// Accept only anonymous clients.
    AcceptAnonOnly,
    /// Accept any client, even ones that try to login using credentials.
    AcceptAll,
}

#[derive(Debug)]
pub(crate) enum Response<T> {
    Email(T),
    Continue,
    Quit,
}

#[derive(Debug)]
pub(crate) struct Data {
    pub email: Vec<u8>,
    pub address_from: String,
    pub address_to: String,
}

/// Read up to a "/r/n".
async fn read(
    mut socket: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
) -> Result<String, Error> {
    let mut buffer = Vec::with_capacity(128 * 1024);
    loop {
        let len = socket.read_buf(&mut buffer).await?;
        if len == 0 {
            let _ = tokio::time::sleep(READ_WAIT).await;
            continue;
        }
        if !buffer.is_empty() {
            break;
        }
    }
    let data = String::from_utf8_lossy(&buffer).to_string();
    #[cfg(feature = "tracing")]
    {
        use tracing::{event, Level};
        event!(Level::TRACE, recv = data);
    }
    Ok(data)
}

async fn write(
    mut socket: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    data: &str,
) -> Result<(), Error> {
    #[cfg(feature = "tracing")]
    {
        use tracing::{event, Level};
        event!(Level::TRACE, send = data);
    }

    socket.write_all(data.as_bytes()).await?;
    Ok(())
}

async fn read_expect(
    socket: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    expected: impl ToString,
) -> Result<(), Error> {
    let expected = expected.to_string();
    let data = read(socket).await?;
    if data == expected {
        Ok(())
    } else {
        Err(Error::UnexpectedData {
            expected,
            actual: data,
        })
    }
}

fn expect_address(
    data: String,
    command: &'static str,
) -> Result<String, Error> {
    if data.starts_with(&format!("{command}:<")) && data.ends_with(">\r\n") {
        let part = &data[(command.len() + 2)..(data.len() - 3)];
        Ok(part.to_string())
    } else {
        Err(Error::UnexpectedData {
            expected: format!("{command}:<...>\r\n"),
            actual: data,
        })
    }
}

fn encode_password(username: &str, password: &str) -> String {
    use base64ct::Encoding;
    let mut data = Vec::with_capacity(2 + username.len() + password.len());
    data.push(0);
    data.extend(username.bytes());
    data.push(0);
    data.extend(password.bytes());
    base64ct::Base64::encode_string(&data)
}

async fn respond_auth_ok(
    mut socket: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
) -> Result<(), Error> {
    write(&mut socket, "235 Authentication successful\r\n").await?;
    Ok(())
}

async fn respond_auth_fail(
    mut socket: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
) -> Result<(), Error> {
    write(&mut socket, "535 Authentication failed\r\n").await?;

    read_expect(&mut socket, "QUIT\r\n").await?;
    write(&mut socket, "221 Ok\r\n").await?;
    Ok(())
}

pub(crate) async fn receive(
    mut socket: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
    server_ip: &IpAddr,
    client_ip: &IpAddr,
    auth: &Auth,
) -> Result<Response<Data>, Error> {
    write(&mut socket, &format!("220 {server_ip}\r\n")).await?;
    read_expect(&mut socket, format!("EHLO [{client_ip}]\r\n")).await?;

    write(&mut socket, &format!("250-{server_ip}\r\n")).await?;
    write(&mut socket, "250 AUTH PLAIN\r\n").await?;

    let mut data = read(&mut socket).await?;
    if data.starts_with("AUTH") {
        match auth {
            Auth::Login { username, password } => {
                let auth = encode_password(username, password);
                if data == format!("AUTH PLAIN {auth}\r\n") {
                    respond_auth_ok(&mut socket).await?;
                } else {
                    respond_auth_fail(&mut socket).await?;
                    return Ok(Response::Quit);
                }
            }
            Auth::AcceptAnonOnly => {
                respond_auth_fail(&mut socket).await?;
                return Ok(Response::Quit);
            }
            Auth::AcceptAll => {
                respond_auth_ok(&mut socket).await?;
            }
        }

        data = read(&mut socket).await?;
    } else {
        match auth {
            Auth::Login { .. } => {
                respond_auth_fail(socket).await?;
                return Ok(Response::Quit);
            }
            Auth::AcceptAnonOnly | Auth::AcceptAll => {}
        }
    }

    if data == "NOOP\r\n" {
        write(&mut socket, "250 Ok\r\n").await?;

        read_expect(&mut socket, "QUIT\r\n").await?;
        write(&mut socket, "221 Ok\r\n").await?;

        Ok(Response::Continue)
    } else if data.starts_with("MAIL") {
        let address_from = expect_address(data, "MAIL FROM")?;
        write(&mut socket, "250 Ok\r\n").await?;

        let data = read(&mut socket).await?;
        let address_to = expect_address(data, "RCPT TO")?;
        write(&mut socket, "250 Ok\r\n").await?;

        read_expect(&mut socket, "DATA\r\n").await?;
        write(&mut socket, "354 Go\r\n").await?;

        let mut email = Vec::with_capacity(128 * 1024);
        socket.read_buf(&mut email).await?;

        read_expect(&mut socket, "\r\n.\r\n").await?;
        write(&mut socket, "250 Ok\r\n").await?;

        read_expect(&mut socket, "QUIT\r\n").await?;
        write(&mut socket, "221 Ok\r\n").await?;

        Ok(Response::Email(Data {
            email,
            address_from,
            address_to,
        }))
    } else {
        Err(Error::UnexpectedContinuation { actual: data })
    }
}
