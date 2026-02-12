#![feature(box_into_inner)]

//! Provides SSR suspense capabilities. Render a placeholder for a future, and
//! stream the replacement elements.
//!
//! Called `columbo` because Columbo always said, "And another thing..."

mod format;

use std::{
  any::Any,
  boxed::Box,
  panic::AssertUnwindSafe,
  pin::Pin,
  string::String,
  sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
  },
};

use bytes::Bytes;
use futures::{FutureExt, StreamExt, stream::once};
use maud::{Markup, Render};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{Instrument, Span, debug, instrument, trace, warn};

use self::format::{SuspensePanic, SuspensePlaceholder, SuspenseReplacement};

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
      handle_suspended_future(id, f(self.clone()), self.tx.clone())
        .instrument(tracing::info_span!("columbo::suspended_task",)),
    );

    Suspense::new(id, placeholder)
  }
}

async fn handle_suspended_future<Fut>(
  id: Id,
  future: Fut,
  tx: mpsc::Sender<Markup>,
) where
  Fut: Future<Output = Markup> + Send + 'static,
{
  // run the future to completion
  let result = AssertUnwindSafe(future).catch_unwind().await;

  // determine what to swap in
  let content = match result {
    Ok(m) => m,
    Err(panic_payload) => {
      let error = panic_payload_to_string(&panic_payload);
      warn!(suspense.id = %id, error, "creating error replacement due to panic");
      SuspensePanic { error }.render()
    }
  };

  // render the wrapper and script
  let payload = SuspenseReplacement {
    id:                &id,
    replacement_inner: &content,
  }
  .render();

  let _ = tx.send(payload).await;
}

pub struct SuspendedResponse {
  rx: mpsc::Receiver<Markup>,
}

impl SuspendedResponse {
  /// Turns the `SuspendedResponse` into a stream for sending as a response.
  #[instrument(name = "columbo::into_stream", skip_all)]
  pub fn into_stream(
    self,
    body: Markup,
  ) -> Pin<
    Box<
      dyn futures::Stream<Item = Result<Bytes, std::io::Error>>
        + Send
        + Sync
        + 'static,
    >,
  > {
    debug!("converting suspended response into stream");

    // stream of markup chunks, including initial body
    let markup_stream =
      once(async move { body }).chain(ReceiverStream::new(self.rx));
    let parent_span = Span::current();
    // debugging to note when stream terminates
    let stream_end = once(async move {
        debug!(parent: parent_span, "receiver stream ended - all senders dropped");
        futures::stream::empty()
      })
      .flatten();
    // final stream, adapted to IO convention
    let stream = markup_stream
      .map(|c| Ok(Bytes::from(c.into_string())))
      .chain(stream_end);

    trace!("stream created with initial body chunk");
    Box::pin(stream)
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

fn panic_payload_to_string(payload: &Box<dyn Any + Send>) -> String {
  if let Some(s) = payload.downcast_ref::<&str>() {
    return s.to_string();
  }
  if let Some(s) = payload.downcast_ref::<String>() {
    return s.clone();
  }
  "Box<dyn Any>".to_string()
}
