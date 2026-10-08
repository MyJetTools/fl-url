use std::future::{poll_fn, Future};
use std::io::Write;
use std::ops::Deref;
use std::pin::Pin;
use std::task::{ready, Context, Poll};
use std::time::Duration;

use bytes::{Buf, Bytes};
use http_body_util::BodyExt;
use hyper::body::Body;
use my_http_client::{BodyPiece, BodyReader, MyHttpClientError};
use serde::de::DeserializeOwned;
use tokio::sync::Mutex;

use crate::non_wasm::escaped_body_guard::EscapedBodyGuard;
use crate::{ConnectionReturner, FlUrlError};

/// About the most of the data a piece of a gzip body which is given decoded has: a few
/// kilobytes of gzip can decode into megabytes, and a piece is not to be much bigger than
/// a piece of the body as it came.
const DECODED_PIECE_SIZE: usize = 64 * 1024;

/// The body of a response. [`crate::FlUrlResponse::get_body`] gives it, and how the
/// body is read is up to its reader — the methods are the ones of the reader of
/// my-http-client:
///
/// * [`Self::next_item`] gives the data of the body piece by piece, as it comes over
///   the network;
/// * [`Self::into_vec`], [`Self::into_string`] and [`Self::into_json`] read the whole
///   of it into memory, and fail with [`FlUrlError::ResponseBodyTooLarge`] on a body
///   bigger than the limit they are given.
///
/// It is a [`rust_extensions::AsyncBytesStream`] as well, for whoever reads a stream of
/// bytes and does not care that it is the body of a response.
///
/// The reader is the `BodyReader` of my-http-client with what a request of FlUrl adds
/// to it: the connection the body comes over, which goes back to the pool once the body
/// is read to its end and is closed when the reader is dropped before that; the
/// timeout of `set_response_body_timeout`; and the decoding of a gzip body which
/// `accept_gzip` asked for. So what it gives is the body itself, with nothing left to
/// do to its pieces but to put them together.
pub struct FlUrlBodyReader {
    /// What changes as the body is read. It is behind a lock, so that the body can be
    /// read through a shared reference as well - by one reader at a time
    reading: Mutex<Reading>,
    content_length: Option<usize>,
}

/// A piece of a response body, the way [`FlUrlBodyReader`] gives it as a
/// [`rust_extensions::AsyncBytesStream`]: the data of it, as a `&[u8]` through `Deref`.
///
/// It is the piece of my-http-client — a part of the buffer the connection is read
/// into, which is read into again once the piece is dropped — or what a piece of a gzip
/// body is decoded into. So a piece is to be used and dropped: what is needed for long
/// is copied out of it.
pub struct FlUrlBodyPiece(PieceData);

enum PieceData {
    Read(BodyPiece),
    Decoded(Vec<u8>),
}

impl Deref for FlUrlBodyPiece {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        match &self.0 {
            PieceData::Read(piece) => piece.as_slice(),
            PieceData::Decoded(data) => data.as_slice(),
        }
    }
}

impl AsRef<[u8]> for FlUrlBodyPiece {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl std::fmt::Debug for FlUrlBodyPiece {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlUrlBodyPiece")
            .field("len", &self.len())
            .finish()
    }
}

struct Reading {
    inner: BodyReader,
    state: State,
    // The connection the body comes over, checked out of the pool until the body is
    // read. Dropping it without returning disposes the connection.
    connection: Option<Box<dyn ConnectionReturner>>,
    // Whether the connection may serve the next request once the body is read.
    connection_is_reusable: bool,
    body_read_timeout: Option<Duration>,
    // Runs while a piece is waited for, when there is a timeout to wait with.
    piece_timer: Option<Pin<Box<tokio::time::Sleep>>>,
    gzip: Option<GzipDecoder>,
    // What is left of a piece of a gzip body. A piece is decoded a part at a time, so
    // that one which decodes into a lot is given in pieces of `DECODED_PIECE_SIZE`.
    gzip_input: Option<BodyPiece>,
    // The piece `next_item` has given. It is good until the next call.
    current: Option<FlUrlBodyPiece>,
}

enum State {
    Reading,
    /// The body is read to its end, and its connection is on the way back to the pool:
    /// the end of the body is told once it is there.
    ReturningConnection(Pin<Box<dyn Future<Output = ()> + Send>>),
    /// The body is read to its end
    Over,
    /// The body is over before its end, and this is why
    Failed(Failure),
}

/// Why a body can not be read to its end. The reader keeps answering with it.
enum Failure {
    Timeout,
    Reading(String),
}

impl Failure {
    fn to_error(&self) -> FlUrlError {
        match self {
            Self::Timeout => FlUrlError::Timeout,
            Self::Reading(reason) => FlUrlError::ReadingHyperBodyError(reason.clone()),
        }
    }
}

impl FlUrlBodyReader {
    /// `connection` is the connection the body comes over, to be settled once the body
    /// is read. `connection_is_reusable`: it may go back to the pool then.
    /// `decode_gzip`: the body is gzip, and is to be given decoded.
    pub(crate) fn new(
        inner: BodyReader,
        connection: Option<Box<dyn ConnectionReturner>>,
        connection_is_reusable: bool,
        body_read_timeout: Option<Duration>,
        decode_gzip: bool,
    ) -> Self {
        Self {
            // The size a response says is the size of its body as it was sent. One
            // which is given decoded is of another size, which nobody knows yet.
            content_length: match decode_gzip {
                true => None,
                false => inner.content_length(),
            },
            reading: Mutex::new(Reading {
                inner,
                state: State::Reading,
                connection,
                connection_is_reusable,
                body_read_timeout,
                piece_timer: None,
                gzip: decode_gzip.then(GzipDecoder::new),
                gzip_input: None,
                current: None,
            }),
        }
    }

