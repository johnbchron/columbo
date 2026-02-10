//! Provides SSR suspense capabilities. Render a placeholder for a future, and
//! stream the replacement elements.
//!
//! Called `columbo` because Columbo always said, "And another thing..."

use std::pin::Pin;

use bytes::Bytes;
use futures::{StreamExt, stream::once};
use maud::{Markup, Render};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{Instrument, Span, debug, instrument, trace, warn};

use self::format::{
  SuspenseJoinError, SuspensePlaceholder, SuspenseReplacement,
};

type Id = ulid::Ulid;

/// Creates a new [`SuspenseContext`] and [`SuspendedResponse`]. The context is
/// for suspending futures, and the response turns into an output stream.
#[instrument(name = "columbo::new", skip_all)]
pub fn new() -> (SuspenseContext, SuspendedResponse) {
  let (tx, rx) = mpsc::channel(16);
  debug!("created new suspense context and response");
  (SuspenseContext { tx }, SuspendedResponse { rx })
}

/// The context with which you can create suspense boundaries for futures.
#[derive(Clone)]
pub struct SuspenseContext {
  tx: mpsc::Sender<Markup>,
}

impl SuspenseContext {
  fn new_id(&self) -> Id { Id::new() }

  /// Suspends a future. The placeholder is sent immediately, and the future
  /// output is streamed and then replaces the placeholder in the browser.
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
  let result = tokio::spawn(future).await;

  let replacement = SuspenseReplacement {
    id:                &id,
    replacement_inner: &result.unwrap_or_else(|e| {
      warn!(
        suspense.id = %id,
        error = %e,
        "creating error replacement due to join failure"
      );
      SuspenseJoinError { join_error: e }.render()
    }),
  }
  .render();

  let _ = tx.send(replacement).await;
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
