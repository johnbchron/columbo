//! Tests for placeholder rendering and ID matching.

use super::*;

#[tokio::test]
async fn ids_match_between_placeholder_and_replacement() {
  let (ctx, resp) = crate::new();

  let suspense = ctx.suspend(async { Html::new("done") }, "wait");

  let document = format!("{suspense}");
  drop(ctx);
  let output = collect_stream(resp.into_stream(document)).await;
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
async fn global_script_injected_once() {
  let (ctx, resp) = crate::new();

  ctx.suspend(async { Html::new("a") }, "...");
  ctx.suspend(async { Html::new("b") }, "...");

  drop(ctx);
  let output = collect_stream(resp.into_stream("")).await;
  let doc = parse(&output);

  let scripts = select(&doc, "script");
  assert_eq!(
    scripts.len(),
    1,
    "global script should be injected exactly once"
  );
}
