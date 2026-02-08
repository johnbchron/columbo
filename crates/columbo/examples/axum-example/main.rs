use axum::{
  Router,
  body::Body,
  response::{IntoResponse, Response},
  routing::get,
};
use maud::{DOCTYPE, PreEscaped};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[axum::debug_handler]
async fn suspended_handler() -> impl IntoResponse {
  let (ctx, resp) = columbo::new();

  let long_suspend = ctx.suspend(
    |ctx| async move {
      tokio::time::sleep(std::time::Duration::from_secs(2)).await;

      let longer_suspend = ctx.suspend(
        |_ctx| async move {
          tokio::time::sleep(std::time::Duration::from_secs(3)).await;
          "I waited 3 seconds!"
        },
        "[loading]".into(),
      );

      maud::html! {
        div {
          "I waited 2 seconds! But there's more: "
          (PreEscaped(longer_suspend.to_string()))
        }
      }
    },
    "[loading]".into(),
  );

  let body = maud::html! {
    (DOCTYPE)
    html {
      head;
      body {
        p {
          "Test 1 2 3"
        }
        (PreEscaped(long_suspend.to_string()))
      }
    }
  }
  .into_string();

  let body = Body::from_stream(resp.into_stream(body));
  Response::builder()
    .header("Content-Type", "text/html; charset=utf-8")
    .header("Transfer-Encoding", "chunked")
    .header("X-Content-Type-Options", "nosniff")
    .body(body)
    .unwrap()
}

#[tokio::main]
async fn main() {
  tracing_subscriber::registry()
    .with(fmt::layer())
    .with(EnvFilter::from_default_env())
    .init();

  let app = Router::new().route("/", get(suspended_handler));

  let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
  axum::serve(listener, app).await.unwrap();
}
