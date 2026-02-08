//! Provides SSR suspense capabilities. Render a placeholder for a future, and
//! stream the replacement elements.
//!
//! Called `columbo` because Columbo always said, "And another thing..."

use std::{fmt, pin::Pin};

use bytes::Bytes;
use futures::{StreamExt, stream::once};
use tokio::{
  sync::mpsc,
  task::{JoinError, JoinHandle},
};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::{Instrument, Span, debug, error, instrument, trace, warn};

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
  tx: mpsc::UnboundedSender<(Id, JoinHandle<String>)>,
}

impl SuspenseContext {
  /// Suspends a future. The placeholder is sent immediately, and the future
  /// output is streamed and then replaces the placeholder in the browser.
  #[instrument(name = "columbo::suspend", skip_all, fields(suspense.id))]
  pub fn suspend<F, Fut, P>(
    &self,
    future: F,
    placeholder: impl Into<String>,
  ) -> Suspense
  where
    F: FnOnce(SuspenseContext) -> Fut + Send + 'static,
    Fut: Future<Output = P> + Send + 'static,
    P: Send + Into<String>,
  {
    let id = Id::new();
    Span::current().record("suspense.id", id.to_string());
    let placeholder = placeholder.into();

    debug!(
      suspense.id = %id,
      placeholder.length = placeholder.len(),
      "suspending future with placeholder"
    );

    let ctx = self.clone();
    let parent_span = Span::current();

    let handle = tokio::spawn(
      async move {
        let result = future(ctx).await.into();
        trace!(result.length = result.len(), "suspended future completed");
        result
      }
      .instrument(tracing::info_span!(
        parent: parent_span,
        "columbo::suspended_task",
        suspense.id = %id
      )),
    );

    match self.tx.send((id, handle)) {
      Ok(_) => {
        trace!(suspense.id = %id, "sent task handle to channel");
      }
      Err(_) => {
        error!(
          suspense.id = %id,
          "failed to send task handle - receiver dropped"
        );
        panic!("columbo: failed send - receiver was dropped");
      }
    };

    Suspense::new(id, placeholder)
  }
}

pub struct SuspendedResponse {
  rx: mpsc::UnboundedReceiver<(Id, JoinHandle<String>)>,
}

impl SuspendedResponse {
  /// Turns the `SuspendedResponse` into a stream for sending as a response.
  ///
  /// Takes a channel of `(Id, JoinHandle<String>)` and awaits the tasks, then
  /// maps the task result to an HTML replacement chunk, and sends it down the
  /// stream. Prepends the HTML body to the stream.
  #[instrument(
    name = "columbo::into_stream",
    skip(self, body),
    fields(body.length = body.len())
  )]
  pub fn into_stream(
    self,
    body: String,
  ) -> Pin<
    Box<
      dyn futures::Stream<Item = Result<Bytes, std::io::Error>>
        + Send
        + Sync
        + 'static,
    >,
  > {
    debug!(
      body.length = body.len(),
      "converting suspended response into stream"
    );
    let parent_span = Span::current();

    let await_task = move |(id, jh): (Id, JoinHandle<String>)| {
      async move {
        debug!(suspense.id = %id, "awaiting suspended task");
        let result = jh.await;

        match &result {
          Ok(content) => {
            debug!(
              suspense.id = %id,
              content.length = content.len(),
              "suspended task completed successfully"
            );
          }
          Err(e) => {
            error!(
              suspense.id = %id,
              error = %e,
              "suspended task failed to join"
            );
          }
        }

        (id, result)
      }
      .instrument(tracing::debug_span!(
        parent: parent_span.clone(),
        "columbo::await_task",
        suspense.id = %id
      ))
    };

    let html_replacement = move |(id, res): (Id, Result<String, JoinError>)| {
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
          SuspenseJoinError { join_error: e }.to_string()
        }),
      }
      .to_string();

      debug!(
        suspense.id = %id,
        replacement.length = replacement.len(),
        "generated HTML replacement chunk"
      );

      Ok(Bytes::from(replacement))
    };

    let parent_span = Span::current();
    let recv_stream = UnboundedReceiverStream::new(self.rx)
      .then(await_task)
      .map(html_replacement)
      .chain(
        once(async move {
          debug!(parent: parent_span, "receiver stream ended - all senders dropped");
          futures::stream::empty()
        })
        .flatten(),
      );

    trace!("stream created with initial body chunk");
    Box::pin(once(async move { Ok(Bytes::from(body)) }).chain(recv_stream))
  }
}

/// A suspended future. Can be interpolated into strings as the placeholder.
pub struct Suspense {
  id:                Id,
  placeholder_inner: String,
}

impl Suspense {
  fn new(id: Id, placeholder_inner: String) -> Self {
    Suspense {
      id,
      placeholder_inner,
    }
  }

  pub fn id(&self) -> Id { self.id }

  pub fn placeholder_html(&self) -> String { self.to_string() }
}

impl fmt::Display for Suspense {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let formatter = SuspensePlaceholder {
      id:                &self.id,
      placeholder_inner: &self.placeholder_inner,
    };
    formatter.fmt(f)
  }
}

mod format {
  use std::fmt;

  use tokio::task::JoinError;

  use crate::Id;

  pub(crate) struct SuspensePlaceholder<'a> {
    pub id:                &'a Id,
    pub placeholder_inner: &'a str,
  }

  impl<'a> fmt::Display for SuspensePlaceholder<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
      const DISPLAY_CONTENTS_STYLE: &str = r#"{display: contents;}"#;
      write!(
        f,
        r#"<span data-columbo-p-id="{id}" style="{DISPLAY_CONTENTS_STYLE}">{inner}</span>"#,
        id = self.id,
        inner = self.placeholder_inner
      )
    }
  }

  pub(crate) struct SuspenseReplacement<'a> {
    pub id:                &'a Id,
    pub replacement_inner: &'a str,
  }

  impl<'a> fmt::Display for SuspenseReplacement<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
      write!(
        f,
        r#"<template data-columbo-r-id="{id}">{replacement}</template>
         <script>
           (function() {{
             const t = document.querySelector('[data-columbo-p-id="{id}"]');
             const r = document.querySelector('[data-columbo-r-id="{id}"]');
             if (t && r && t.parentNode) {{
               t.parentNode.replaceChild(r.content, t);
             }}
           }})();
         </script>"#,
        id = self.id,
        replacement = self.replacement_inner
      )
    }
  }

  pub(crate) struct SuspenseJoinError {
    pub join_error: JoinError,
  }

  impl fmt::Display for SuspenseJoinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
      write!(
        f,
        r#"<div style="font-family: monospace; padding: 20px; background: #ffe6e6; color: #000; border: 2px solid #c00;">
        <h1 style="color: #c00; font-size: 18px; margin: 0 0 10px 0;">Columbo Task JoinError</h1>
        <p style="margin: 10px 0;">Columbo could not swap in a suspended response because the joining the suspended task failed.</p>
        <h2 style="font-size: 16px; margin: 20px 0 10px 0;">Error:</h2>
        <pre style="background: #f5f5f5; padding: 10px; overflow: auto; border: 1px solid #ccc; font-size: 12px;">{error}</pre>
    </div>"#,
        error = self.join_error
      )
    }
  }
}
