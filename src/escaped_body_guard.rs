use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use hyper::body::{Body, Frame};

use crate::ConnectionReturner;

/// Wraps a response body that escapes fl-url's control (`into_hyper_response`)
/// so the checked-out connection stays alive while the body is streaming.
/// On clean end-of-body the connection is returned to the pool (if healthy);
/// dropping the body mid-stream or a read error disposes it instead.
pub(crate) struct EscapedBodyGuard {
    inner: http_body_util::combinators::BoxBody<Bytes, String>,
    returner: Option<Box<dyn ConnectionReturner>>,
    return_healthy: bool,
}

impl EscapedBodyGuard {
    pub fn new(
        inner: http_body_util::combinators::BoxBody<Bytes, String>,
        returner: Box<dyn ConnectionReturner>,
        return_healthy: bool,
    ) -> Self {
        Self {
            inner,
            returner: Some(returner),
            return_healthy,
        }
    }

    fn settle(&mut self) {
        let Some(returner) = self.returner.take() else {
            return;
        };

        if self.return_healthy {
            // poll_frame/Drop are sync; hand the async return-to-pool off to
            // the runtime. Body polling/drop happens inside one.
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(returner.return_connection());
            }
            // No runtime: dropping the returner disposes the connection.
        }
        // Not healthy: dropping the returner disposes the connection.
    }
}

impl Drop for EscapedBodyGuard {
    fn drop(&mut self) {
        // A conforming consumer (e.g. hyper's own HTTP/1 server re-serving this
        // body) stops polling as soon as is_end_stream() is true and drops the
        // body WITHOUT a final poll returning Ready(None) — so settle() never
        // ran in poll_frame. Recover the connection here: if the body reports
        // end-of-stream it was fully drained and is safe to return; otherwise
        // it was dropped mid-stream and must be disposed.
        if self.returner.is_some() {
            if !self.inner.is_end_stream() {
                self.return_healthy = false;
            }
            self.settle();
        }
    }
}

impl hyper::body::Body for EscapedBodyGuard {
    type Data = Bytes;
    type Error = String;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.get_mut();

        match Pin::new(&mut this.inner).poll_frame(cx) {
            Poll::Ready(None) => {
                this.settle();
                Poll::Ready(None)
            }
            Poll::Ready(Some(Err(err))) => {
                // Mid-body error: the connection is not reusable.
                this.return_healthy = false;
                this.settle();
                Poll::Ready(Some(Err(err)))
            }
            other => other,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> hyper::body::SizeHint {
        self.inner.size_hint()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::sync::Arc;

    // 0 = untouched, 1 = returned to pool, 2 = disposed (returner dropped
    // without return).
    struct SpyReturner(Arc<AtomicU8>);

    #[async_trait::async_trait]
    impl ConnectionReturner for SpyReturner {
        async fn return_connection(self: Box<Self>) {
            self.0.store(1, Ordering::SeqCst);
        }
    }

    impl Drop for SpyReturner {
        fn drop(&mut self) {
            // Only counts as dispose if return_connection did not fire first.
            let _ = self
                .0
                .compare_exchange(0, 2, Ordering::SeqCst, Ordering::SeqCst);
        }
    }

    fn guard_over(bytes: &'static [u8], outcome: &Arc<AtomicU8>) -> EscapedBodyGuard {
        use http_body_util::{BodyExt, Full};
        let body = Full::new(Bytes::from_static(bytes))
            .map_err(|_: std::convert::Infallible| String::new())
            .boxed();
        let returner: Box<dyn ConnectionReturner> = Box::new(SpyReturner(outcome.clone()));
        EscapedBodyGuard::new(body, returner, true)
    }

    async fn poll_once(guard: &mut EscapedBodyGuard) {
        // EscapedBodyGuard is Unpin (all fields are), so Pin::new is fine.
        let _ = std::future::poll_fn(|cx| Pin::new(&mut *guard).poll_frame(cx)).await;
    }

    // A consumer that stops polling as soon as is_end_stream() is true (hyper's
    // server for Content-Length bodies) must still get the connection pooled.
    #[tokio::test]
    async fn content_length_body_pooled_when_consumer_stops_at_end_stream() {
        let outcome = Arc::new(AtomicU8::new(0));
        {
            let mut guard = guard_over(b"hello", &outcome);
            poll_once(&mut guard).await; // drains the single data frame
            assert!(guard.is_end_stream());
            // Consumer drops WITHOUT polling to Ready(None).
        }
        tokio::task::yield_now().await;
        assert_eq!(outcome.load(Ordering::SeqCst), 1, "connection must be pooled");
    }

    #[tokio::test]
    async fn empty_body_pooled_without_any_poll() {
        let outcome = Arc::new(AtomicU8::new(0));
        {
            let _guard = guard_over(b"", &outcome); // never polled, then dropped
        }
        tokio::task::yield_now().await;
        assert_eq!(outcome.load(Ordering::SeqCst), 1, "empty body must pool");
    }

    #[tokio::test]
    async fn body_dropped_mid_stream_is_disposed() {
        let outcome = Arc::new(AtomicU8::new(0));
        {
            // Dropped before any poll: is_end_stream() is false (data pending),
            // so the connection must be disposed, not pooled.
            let _guard = guard_over(b"hello", &outcome);
        }
        tokio::task::yield_now().await;
        assert_eq!(outcome.load(Ordering::SeqCst), 2, "unread body must dispose");
    }
}
