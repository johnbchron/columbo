use std::{
  io,
  pin::Pin,
  task::{Context, Poll},
};

use bytes::Bytes;
use futures::{Stream, StreamExt, stream::once};
use maud::Markup;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::{Span, debug, trace};

use crate::SuspendedResponse;

pub struct MarkupStream {
  inner: Pin<
    Box<dyn Stream<Item = Result<Bytes, io::Error>> + Send + Sync + 'static>,
  >,
}

impl MarkupStream {
  pub(crate) fn new(resp: SuspendedResponse, main_chunk: Markup) -> Self {
    // stream of markup chunks, including initial body
    let markup_stream = once(async move { main_chunk })
      .chain(UnboundedReceiverStream::new(resp.rx));

    let parent_span = Span::current();
    // debugging to note when stream terminates
    let stream_end = once(async move {
        debug!(parent: parent_span, "receiver stream ended - all senders dropped");
        futures::stream::empty()
      })
      .flatten();

    // final stream, adapted to IO convention
    let stream = markup_stream
      .map(|c| Ok(Bytes::from(c.into_string())))
      .chain(stream_end);

    trace!("stream created with initial body chunk");
    MarkupStream {
      inner: Box::pin(stream),
    }
  }
}

impl Stream for MarkupStream {
  type Item = Result<Bytes, io::Error>;

  fn poll_next(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Option<Self::Item>> {
    self.inner.poll_next_unpin(cx)
  }
}