    /// The body of a response which has none
    pub fn empty() -> Self {
        Self::new(BodyReader::empty(), None, false, None, false)
    }

    /// The size of the whole body when the response says it. `None` is a body which
    /// does not — chunked, or lasting until the connection is closed — and a gzip body
    /// which is given decoded: its `Content-Length` counts the encoded bytes.
    pub fn content_length(&self) -> Option<usize> {
        self.content_length
    }

    /// How much of the body is not read yet. `None` is a body which does not say it —
    /// and a body which is being read through a shared reference right now
    pub fn remains_to_read(&self) -> Option<usize> {
        self.reading.try_lock().ok()?.remains_to_read()
    }

    /// The next piece of the data of the body, as it has come over the network —
    /// decoded, when it is a gzip body `accept_gzip` asked for. `None` is the end of the
    /// body.
    ///
    /// The piece is good until the next call: the call lets go of it, so that the
    /// connection can be read into its buffer while the piece after it is waited for.
    /// What is needed for longer is copied out of it.
    ///
    /// A body which is cut short — the connection is closed, the framing is broken, a
    /// piece has not come within `set_response_body_timeout` — ends with an error, and
    /// keeps answering with it: what was read before is not the whole body.
    ///
    /// `set_timeout` does not bound it, so a body may last for as long as the upstream
    /// keeps sending it.
    pub async fn next_item(&mut self) -> Result<Option<&[u8]>, FlUrlError> {
        let reading = self.reading.get_mut();

        // The piece given before is let go of: the connection may be read into its
        // buffer while the next one is waited for
        reading.current = None;

        let piece = poll_fn(|cx| reading.poll_next(cx)).await?;
        reading.current = piece;

        Ok(reading.current.as_deref())
    }

    /// Reads the body to its end and gives it as a whole — what is left of it, when a
    /// part is taken by [`Self::next_item`] already.
    ///
    /// `max_size` is how big the body may be to be held in memory, in bytes: a bigger
    /// one fails with [`FlUrlError::ResponseBodyTooLarge`] — before a byte of it is
    /// read when the response says how big it is, as soon as it grows past the limit
    /// when it does not. A gzip body which is given decoded is held to the limit
    /// decoded. There is no limit but this one: `usize::MAX` takes a body of any size.
    ///
    /// The body has to be complete within `set_timeout` of the request it is the
    /// answer to — the clock of the request does not start over for the body — and
    /// within `set_response_body_timeout`, when there is one.
    pub async fn into_vec(self, max_size: usize) -> Result<Vec<u8>, FlUrlError> {
        self.reading.into_inner().into_vec(max_size).await
    }

    /// Reads the body to its end and gives it as a string. `max_size` is the limit of
    /// [`Self::into_vec`], in bytes.
    pub async fn into_string(self, max_size: usize) -> Result<String, FlUrlError> {
        let body = self.into_vec(max_size).await?;

        String::from_utf8(body).map_err(|err| FlUrlError::CanNotConvertToUtf8(err.utf8_error()))
    }

    /// Reads the body to its end and deserializes it from JSON. `max_size` is the
    /// limit of [`Self::into_vec`], in bytes.
    pub async fn into_json<TResult: DeserializeOwned>(
        self,
        max_size: usize,
    ) -> Result<TResult, FlUrlError> {
        let body = self.into_vec(max_size).await?;

        Ok(serde_json::from_slice(&body)?)
    }

    /// The body as hyper's, as it came: nothing is decoded. The connection goes with
    /// it, and is settled once that body is read to its end.
    pub(crate) fn into_hyper_body(self) -> http_body_util::combinators::BoxBody<Bytes, String> {
        self.reading.into_inner().into_hyper_body()
    }
}

/// The body as a stream of bytes. A chunk is a piece of the body with all there is to
/// do to it done — a gzip body which `accept_gzip` asked for comes decoded — so the
/// chunks put together are the body. `None` is its end.
///
/// What [`FlUrlBodyReader::next_item`] is about holds here too: the connection goes back
/// to the pool once the end of the body is read, `set_response_body_timeout` is how
/// long a chunk may be waited for, and a body which is cut short ends with an error. A
/// chunk is used and dropped — see [`FlUrlBodyPiece`].
///
/// It reads through a shared reference, so the reader can be handed over as an
/// `Arc<dyn AsyncBytesStream<FlUrlError, Chunk = FlUrlBodyPiece> + Send + Sync>`. The
/// body is still one stream: the callers take turns, and each chunk goes to one of them.
///
/// The `into_vec()` the trait comes with has no limit of the size; the one of the
/// reader itself, [`FlUrlBodyReader::into_vec`], takes one.
#[async_trait::async_trait]
impl rust_extensions::AsyncBytesStream<FlUrlError> for FlUrlBodyReader {
    type Chunk = FlUrlBodyPiece;

    async fn get_next(&self) -> Result<Option<FlUrlBodyPiece>, FlUrlError> {
        let mut reading = self.reading.lock().await;
        poll_fn(|cx| reading.poll_next(cx)).await
    }

    /// The size of the whole body when the response says it — see
    /// [`FlUrlBodyReader::content_length`].
    fn get_size(&self) -> Option<usize> {
        self.content_length
    }
}

