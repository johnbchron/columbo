use tokio_util::sync::CancellationToken;

/// A wrapper around [`CancellationToken`] that calls `cancel()` when dropped.
#[derive(Clone)]
pub(crate) struct CancelOnDrop {
  inner: CancellationToken,
}

impl CancelOnDrop {
  pub(crate) fn new(inner: CancellationToken) -> Self { CancelOnDrop { inner } }

  pub(crate) fn inner(&self) -> &CancellationToken { &self.inner }
}

impl Drop for CancelOnDrop {
  fn drop(&mut self) {
    tracing::debug!(
      "response stream dropped; indicating cancellation to listening futures"
    );
    self.inner.cancel()
  }
}
