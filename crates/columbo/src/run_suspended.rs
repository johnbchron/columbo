use std::{panic::AssertUnwindSafe, sync::Arc};

use futures::FutureExt;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{trace, warn};

use crate::{ColumboOptions, Html, Id, format};

pub(crate) async fn run_suspended_future<Fut, M>(
  id: Id,
  future: Fut,
  tx: mpsc::UnboundedSender<Html>,
  opts: Arc<ColumboOptions>,
  cancel: CancellationToken,
) where
  Fut: Future<Output = M> + Send + 'static,
  M: Into<Html>,
{
  let auto_cancel = opts.auto_cancel.unwrap_or(false);

  // catch panics in future
  let future = AssertUnwindSafe(future).catch_unwind();
  // race the future against the cancellation token
  let result = if auto_cancel {
    tokio::select! {
      _ = cancel.cancelled() => {
        trace!(suspense.id = %id, "task exited via auto_cancel");
        return; // exit immediately; nothing to send
      }
      result = future => result,
    }
  } else {
    future.await
  };

  // determine what to swap in
  let content: Html = match result {
    Ok(m) => m.into(),
    Err(panic_payload) => {
      warn!(suspense.id = %id, "suspended task panicked; rendering panic");
      opts
        .panic_renderer
        .unwrap_or(crate::format::default_panic_renderer)(panic_payload)
    }
  };

  // render the wrapper
  let payload = format::render_replacement(&id, &content);

  let _ = tx.send(payload).inspect_err(|_| {
    trace!(suspense.id = %id, "future completed but receiver is dropped");
  });
}