impl Reading {
    fn remains_to_read(&self) -> Option<usize> {
        match &self.state {
            State::Reading if self.gzip.is_some() => None,
            State::Reading => self.inner.remains_to_read(),
            State::ReturningConnection(_) | State::Over => Some(0),
            State::Failed(_) => None,
        }
    }

    fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Result<Option<FlUrlBodyPiece>, FlUrlError>> {
        loop {
            match &mut self.state {
                State::Reading => {}
                State::ReturningConnection(returning) => {
                    ready!(returning.as_mut().poll(cx));
                    self.state = State::Over;
                    return Poll::Ready(Ok(None));
                }
                State::Over => return Poll::Ready(Ok(None)),
                State::Failed(failure) => return Poll::Ready(Err(failure.to_error())),
            }

            // What is left of a piece of a gzip body is decoded before the next piece is
            // read. It may decode into nothing yet: the decoder gives the bytes once it
            // has enough of the input to tell them.
            match self.decode_gzip_input() {
                Some(Ok(decoded)) if decoded.is_empty() => continue,
                Some(Ok(decoded)) => {
                    return Poll::Ready(Ok(Some(FlUrlBodyPiece(PieceData::Decoded(decoded)))));
                }
                Some(Err(failure)) => return Poll::Ready(Err(self.fail(failure))),
                None => {}
            }

            let frame = match Pin::new(&mut self.inner).poll_frame(cx) {
                Poll::Ready(frame) => {
                    self.piece_timer = None;
                    frame
                }
                Poll::Pending => {
                    if let Some(timeout) = self.body_read_timeout {
                        let timer = self
                            .piece_timer
                            .get_or_insert_with(|| Box::pin(tokio::time::sleep(timeout)));

                        if timer.as_mut().poll(cx).is_ready() {
                            return Poll::Ready(Err(self.fail(Failure::Timeout)));
                        }
                    }

                    return Poll::Pending;
                }
            };

            let piece = match frame {
                Some(Ok(frame)) => match frame.into_data() {
                    Ok(piece) => piece,
                    // The trailers of an HTTP/2 response are not a part of the body
                    Err(_trailers) => continue,
                },
                Some(Err(reason)) => {
                    return Poll::Ready(Err(self.fail(Failure::Reading(reason))));
                }
                None => {
                    // What the decoder has held back is the last piece of the body, and
                    // a gzip which ends half way is a body which is cut short.
                    let last_piece = match self.gzip.take().map(GzipDecoder::finish) {
                        Some(Ok(last_piece)) => last_piece,
                        Some(Err(failure)) => return Poll::Ready(Err(self.fail(failure))),
                        None => Vec::new(),
                    };

                    // The connection goes back before the end of the body is told:
                    // whoever reads it may send the next request at once, and the pool
                    // has to have the connection by then.
                    self.state = match self.take_reusable_connection() {
                        Some(connection) => {
                            State::ReturningConnection(connection.return_connection())
                        }
                        None => State::Over,
                    };

                    if last_piece.is_empty() {
                        continue;
                    }

                    return Poll::Ready(Ok(Some(FlUrlBodyPiece(PieceData::Decoded(last_piece)))));
                }
            };

            if self.gzip.is_some() {
                // It is decoded a part at a time, at the top of the loop
                self.gzip_input = Some(piece);
                continue;
            }

            return Poll::Ready(Ok(Some(FlUrlBodyPiece(PieceData::Read(piece)))));
        }
    }

    /// Decodes the next part of what is left of a piece of a gzip body. `None`: there is
    /// nothing left of one.
    fn decode_gzip_input(&mut self) -> Option<Result<Vec<u8>, Failure>> {
        let gzip = self.gzip.as_mut()?;
        let input = self.gzip_input.as_mut()?;

        let decoded = gzip.decode(input);

        // The piece is let go of once all of it is decoded: its buffer is free to be
        // read into again.
        if !input.has_remaining() {
            self.gzip_input = None;
        }

        Some(decoded)
    }

    async fn into_vec(mut self, max_size: usize) -> Result<Vec<u8>, FlUrlError> {
        match &self.state {
            State::Reading => {}
            // The body is read already: all there is left is to see its connection
            // into the pool.
            State::ReturningConnection(_) | State::Over => {
                while poll_fn(|cx| self.poll_next(cx)).await?.is_some() {}
                return Ok(Vec::new());
            }
            State::Failed(failure) => return Err(failure.to_error()),
        }

        // Both leave the reader for the time of the read. A read which fails or is
        // dropped half way drops the connection with it, which disposes it: its socket
        // still carries what was not read of the body.
        let connection = self.connection.take();

        // The limit is about the body as it is given. One which is to be decoded comes
        // over the wire encoded, and is held to what a body of the limit may take there.
        let wire_limit = match &self.gzip {
            Some(_) => gzip_wire_limit(max_size),
            None => max_size,
        };

        let read = std::mem::replace(&mut self.inner, BodyReader::empty()).into_vec(wire_limit);

        let body = match self.body_read_timeout {
            Some(timeout) => match tokio::time::timeout(timeout, read).await {
                Ok(body) => body,
                Err(_elapsed) => return Err(FlUrlError::Timeout),
            },
            None => read.await,
        };

        let body = body.map_err(|err| match reading_error(err) {
            FlUrlError::ResponseBodyTooLarge { .. } => {
                FlUrlError::ResponseBodyTooLarge { limit: max_size }
            }
            err => err,
        })?;

        // The body is off the wire, and the connection is free. Decoding comes after:
        // a body which does not decode is bad data, not a bad connection.
        self.state = State::Over;

        if let Some(connection) = connection {
            if self.connection_is_reusable {
                connection.return_connection().await;
            }
        }

        let Some(mut gzip) = self.gzip.take() else {
            return Ok(body);
        };

        let mut result = Vec::new();

        // What is left of a piece `next_item` has begun to decode comes before the rest
        if let Some(mut input) = self.gzip_input.take() {
            gzip.decode_into(&mut input, &mut result, max_size)?;
        }

        let mut rest = body.as_slice();
        gzip.decode_into(&mut rest, &mut result, max_size)?;
        gzip.finish_into(&mut result, max_size)?;

        Ok(result)
    }

