use std::{
  any::Any,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
  time::Duration,
};

use columbo::ColumboOptions;
use futures::{Stream, StreamExt};
use maud::{Markup, Render, html};
use scraper::{Html, Selector};

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
fn parse(html: &str) -> Html { Html::parse_document(html) }

/// Select all elements matching a CSS selector.
fn select<'a>(doc: &'a Html, css: &str) -> Vec<scraper::ElementRef<'a>> {
  let sel = Selector::parse(css).unwrap();
  doc.select(&sel).collect()
}

// ---------------------------------------------------------------------------
// Basic flow
// ---------------------------------------------------------------------------

#[tokio::test]
async fn basic_suspend_and_stream() {
  let (ctx, resp) = columbo::new();

  let suspense = ctx.suspend(
    |_ctx| async {
      html! { p { "hello world" } }
    },
    html! { "loading..." },
  );

  let doc = html! { div { (suspense) } };

  // Drop context so the channel closes after the spawned task finishes
  drop(ctx);
  let output = collect_stream(resp.into_stream(doc)).await;
  let doc = parse(&output);

  // The document body should contain the placeholder span
  let placeholders = select(&doc, "span[data-columbo-p-id]");
  assert_eq!(placeholders.len(), 1, "expected one placeholder");
  assert!(placeholders[0].text().any(|t| t.contains("loading...")));

  // The streamed replacement should appear
  let replacements = select(&doc, "template[data-columbo-r-id]");
  assert_eq!(replacements.len(), 1, "expected one replacement");
  assert!(replacements[0].inner_html().contains("hello world"));

  // The global script should be injected
  assert!(!select(&doc, "script").is_empty(), "expected script tag");
}

// ---------------------------------------------------------------------------
// Placeholder rendering
// ---------------------------------------------------------------------------

#[tokio::test]
async fn placeholder_renders_with_display_contents() {
  let (ctx, _resp) = columbo::new();

  let suspense = ctx.suspend(
    |_ctx| async {
      html! {}
    },
    html! { "placeholder text" },
  );

  let rendered = suspense.render().into_string();
  let doc = parse(&rendered);

  let placeholders = select(&doc, "span[data-columbo-p-id]");
  assert_eq!(placeholders.len(), 1, "expected one placeholder");
  assert_eq!(
    placeholders[0].value().attr("style"),
    Some("display: contents;")
  );
  assert!(
    placeholders[0]
      .text()
      .any(|t| t.contains("placeholder text"))
  );
}

#[tokio::test]
async fn placeholder_and_replacement_ids_match() {
  let (ctx, resp) = columbo::new();

  let suspense = ctx.suspend(
    |_| async {
      html! { "done" }
    },
    html! { "wait" },
  );

  let doc = html! { (suspense) };
  drop(ctx);
  let output = collect_stream(resp.into_stream(doc)).await;
  let doc = parse(&output);

  let placeholders = select(&doc, "span[data-columbo-p-id]");
  let replacements = select(&doc, "template[data-columbo-r-id]");
  assert_eq!(placeholders.len(), 1, "expected one placeholder");
  assert_eq!(replacements.len(), 1, "expected one replacement");

  let p_id = placeholders[0].value().attr("data-columbo-p-id").unwrap();
  let r_id = replacements[0].value().attr("data-columbo-r-id").unwrap();
  assert_eq!(p_id, r_id, "placeholder and replacement IDs should match");
}

// ---------------------------------------------------------------------------
// Streaming behavior
// ---------------------------------------------------------------------------

#[tokio::test]
async fn results_stream_in_completion_order() {
  let (ctx, resp) = columbo::new();

  // Second suspension completes faster
  ctx.suspend(
    |_| async {
      tokio::time::sleep(Duration::from_millis(100)).await;
      html! { "slow" }
    },
    html! { "..." },
  );
  ctx.suspend(
    |_| async {
      tokio::time::sleep(Duration::from_millis(10)).await;
      html! { "fast" }
    },
    html! { "..." },
  );

  drop(ctx);
  let doc = html! { "body" };
  let output = collect_stream(resp.into_stream(doc)).await;
  let doc = parse(&output);

  // Templates appear in the document in the order they were streamed
  let replacements = select(&doc, "template[data-columbo-r-id]");
  let inners: Vec<String> =
    replacements.iter().map(|el| el.inner_html()).collect();
  let fast_idx = inners.iter().position(|h| h.contains("fast")).unwrap();
  let slow_idx = inners.iter().position(|h| h.contains("slow")).unwrap();
  assert!(
    fast_idx < slow_idx,
    "expected 'fast' ({fast_idx}) before 'slow' ({slow_idx})"
  );
}

