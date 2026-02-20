//! Tests for panic handling in suspended futures.

use super::*;

#[tokio::test]
async fn default_panic_renderer() {
  let (ctx, resp) = crate::new();

  ctx.suspend(
    async {
      panic!("something went wrong");
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
    inner.contains("Columbo Suspense Panic"),
    "expected default panic heading"
  );
}

#[tokio::test]
async fn non_string_panic_payload() {
  let (ctx, resp) = crate::new();

  ctx.suspend(
    async {
      std::panic::panic_any(42u32);
      #[allow(unreachable_code)]
      Html::new("")
    },
    "loading",
  );

  drop(ctx);
  let output = collect_stream(resp.into_stream("body")).await;
  let doc = parse(&output);

  let replacements = select(&doc, "template[data-columbo-r-id]");
  let inner: String = replacements.iter().map(|el| el.inner_html()).collect();
  assert!(
    inner.contains("Columbo Suspense Panic"),
    "expected default panic heading"
  );
  assert!(
    inner.contains("Box<dyn Any>") || inner.contains("Box&lt;dyn Any&gt;"),
    "expected non-string panic fallback message"
  );
}