    fn into_hyper_body(mut self) -> http_body_util::combinators::BoxBody<Bytes, String> {
        // A piece goes into the `Bytes` as it is: they hold the buffer it is read into,
        // and the data is not copied
        let inner = std::mem::replace(&mut self.inner, BodyReader::empty())
            .map_frame(|frame| frame.map_data(Bytes::from_owner))
            .boxed();

        match self.connection.take() {
            Some(connection) => {
                EscapedBodyGuard::new(inner, connection, self.connection_is_reusable).boxed()
            }
            None => inner,
        }
    }

    /// The body is over before its end. Its connection still carries what was not read
    /// of it, so it is disposed and not returned.
    fn fail(&mut self, failure: Failure) -> FlUrlError {
        let err = failure.to_error();

        self.connection = None;
        self.piece_timer = None;
        self.gzip = None;
        self.gzip_input = None;
        self.state = State::Failed(failure);

        err
    }

    /// The connection, when it is one to go back to the pool. One which is not is
    /// dropped here, which disposes it.
    fn take_reusable_connection(&mut self) -> Option<Box<dyn ConnectionReturner>> {
        let connection = self.connection.take()?;
        self.connection_is_reusable.then_some(connection)
    }
}

impl Drop for Reading {
    fn drop(&mut self) {
        // Whoever reads a body of a known size may stop at its last piece, without
        // asking for the end of it. Such a body is read whole, and its connection is as
        // good to go back as one of a body read to the end.
        if !matches!(self.state, State::Reading) || !self.inner.is_end_stream() {
            return;
        }

        let Some(connection) = self.take_reusable_connection() else {
            return;
        };

        // Dropping is sync, and the way back to the pool is async. With no runtime to
        // hand it to the connection is dropped, which disposes it.
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(connection.return_connection());
        }
    }
}

fn reading_error(err: MyHttpClientError) -> FlUrlError {
    match err {
        MyHttpClientError::RequestTimeout(_) => FlUrlError::Timeout,
        MyHttpClientError::CanNotExecuteRequest(reason) => FlUrlError::ReadingHyperBodyError(reason),
        // A body over the limit becomes `FlUrlError::ResponseBodyTooLarge` here
        other => other.into(),
    }
}

/// How big a gzip body of `max_size` decoded bytes may be on the wire. Gzip makes a body
/// smaller as a rule, but one which does not compress comes out a little bigger: the
/// header and the trailer of gzip, and a few bytes for every block deflate has stored
/// as it is.
fn gzip_wire_limit(max_size: usize) -> usize {
    max_size.saturating_add(max_size / 1000).saturating_add(1024)
}

/// Decodes a gzip body as it comes, piece by piece. It holds nothing but its own state —
/// the window of deflate, 32 KB — whatever the size of the body.
struct GzipDecoder {
    // MultiGzDecoder (not GzDecoder) so concatenated gzip members — a valid,
    // spec-allowed encoding some servers emit — are all decoded, not just the first.
    decoder: flate2::write::MultiGzDecoder<Vec<u8>>,
    // A body with nothing in it — the answer to a HEAD request, a 304 — is not a gzip
    // stream which is cut short: there is nothing to decode in it.
    has_input: bool,
}

impl GzipDecoder {
    fn new() -> Self {
        Self {
            decoder: flate2::write::MultiGzDecoder::new(Vec::new()),
            has_input: false,
        }
    }

    /// Decodes `input` until about `DECODED_PIECE_SIZE` of data is decoded, or until
    /// all of it is, and gives what is decoded. What is not decoded is left in `input`.
    ///
    /// It may give nothing: the decoder gives the bytes once it has enough of the input
    /// to tell them.
    fn decode(&mut self, input: &mut impl Buf) -> Result<Vec<u8>, Failure> {
        self.has_input |= input.has_remaining();

        while input.has_remaining() && self.decoder.get_ref().len() < DECODED_PIECE_SIZE {
            // A write decodes 32 KB at most, and takes as much of the input as it needs
            let taken = self.decoder.write(input.chunk()).map_err(gzip_failure)?;

            // It takes some of an input which is not empty, so this is not to happen. If
            // it does, the loop is not to go on for ever.
            if taken == 0 {
                return Err(gzip_failure(std::io::ErrorKind::WriteZero.into()));
            }

            input.advance(taken);
        }

        // What the decoder has decoded of the input and holds on to is given as well
        self.decoder.flush().map_err(gzip_failure)?;

        Ok(std::mem::take(self.decoder.get_mut()))
    }

    /// The end of the body: what the decoder has held back. Fails when the gzip ends
    /// half way.
    fn finish(self) -> Result<Vec<u8>, Failure> {
        if !self.has_input {
            return Ok(Vec::new());
        }

        self.decoder.finish().map_err(gzip_failure)
    }

