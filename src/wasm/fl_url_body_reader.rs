use std::future::poll_fn;
use std::task::{ready, Context, Poll};

use serde::de::DeserializeOwned;
use web_sys::{AbortController, ReadableStreamDefaultReader, Response};

use crate::wasm::fetch::{
    announced_length, body_stream_reader, cancel_body, cancel_response_body, read_response_body,
    BodyPieceRead, BodyReadFailure,
};
use crate::FlUrlError;

/// The body of a response. [`crate::FlUrlResponse::get_body`] gives it, and how the
/// body is read is up to its reader — the same methods the reader of the native
/// backend has:
///
/// * [`Self::next_item`] gives the body piece by piece, as it comes over the network;
/// * [`Self::into_vec`], [`Self::into_string`] and [`Self::into_json`] read the whole
///   of it into memory, and fail with [`FlUrlError::ResponseBodyTooLarge`] on a body
///   bigger than the limit they are given.
///
/// The pieces come off the stream of the response (`Response.body`), so a body which
/// is not read is not downloaded whole: the browser holds the upstream back once it
/// has buffered what it is willing to, and a reader which is dropped before the end
/// of the body ends the download.
pub struct FlUrlBodyReader {
    state: State,
    content_length: Option<usize>,
    /// How much of the body is received already
    received: usize,
    // The `AbortController` of the request. It is what the wait for the body is
    // bounded with, and what ends the download of a body which is not read to its end.
    controller: Option<AbortController>,
    body_timeout_millis: Option<i32>,
    // The piece `next_item` has given. It is good until the next call.
    current: Option<Vec<u8>>,
}

enum State {
    /// Nothing of the body is read, and its stream is not taken yet: the body may
    /// still be had as a whole in one go.
    NotStarted(Response),
    /// The body is on the stream. `pending` is a `read()` which has not settled yet:
    /// it is kept here and not by the future of `next_item`, so a future which is
    /// dropped half way does not take the piece it was waiting for along.
    Reading {
        reader: ReadableStreamDefaultReader,
        pending: Option<BodyPieceRead>,
    },
    /// The body is read to its end
    Over,
    /// The body is over before its end, and this is why
    Failed(BodyReadFailure),
}

impl FlUrlBodyReader {
    /// The body of `response`, nothing of which is read yet. `body_timeout_millis` is
    /// `set_response_body_timeout`.
    pub(crate) fn of_response(
        response: Response,
        controller: Option<AbortController>,
        body_timeout_millis: Option<i32>,
    ) -> Self {
        // The answer to a HEAD request, a 204, a 304: there is no body to read.
        if response.body().is_none() {
            return Self::empty();
        }

        Self {
            content_length: announced_length(&response),
            state: State::NotStarted(response),
            received: 0,
            controller,
            body_timeout_millis,
            current: None,
        }
    }

    /// The body of a response which has none
    pub fn empty() -> Self {
        Self {
            state: State::Over,
            content_length: Some(0),
            received: 0,
            controller: None,
            body_timeout_millis: None,
            current: None,
        }
    }

    /// The size of the whole body when the response says it. `None` is a body which
    /// does not — and one the browser decodes (`Content-Encoding`): its
    /// `Content-Length` counts the encoded bytes, and the body comes decoded.
    pub fn content_length(&self) -> Option<usize> {
        self.content_length
    }

    /// How much of the body is not read yet. `None` is a body which does not say it
    pub fn remains_to_read(&self) -> Option<usize> {
        match &self.state {
            State::NotStarted(_) | State::Reading { .. } => {
                Some(self.content_length?.saturating_sub(self.received))
            }
            State::Over => Some(0),
            State::Failed(_) => None,
        }
    }

    /// The next piece of the body, as the browser has it from the network. `None` is
    /// the end of the body.
    ///
    /// The piece is good until the next call: the call lets go of it. What is needed for
    /// longer is copied out of it.
    ///
    /// A body which is cut short — the connection is closed, a piece has not come
    /// within `set_response_body_timeout` — ends with an error, and keeps answering
    /// with it: what was read before is not the whole body.
    ///
    /// `set_timeout` does not bound it, so a body may last for as long as the upstream
    /// keeps sending it.
    pub async fn next_item(&mut self) -> Result<Option<&[u8]>, FlUrlError> {
        self.current = None;

        let piece = poll_fn(|cx| self.poll_next_piece(cx)).await?;
        self.current = piece;

        Ok(self.current.as_deref())
    }

