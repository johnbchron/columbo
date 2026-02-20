//! String-based tests that work without the maud feature.
//!
//! These tests use `Html::new()` directly and string interpolation
//! via `format!` and `Display`.

use super::*;

#[tokio::test]
async fn suspend_and_stream() {
  let (ctx, resp) = crate::new();

  let suspense = ctx.suspend(
    async { Html::new("<p>hello from strings</p>") },
    "loading...",
  );

  let document =
    format!("<!DOCTYPE html><html><body><div>{suspense}</div></body></html>");

  drop(ctx);
  let output = collect_stream(resp.into_stream(document)).await;
  let doc = parse(&output);

  let placeholders = select(&doc, "span[data-columbo-p-id]");
  assert_eq!(placeholders.len(), 1, "expected one placeholder");
  assert!(placeholders[0].text().any(|t| t.contains("loading...")));

  let replacements = select(&doc, "template[data-columbo-r-id]");
  assert_eq!(replacements.len(), 1, "expected one replacement");
  assert!(replacements[0].inner_html().contains("hello from strings"));
}

#[tokio::test]
async fn display_impl_renders_placeholder() {
  let (ctx, _resp) = crate::new();

  let suspense = ctx.suspend(async { Html::new("result") }, "my placeholder");

  let rendered = format!("{suspense}");
  let doc = parse(&rendered);

  let placeholders = select(&doc, "span[data-columbo-p-id]");
  assert_eq!(placeholders.len(), 1, "expected one placeholder");
  assert!(placeholders[0].text().any(|t| t.contains("my placeholder")));
}

#[tokio::test]
async fn render_to_html_method() {
  let (ctx, _resp) = crate::new();

  let suspense = ctx.suspend(async { Html::new("") }, "placeholder text");

  let rendered = suspense.render_to_html().into_string();
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
async fn many_concurrent_suspensions() {
  let (ctx, resp) = crate::new();

  for i in 0..20 {
    let marker = format!("item-{i}");
    ctx.suspend(async move { Html::new(marker) }, "...");
  }

  drop(ctx);
  let output = collect_stream(resp.into_stream("")).await;
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

#[tokio::test]
async fn nested_suspense() {
  let (ctx, resp) = crate::new();

  ctx.suspend(
    {
      let ctx = ctx.clone();
      async move {
        ctx.suspend(async { Html::new("nested-child") }, "child-loading");
        Html::new("parent-result")
      }
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
