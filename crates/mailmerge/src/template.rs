//! Template rendering.
//!
//! Templates are data templates, not programs. minijinja is configured so a
//! template cannot read the filesystem, reach the network, or execute anything:
//! no loader is installed, so `{% include %}`/`{% import %}`/`{% extends %}`
//! have nothing to resolve, and only the record's own properties are in scope.
//!
//! Undefined variables are a hard error rather than an empty string, so a typo
//! in a placeholder fails the batch instead of mailing everyone a sentence with
//! a hole in it.

use anyhow::{bail, Context};
use minijinja::{Environment, UndefinedBehavior};
use std::collections::BTreeMap;

pub struct Templates {
    env: Environment<'static>,
}

impl Templates {
    pub fn new() -> Self {
        let mut env = Environment::new();
        // A missing or misspelled property fails loudly.
        env.set_undefined_behavior(UndefinedBehavior::Strict);
        // No loader is set: template inheritance and includes cannot reach the
        // filesystem. Left explicit so removing it is a deliberate act.
        Self { env }
    }

    /// Register a template under one of the [`SUBJECT`], [`HTML`], [`TEXT`]
    /// names.
    ///
    /// The name is load-bearing: minijinja selects auto-escaping from the
    /// suffix, so `message.html` gets HTML escaping while `subject.txt` and
    /// `message.txt` do not — which is what you want, since HTML entities in a
    /// subject line would be a bug.
    pub fn add(&mut self, name: &'static str, source: String) -> anyhow::Result<()> {
        self.env
            .add_template_owned(name, source)
            .with_context(|| format!("parsing template `{name}`"))?;
        Ok(())
    }

    /// Render one template against one record's properties.
    pub fn render(
        &self,
        name: &str,
        properties: &BTreeMap<String, String>,
    ) -> anyhow::Result<String> {
        let template = self
            .env
            .get_template(name)
            .with_context(|| format!("looking up template `{name}`"))?;

        template.render(properties).map_err(|error| {
            // minijinja reports undefined variables here; surface the property
            // name so the operator can fix the data or the template.
            anyhow::anyhow!("rendering `{name}`: {error}")
        })
    }
}

impl Default for Templates {
    fn default() -> Self {
        Self::new()
    }
}

/// Names used for the four templates, chosen so minijinja's suffix-based
/// auto-escaping turns HTML escaping on for exactly the HTML ones.
pub const SUBJECT: &str = "subject.txt";
pub const HTML: &str = "message.html";
pub const TEXT: &str = "message.txt";

/// Reject a template that tries to reach outside its data.
///
/// Belt and braces alongside the absent loader: catching this at parse time
/// gives a clear message instead of a confusing runtime lookup failure.
pub fn reject_unsafe_constructs(source: &str, label: &str) -> anyhow::Result<()> {
    for construct in ["{% include", "{% import", "{% extends", "{% from"] {
        if source.contains(construct) {
            bail!(
                "{label} uses `{construct}`, which is not available: templates are data \
                 templates and cannot load other files"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    fn render_html(source: &str, pairs: &[(&str, &str)]) -> anyhow::Result<String> {
        let mut templates = Templates::new();
        templates.add(HTML, source.to_string())?;
        templates.render(HTML, &props(pairs))
    }

    #[test]
    fn placeholders_interpolate() {
        let out = render_html(
            "<p>Hi {{ first_name }},</p>",
            &[("first_name", "Dumi")],
        )
        .unwrap();
        assert_eq!(out, "<p>Hi Dumi,</p>");
    }

    #[test]
    fn html_values_are_escaped_by_default() {
        // A malicious property value must not become live markup.
        let out = render_html(
            "<p>{{ first_name }}</p>",
            &[("first_name", "<script>alert(1)</script>")],
        )
        .unwrap();
        assert!(!out.contains("<script>"), "value was not escaped: {out}");
        assert!(out.contains("&lt;script&gt;"), "{out}");
    }

    #[test]
    fn quotes_in_values_cannot_break_out_of_an_attribute() {
        let out = render_html(
            r#"<a href="{{ url }}">x</a>"#,
            &[("url", r#"https://x.example" onclick="steal()"#)],
        )
        .unwrap();
        assert!(!out.contains(r#"" onclick=""#), "attribute escaped out: {out}");
    }

    #[test]
    fn a_missing_property_fails_the_batch() {
        let err = render_html("<p>Hi {{ first_name }}</p>", &[]).unwrap_err();
        assert!(err.to_string().contains("first_name"), "{err}");
    }

    #[test]
    fn an_unresolved_placeholder_is_never_rendered_as_empty() {
        // The failure mode this prevents: mailing everyone "Hi ,".
        assert!(render_html("Hi {{ nope }}", &[("first_name", "Dumi")]).is_err());
    }

    #[test]
    fn subject_template_does_not_html_escape() {
        let mut templates = Templates::new();
        templates
            .add(SUBJECT, "Digest for {{ name }}".to_string())
            .unwrap();
        let out = templates
            .render(SUBJECT, &props(&[("name", "Ben & Co")]))
            .unwrap();
        assert_eq!(out, "Digest for Ben & Co");
    }

    #[test]
    fn includes_are_rejected_with_a_clear_message() {
        let err = reject_unsafe_constructs("{% include 'secrets.txt' %}", "html template")
            .unwrap_err();
        assert!(err.to_string().contains("data templates"), "{err}");
        assert!(reject_unsafe_constructs("<p>{{ ok }}</p>", "html").is_ok());
    }

    #[test]
    fn each_record_renders_only_its_own_values() {
        // The privacy property the whole feature turns on.
        let source = r#"<a href="{{ url }}">read</a>"#;
        let a = render_html(source, &[("url", "https://x.example/a?t=TOKEN_A")]).unwrap();
        let b = render_html(source, &[("url", "https://x.example/b?t=TOKEN_B")]).unwrap();
        assert!(a.contains("TOKEN_A") && !a.contains("TOKEN_B"));
        assert!(b.contains("TOKEN_B") && !b.contains("TOKEN_A"));
    }
}
