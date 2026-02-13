use std::{panic::AssertUnwindSafe, sync::Arc};

use futures::FutureExt;
use maud::{Markup, Render};
use tokio::sync::mpsc;
use tracing::{trace, warn};

use crate::{ColumboOptions, Id, format::SuspenseReplacement};

pub(crate) async fn run_suspended_future<Fut>(
  id: Id,
  future: Fut,
  tx: mpsc::UnboundedSender<Markup>,
  opts: Arc<ColumboOptions>,
) where
  Fut: Future<Output = Markup> + Send + 'static,
{
  // run the future to completion
  let result = AssertUnwindSafe(future).catch_unwind().await;

  // determine what to swap in
  let content = match result {
    Ok(m) => m,
    Err(panic_payload) => {
      warn!(suspense.id = %id, "suspended task panicked; rendering panic");
      opts
        .panic_renderer
        .unwrap_or(crate::format::default_panic_renderer)(panic_payload)
    }
  };

  // render the wrapper and script
  let payload = SuspenseReplacement {
    id:    &id,
    inner: &content,
  }
  .render();

  let _ = tx.send(payload).inspect_err(|_| {
    trace!(suspense.id = %id, "future completed but receiver is dropped");
  });
}