#[tokio::test]
async fn stream_first_chunk_is_document() {
  let (ctx, resp) = columbo::new();

  ctx.suspend(
    |_| async {
      tokio::time::sleep(Duration::from_millis(50)).await;
      html! { "delayed" }
    },
    html! { "..." },
  );

  drop(ctx);
  let doc = html! { h1 { "First Chunk" } };
  let stream = resp.into_stream(doc);
  tokio::pin!(stream);

  // First chunk should contain the document body
  let first = stream.next().await.unwrap().unwrap();
  let first_str = String::from_utf8(first.to_vec()).unwrap();
  let doc = parse(&first_str);

  let headings = select(&doc, "h1");
  assert!(
    !headings.is_empty()
      && headings[0].text().any(|t| t.contains("First Chunk")),
    "first chunk should be the document body"
  );
}

#[tokio::test]
async fn global_script_injected_once() {
  let (ctx, resp) = columbo::new();

  ctx.suspend(
    |_| async {
      html! { "a" }
    },
    html! { "..." },
  );
  ctx.suspend(
    |_| async {
      html! { "b" }
    },
    html! { "..." },
  );

  drop(ctx);
  let output = collect_stream(resp.into_stream(html! {})).await;
  let doc = parse(&output);

  let scripts = select(&doc, "script");
  assert_eq!(
    scripts.len(),
    1,
    "global script should be injected exactly once"
  );
}

// ---------------------------------------------------------------------------
// Nested suspense
// ---------------------------------------------------------------------------

#[tokio::test]
async fn nested_suspense() {
  let (ctx, resp) = columbo::new();

  ctx.suspend(
    |inner_ctx| async move {
      // Spawn a child suspension from within the parent
      inner_ctx.suspend(
        |_| async {
          html! { "nested-child" }
        },
        html! { "child-loading" },
      );
      html! { "parent-result" }
    },
    html! { "parent-loading" },
  );

  drop(ctx);
  let doc = html! { "body" };
  let output = collect_stream(resp.into_stream(doc)).await;
  let doc = parse(&output);

  let replacements = select(&doc, "template[data-columbo-r-id]");
  let all_inner: String =
    replacements.iter().map(|el| el.inner_html()).collect();
  assert!(all_inner.contains("parent-result"), "missing parent-result");
  assert!(all_inner.contains("nested-child"), "missing nested-child");
}

// ---------------------------------------------------------------------------
// Panic handling
// ---------------------------------------------------------------------------

#[tokio::test]
async fn panic_renders_default_error() {
  let (ctx, resp) = columbo::new();

  ctx.suspend(
    |_| async { panic!("something went wrong") },
    html! { "loading" },
  );

  drop(ctx);
  let doc = html! { "body" };
  let output = collect_stream(resp.into_stream(doc)).await;
  let doc = parse(&output);

  let replacements = select(&doc, "template[data-columbo-r-id]");
  assert!(!replacements.is_empty(), "expected replacement template");
  let inner: String = replacements.iter().map(|el| el.inner_html()).collect();
  assert!(
    inner.contains("Columbo Suspense Panic"),
    "expected default panic heading"
  );
}

#[tokio::test]
async fn panic_with_custom_renderer() {
  fn custom_renderer(_: Box<dyn Any + Send>) -> Markup {
    html! { div.custom-error { "custom error fallback" } }
  }

  let (ctx, resp) = columbo::new_with_opts(ColumboOptions {
    panic_renderer: Some(custom_renderer),
    ..Default::default()
  });

  ctx.suspend(|_| async { panic!("boom") }, html! { "loading" });

  drop(ctx);
  let doc = html! { "body" };
  let output = collect_stream(resp.into_stream(doc)).await;
  let doc = parse(&output);

  let replacements = select(&doc, "template[data-columbo-r-id]");
  assert!(!replacements.is_empty(), "expected replacement template");
  let inner: String = replacements.iter().map(|el| el.inner_html()).collect();
  assert!(
    inner.contains("custom error fallback"),
    "expected custom error content"
  );
  // Should NOT contain the default panic renderer
  assert!(
    !inner.contains("Columbo Suspense Panic"),
    "should not contain default panic heading"
  );
}

