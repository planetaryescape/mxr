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
            // minijinja says only "undefined value", which is no help when a
            // campaign of 500 records dies on one misspelled column. Name the
            // properties the template asked for and the data does not have.
            let missing: Vec<String> = if error.kind() == minijinja::ErrorKind::UndefinedError {
                let mut names: Vec<String> = template
                    .undeclared_variables(false)
                    .into_iter()
                    .filter(|variable| !properties.contains_key(variable))
                    .collect();
                names.sort();
                names
            } else {
                Vec::new()
            };

            match missing.as_slice() {
                [] => anyhow::anyhow!("rendering `{name}`: {error}"),
                names => {
                    anyhow::anyhow!("rendering `{name}`: no value for `{}`", names.join("`, `"))
                }
            }
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

/// Tags that would pull in another file if a loader were ever installed.
const FILE_LOADING_TAGS: [&str; 4] = ["include", "import", "extends", "from"];

/// Reject a template that tries to reach outside its data.
///
/// Belt and braces alongside the absent loader: catching this at parse time
/// gives a clear message instead of a confusing runtime lookup failure.
///
/// The tag is read out of the block rather than matched as a literal prefix,
/// so the whitespace-control and spacing variants (`{%- include`, `{%include`)
/// get the same clear message instead of slipping through to a lookup failure.
pub fn reject_unsafe_constructs(source: &str, label: &str) -> anyhow::Result<()> {
    for (offset, _) in source.match_indices("{%") {
        let block = source[offset + 2..]
            .trim_start_matches(['-', '+'])
            .trim_start();
        let tag: String = block
            .chars()
            .take_while(char::is_ascii_alphabetic)
            .collect();
        if FILE_LOADING_TAGS.contains(&tag.as_str()) {
            bail!(
                "{label} uses `{tag}`, which is not available: templates are data \
                 templates and cannot load other files"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "tests assert directly on fixtures")]

    use super::*;

    fn props(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    fn render_as(
        name: &'static str,
        source: &str,
        pairs: &[(&str, &str)],
    ) -> anyhow::Result<String> {
        let mut templates = Templates::new();
        templates.add(name, source.to_string())?;
        templates.render(name, &props(pairs))
    }

    fn render_html(source: &str, pairs: &[(&str, &str)]) -> anyhow::Result<String> {
        render_as(HTML, source, pairs)
    }

    #[test]
    fn placeholders_interpolate() {
        let out = render_html("<p>Hi {{ first_name }},</p>", &[("first_name", "Dumi")]).unwrap();
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
        assert!(!out.contains("</script>"), "value was not escaped: {out}");
        // The value contributed no markup at all: the only angle brackets left
        // are the template's own `<p>` and `</p>`.
        assert!(
            out.starts_with("<p>") && out.ends_with("</p>"),
            "template markup did not survive: {out}"
        );
        let inner = out.trim_start_matches("<p>").trim_end_matches("</p>");
        assert!(!inner.contains(['<', '>']), "value became markup: {out}");
        assert!(inner.contains("alert(1)"), "value was lost entirely: {out}");
    }

    #[test]
    fn quotes_in_values_cannot_break_out_of_an_attribute() {
        let out = render_html(
            r#"<a href="{{ url }}">x</a>"#,
            &[("url", r#"https://x.example" onclick="steal()"#)],
        )
        .unwrap();
        // The only quotes left in the output are the template's own two, so
        // the value cannot have closed the attribute.
        assert_eq!(out.matches('"').count(), 2, "attribute escaped out: {out}");
        assert!(!out.contains("onclick=\""), "{out}");
    }

    #[test]
    fn a_missing_property_fails_the_batch() {
        // The failure mode this prevents: mailing everyone "Hi ,".
        let err = render_html("<p>Hi {{ first_name }},</p>", &[]).unwrap_err();
        assert!(err.to_string().contains("first_name"), "{err}");

        // The message has to name the property that is missing, and only that
        // one: "undefined value" sends the operator hunting through every
        // placeholder in the template.
        let err = render_html(
            "<p>Hi {{ first_name }}, your {{ frist_name }} plan</p>",
            &[("first_name", "Dumi")],
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("frist_name"), "{err}");
        assert!(
            !err.contains("`first_name`"),
            "blamed a property that was present: {err}"
        );
    }

    #[test]
    fn a_missing_property_inside_a_condition_also_fails() {
        // Strictness has to reach into control flow, or `{% if plan %}` quietly
        // takes the else branch for every record whose column was misspelled.
        assert!(render_html("{% if plan %}pro{% endif %}", &[("plna", "pro")]).is_err());
    }

    #[test]
    fn the_subject_and_text_templates_do_not_html_escape() {
        // HTML entities in a subject line would be a bug, not an escape.
        assert_eq!(
            render_as(SUBJECT, "Digest for {{ name }}", &[("name", "Ben & Co")]).unwrap(),
            "Digest for Ben & Co"
        );
        assert_eq!(
            render_as(TEXT, "Hi {{ name }}", &[("name", "Ben & Co")]).unwrap(),
            "Hi Ben & Co"
        );
        // Same value, HTML template: escaped. The template name is what decides.
        assert_eq!(
            render_as(HTML, "<p>{{ name }}</p>", &[("name", "Ben & Co")]).unwrap(),
            "<p>Ben &amp; Co</p>"
        );
    }

    #[test]
    fn file_loading_tags_are_rejected_with_a_clear_message() {
        for source in [
            "{% include 'secrets.txt' %}",
            "{%include 'secrets.txt' %}",
            "{%- include 'secrets.txt' %}",
            "{%   import 'x.html' as m %}",
            "{% extends 'base.html' %}",
            "{% from 'x.html' import macro %}",
            "<p>ok</p>\n{%-include '/etc/passwd' %}",
        ] {
            let err = reject_unsafe_constructs(source, "html template")
                .unwrap_err()
                .to_string();
            assert!(err.contains("data templates"), "accepted `{source}`");
            assert!(err.contains("html template"), "{err}");
        }
    }

    #[test]
    fn ordinary_templates_are_not_mistaken_for_file_loading() {
        for source in [
            "<p>{{ ok }}</p>",
            "{% if included %}yes{% endif %}",
            "<p>{{ import_url }}</p>",
            "{% for row in rows %}{{ row }}{% endfor %}",
        ] {
            assert!(
                reject_unsafe_constructs(source, "html").is_ok(),
                "false positive on `{source}`"
            );
        }
    }

    #[test]
    fn an_include_cannot_reach_the_filesystem_even_if_the_guard_is_bypassed() {
        // The guard is belt; the absent loader is braces. Render one directly.
        let mut templates = Templates::new();
        templates
            .add(HTML, "{% include 'Cargo.toml' %}".to_string())
            .unwrap();
        let err = templates.render(HTML, &props(&[])).unwrap_err();
        assert!(!err.to_string().contains("[package]"), "{err}");
    }

    #[test]
    fn each_record_renders_only_its_own_values() {
        // The privacy property the whole feature turns on: one shared
        // Environment, rendered record after record, must not carry anything
        // from the previous record into the next.
        let mut templates = Templates::new();
        templates
            .add(HTML, r#"<a href="{{ url }}">read</a>"#.to_string())
            .unwrap();

        let a = templates
            .render(HTML, &props(&[("url", "https://x.example/a?t=TOKEN_A")]))
            .unwrap();
        let b = templates
            .render(HTML, &props(&[("url", "https://x.example/b?t=TOKEN_B")]))
            .unwrap();

        assert!(a.contains("TOKEN_A"), "{a}");
        assert!(
            !a.contains("TOKEN_B"),
            "record A leaked record B's token: {a}"
        );
        assert!(b.contains("TOKEN_B"), "{b}");
        assert!(
            !b.contains("TOKEN_A"),
            "record B leaked record A's token: {b}"
        );
    }

    #[test]
    fn a_property_present_for_one_record_is_not_inherited_by_the_next() {
        // The nastier half of the same property: if the previous record's
        // context survived, this render would succeed with A's value.
        let mut templates = Templates::new();
        templates
            .add(HTML, "<p>{{ token }}</p>".to_string())
            .unwrap();
        assert!(templates
            .render(HTML, &props(&[("token", "TOKEN_A")]))
            .is_ok());
        assert!(templates.render(HTML, &props(&[])).is_err());
    }

    #[test]
    fn a_template_that_does_not_parse_is_reported_by_name() {
        let mut templates = Templates::new();
        let err = templates
            .add(HTML, "<p>{{ unclosed </p>".to_string())
            .unwrap_err();
        assert!(err.to_string().contains(HTML), "{err}");
    }

    #[test]
    fn rendering_an_unregistered_template_is_an_error() {
        let templates = Templates::new();
        assert!(templates.render(TEXT, &props(&[])).is_err());
    }
}
