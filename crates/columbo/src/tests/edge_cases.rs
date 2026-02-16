//! Edge case tests.

use super::*;

#[tokio::test]
async fn no_suspensions() {
  let (ctx, resp) = crate::new();

  drop(ctx);

  let output = collect_stream(resp.into_stream("<p>just a page</p>")).await;
  let doc = parse(&output);

  let paragraphs = select(&doc, "p");
  assert!(!paragraphs.is_empty(), "expected a paragraph element");
  assert!(paragraphs[0].text().any(|t| t.contains("just a page")));
}
