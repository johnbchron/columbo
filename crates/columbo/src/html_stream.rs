use std::{
  io,
  pin::Pin,
  task::{Context, Poll},
};

use bytes::Bytes;
use futures::{Stream, StreamExt, stream::once};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::{Span, debug, trace};

use crate::{Html, SuspendedResponse, cancel_on_drop::CancelOnDrop, format};

pub struct HtmlStream {
  inner: Pin<
    Box<dyn Stream<Item = Result<Bytes, io::Error>> + Send + Sync + 'static>,
  >,
  _cancel: CancelOnDrop,
}

impl HtmlStream {
  pub(crate) fn new(resp: SuspendedResponse, main_chunk: Html) -> Self {
    // include the script conditionally
    let include_script = resp.opts.include_script.unwrap_or(true);
    let script = include_script
      .then(format::render_global_script)
      .unwrap_or_default();

    let main_chunk =
      Html::new(format!("{}{}", main_chunk.as_str(), script.as_str()));

    // stream of Html chunks, including initial body
    let html_stream = once(async move { main_chunk })
      .chain(UnboundedReceiverStream::new(resp.rx));

    let parent_span = Span::current();
    // debugging to note when stream terminates
    let stream_end = once(async move {
        debug!(parent: parent_span, "receiver stream ended - all senders dropped");
        futures::stream::empty()
      })
      .flatten();

    // final stream, adapted to IO convention
    let stream = html_stream
      .map(|c| Ok(Bytes::from(c.into_string())))
      .chain(stream_end);

    trace!("stream created with initial body chunk");
    HtmlStream {
      inner:   Box::pin(stream),
      _cancel: resp.cancel,
    }
  }
}

impl Stream for HtmlStream {
  type Item = Result<Bytes, io::Error>;

  fn poll_next(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Option<Self::Item>> {
    self.inner.poll_next_unpin(cx)
  }
}
