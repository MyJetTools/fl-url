//! The bridge between a `#[http_body_as_stream]` model field and hyper's outgoing
//! body.
//!
//! my-http-utils owns the channel and knows nothing about any transport; hyper wants
//! a [`hyper::body::Body`]. This is the whole of the adapter between them: one
//! `poll_frame` that forwards to
//! [`HttpBodyReader::poll_next_chunk`](my_http_utils::http_input::HttpBodyReader::poll_next_chunk).
//!
//! The poll-based reader method is what keeps this allocation-free — the `&self`
//! `get_next_chunk` would have to be driven through a boxed future kept alive across
//! polls, since `poll_frame` has a `Context` and no place to `.await`.

use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use hyper::body::{Body, Frame};
use my_http_utils::http_input::{HttpBodyReader, HttpParseError};

/// An outgoing body that pulls its chunks out of a model's `HttpBodyAsStream`.
pub(crate) struct ModelBodyStream {
    reader: HttpBodyReader,
}

impl ModelBodyStream {
    pub fn new(reader: HttpBodyReader) -> Self {
        Self { reader }
    }
}

impl Body for ModelBodyStream {
    type Data = Bytes;
    /// The reader's own error, which already tells a truncated body from a clean end
    /// — `execute_streamed_impl` only needs it to be `Display`.
    type Error = HttpParseError;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        // Every field of `HttpBodyReader` is `Unpin`, so this type is too and the pin
        // can be dropped without unsafe.
        let reader = &mut self.get_mut().reader;

        match reader.poll_next_chunk(cx) {
            Poll::Pending => Poll::Pending,
            // The body arrived in full — the reader only reports this once the sender
            // marked it complete; a channel that closed without that comes back as an
            // `Err` below instead of ending the body silently.
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Ready(Some(Ok(chunk))) => Poll::Ready(Some(Ok(Frame::data(Bytes::from(chunk))))),
            Poll::Ready(Some(Err(err))) => Poll::Ready(Some(Err(err))),
        }
    }

    // `size_hint` is deliberately left at its default (unknown). The framing of a
    // streamed request is decided by the `content_size` handed to
    // `RequestToExecute::streamed`, which is where the `Content-Length` header comes
    // from; announcing a size here as well would risk a second, competing one.
}
