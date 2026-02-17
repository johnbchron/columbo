#[cfg(feature = "axum")]
use axum_core::{
  body::Body,
  response::{IntoResponse, Response},
};

#[cfg(feature = "axum")]
use crate::html_stream::HtmlStream;

#[cfg(feature = "axum")]
impl IntoResponse for HtmlStream {
  fn into_response(self) -> Response {
    let body = Body::from_stream(self);
    Response::builder()
      .header("Content-Type", "text/html; charset=utf-8")
      .header("X-Content-Type-Options", "nosniff")
      .body(body)
      .expect("infalliable: static header values")
  }
}
