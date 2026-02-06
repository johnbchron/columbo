//! Provides SSR suspense capabilities. Render a placeholder for a future, and
//! stream the replacement elements.
//!
//! Called `columbo` because Columbo always said, "And another thing..."

use std::{collections::HashMap, fmt, pin::Pin, sync::Mutex};

use bytes::Bytes;
use futures::StreamExt;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::ReceiverStream;

use self::format::{
  SuspenseJoinError, SuspensePlaceholder, SuspenseReplacement,
};

type Id = ulid::Ulid;

/// The context with which you can create suspense boundaries for futures.
#[derive(Default)]
pub struct SuspenseContext {
  map: Mutex<HashMap<Id, JoinHandle<String>>>,
}

impl SuspenseContext {
  pub fn new() -> Self {
    SuspenseContext {
      map: Mutex::new(HashMap::new()),
    }
  }

  /// Suspends a future. The placeholder is sent immediately, and the future
  /// output is streamed and then replaces the placeholder in the browser.
  pub fn suspend<F>(&self, future: F, placeholder_inner: String) -> Suspense
  where
    F: Future<Output = String> + Send + 'static,
  {
    let id = Id::new();
    let handle = tokio::spawn(future);
    {
      let mut lock = self.map.lock().expect("columbo mutex was poisoned");
      lock.insert(id, handle);
    }

    Suspense::new(id, placeholder_inner)
  }

  /// Turns the context into a stream for sending as a response.
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
    // start the stream with the main body
    let stream = futures::stream::once(async move { Ok(Bytes::from(body)) });

    let map = self.map.into_inner().expect("columbo mutex was poisoned");
    // return early if no suspense
    if map.is_empty() {
      return Box::pin(stream);
    }

    let (tx, rx) = tokio::sync::mpsc::channel(16);
    let stream =
      stream.chain(ReceiverStream::new(rx).map(|s| Ok(Bytes::from(s))));

    // send the output of each suspense as it joins
    for (id, handle) in map {
      let tx = tx.clone();
      tokio::spawn(async move {
        // get the replacement content, or an error if JoinError
        let replacement_content = match handle.await {
          Ok(c) => c,
          Err(e) => SuspenseJoinError { join_error: e }.to_string(),
        };
        let content = SuspenseReplacement {
          id:                &id,
          replacement_inner: &replacement_content,
        }
        .to_string();

        let _ = tx.send(content).await;
      });
    }

    Box::pin(stream)
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
        r#"<div data-columbo-p-id="{id}" style="{DISPLAY_CONTENTS_STYLE}">{inner}</div>"#,
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
             if (t && r) {{
               t.replaceWith(...r.content.childNodes);
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
