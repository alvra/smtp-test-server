use std::collections::HashMap;

/// An parsed email as received by the server.
#[derive(Debug)]
#[non_exhaustive]
pub struct Email {
    /// The email address of the sender.
    ///
    /// This is the address as received in the SMTP exchange
    /// and does not include a name.
    pub address_from: String,

    /// The email address of the recipient.
    ///
    /// This is the address as received in the SMTP exchange
    /// and does not include a name.
    pub address_to: String,

    /// The subject of this email,
    /// taken from the headers.
    pub subject: String,

    /// The map of headers.
    pub headers: HashMap<String, String>,

    /// The text part of this email.
    pub body_text: String,

    /// The html part of this email.
    pub body_html: String,
}

impl Email {
    pub(crate) fn parse(data: crate::smtp::Data) -> Result<Self, ParseError> {
        let mail = mailparse::parse_mail(&data.email)?;
        Ok(convert_email(data.address_from, data.address_to, mail)?)
    }

    /// Get the complete `From` header
    /// which includes the name and email address.
    pub fn get_from(&self) -> &str {
        self.headers.get("From").unwrap()
    }

    /// Get the complete `To` header
    /// which includes the name and email address.
    pub fn get_to(&self) -> &str {
        self.headers.get("To").unwrap()
    }
}

/// An error during email parsing.
#[derive(thiserror::Error, Debug)]
pub enum ParseError {
    #[error(transparent)]
    Parse(#[from] mailparse::MailParseError),
    #[error(transparent)]
    Convert(#[from] ConversionError),
}

/// An error during conversion from
/// [`mailparse::ParsedMail`]
/// into [`Email`].
#[derive(thiserror::Error, Debug)]
pub enum ConversionError {
    #[error("missing `From` address")]
    MisingFromAddress,
    #[error("multiple `From` addresses")]
    MultipleFromAddresses(Vec<String>),
    #[error("mismatch `From` address; smtp: {smtp}, email: {email}")]
    FromAddressMismatch { smtp: String, email: String },
    #[error("missing `To` address")]
    MisingToAddress,
    #[error("multiple `To` addresses")]
    MultipleToAddresses(Vec<String>),
    #[error("mismatch `To` address; smtp: {smtp}, email: {email}")]
    ToAddressMismatch { smtp: String, email: String },
    #[error("missing `Subject` header")]
    MisingSubject,
    #[error("multiple `Subject` headers")]
    MultipleSubjects(Vec<String>),
    #[error("unexpected part count; expected 2, received {0}")]
    UnexpectedPartCount(usize),
    #[error(
        "unexpected part mimetype; expected {expected:?}, received {actual:?}"
    )]
    UnexpectedPartMime {
        actual: String,
        expected: &'static str,
    },
}

/// Convert a [`mailparse::ParsedMail`] into an [`Email`].
fn convert_email(
    address_from: String,
    address_to: String,
    mail: mailparse::ParsedMail,
) -> Result<Email, ConversionError> {
    use mailparse::MailHeaderMap;
    let mut from_addrs = mail.headers.get_all_values("From");
    let from_addr = if from_addrs.len() > 1 {
        return Err(ConversionError::MultipleFromAddresses(from_addrs));
    } else {
        from_addrs.pop().ok_or(ConversionError::MisingFromAddress)?
    };
    if !from_addr.contains(&format!("<{address_from}>")) {
        return Err(ConversionError::FromAddressMismatch {
            smtp: address_from,
            email: from_addr,
        });
    }
    let mut to_addrs = mail.headers.get_all_values("To");
    let to_addr = if to_addrs.len() > 1 {
        return Err(ConversionError::MultipleToAddresses(to_addrs));
    } else {
        to_addrs.pop().ok_or(ConversionError::MisingToAddress)?
    };
    if !to_addr.contains(&format!("<{address_to}>")) {
        return Err(ConversionError::ToAddressMismatch {
            smtp: address_to,
            email: to_addr,
        });
    }
    let mut subjects = mail.headers.get_all_values("Subject");
    let subject = if subjects.len() > 1 {
        return Err(ConversionError::MultipleSubjects(subjects));
    } else {
        let subject = subjects.pop().ok_or(ConversionError::MisingSubject)?;
        subject
            .strip_suffix("\r\n")
            .map(|s| s.to_string())
            .unwrap_or(subject)
    };
    if mail.subparts.len() != 2 {
        return Err(ConversionError::UnexpectedPartCount(mail.subparts.len()));
    }
    let part1 = mail.subparts[0].get_body().unwrap();
    let part2 = mail.subparts[1].get_body().unwrap();
    let part1_mime = mail.subparts[0].ctype.mimetype.to_string();
    let part2_mime = mail.subparts[1].ctype.mimetype.to_string();
    if part1_mime != "text/plain" {
        return Err(ConversionError::UnexpectedPartMime {
            actual: part1_mime,
            expected: "text/plain",
        });
    }
    if part2_mime != "text/html" {
        return Err(ConversionError::UnexpectedPartMime {
            actual: part2_mime,
            expected: "text/html",
        });
    }
    Ok(Email {
        address_from,
        address_to,
        subject,
        headers: mail
            .headers
            .into_iter()
            .map(|header| (header.get_key(), header.get_value()))
            .collect(),
        body_text: part1,
        body_html: part2,
    })
}
