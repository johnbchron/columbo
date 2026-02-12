#![feature(box_into_inner)]

//! Provides SSR suspense capabilities. Render a placeholder for a future, and
//! stream the replacement elements.
//!
//! Called `columbo` because Columbo always said, "And another thing..."

mod format;
mod markup_stream;
mod run_suspended;

use std::sync::{
  Arc,
  atomic::{AtomicUsize, Ordering},
};

use maud::Markup;
use tokio::sync::mpsc;
use tracing::{Instrument, Span, debug, instrument, warn};

use self::{
  format::SuspensePlaceholder, markup_stream::MarkupStream,
  run_suspended::run_suspended_future,
};

type Id = usize;

/// Creates a new [`SuspenseContext`] and [`SuspendedResponse`]. The context is
/// for suspending futures, and the response turns into an output stream.
#[instrument(name = "columbo::new", skip_all)]
pub fn new() -> (SuspenseContext, SuspendedResponse) {
  let (tx, rx) = mpsc::channel(16);
  debug!("created new suspense context and response");
  (
    SuspenseContext {
      next_id: Arc::new(AtomicUsize::new(0)),
      tx,
    },
    SuspendedResponse { rx },
  )
}

/// The context with which you can create suspense boundaries for futures.
#[derive(Clone)]
pub struct SuspenseContext {
  next_id: Arc<AtomicUsize>,
  tx:      mpsc::Sender<Markup>,
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
  /// Suspended futures must be `Send` because they are handed to `tokio`.
  #[instrument(name = "columbo::suspend", skip_all, fields(suspense.id))]
  pub fn suspend<F, Fut>(&self, f: F, placeholder: Markup) -> Suspense
  where
    F: FnOnce(SuspenseContext) -> Fut,
    Fut: Future<Output = Markup> + Send + 'static,
  {
    let id = self.new_id();
    Span::current().record("suspense.id", id.to_string());

    tokio::spawn(
      run_suspended_future(id, f(self.clone()), self.tx.clone())
        .instrument(tracing::info_span!("columbo::suspended_task",)),
    );

    Suspense::new(id, placeholder)
  }
}

pub struct SuspendedResponse {
  rx: mpsc::Receiver<Markup>,
}

impl SuspendedResponse {
  /// Turns the `SuspendedResponse` into a stream for sending as a response.
  #[instrument(name = "columbo::into_stream", skip_all)]
  pub fn into_stream(self, body: Markup) -> MarkupStream {
    debug!("converting suspended response into stream");
    MarkupStream::new(self, body)
  }
}

/// A suspended future. Can be interpolated into strings as the placeholder.
pub struct Suspense {
  id:                Id,
  placeholder_inner: Markup,
}

impl Suspense {
  fn new(id: Id, placeholder_inner: Markup) -> Self {
    Suspense {
      id,
      placeholder_inner,
    }
  }
}

impl maud::Render for Suspense {
  fn render(&self) -> maud::Markup {
    SuspensePlaceholder {
      id:                &self.id,
      placeholder_inner: &self.placeholder_inner,
    }
    .render()
  }
}