    /// Reads the body to its end and gives it as a whole — what is left of it, when a
    /// part is taken by [`Self::next_item`] already.
    ///
    /// `max_size` is how big the body may be to be held in memory, in bytes: a bigger
    /// one fails with [`FlUrlError::ResponseBodyTooLarge`] — before a byte of it is
    /// read when the response says how big it is, as soon as it grows past the limit
    /// when it does not. Either way the rest of it is not downloaded. There is no
    /// limit but this one: `usize::MAX` takes a body of any size.
    ///
    /// `set_response_body_timeout` bounds it; `set_timeout` does not — under wasm the
    /// timeout of the request ends with the head of the response.
    pub async fn into_vec(mut self, max_size: usize) -> Result<Vec<u8>, FlUrlError> {
        if self.remains_to_read().is_some_and(|size| size > max_size) {
            return Err(FlUrlError::ResponseBodyTooLarge { limit: max_size });
        }

        if let Some(response) = self.response_to_read_in_one_go(max_size) {
            let body = read_response_body(
                &response,
                self.controller.as_ref(),
                self.body_timeout_millis,
            )
            .await?;

            self.state = State::Over;

            // The size a response says is the size of what was sent, and the browser
            // may have decoded it into more.
            if body.len() > max_size {
                return Err(FlUrlError::ResponseBodyTooLarge { limit: max_size });
            }

            return Ok(body);
        }

        let mut result = Vec::new();

        while let Some(piece) = poll_fn(|cx| self.poll_next_piece(cx)).await? {
            if piece.len() > max_size - result.len() {
                return Err(FlUrlError::ResponseBodyTooLarge { limit: max_size });
            }

            if result.is_empty() {
                // A body which has come in one piece is taken as it is, with no copy
                result = piece;

                // The size is what the response says, and the limit may be none at
                // all: a size which can not be allocated must not bring the page down
                if let Some(remains) = self.remains_to_read() {
                    let _ = result.try_reserve(remains);
                }
            } else {
                result.extend_from_slice(&piece);
            }
        }

        Ok(result)
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

    /// The next piece of the body, copied out of the memory of JS. `None` is the end of
    /// the body.
    fn poll_next_piece(&mut self, cx: &mut Context<'_>) -> Poll<Result<Option<Vec<u8>>, FlUrlError>> {
        loop {
            let (reader, pending) = match &mut self.state {
                State::NotStarted(response) => {
                    let state = match body_stream_reader(response) {
                        Ok(Some(reader)) => State::Reading {
                            reader,
                            pending: None,
                        },
                        Ok(None) => State::Over,
                        Err(failure) => State::Failed(failure),
                    };

                    self.state = state;
                    continue;
                }
                State::Reading { reader, pending } => (reader, pending),
                State::Over => return Poll::Ready(Ok(None)),
                State::Failed(failure) => return Poll::Ready(Err(failure.to_error())),
            };

            let read = pending.get_or_insert_with(|| {
                BodyPieceRead::start(reader, self.controller.as_ref(), self.body_timeout_millis)
            });

            let piece = ready!(read.poll(cx));
            *pending = None;

            match piece {
                // A piece with nothing in it is not the end of the body
                Ok(Some(piece)) if piece.is_empty() => {}
                Ok(Some(piece)) => {
                    self.received += piece.len();
                    return Poll::Ready(Ok(Some(piece)));
                }
                Ok(None) => {
                    self.state = State::Over;
                    return Poll::Ready(Ok(None));
                }
                Err(failure) => {
                    let err = failure.to_error();
                    self.end_download();
                    self.state = State::Failed(failure);
                    return Poll::Ready(Err(err));
                }
            }
        }
    }

    /// The response, when its body is better had with one `Response.arrayBuffer()`
    /// than piece by piece: one promise and one copy into wasm memory instead of a
    /// promise and a copy for every piece.
    ///
    /// That is a body nothing of which is taken yet, and which can not turn out to be
    /// over the limit half way: there is no limit, or the response says its size. One
    /// which may has to be read piece by piece, to be cut off as it grows.
    fn response_to_read_in_one_go(&self, max_size: usize) -> Option<Response> {
        let State::NotStarted(response) = &self.state else {
            return None;
        };

        if max_size == usize::MAX || self.content_length.is_some() {
            // The handle to the very same response: `Response::clone()` on its own is
            // the method of JS, which makes another response out of this one.
            return Some(Clone::clone(response));
        }

        None
    }

    /// Nobody is going to read what is left of the body, so it is not downloaded
    /// either. Does nothing to a body which is over.
    fn end_download(&self) {
        match (&self.state, &self.controller) {
            (State::NotStarted(_) | State::Reading { .. }, Some(controller)) => controller.abort(),
            (State::NotStarted(response), None) => cancel_response_body(response),
            (State::Reading { reader, .. }, None) => cancel_body(reader),
            (State::Over | State::Failed(_), _) => {}
        }
    }
}

impl Drop for FlUrlBodyReader {
    fn drop(&mut self) {
        self.end_download();
    }
}
