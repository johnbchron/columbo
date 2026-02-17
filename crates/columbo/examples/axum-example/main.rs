use std::{any::Any, collections::HashMap, time::Duration};

use axum::{Router, extract::Query, response::IntoResponse, routing::get};
use columbo::{ColumboOptions, Html};
use maud::{DOCTYPE, html};
use nanorand::Rng;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[axum::debug_handler]
async fn nested_handler() -> impl IntoResponse {
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

  let document = html! {
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

  resp.into_stream(document)
}

#[axum::debug_handler]
async fn panicking_handler() -> impl IntoResponse {
  let (ctx, resp) = columbo::new();

  let panicking_suspend = ctx.suspend(
    |_ctx| async move {
      tokio::time::sleep(Duration::from_secs(1)).await;
      panic!("I don't know! The programmer told me to panic!");
      #[allow(unreachable_code)]
      ""
    },
    html! {
      "loading..."
    },
  );

  let document = html! {
    (DOCTYPE)
    html {
      head;
      body {
        p {
          "The following will panic:"
        }
        (panicking_suspend)
      }
    }
  };

  resp.into_stream(document)
}

fn panic_renderer(_panic_object: Box<dyn Any + Send>) -> Html {
  Html::new("panic")
}

async fn custom_panicking_handler() -> impl IntoResponse {
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
    maud::html! { "Loading..." },
  );

  // directly interpolate the suspense into the document
  let document = maud::html! {
    (panicking_suspense)
    p { "at the disco" }
  };

  // produce a body stream with the document and suspended results
  resp.into_stream(document)
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

  let document = html! {
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
  };
  resp.into_stream(document)
}

#[axum::debug_handler]
async fn manually_cancelled_handler() -> impl IntoResponse {
  let (ctx, resp) = columbo::new();

  let suspend = ctx.suspend(
    |ctx| async move {
      ctx.cancelled().await;
      tracing::warn!("suspended future cancelled");
      ""
    },
    "[loading]",
  );

  let document = html! {
    (DOCTYPE)
    html {
      head;
      body {
        p {
          "This will never resolve:"
        }
        (suspend)
      }
    }
  };

  resp.into_stream(document)
}

#[axum::debug_handler]
async fn auto_cancelled_handler() -> impl IntoResponse {
  let (ctx, resp) = columbo::new_with_opts(ColumboOptions {
    auto_cancel: Some(true),
    ..Default::default()
  });

  let suspend = ctx.suspend(
    |_ctx| async move {
      tokio::time::sleep(Duration::from_secs(10)).await;
      tracing::warn!("handler completed");
      html! { "[10 seconds later]" }
    },
    html! { "[loading]" },
  );

  let document = html! {
    (DOCTYPE)
    html {
      head;
      body {
        p {
          "This will never resolve in 10 seconds:"
        }
        (suspend)
      }
    }
  };

  resp.into_stream(document)
}

#[tokio::main]
async fn main() {
  tracing_subscriber::registry()
    .with(fmt::layer())
    .with(EnvFilter::from_default_env())
    .init();

  let app = Router::new()
    .route("/", get(nested_handler))
    .route("/multi", get(multi_suspended_handler))
    .route("/panic", get(panicking_handler))
    .route("/custom_panic", get(custom_panicking_handler))
    .route("/manual_cancel", get(manually_cancelled_handler))
    .route("/auto_cancel", get(auto_cancelled_handler));

  let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
  axum::serve(listener, app).await.unwrap();
}
