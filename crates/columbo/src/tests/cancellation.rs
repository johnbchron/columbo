//! Tests for cancellation behavior.

use std::{
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
  time::Duration,
};

use crate::{ColumboOptions, Html};

#[tokio::test]
async fn manual_cancellation_via_cancelled_method() {
  let (ctx, resp) = crate::new();
  let ctx_clone = ctx.clone();

  let flag = Arc::new(AtomicBool::new(false));
  let flag_clone = flag.clone();

  ctx.suspend(
    async move {
      ctx_clone.cancelled().await;
      flag_clone.store(true, Ordering::SeqCst);
      Html::new("")
    },
    "loading",
  );

  drop(resp);

  tokio::time::sleep(Duration::from_millis(50)).await;

  assert!(
    flag.load(Ordering::SeqCst),
    "cancelled() should have yielded"
  );
}

#[tokio::test]
async fn auto_cancel_option() {
  let completed = Arc::new(AtomicBool::new(false));
  let completed_clone = completed.clone();

  let (ctx, resp) = crate::new_with_opts(ColumboOptions {
    auto_cancel: Some(true),
    ..Default::default()
  });

  ctx.suspend(
    async move {
      tokio::time::sleep(Duration::from_secs(10)).await;
      completed_clone.store(true, Ordering::SeqCst);
      Html::new("")
    },
    "loading",
  );

  tokio::time::sleep(Duration::from_millis(10)).await;

  drop(ctx);
  drop(resp);

  tokio::time::sleep(Duration::from_millis(100)).await;
  assert!(
    !completed.load(Ordering::SeqCst),
    "auto_cancel should prevent the future from completing"
  );
}
