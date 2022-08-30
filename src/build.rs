use lettre::{
    error::Error,
    message::{header, MessageBuilder, MultiPart, SinglePart},
    Message,
};

/// Extension trait for [`lettre::message::MessageBuilder`]
/// to simplify adding text and html parts to a message.
///
/// This trait is only available with the `lettre` feature.
pub trait MessageBuilderExt {
    /// Add a text part and complete the message.
    fn body_text(self, text: String) -> Result<Message, Error>;

    /// Add an html part and complete the message.
    fn body_html(self, html: String) -> Result<Message, Error>;

    /// Add both a text and html part and complete the message.
    fn body_text_and_html(
        self,
        text: String,
        html: String,
    ) -> Result<Message, Error>;
}

impl MessageBuilderExt for MessageBuilder {
    fn body_text(self, text: String) -> Result<Message, Error> {
        self.singlepart(
            SinglePart::builder()
                .header(header::ContentType::TEXT_PLAIN)
                .body(text),
        )
    }

    fn body_html(self, html: String) -> Result<Message, Error> {
        self.singlepart(
            SinglePart::builder()
                .header(header::ContentType::TEXT_HTML)
                .body(html),
        )
    }

    fn body_text_and_html(
        self,
        text: String,
        html: String,
    ) -> Result<Message, Error> {
        self.multipart(
            MultiPart::alternative()
                .singlepart(
                    SinglePart::builder()
                        .header(header::ContentType::TEXT_PLAIN)
                        .body(text),
                )
                .singlepart(
                    SinglePart::builder()
                        .header(header::ContentType::TEXT_HTML)
                        .body(html),
                ),
        )
    }
}
