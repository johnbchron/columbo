# `columbo`

Provides SSR suspense capabilities. Render a placeholder for a future, and
stream the replacement elements.

Called `columbo` because Columbo always said, "And another thing..."

> For the purposes of this library, the verb `suspend` generally means
> "defer the rendering and sending of an async workload", in the context of
> rendering a web document.

## Overview
The entrypoint for the library is the [`new()`] function, which returns a
[`SuspenseContext`] and a [`SuspendedResponse`]. The [`SuspenseContext`]
allows you to [`suspend()`](SuspenseContext::suspend) futures to be sent
down the stream when they are completed, wrapped with just enough HTML to be
interpolated into wherever the resulting [`Suspense`] struct was rendered
into the document as a placeholder. [`SuspendedResponse`] acts as a receiver
for these suspended results. When done rendering your document, pass it your
document and call [`into_stream()`](SuspendedResponse::into_stream) to get
seamless SSR streaming suspense.

So in summary:
- Use [`SuspenseContext`] to call [`suspend()`](SuspenseContext::suspend)
  and suspend futures.
- Call [`into_stream()`](SuspendedResponse::into_stream) to setup your
  response stream.

The [`suspend()`](SuspenseContext::suspend) function provides access to
itself for the futures it suspends by taking a closure returning a future,
so futures can spawn additional suspensions or listen for cancellation.

Responses are streamed in completion order, not registration order, so the
future that completes first will stream first.

### Cancel Safety
By default, if [`SuspendedResponse`] or the type resulting from
[`into_stream()`](SuspendedResponse::into_stream) are dropped, the futures
that have been suspended will continue to run, but their results will be
inaccessible. If you would like for tasks to cancel instead, you can enable
`auto_cancel` in [`ColumboOptions`], or you can use
[`cancelled()`](SuspenseContext::cancelled) or
[`is_cancelled()`](SuspenseContext::is_cancelled) to exit early from within
the future.

## Axum Example

```rust
use axum::{
  body::Body,
  response::{IntoResponse, Response},
};

async fn handler() -> impl IntoResponse {
  // columbo entrypoint
  let (ctx, resp) = columbo::new();

  // suspend a future, providing a future and a placeholder
  let suspense = ctx.suspend(
    // takes a closure that returns a future, allowing nested suspense
    |_ctx| async move {
      tokio::time::sleep(std::time::Duration::from_secs(2)).await;

      // the future can return any type that implements Into<Html>
      "<p>Good things come to those who wait.</p>"
    },
    // placeholder replaced when result is streamed
    "Loading...",
  );

  // directly interpolate the suspense into the document
  let document = format!(
    "<!DOCTYPE html><html><head></head><body><p>Aphorism \
     incoming...</p>{suspense}</body></html>"
  );

  // produce a body stream with the document and suspended results
  let stream = resp.into_stream(document);
  let body = Body::from_stream(stream);
  Response::builder()
    .header("Content-Type", "text/html; charset=utf-8")
    .header("Transfer-Encoding", "chunked")
    .body(body)
    .unwrap()
}
```

Use [`new_with_opts`] to configure `columbo` behavior:
```rust
use std::any::Any;

use axum::{
  body::Body,
  response::{IntoResponse, Response},
};
use columbo::{ColumboOptions, Html};

fn panic_renderer(_panic_object: Box<dyn Any + Send>) -> Html {
  Html::new("panic")
}

async fn handler() -> impl IntoResponse {
  let (ctx, resp) = columbo::new_with_opts(ColumboOptions {
    panic_renderer: Some(panic_renderer),
    ..Default::default()
  });

  // suspend a future, providing a future and a placeholder
  let panicking_suspense = ctx.suspend(
    // takes a closure that returns a future, allowing nested suspense
    |_ctx| async move {
      tokio::time::sleep(std::time::Duration::from_secs(2)).await;

      panic!("");
      #[allow(unreachable_code)]
      ""
    },
    // placeholder replaced when result is streamed
    "Loading...",
  );

  // directly interpolate the suspense into the document
  let document = format!("{panicking_suspense}<p>at the disco</p>");

  // produce a body stream with the document and suspended results
  let stream = resp.into_stream(document);
  let body = Body::from_stream(stream);
  Response::builder()
    .header("Content-Type", "text/html; charset=utf-8")
    .header("Transfer-Encoding", "chunked")
    .body(body)
    .unwrap()
}
```

## Integrations

Both integrations are enabled by default. Disable them individually via
`default-features = false` and re-enable selectively with `features = ["axum"]`
or `features = ["maud"]`.

### Axum (`feature = "axum"`)

The `axum` feature implements `IntoResponse` for `HtmlStream`, the type
returned by [`into_stream()`](SuspendedResponse::into_stream). This means you
can return the stream directly from an Axum handler — no manual `Response`
construction required:

```rust
use axum::response::IntoResponse;

async fn handler() -> impl IntoResponse {
  let (ctx, resp) = columbo::new();

  let suspense = ctx.suspend(
    |_ctx| async move {
      tokio::time::sleep(std::time::Duration::from_secs(1)).await;
      "<p>Done.</p>"
    },
    "Loading...",
  );

  let document = format!("<html><body>{suspense}</body></html>");
  resp.into_stream(document) // implements IntoResponse directly
}
```

The response is automatically sent with `Content-Type: text/html; charset=utf-8`
and `X-Content-Type-Options: nosniff`.

### Maud (`feature = "maud"`)

The `maud` feature adds two conveniences:

1. **`maud::Markup` → `Html`**: `maud::Markup` implements `Into<Html>`, so
   `html! { ... }` blocks can be passed directly as futures' return values or
   as placeholders.

2. **`Suspense` implements `maud::Render`**: [`Suspense`] values can be
   interpolated directly into `html! { ... }` macros with `(suspense)`.

```rust
use maud::{DOCTYPE, html};

async fn handler() -> impl IntoResponse {
  let (ctx, resp) = columbo::new();

  let suspense = ctx.suspend(
    |_ctx| async move {
      tokio::time::sleep(std::time::Duration::from_secs(1)).await;
      html! { p { "Loaded!" } } // maud::Markup accepted directly
    },
    html! { "[loading]" },      // placeholder also accepts maud::Markup
  );

  let document = html! {       // Suspense interpolates via maud::Render
    (DOCTYPE)
    html {
      body { (suspense) }
    }
  };

  resp.into_stream(document)
}
```

## Architecture

Internally, [`SuspenseContext`] holds a channel sender. When
[`suspend()`](SuspenseContext::suspend) is called, it launches a task which
runs the given future to completion. The result of this future (or a panic
message if it panicked) is wrapped in a `<template>` tag and sent as a
message to the channel. A single global `<script>` is injected once into
the initial document chunk and handles swapping each `<template>` into its
placeholder.

[`SuspendedResponse`] contains a receiver. It just sits around until you
call [`into_stream()`](SuspendedResponse::into_stream), at which point the
receiver is turned into a stream whose elements are preceded by the
document you provide.
