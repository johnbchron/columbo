//! Provides SSR suspense capabilities. Render a placeholder for a future, and
//! stream the replacement elements.
//!
//! Called `columbo` because Columbo always said, "And another thing..."

use std::pin::Pin;

use bytes::Bytes;
use futures::{StreamExt, stream::once};
use maud::{Markup, Render};
use tokio::{
  sync::mpsc,
  task::{JoinError, JoinHandle},
};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::{Instrument, Span, debug, instrument, trace, warn};

use self::format::{
  SuspenseJoinError, SuspensePlaceholder, SuspenseReplacement,
};

type Id = ulid::Ulid;

/// Creates a new [`SuspenseContext`] and [`SuspendedResponse`]. The context is
/// for suspending futures, and the response turns into an output stream.
#[instrument(name = "columbo::new", skip_all)]
pub fn new() -> (SuspenseContext, SuspendedResponse) {
  let (tx, rx) = mpsc::unbounded_channel();
  debug!("created new suspense context and response");
  (SuspenseContext { tx }, SuspendedResponse { rx })
}

/// The context with which you can create suspense boundaries for futures.
#[derive(Clone)]
pub struct SuspenseContext {
  tx: mpsc::UnboundedSender<(Id, JoinHandle<Markup>)>,
}

impl SuspenseContext {
  /// Suspends a future. The placeholder is sent immediately, and the future
  /// output is streamed and then replaces the placeholder in the browser.
  #[instrument(name = "columbo::suspend", skip_all, fields(suspense.id))]
  pub fn suspend<F, Fut>(&self, f: F, placeholder: Markup) -> Suspense
  where
    F: FnOnce(SuspenseContext) -> Fut,
    Fut: Future<Output = Markup> + Send + 'static,
  {
    let id = Id::new();
    Span::current().record("suspense.id", id.to_string());

    let ctx = self.clone();
    let future = f(ctx);

    let parent_span = Span::current();
    let handle = tokio::spawn(
      async move {
        let result = future.await;
        trace!("suspended future completed");
        result
      }
      .instrument(tracing::info_span!(
        parent: parent_span,
        "columbo::suspended_task",
        suspense.id = %id
      )),
    );

    self
      .tx
      .send((id, handle))
      .expect("columbo: failed send - receiver was dropped");

    Suspense::new(id, placeholder)
  }
}

pub struct SuspendedResponse {
  rx: mpsc::UnboundedReceiver<(Id, JoinHandle<Markup>)>,
}

impl SuspendedResponse {
  /// Turns the `SuspendedResponse` into a stream for sending as a response.
  ///
  /// Takes a channel of `(Id, JoinHandle<String>)` and awaits the tasks, then
  /// maps the task result to an HTML replacement chunk, and sends it down the
  /// stream. Prepends the HTML body to the stream.
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
    let parent_span = Span::current();

    let await_task = move |(id, jh): (Id, JoinHandle<Markup>)| {
      async move {
        debug!(suspense.id = %id, "awaiting suspended task");
        let result = jh.await;
        debug!(suspense.id = %id, "suspended task completed");

        (id, result)
      }
      .instrument(tracing::debug_span!(
        parent: parent_span.clone(),
        "columbo::await_task",
        suspense.id = %id
      ))
    };

    let html_replacement = move |(id, res): (Id, Result<Markup, JoinError>)| {
      let span = tracing::debug_span!(
        "columbo::create_replacement",
        suspense.id = %id
      );
      let _enter = span.enter();

      let replacement = SuspenseReplacement {
        id:                &id,
        replacement_inner: &res.unwrap_or_else(|e| {
          warn!(
            suspense.id = %id,
            error = %e,
            "creating error replacement due to join failure"
          );
          SuspenseJoinError { join_error: e }.render()
        }),
      }
      .render();

      debug!(suspense.id = %id, "generated HTML replacement chunk");

      replacement
    };

    // stream of join handles and IDs, buffered out of order
    let task_result_stream = UnboundedReceiverStream::new(self.rx)
      .map(await_task)
      .buffer_unordered(8);
    // stream of markup chunks, including initial body
    let markup_stream =
      once(async move { body }).chain(task_result_stream.map(html_replacement));
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

mod format {
  use maud::{Markup, PreEscaped, Render, html};
  use tokio::task::JoinError;

  use crate::Id;

  pub(crate) struct SuspensePlaceholder<'a> {
    pub id:                &'a Id,
    pub placeholder_inner: &'a Markup,
  }

  impl<'a> Render for SuspensePlaceholder<'a> {
    fn render(&self) -> Markup {
      html! {
        span
          data-columbo-p-id=(self.id)
          style="display: contents;"
        {
          (self.placeholder_inner)
        }
      }
    }
  }

  pub(crate) struct SuspenseReplacement<'a> {
    pub id:                &'a Id,
    pub replacement_inner: &'a Markup,
  }

  impl<'a> Render for SuspenseReplacement<'a> {
    fn render(&self) -> Markup {
      let script = format!(
        r#"(function() {{
          const t = document.querySelector('[data-columbo-p-id="{id}"]');
          const r = document.querySelector('[data-columbo-r-id="{id}"]');
          if (t && r && t.parentNode) {{
            t.parentNode.replaceChild(r.content, t);
          }}
        }})();"#,
        id = self.id
      );

      html! {
        template data-columbo-r-id=(self.id) {
          (self.replacement_inner)
        }
        script {
          (PreEscaped(script))
        }
      }
    }
  }

  pub(crate) struct SuspenseJoinError {
    pub join_error: JoinError,
  }

  impl Render for SuspenseJoinError {
    fn render(&self) -> Markup {
      html! {
        div style="font-family: monospace; padding: 20px; background: #ffe6e6; color: #000; border: 2px solid #c00;" {
          h1 style="color: #c00; font-size: 18px; margin: 0 0 10px 0;" {
            "Columbo Task JoinError"
          }
          p style="margin: 10px 0;" {
            "Columbo could not swap in a suspended response because the joining the suspended task failed."
          }
          h2 style="font-size: 16px; margin: 20px 0 10px 0;" {
            "Error:"
          }
          pre style="background: #f5f5f5; padding: 10px; overflow: auto; border: 1px solid #ccc; font-size: 12px;" {
            (self.join_error)
          }
        }
      }
    }
  }
}
