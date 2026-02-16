use std::any::Any;

use crate::{Html, Id};

const GLOBAL_SCRIPT_CONTENTS: &str = include_str!("./columbo.js");

pub(crate) fn render_placeholder(id: &Id, inner: &Html) -> Html {
  Html::new(format!(
    r#"<span data-columbo-p-id="{id}" style="display: contents;">{inner}</span>"#,
    id = id,
    inner = inner.as_str(),
  ))
}

pub(crate) fn render_replacement(id: &Id, inner: &Html) -> Html {
  Html::new(format!(
    r#"<template data-columbo-r-id="{id}">{inner}</template>"#,
    id = id,
    inner = inner.as_str(),
  ))
}

pub(crate) fn render_global_script() -> Html {
  Html::new(format!("<script>{GLOBAL_SCRIPT_CONTENTS}</script>"))
}

pub(crate) fn default_panic_renderer(panic: Box<dyn Any + Send>) -> Html {
  let error = panic_payload_to_string(&panic);
  render_panic(&error)
}

fn panic_payload_to_string(payload: &dyn Any) -> String {
  if let Some(s) = payload.downcast_ref::<&str>() {
    return s.to_string();
  }
  if let Some(s) = payload.downcast_ref::<String>() {
    return s.clone();
  }
  "Box<dyn Any>".to_string()
}

fn render_panic(error: &str) -> Html {
  // Escape the error string for safe HTML embedding
  let escaped = html_escape(error);
  Html::new(format!(
    r#"<div style="font-family: monospace; padding: 20px; background: #ffe6e6; color: #000; border: 2px solid #c00;">\
<h1 style="color: #c00; font-size: 18px; margin: 0 0 10px 0;">Columbo Suspense Panic</h1>\
<p style="margin: 10px 0;">Columbo could not swap in a suspended response because the suspended future panicked.</p>\
<h2 style="font-size: 16px; margin: 20px 0 10px 0;">Error:</h2>\
<pre style="background: #f5f5f5; padding: 10px; overflow: auto; border: 1px solid #ccc; font-size: 12px;">{escaped}</pre>\
</div>"#
  ))
}

fn html_escape(s: &str) -> String {
  s.replace('&', "&amp;")
    .replace('<', "&lt;")
    .replace('>', "&gt;")
    .replace('"', "&quot;")
}
