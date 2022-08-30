use std::str::FromStr;

/// The configuration for a SMTP server.
///
/// This type can be parsed from a string.
#[derive(PartialEq, Eq, Debug)]
pub struct Config<T> {
    pub address: T,
    pub port: Option<u16>,
    pub username_password: Option<(String, String)>,
}

impl<T: FromStr> FromStr for Config<T> {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (username_password, host) =
            if let Some((user, host)) = s.split_once('@') {
                if let Some((username, password)) = user.split_once(':') {
                    let username_password =
                        (username.to_string(), password.to_string());
                    (Some(username_password), host)
                } else {
                    return Err("missing ':' in user");
                }
            } else {
                (None, s)
            };
        if let Some((address, port)) = host.split_once(':') {
            Ok(Config {
                address: T::from_str(address).map_err(|_| "invalid address")?,
                port: Some(port.parse().map_err(|_| "invalid port number")?),
                username_password,
            })
        } else {
            Ok(Config {
                address: T::from_str(host).map_err(|_| "invalid address")?,
                port: None,
                username_password,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[tokio::test]
    async fn parse_host() {
        assert_eq!(
            "127.0.0.1".parse(),
            Ok(Config {
                address: "127.0.0.1".to_string(),
                port: None,
                username_password: None,
            })
        )
    }

    #[tokio::test]
    async fn parse_host_port() {
        assert_eq!(
            "127.0.0.1:587".parse(),
            Ok(Config {
                address: "127.0.0.1".to_string(),
                port: Some(587),
                username_password: None,
            })
        )
    }

    #[tokio::test]
    async fn parse_user_pass_host() {
        assert_eq!(
            "user:pwd@127.0.0.1".parse(),
            Ok(Config {
                address: "127.0.0.1".to_string(),
                port: None,
                username_password: Some((
                    "user".to_string(),
                    "pwd".to_string()
                )),
            })
        )
    }

    #[tokio::test]
    async fn parse_user_pass_host_port() {
        assert_eq!(
            "user:pwd@127.0.0.1:587".parse(),
            Ok(Config {
                address: "127.0.0.1".to_string(),
                port: Some(587),
                username_password: Some((
                    "user".to_string(),
                    "pwd".to_string()
                )),
            })
        )
    }
}
