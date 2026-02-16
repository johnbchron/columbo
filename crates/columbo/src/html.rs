use std::fmt;

/// Pre-escaped HTML content. Columbo's canonical markup type.
///
/// This is a thin wrapper around a `String` that represents raw HTML. No
/// escaping is performed — the caller is responsible for ensuring the content
/// is safe.
///
/// External types convert into `Html` via [`From`] implementations:
/// - `From<String>` and `From<&str>` for raw HTML strings.
/// - `From<maud::Markup>` when the `maud` feature is enabled.
#[derive(Clone, Debug, Default)]
pub struct Html(String);

impl Html {
  /// Create a new `Html` from raw HTML content.
  pub fn new(raw_html: impl Into<String>) -> Self { Html(raw_html.into()) }

  /// Consume the `Html` and return the inner string.
  pub fn into_string(self) -> String { self.0 }

  /// View the inner HTML as a string slice.
  pub fn as_str(&self) -> &str { &self.0 }
}

impl fmt::Display for Html {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.0)
  }
}

impl From<String> for Html {
  fn from(s: String) -> Self { Html(s) }
}

impl From<&str> for Html {
  fn from(s: &str) -> Self { Html(s.to_owned()) }
}

#[cfg(feature = "maud")]
impl From<maud::Markup> for Html {
  fn from(m: maud::Markup) -> Self { Html(m.into_string()) }
}
