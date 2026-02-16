//! Tests that use the maud feature.
//!
//! These tests verify that `maud::Markup` integrates seamlessly with
//! columbo via the `Into<Html>` trait.

use maud::html;

use super::*;

#[tokio::test]
async fn basic_suspend_and_stream() {
  let (ctx, resp) = crate::new();

  let suspense = ctx.suspend(
    |_ctx| async {
      html! { p { "hello world" } }
    },
    html! { "loading..." },
  );

  let doc = html! { div { (suspense) } };

  drop(ctx);
  let output = collect_stream(resp.into_stream(doc)).await;
  let doc = parse(&output);

  let placeholders = select(&doc, "span[data-columbo-p-id]");
  assert_eq!(placeholders.len(), 1, "expected one placeholder");
  assert!(placeholders[0].text().any(|t| t.contains("loading...")));

  let replacements = select(&doc, "template[data-columbo-r-id]");
  assert_eq!(replacements.len(), 1, "expected one replacement");
  assert!(replacements[0].inner_html().contains("hello world"));

  assert!(!select(&doc, "script").is_empty(), "expected script tag");
}

#[tokio::test]
async fn maud_render_impl_on_suspense() {
  let (ctx, resp) = crate::new();

  let suspense = ctx.suspend(
    |_| async {
      html! { "done" }
    },
    html! { "wait" },
  );

  // Suspense implements maud::Render, so it can be interpolated into html!
  let doc = html! { (suspense) };
  drop(ctx);
  let output = collect_stream(resp.into_stream(doc)).await;
  let doc = parse(&output);

  let placeholders = select(&doc, "span[data-columbo-p-id]");
  let replacements = select(&doc, "template[data-columbo-r-id]");
  assert_eq!(placeholders.len(), 1);
  assert_eq!(replacements.len(), 1);

  let p_id = placeholders[0].value().attr("data-columbo-p-id").unwrap();
  let r_id = replacements[0].value().attr("data-columbo-r-id").unwrap();
  assert_eq!(p_id, r_id, "placeholder and replacement IDs should match");
}

#[tokio::test]
async fn streaming_completion_order() {
  let (ctx, resp) = crate::new();

  ctx.suspend(
    |_| async {
      tokio::time::sleep(Duration::from_millis(100)).await;
      html! { "slow" }
    },
    "...",
  );
  ctx.suspend(
    |_| async {
      tokio::time::sleep(Duration::from_millis(10)).await;
      html! { "fast" }
    },
    "...",
  );

  drop(ctx);
  let output = collect_stream(resp.into_stream("body")).await;
  let doc = parse(&output);

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
async fn first_chunk_is_document() {
  let (ctx, resp) = crate::new();

  ctx.suspend(
    |_| async {
      tokio::time::sleep(Duration::from_millis(50)).await;
      html! { "delayed" }
    },
    "...",
  );

  drop(ctx);
  let doc = html! { h1 { "First Chunk" } };
  let stream = resp.into_stream(doc);
  tokio::pin!(stream);

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
async fn nested_suspense() {
  let (ctx, resp) = crate::new();

  ctx.suspend(
    |inner_ctx| async move {
      inner_ctx.suspend(
        |_| async {
          html! { "nested-child" }
        },
        "child-loading",
      );
      html! { "parent-result" }
    },
    "parent-loading",
  );

  drop(ctx);
  let output = collect_stream(resp.into_stream("body")).await;
  let doc = parse(&output);

  let replacements = select(&doc, "template[data-columbo-r-id]");
  let all_inner: String =
    replacements.iter().map(|el| el.inner_html()).collect();
  assert!(all_inner.contains("parent-result"), "missing parent-result");
  assert!(all_inner.contains("nested-child"), "missing nested-child");
}

#[tokio::test]
async fn custom_panic_renderer() {
  fn custom_renderer(_: Box<dyn Any + Send>) -> Html {
    Html::new(r#"<div class="custom-error">custom error fallback</div>"#)
  }

  let (ctx, resp) = crate::new_with_opts(ColumboOptions {
    panic_renderer: Some(custom_renderer),
    ..Default::default()
  });

  ctx.suspend(
    |_| async {
      panic!("boom");
      #[allow(unreachable_code)]
      Html::new("")
    },
    "loading",
  );

  drop(ctx);
  let output = collect_stream(resp.into_stream("body")).await;
  let doc = parse(&output);

  let replacements = select(&doc, "template[data-columbo-r-id]");
  assert!(!replacements.is_empty(), "expected replacement template");
  let inner: String = replacements.iter().map(|el| el.inner_html()).collect();
  assert!(
    inner.contains("custom error fallback"),
    "expected custom error content"
  );
  assert!(
    !inner.contains("Columbo Suspense Panic"),
    "should not contain default panic heading"
  );
}
