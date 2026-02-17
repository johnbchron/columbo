#![doc = include_str!("../README.md")]

mod cancel_on_drop;
mod format;
mod html;
mod html_stream;

#[cfg(test)]
mod tests;

use std::{
  any::Any,
  fmt,
  panic::AssertUnwindSafe,
  sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
  },
};

use futures::FutureExt;
pub use html::Html;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, Span, debug, instrument, trace, warn};

use self::{cancel_on_drop::CancelOnDrop, html_stream::HtmlStream};

type Id = usize;

/// Creates a new [`SuspenseContext`] and [`SuspendedResponse`]. The context is
/// for suspending futures, and the response turns into an output stream.
pub fn new() -> (SuspenseContext, SuspendedResponse) {
  new_with_opts(ColumboOptions::default())
}

/// Creates a new [`SuspenseContext`] and [`SuspendedResponse`], with the given
/// [`ColumboOptions`]. The context is for suspending futures, and the response
/// turns into an output stream.
#[instrument(name = "columbo::new", skip_all)]
pub fn new_with_opts(
  options: ColumboOptions,
) -> (SuspenseContext, SuspendedResponse) {
  let (tx, rx) = mpsc::unbounded_channel();
  let cancel = CancellationToken::new();

  debug!("created new suspense context and response");
  (
    SuspenseContext {
      next_id: Arc::new(AtomicUsize::new(0)),
      tx,
      opts: Arc::new(options),
      cancel: cancel.clone(),
    },
    SuspendedResponse {
      rx,
      cancel: CancelOnDrop::new(cancel),
    },
  )
}

/// The context with which you can create suspense boundaries for futures.
#[derive(Clone)]
pub struct SuspenseContext {
  next_id: Arc<AtomicUsize>,
  tx:      mpsc::UnboundedSender<Html>,
  opts:    Arc<ColumboOptions>,
  cancel:  CancellationToken,
}

impl SuspenseContext {
  fn new_id(&self) -> Id {
    // IDs don't need to be sequential, only unique
    self.next_id.fetch_add(1, Ordering::Relaxed)
  }

  /// Suspends async work and streams the result. This function takes a closure
  /// that returns a future, allowing the future to spawn more suspensions. The
  /// placeholder is sent immediately, while the future output is streamed and
  /// replaces the placeholder in the browser.
  ///
  /// The future can return any type that implements [`Into<Html>`], including
  /// `String`, `&str`, or types like `maud::Markup`.
  ///
  /// Suspended futures must be `Send` because they are handed to `tokio`.
  #[instrument(name = "columbo::suspend", skip_all, fields(suspense.id))]
  pub fn suspend<F, Fut, M>(
    &self,
    f: F,
    placeholder: impl Into<Html>,
  ) -> Suspense
  where
    F: FnOnce(SuspenseContext) -> Fut,
    Fut: Future<Output = M> + Send + 'static,
    M: Into<Html> + 'static,
  {
    let id = self.new_id();
    Span::current().record("suspense.id", id.to_string());

    tokio::spawn(
      self
        .clone()
        .run_suspended(id, f(self.clone()))
        .instrument(tracing::info_span!("columbo::suspended_task")),
    );

    Suspense::new(id, placeholder.into())
  }

  /// Yields if [`SuspendedResponse`] or the resulting stream type is dropped.
  ///
  /// Useful for exiting from suspended futures that should stop if the
  /// connection is dropped. Suspended futures are not aborted otherwise, so
  /// they will continue to execute if you don't listen for cancellation.
  pub async fn cancelled(&self) { self.cancel.cancelled().await; }

  /// Returns true if [`SuspendedResponse`] or the resulting stream type is
  /// dropped.
  ///
  /// Useful for exiting from suspended futures that should stop if the
  /// connection is dropped. Suspended futures are not aborted otherwise, so
  /// they will continue to execute if you don't listen for cancellation.
  pub fn is_cancelled(&self) -> bool { self.cancel.is_cancelled() }

  async fn run_suspended<Fut, M>(self, id: Id, future: Fut)
  where
    Fut: Future<Output = M> + Send + 'static,
    M: Into<Html>,
  {
    let auto_cancel = self.opts.auto_cancel.unwrap_or(false);

    // catch panics in future
    let future = AssertUnwindSafe(future).catch_unwind();
    // race the future against the cancellation token
    let result = if auto_cancel {
      tokio::select! {
        _ = self.cancel.cancelled() => {
          trace!(suspense.id = %id, "task exited via auto_cancel");
          return; // exit immediately; nothing to send
        }
        result = future => result,
      }
    } else {
      future.await
    };

    // determine what to swap in
    let panic_handler = self
      .opts
      .panic_renderer
      .unwrap_or(crate::format::default_panic_renderer);
    let content: Html = match result {
      Ok(m) => m.into(),
      Err(panic_payload) => {
        warn!(suspense.id = %id, "suspended task panicked; rendering panic");
        panic_handler(panic_payload)
      }
    };

    // render the wrapper
    let payload = format::render_replacement(&id, &content);

    let _ = self.tx.send(payload).inspect_err(|_| {
      trace!(suspense.id = %id, "future completed but receiver is dropped");
    });
  }
}

/// Options for configuring `columbo` suspense.
#[derive(Clone, Debug, Default)]
pub struct ColumboOptions {
  /// Renders a panic fallback given the panic object.
  pub panic_renderer: Option<fn(Box<dyn Any + Send>) -> Html>,
  /// Whether to automatically cancel suspended futures at the next await bound
  /// when the response is dropped.
  pub auto_cancel:    Option<bool>,
}

/// Contains suspended results. Can be turned into a byte stream with a
/// prepended document.
pub struct SuspendedResponse {
  rx:     mpsc::UnboundedReceiver<Html>,
  cancel: CancelOnDrop,
}

impl SuspendedResponse {
  /// Turns the `SuspendedResponse` into a stream for sending as a response.
  #[instrument(name = "columbo::into_stream", skip_all)]
  pub fn into_stream(self, body: impl Into<Html>) -> HtmlStream {
    debug!("converting suspended response into stream");
    HtmlStream::new(self, body.into())
  }
}

/// A suspended future. Can be interpolated into markup as the placeholder.
pub struct Suspense {
  id:          Id,
  placeholder: Html,
}

impl fmt::Debug for Suspense {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Suspense").field("id", &self.id).finish()
  }
}

impl Suspense {
  fn new(id: Id, placeholder: Html) -> Self { Suspense { id, placeholder } }

  /// Render the placeholder HTML.
  pub fn render_to_html(&self) -> Html {
    format::render_placeholder(&self.id, &self.placeholder)
  }
}

impl fmt::Display for Suspense {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.render_to_html().as_str())
  }
}

#[cfg(feature = "maud")]
impl maud::Render for Suspense {
  fn render(&self) -> maud::Markup {
    maud::PreEscaped(self.render_to_html().into_string())
  }
}
