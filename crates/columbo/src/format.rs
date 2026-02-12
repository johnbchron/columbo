use maud::{Markup, PreEscaped, Render, html};

use crate::Id;

pub(crate) struct SuspensePlaceholder<'a> {
  pub id:                &'a Id,
  pub placeholder_inner: &'a Markup,
}

impl<'a> Render for SuspensePlaceholder<'a> {
  fn render(&self) -> Markup {
    html! {
      span
        data-columbo-p-id=(self.id)
        style="display: contents;"
      {
        (self.placeholder_inner)
      }
    }
  }
}

pub(crate) struct SuspenseReplacement<'a> {
  pub id:                &'a Id,
  pub replacement_inner: &'a Markup,
}

impl<'a> Render for SuspenseReplacement<'a> {
  fn render(&self) -> Markup {
    let script = format!(
      r#"(function() {{
          const t = document.querySelector('[data-columbo-p-id="{id}"]');
          const r = document.querySelector('[data-columbo-r-id="{id}"]');
          if (t && r && t.parentNode) {{
            t.parentNode.replaceChild(r.content, t);
          }}
        }})();"#,
      id = self.id
    );

    html! {
      template data-columbo-r-id=(self.id) {
        (self.replacement_inner)
      }
      script {
        (PreEscaped(script))
      }
    }
  }
}

pub(crate) struct SuspensePanic {
  pub error: String,
}

impl Render for SuspensePanic {
  fn render(&self) -> Markup {
    html! {
      div style="font-family: monospace; padding: 20px; background: #ffe6e6; color: #000; border: 2px solid #c00;" {
        h1 style="color: #c00; font-size: 18px; margin: 0 0 10px 0;" {
          "Columbo Suspense Panic"
        }
        p style="margin: 10px 0;" {
          "Columbo could not swap in a suspended response because the suspended future panicked."
        }
        h2 style="font-size: 16px; margin: 20px 0 10px 0;" {
          "Error:"
        }
        pre style="background: #f5f5f5; padding: 10px; overflow: auto; border: 1px solid #ccc; font-size: 12px;" {
          (self.error)
        }
      }
    }
  }
}
