#![feature(box_into_inner)]

//! Provides SSR suspense capabilities. Render a placeholder for a future, and
//! stream the replacement elements.
//!
//! Called `columbo` because Columbo always said, "And another thing..."
//!
//! > For the purposes of this library, the verb `suspend` generally means
//! > "defer the rendering and sending of an async workload", in the context of
//! > rendering a web document.
//!
//! # Overview
//! The entrypoint for the library is the [`new()`] function, which returns a
//! [`SuspenseContext`] and a [`SuspendedResponse`]. The [`SuspenseContext`]
//! allows you to [`suspend()`](SuspenseContext::suspend) futures to be sent
//! down the stream when they are completed, wrapped with just enough HTML to be
//! interpolated into wherever the resulting [`Suspense`] struct was rendered
//! into the document as a placeholder. [`SuspendedResponse`] acts as a receiver
//! for these suspended results. When done rendering your document, pass it your
//! document and call [`into_stream()`](SuspendedResponse::into_stream) to get
//! seamless SSR streaming suspense.
//!
//! So in summary:
//! - Use [`SuspenseContext`] to call [`suspend()`](SuspenseContext::suspend)
//!   and suspend futures.
//! - Call [`into_stream()`](SuspendedResponse::into_stream) to setup your
//!   response stream.
//!
//! The [`suspend()`](SuspenseContext::suspend) function provides access to
//! itself for the futures it suspends by taking a closure returning a future,
//! so futures can spawn additional suspensions.
//!
//! # Axum Example
//!
//! ```rust
//! use axum::{
//!   body::Body,
//!   response::{IntoResponse, Response},
//! };
//!
//! async fn handler() -> impl IntoResponse {
//!   // columbo entrypoint
//!   let (ctx, resp) = columbo::new();
//!
//!   // suspend a future, providing a future and a placeholder
//!   let suspense = ctx.suspend(
//!     // takes a closure that returns a future, allowing nested suspense
//!     |_ctx| async move {
//!       tokio::time::sleep(std::time::Duration::from_secs(2)).await;
//!
//!       // the future's output is markup
//!       maud::html! {
//!         p { "Good things come to those who wait." }
//!       }
//!     },
//!     // placeholder replaced when result is streamed
//!     maud::html! { "Loading..." },
//!   );
//!
//!   // directly interpolate the suspense into the document
//!   let document = maud::html! {
//!     (maud::DOCTYPE)
//!     html {
//!       head;
//!       body {
//!         p { "Aphorism incoming..." }
//!         (suspense)
//!       }
//!     }
//!   };
//!
//!   // produce a body stream with the document and suspended results
//!   let stream = resp.into_stream(document);
//!   let body = Body::from_stream(stream);
//!   Response::builder()
//!     .header("Content-Type", "text/html; charset=utf-8")
//!     .header("Transfer-Encoding", "chunked")
//!     .body(body)
//!     .unwrap()
//! }
//! ```
//!
//! # Architecture
//!
//! Internally, [`SuspenseContext`] holds a channel sender. When
//! [`suspend()`](SuspenseContext::suspend) is called, it launches a task which
//! runs the given future to completion. The result of this future (or a panic
//! message if it panicked) is wrapped in a `<template>` tag and given an
//! accompanying `<script>` to put it in the right place. All the resulting
//! markup is sent as a message to the channel.
//!
//! [`SuspendedResponse`] contains a receiver. It just sits around until you
//! call [`into_stream()`](SuspendedResponse::into_stream), at which point the
//! receiver is turned into a stream whose elements are preceeded by the
//! document you provide.

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

/// Contains suspended results. Can be turned into a byte stream with a
/// prepended document.
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

/// A suspended future. Can be interpolated into markup as the placeholder.
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
