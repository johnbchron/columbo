use std::{collections::HashMap, time::Duration};

use axum::{
  Router,
  body::Body,
  extract::Query,
  response::{IntoResponse, Response},
  routing::get,
};
use maud::{DOCTYPE, html};
use nanorand::Rng;
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
          html! { "I waited 3 seconds!" }
        },
        html! { "[loading]" },
      );

      html! {
        div {
          "I waited 2 seconds! But there's more: "
          (longer_suspend)
        }
      }
    },
    html! { "[loading]" },
  );

  let body = html! {
    (DOCTYPE)
    html {
      head;
      body {
        p {
          "Test 1 2 3"
        }
        (long_suspend)
      }
    }
  };

  let body = Body::from_stream(resp.into_stream(body));
  Response::builder()
    .header("Content-Type", "text/html; charset=utf-8")
    .header("Transfer-Encoding", "chunked")
    .header("X-Content-Type-Options", "nosniff")
    .body(body)
    .unwrap()
}

#[axum::debug_handler]
async fn multi_suspended_handler(
  Query(q_map): Query<HashMap<String, String>>,
) -> impl IntoResponse {
  let n = q_map
    .get("n")
    .and_then(|v| v.parse::<usize>().ok())
    .unwrap_or(5);
  let (ctx, resp) = columbo::new();
  let mut rng = nanorand::tls_rng();

  let suspense_items = (0..n)
    .map(|_| {
      ctx.suspend(
        |_ctx| {
          let duration = Duration::from_millis(rng.generate_range(500..1500));
          async move {
            tokio::time::sleep(duration).await;
            html! {
              div style="width: 24px; height: 24px; background-color: #00FF00;" {}
            }
          }
        },
        html! {
          div style="width: 24px; height: 24px; background-color: #FF0000;" {}
        },
      )
    })
    .collect::<Vec<_>>();

  let stream = resp.into_stream(html! {
    (DOCTYPE)
    html {
      head;
      body {
        p { "Multi-suspense example: " (n) " suspended futures" }
        div style="display: flex; flex-wrap: wrap; gap: 1rem; width: auto;" {
          @for suspense in suspense_items {
            (suspense)
          }
        }
      }
    }
  });

  let body = Body::from_stream(stream);
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

  let app = Router::new()
    .route("/", get(suspended_handler))
    .route("/multi", get(multi_suspended_handler));

  let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
  axum::serve(listener, app).await.unwrap();
}
