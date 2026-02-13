use std::{
  io,
  pin::Pin,
  task::{Context, Poll},
};

use bytes::Bytes;
use futures::{Stream, StreamExt, stream::once};
use maud::{Markup, html};
use tokio_stream::wrappers::UnboundedReceiverStream;
use tracing::{Span, debug, trace};

use crate::{
  SuspendedResponse, cancel_on_drop::CancelOnDrop, format::GlobalSuspenseScript,
};

pub struct MarkupStream {
  inner: Pin<
    Box<dyn Stream<Item = Result<Bytes, io::Error>> + Send + Sync + 'static>,
  >,
  _cancel: CancelOnDrop,
}

impl MarkupStream {
  pub(crate) fn new(resp: SuspendedResponse, main_chunk: Markup) -> Self {
    let main_chunk = html! {
      (main_chunk)
      (GlobalSuspenseScript)
    };

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
      inner:   Box::pin(stream),
      _cancel: resp.cancel,
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
