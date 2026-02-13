use std::{any::Any, panic::AssertUnwindSafe};

use futures::FutureExt;
use maud::{Markup, Render};
use tokio::sync::mpsc;
use tracing::warn;

use crate::{
  Id,
  format::{SuspensePanic, SuspenseReplacement},
};

pub(crate) async fn run_suspended_future<Fut>(
  id: Id,
  future: Fut,
  tx: mpsc::UnboundedSender<Markup>,
) where
  Fut: Future<Output = Markup> + Send + 'static,
{
  // run the future to completion
  let result = AssertUnwindSafe(future).catch_unwind().await;

  // determine what to swap in
  let content = match result {
    Ok(m) => m,
    Err(panic_payload) => {
      let error = panic_payload_to_string(&panic_payload);
      warn!(suspense.id = %id, error, "creating error replacement due to panic");
      SuspensePanic { error }.render()
    }
  };

  // render the wrapper and script
  let payload = SuspenseReplacement {
    id:    &id,
    inner: &content,
  }
  .render();

  // send, ignoring if receiver is closed
  let _ = tx.send(payload);
}

fn panic_payload_to_string(payload: &Box<dyn Any + Send>) -> String {
  if let Some(s) = payload.downcast_ref::<&str>() {
    return s.to_string();
  }
  if let Some(s) = payload.downcast_ref::<String>() {
    return s.clone();
  }
  "Box<dyn Any>".to_string()
}
