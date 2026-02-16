//! Integration tests for columbo.
//!
//! Tests are split into string-based tests (always compiled) and maud-based
//! tests (feature-gated behind `#[cfg(feature = "maud")]`).

mod cancellation;
mod edge_cases;
#[cfg(feature = "maud")]
mod maud_integration;
mod panic_handling;
mod placeholders;
mod string_based;

use std::{any::Any, time::Duration};

use futures::{Stream, StreamExt};
use scraper::Selector;

use crate::{ColumboOptions, Html};

/// Collect all chunks from a stream of `Result<Bytes, io::Error>` into a single
/// string.
async fn collect_stream(
  stream: impl Stream<Item = Result<bytes::Bytes, std::io::Error>>,
) -> String {
  tokio::pin!(stream);
  let mut out = String::new();
  while let Some(chunk) = stream.next().await {
    out.push_str(core::str::from_utf8(&chunk.unwrap()).unwrap());
  }
  out
}

/// Parse an HTML string into a scraper document.
fn parse(html: &str) -> scraper::Html { scraper::Html::parse_document(html) }

/// Select all elements matching a CSS selector.
fn select<'a>(
  doc: &'a scraper::Html,
  css: &str,
) -> Vec<scraper::ElementRef<'a>> {
  let sel = Selector::parse(css).unwrap();
  doc.select(&sel).collect()
}
