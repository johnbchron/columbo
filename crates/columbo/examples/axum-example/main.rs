use axum::{
  Router,
  body::Body,
  response::{IntoResponse, Response},
  routing::get,
};
use columbo::SuspenseContext;
use maud::{DOCTYPE, PreEscaped};

#[axum::debug_handler]
async fn suspended_handler() -> impl IntoResponse {
  let ctx = SuspenseContext::new();

  let long_suspend = ctx.suspend(
    async move {
      tokio::time::sleep(std::time::Duration::from_secs(2)).await;
      "I waited 5 seconds!".into()
    },
    "Loading...".into(),
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

  let body = Body::from_stream(ctx.into_stream(body));
  Response::builder()
    .header("Content-Type", "text/html; charset=utf-8")
    .header("Transfer-Encoding", "chunked")
    .header("X-Content-Type-Options", "nosniff")
    .body(body)
    .unwrap()
}

#[tokio::main]
async fn main() {
  let app = Router::new().route("/", get(suspended_handler));

  let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
  axum::serve(listener, app).await.unwrap();
}