    /// Decodes all of `input` onto `result`, which is held to `max_size`: a few
    /// kilobytes of gzip can inflate into gigabytes.
    fn decode_into(
        &mut self,
        input: &mut impl Buf,
        result: &mut Vec<u8>,
        max_size: usize,
    ) -> Result<(), FlUrlError> {
        while input.has_remaining() {
            let decoded = self.decode(input).map_err(|failure| failure.to_error())?;
            append_within(result, &decoded, max_size)?;
        }

        Ok(())
    }

    /// [`Self::finish`] onto `result`, which is held to `max_size`.
    fn finish_into(self, result: &mut Vec<u8>, max_size: usize) -> Result<(), FlUrlError> {
        let last_piece = self.finish().map_err(|failure| failure.to_error())?;
        append_within(result, &last_piece, max_size)
    }
}

fn append_within(result: &mut Vec<u8>, data: &[u8], max_size: usize) -> Result<(), FlUrlError> {
    if data.len() > max_size - result.len() {
        return Err(FlUrlError::ResponseBodyTooLarge { limit: max_size });
    }

    result.extend_from_slice(data);
    Ok(())
}

fn gzip_failure(err: std::io::Error) -> Failure {
    Failure::Reading(format!("Failed to decompress gzip body: {}", err))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::sync::Arc;

    use my_http_client::BodySender;
    use rust_extensions::{AsyncBytesStream, DoubleBuffer, DoubleBufferChunk};

    use super::*;

    const UNTOUCHED: u8 = 0;
    const RETURNED: u8 = 1;
    const DISPOSED: u8 = 2;

    struct SpyReturner(Arc<AtomicU8>);

    #[async_trait::async_trait]
    impl ConnectionReturner for SpyReturner {
        async fn return_connection(self: Box<Self>) {
            self.0.store(RETURNED, Ordering::SeqCst);
        }
    }

    impl Drop for SpyReturner {
        fn drop(&mut self) {
            // Counts as disposed only when the connection was not returned first.
            let _ =
                self.0
                    .compare_exchange(UNTOUCHED, DISPOSED, Ordering::SeqCst, Ordering::SeqCst);
        }
    }

    struct Setup {
        content_length: Option<usize>,
        reusable: bool,
        timeout: Option<Duration>,
        gzip: bool,
    }

    impl Default for Setup {
        fn default() -> Self {
            Self {
                content_length: None,
                reusable: true,
                timeout: None,
                gzip: false,
            }
        }
    }

    /// The side a body is written from, its reader, and what became of the connection.
    fn body(setup: Setup) -> (BodySender, FlUrlBodyReader, Arc<AtomicU8>) {
        let connection = Arc::new(AtomicU8::new(UNTOUCHED));
        let (upstream, inner) = BodyReader::new(setup.content_length);

        let reader = FlUrlBodyReader::new(
            inner,
            Some(Box::new(SpyReturner(connection.clone()))),
            setup.reusable,
            setup.timeout,
            setup.gzip,
        );

        (upstream, reader, connection)
    }

    fn state_of(connection: &AtomicU8) -> u8 {
        connection.load(Ordering::SeqCst)
    }

    /// A piece the way the read loop of my-http-client sends it: `data` is read into a
    /// buffer of a `DoubleBuffer` - one of its own.
    async fn piece_of(data: &[u8]) -> DoubleBufferChunk {
        let (writer, reader) = DoubleBuffer::new(data.len());

        let mut buffer = writer.get_buffer_to_read().await.unwrap();
        buffer[..data.len()].copy_from_slice(data);
        buffer.send(data.len());

        reader.get_next().await.unwrap().unwrap()
    }

    /// Up to three of them wait for the reader: two pieces and the end of the body.
    async fn send(upstream: &BodySender, data: &[u8]) {
        assert!(upstream.send(piece_of(data).await).await);
    }

    fn gzipped(body: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        encoder.write_all(body).unwrap();
        encoder.finish().unwrap()
    }

    /// Bytes which do not repeat for long, so that gzip makes little of them.
    fn noise(size: usize) -> Vec<u8> {
        let mut state: u32 = 1;
        (0..size)
            .map(|_| {
                state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                (state >> 16) as u8
            })
            .collect()
    }

    #[tokio::test]
    async fn a_body_read_to_its_end_returns_the_connection() {
        let (upstream, mut reader, connection) = body(Setup {
            content_length: Some(10),
            ..Default::default()
        });
        assert_eq!(reader.content_length(), Some(10));

        send(&upstream, b"hello").await;
        assert_eq!(reader.next_item().await.unwrap().unwrap(), b"hello");
        assert_eq!(reader.remains_to_read(), Some(5));
        assert_eq!(state_of(&connection), UNTOUCHED);

        send(&upstream, b"world").await;
        upstream.complete().await;

        assert_eq!(reader.next_item().await.unwrap().unwrap(), b"world");
        assert!(reader.next_item().await.unwrap().is_none());
        // The connection is in the pool by the time the end of the body is told.
        assert_eq!(state_of(&connection), RETURNED);

        // A body which is over keeps being over.
        assert!(reader.next_item().await.unwrap().is_none());
        assert_eq!(reader.remains_to_read(), Some(0));
        assert!(reader.into_vec(0).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn into_vec_gives_what_is_left_of_the_body() {
        let (upstream, mut reader, connection) = body(Setup::default());

        send(&upstream, b"hello ").await;
        assert_eq!(reader.next_item().await.unwrap().unwrap(), b"hello ");

        send(&upstream, b"wor").await;
        send(&upstream, b"ld").await;
        upstream.complete().await;

        assert_eq!(reader.into_vec(5).await.unwrap(), b"world");
        assert_eq!(state_of(&connection), RETURNED);
    }

    #[tokio::test]
    async fn into_vec_refuses_a_body_over_its_limit() {
        // The body says it is over the limit: refused with nothing of it sent yet.
        let (_upstream, reader, connection) = body(Setup {
            content_length: Some(11),
            ..Default::default()
        });
        assert!(matches!(
            reader.into_vec(10).await,
            Err(FlUrlError::ResponseBodyTooLarge { limit: 10 })
        ));
        assert_eq!(state_of(&connection), DISPOSED);

        // It does not say: cut off once it grows past the limit.
        let (upstream, reader, connection) = body(Setup::default());
        send(&upstream, b"hello ").await;
        send(&upstream, b"world").await;
        assert!(matches!(
            reader.into_vec(10).await,
            Err(FlUrlError::ResponseBodyTooLarge { limit: 10 })
        ));
        assert_eq!(state_of(&connection), DISPOSED);
    }

    #[tokio::test]
    async fn into_string_and_into_json_read_the_whole_body() {
        let (upstream, reader, _connection) = body(Setup::default());
        send(&upstream, "при".as_bytes()).await;
        send(&upstream, "вет".as_bytes()).await;
        upstream.complete().await;
        assert_eq!(reader.into_string(usize::MAX).await.unwrap(), "привет");

        let (upstream, reader, _connection) = body(Setup::default());
        send(&upstream, b"\xff\xfe").await;
        upstream.complete().await;
        assert!(matches!(
            reader.into_string(usize::MAX).await,
            Err(FlUrlError::CanNotConvertToUtf8(_))
        ));

        let (upstream, reader, connection) = body(Setup::default());
        send(&upstream, br#"{"name":"#).await;
        send(&upstream, br#""John","age":42}"#).await;
        upstream.complete().await;
        let json: serde_json::Value = reader.into_json(usize::MAX).await.unwrap();
        assert_eq!(json["name"], "John");
        assert_eq!(json["age"], 42);
        assert_eq!(state_of(&connection), RETURNED);

        // A body which is not the JSON it is read as is bad data, on a good connection.
        let (upstream, reader, connection) = body(Setup::default());
        send(&upstream, b"not a json").await;
        upstream.complete().await;
        assert!(matches!(
            reader.into_json::<serde_json::Value>(usize::MAX).await,
            Err(FlUrlError::SerializationError(_))
        ));
        assert_eq!(state_of(&connection), RETURNED);
    }

    #[tokio::test]
    async fn a_reader_dropped_before_the_end_disposes_the_connection() {
        let (upstream, mut reader, connection) = body(Setup::default());

        send(&upstream, b"hello").await;
        assert_eq!(reader.next_item().await.unwrap().unwrap(), b"hello");
        drop(reader);

        assert_eq!(state_of(&connection), DISPOSED);
        // What is left of the body on the connection is dropped as well.
        upstream.reader_is_dropped().await;
    }

    #[tokio::test]
    async fn a_read_dropped_half_way_disposes_the_connection() {
        let (upstream, reader, connection) = body(Setup::default());
        send(&upstream, b"hello").await;

        // The body is not over, so the read waits — and whoever waits gives up.
        let read = tokio::time::timeout(Duration::from_millis(50), reader.into_vec(usize::MAX));
        assert!(read.await.is_err());

        assert_eq!(state_of(&connection), DISPOSED);
    }

    #[tokio::test]
    async fn a_body_which_fails_keeps_answering_with_the_reason() {
        let (upstream, mut reader, connection) = body(Setup::default());

        send(&upstream, b"hello").await;
        upstream.fail("the connection is closed".to_string()).await;

        assert_eq!(reader.next_item().await.unwrap().unwrap(), b"hello");

        for _ in 0..2 {
            assert!(matches!(
                reader.next_item().await,
                Err(FlUrlError::ReadingHyperBodyError(reason)) if reason == "the connection is closed"
            ));
        }

        assert_eq!(state_of(&connection), DISPOSED);
        assert_eq!(reader.remains_to_read(), None);
        assert!(reader.into_vec(usize::MAX).await.is_err());
    }

    #[tokio::test]
    async fn a_connection_which_must_not_be_reused_is_disposed_at_the_end() {
        let (upstream, mut reader, connection) = body(Setup {
            reusable: false,
            ..Default::default()
        });
        upstream.complete().await;
        assert!(reader.next_item().await.unwrap().is_none());
        assert_eq!(state_of(&connection), DISPOSED);

        let (upstream, reader, connection) = body(Setup {
            reusable: false,
            ..Default::default()
        });
        upstream.complete().await;
        assert!(reader.into_vec(usize::MAX).await.unwrap().is_empty());
        assert_eq!(state_of(&connection), DISPOSED);
    }

    #[tokio::test]
    async fn a_silent_body_fails_once_the_timeout_runs_out() {
        let silent = || {
            body(Setup {
                timeout: Some(Duration::from_millis(50)),
                ..Default::default()
            })
        };

        // Piece by piece: each piece has the timeout to come in.
        let (upstream, mut reader, connection) = silent();
        send(&upstream, b"hello").await;
        assert_eq!(reader.next_item().await.unwrap().unwrap(), b"hello");

        for _ in 0..2 {
            assert!(matches!(reader.next_item().await, Err(FlUrlError::Timeout)));
        }
        assert_eq!(state_of(&connection), DISPOSED);

        // As a whole: the whole body has it.
        let (_upstream, reader, connection) = silent();
        assert!(matches!(
            reader.into_vec(usize::MAX).await,
            Err(FlUrlError::Timeout)
        ));
        assert_eq!(state_of(&connection), DISPOSED);
    }

    #[tokio::test]
    async fn a_gzip_body_is_given_decoded() {
        let text = b"hello hello hello hello hello hello hello hello hello world";
        let encoded = gzipped(text);
        let (first, second) = encoded.split_at(encoded.len() / 2);

        let gzip = || {
            body(Setup {
                content_length: Some(encoded.len()),
                gzip: true,
                ..Default::default()
            })
        };

        // Piece by piece
        let (upstream, mut reader, connection) = gzip();
        // The size the response says is the size of the encoded body, not of this one.
        assert_eq!(reader.content_length(), None);
        assert_eq!(reader.remains_to_read(), None);

        send(&upstream, first).await;
        send(&upstream, second).await;
        upstream.complete().await;

        let mut decoded = Vec::new();
        while let Some(piece) = reader.next_item().await.unwrap() {
            decoded.extend_from_slice(piece);
        }
        assert_eq!(decoded, text);
        assert_eq!(state_of(&connection), RETURNED);

        // As a whole, within a limit which is the size of the decoded body
        let (upstream, reader, connection) = gzip();
        send(&upstream, first).await;
        send(&upstream, second).await;
        upstream.complete().await;
        assert_eq!(reader.into_vec(text.len()).await.unwrap(), text);
        assert_eq!(state_of(&connection), RETURNED);

        // The limit is held by what the body decodes to. The body is read off the wire
        // by then, so the connection is a good one.
        let (upstream, reader, connection) = gzip();
        send(&upstream, &encoded).await;
        upstream.complete().await;
        assert!(encoded.len() < text.len() - 1);
        assert!(matches!(
            reader.into_vec(text.len() - 1).await,
            Err(FlUrlError::ResponseBodyTooLarge { limit }) if limit == text.len() - 1
        ));
        assert_eq!(state_of(&connection), RETURNED);
    }

    /// A piece of a few kilobytes may decode into megabytes. It is given in pieces of
    /// about `DECODED_PIECE_SIZE`, not as one.
    #[tokio::test]
    async fn a_gzip_piece_which_decodes_into_a_lot_is_given_in_pieces() {
        let text = vec![0u8; 2 * 1024 * 1024];
        let encoded = gzipped(&text);
        assert!(encoded.len() < 16 * 1024);

        let (upstream, mut reader, connection) = body(Setup {
            gzip: true,
            ..Default::default()
        });
        send(&upstream, &encoded).await;
        upstream.complete().await;

        let mut pieces = 0;
        let mut decoded = Vec::new();
        while let Some(piece) = reader.next_item().await.unwrap() {
            assert!(piece.len() <= 2 * DECODED_PIECE_SIZE, "{}", piece.len());
            pieces += 1;
            decoded.extend_from_slice(piece);
        }

        assert!(pieces >= text.len() / (2 * DECODED_PIECE_SIZE), "{pieces}");
        assert_eq!(decoded, text);
        assert_eq!(state_of(&connection), RETURNED);

        // Read as a whole, it is held to the limit as it decodes
        let (upstream, reader, _connection) = body(Setup {
            gzip: true,
            ..Default::default()
        });
        send(&upstream, &encoded).await;
        upstream.complete().await;
        assert!(matches!(
            reader.into_vec(1024 * 1024).await,
            Err(FlUrlError::ResponseBodyTooLarge { limit }) if limit == 1024 * 1024
        ));
    }

    /// What is left of a gzip piece `next_item` has begun to decode is decoded before
    /// the rest of the body, which `into_vec` reads.
    #[tokio::test]
    async fn into_vec_gives_what_is_left_of_a_gzip_body() {
        let text = noise(300 * 1024);
        let encoded = gzipped(&text);
        let (first, second) = encoded.split_at(encoded.len() / 2);

        let (upstream, mut reader, connection) = body(Setup {
            gzip: true,
            ..Default::default()
        });
        send(&upstream, first).await;
        send(&upstream, second).await;
        upstream.complete().await;

        let start = reader.next_item().await.unwrap().unwrap().to_vec();
        assert!(!start.is_empty() && start.len() < text.len());

        let rest = reader.into_vec(text.len() - start.len()).await.unwrap();
        assert_eq!([start, rest].concat(), text);
        assert_eq!(state_of(&connection), RETURNED);
    }

    #[tokio::test]
    async fn a_gzip_body_which_ends_half_way_fails() {
        let encoded = gzipped(b"hello hello hello hello world");
        let half = &encoded[..encoded.len() / 2];

        let (upstream, mut reader, _connection) = body(Setup {
            gzip: true,
            ..Default::default()
        });
        send(&upstream, half).await;
        upstream.complete().await;

        let mut failed = false;
        for _ in 0..3 {
            match reader.next_item().await {
                Err(FlUrlError::ReadingHyperBodyError(_)) => {
                    failed = true;
                    break;
                }
                Err(err) => panic!("{err:?}"),
                Ok(_) => {}
            }
        }
        assert!(failed);

        let (upstream, reader, _connection) = body(Setup {
            gzip: true,
            ..Default::default()
        });
        send(&upstream, half).await;
        upstream.complete().await;
        assert!(matches!(
            reader.into_vec(usize::MAX).await,
            Err(FlUrlError::ReadingHyperBodyError(_))
        ));
    }

    /// The answer to a HEAD request carries the `Content-Encoding` of a body it does
    /// not have: nothing to decode is not a gzip which is cut short.
    #[tokio::test]
    async fn an_empty_body_has_nothing_to_decode() {
        let empty = || {
            body(Setup {
                gzip: true,
                ..Default::default()
            })
        };

        let (upstream, mut reader, connection) = empty();
        upstream.complete().await;
        assert!(reader.next_item().await.unwrap().is_none());
        assert_eq!(state_of(&connection), RETURNED);

        let (upstream, reader, connection) = empty();
        upstream.complete().await;
        assert!(reader.into_vec(0).await.unwrap().is_empty());
        assert_eq!(state_of(&connection), RETURNED);
    }

    /// What reads a stream of bytes, with no idea that it is the body of a response.
    async fn put_together(
        src: &(impl AsyncBytesStream<FlUrlError> + ?Sized),
    ) -> Result<Vec<u8>, FlUrlError> {
        let mut result = Vec::new();

        while let Some(chunk) = src.get_next().await? {
            result.extend_from_slice(&chunk);
        }

        Ok(result)
    }

    type BytesStream = dyn AsyncBytesStream<FlUrlError, Chunk = FlUrlBodyPiece> + Send + Sync;

    #[tokio::test]
    async fn as_a_bytes_stream_the_body_comes_in_chunks() {
        let (upstream, reader, connection) = body(Setup {
            content_length: Some(11),
            ..Default::default()
        });
        send(&upstream, b"hello ").await;
        send(&upstream, b"world").await;
        upstream.complete().await;

        // Read through a shared reference, by a task of its own.
        let reader: Arc<BytesStream> = Arc::new(reader);
        assert_eq!(reader.get_size(), Some(11));
        let read = tokio::spawn(async move { put_together(reader.as_ref()).await });

        assert_eq!(read.await.unwrap().unwrap(), b"hello world");
        // The reader is gone with its task by now, and the connection went back before.
        assert_eq!(state_of(&connection), RETURNED);
    }

    /// The `into_vec()` the trait comes with reads the chunks the reader gives: the body
    /// with all there is to do to it done, and its connection back in the pool.
    #[tokio::test]
    async fn as_a_bytes_stream_into_vec_reads_the_whole_body() {
        let (upstream, reader, connection) = body(Setup::default());
        send(&upstream, b"hello ").await;
        send(&upstream, b"world").await;
        upstream.complete().await;

        let reader: Arc<BytesStream> = Arc::new(reader);
        assert_eq!(reader.into_vec().await.unwrap(), b"hello world");
        assert_eq!(state_of(&connection), RETURNED);
    }

    #[tokio::test]
    async fn as_a_bytes_stream_a_gzip_body_comes_decoded() {
        let text = b"hello hello hello hello hello hello hello hello hello world";
        let encoded = gzipped(text);
        let (first, second) = encoded.split_at(encoded.len() / 2);

        let (upstream, reader, connection) = body(Setup {
            gzip: true,
            ..Default::default()
        });
        send(&upstream, first).await;
        send(&upstream, second).await;
        upstream.complete().await;

        // The size the response says is the size of the encoded body.
        assert_eq!(reader.get_size(), None);

        // The chunks are the body itself: all there is left to do is to put them
        // together.
        assert_eq!(put_together(&reader).await.unwrap(), text);
        assert_eq!(state_of(&connection), RETURNED);
    }

    #[tokio::test]
    async fn as_a_bytes_stream_a_body_which_fails_keeps_answering_with_the_reason() {
        let (upstream, reader, connection) = body(Setup {
            timeout: Some(Duration::from_millis(50)),
            ..Default::default()
        });
        send(&upstream, b"hello").await;

        assert_eq!(&*reader.get_next().await.unwrap().unwrap(), b"hello");

        for _ in 0..2 {
            assert!(matches!(reader.get_next().await, Err(FlUrlError::Timeout)));
        }
        assert_eq!(state_of(&connection), DISPOSED);
    }

    /// The body is one stream whoever reads it: while a chunk is waited for through a
    /// shared reference the reader has nothing to say about what is left of it.
    #[tokio::test]
    async fn a_body_being_read_through_a_shared_reference_does_not_say_what_remains() {
        let (upstream, reader, _connection) = body(Setup {
            content_length: Some(10),
            ..Default::default()
        });
        let reader = Arc::new(reader);
        assert_eq!(reader.remains_to_read(), Some(10));

        let waiting = {
            let reader = reader.clone();
            tokio::spawn(async move { reader.get_next().await.map(|chunk| chunk.map(|chunk| chunk.to_vec())) })
        };
        tokio::task::yield_now().await;
        assert_eq!(reader.remains_to_read(), None);

        send(&upstream, b"hello").await;
        assert_eq!(waiting.await.unwrap().unwrap().unwrap(), b"hello");
        assert_eq!(reader.remains_to_read(), Some(5));
        // The size the response has said does not change.
        assert_eq!(reader.content_length(), Some(10));
    }

    /// A response is borrowed across an `.await` of a spawned task — a header read
    /// before something is waited for — and that takes both to be `Sync`.
    #[test]
    fn the_reader_and_the_response_are_send_and_sync() {
        fn assert_send_and_sync<T: Send + Sync>() {}

        assert_send_and_sync::<FlUrlBodyReader>();
        assert_send_and_sync::<crate::FlUrlResponse>();
    }

    #[tokio::test]
    async fn a_body_which_is_not_there_is_an_empty_one() {
        let mut reader = FlUrlBodyReader::empty();

        assert_eq!(reader.content_length(), Some(0));
        assert_eq!(reader.remains_to_read(), Some(0));
        assert!(reader.next_item().await.unwrap().is_none());
        assert!(FlUrlBodyReader::empty().into_vec(0).await.unwrap().is_empty());
    }
}