#[tokio::test]
async fn panic_with_non_string_payload() {
  let (ctx, resp) = columbo::new();

  ctx.suspend(
    |_| async { std::panic::panic_any(42u32) },
    html! { "loading" },
  );

  drop(ctx);
  let doc = html! { "body" };
  let output = collect_stream(resp.into_stream(doc)).await;
  let doc = parse(&output);

  let replacements = select(&doc, "template[data-columbo-r-id]");
  let inner: String = replacements.iter().map(|el| el.inner_html()).collect();
  assert!(
    inner.contains("Columbo Suspense Panic"),
    "expected default panic heading"
  );
  // scraper decodes HTML entities, so the inner_html may contain the raw
  // form; check for both the decoded and encoded representations
  assert!(
    inner.contains("Box<dyn Any>") || inner.contains("Box&lt;dyn Any&gt;"),
    "expected non-string panic fallback message"
  );
}

// ---------------------------------------------------------------------------
// Cancellation
// ---------------------------------------------------------------------------

#[tokio::test]
async fn is_cancelled_reflects_drop() {
  let (ctx, resp) = columbo::new();
  let ctx_clone = ctx.clone();

  let flag = Arc::new(AtomicBool::new(false));
  let flag_clone = flag.clone();

  ctx.suspend(
    move |_| async move {
      // Wait for cancellation signal
      ctx_clone.cancelled().await;
      flag_clone.store(true, Ordering::SeqCst);
      html! {}
    },
    html! { "loading" },
  );

  // Don't consume the stream -- drop it to signal cancellation
  drop(resp);

  // Give the task a moment to observe cancellation
  tokio::time::sleep(Duration::from_millis(50)).await;

  assert!(
    flag.load(Ordering::SeqCst),
    "cancelled() should have yielded"
  );
}

#[tokio::test]
async fn auto_cancel_stops_futures() {
  let completed = Arc::new(AtomicBool::new(false));
  let completed_clone = completed.clone();

  let (ctx, resp) = columbo::new_with_opts(ColumboOptions {
    auto_cancel: Some(true),
    ..Default::default()
  });

  ctx.suspend(
    move |_| async move {
      // This sleep should be interrupted by auto_cancel
      tokio::time::sleep(Duration::from_secs(10)).await;
      completed_clone.store(true, Ordering::SeqCst);
      html! {}
    },
    html! { "loading" },
  );

  // Give the task a moment to start
  tokio::time::sleep(Duration::from_millis(10)).await;

  // Drop the response to trigger cancellation
  drop(ctx);
  drop(resp);

  // Wait a bit and verify the future did NOT run to completion
  tokio::time::sleep(Duration::from_millis(100)).await;
  assert!(
    !completed.load(Ordering::SeqCst),
    "auto_cancel should prevent the future from completing"
  );
}

// ---------------------------------------------------------------------------
// Edge cases
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stream_works_with_no_suspensions() {
  let (ctx, resp) = columbo::new();

  // Drop the context so the channel closes
  drop(ctx);

  let doc = html! { p { "just a page" } };
  let output = collect_stream(resp.into_stream(doc)).await;
  let doc = parse(&output);

  let paragraphs = select(&doc, "p");
  assert!(!paragraphs.is_empty(), "expected a paragraph element");
  assert!(paragraphs[0].text().any(|t| t.contains("just a page")));
}

#[tokio::test]
async fn many_concurrent_suspensions() {
  let (ctx, resp) = columbo::new();

  for i in 0..20 {
    let marker = format!("item-{i}");
    ctx.suspend(
      move |_| async move {
        html! { (marker) }
      },
      html! { "..." },
    );
  }

  drop(ctx);
  let output = collect_stream(resp.into_stream(html! {})).await;
  let doc = parse(&output);

  let replacements = select(&doc, "template[data-columbo-r-id]");
  assert_eq!(replacements.len(), 20, "expected 20 replacements");
  let all_inner: String =
    replacements.iter().map(|el| el.inner_html()).collect();
  for i in 0..20 {
    assert!(
      all_inner.contains(&format!("item-{i}")),
      "missing item-{i} in output"
    );
  }
}
