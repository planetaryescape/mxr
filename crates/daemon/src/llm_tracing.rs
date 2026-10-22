//! Sends traces of model calls (see `mxr_llm::trace`) to a Langfuse on this
//! machine, over OTLP/HTTP.
//!
//! Traces hold whole prompts and replies, mail included, so mxr's "local
//! only, no telemetry" rule decides where they may go: only a Langfuse whose
//! URL is provably loopback (the same test as for LLM endpoints). Its HTTP
//! client uses no proxy and follows no redirect, so neither `HTTP_PROXY` nor
//! a 307 can carry a trace off the machine.
//!
//! Configured with Langfuse's own variables, read by the daemon at start:
//! `LANGFUSE_BASE_URL` (or `LANGFUSE_HOST`), `LANGFUSE_PUBLIC_KEY`,
//! `LANGFUSE_SECRET_KEY`, and optionally `LANGFUSE_TRACING_ENVIRONMENT`.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use opentelemetry_otlp::{Protocol, SpanExporter, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::Resource;
use std::collections::HashMap;
use std::time::Duration;

const EXPORT_TIMEOUT: Duration = Duration::from_secs(10);

/// The exporter while it runs. Call [`Self::shutdown`] on the way out, or
/// the last few seconds of traces are lost.
pub(crate) struct LlmTracing {
    provider: SdkTracerProvider,
}

#[derive(Debug, PartialEq, Eq)]
enum Settings {
    Off,
    Refused(String),
    On {
        base_url: String,
        public_key: String,
        secret_key: String,
        environment: Option<String>,
    },
}

/// Start exporting if Langfuse is configured for this machine. Logs why
/// not when it is configured but refused.
pub(crate) fn start() -> Option<LlmTracing> {
    match settings(|name| std::env::var(name).ok()) {
        Settings::Off => None,
        Settings::Refused(reason) => {
            tracing::warn!("LLM tracing is off: {reason}");
            None
        }
        Settings::On {
            base_url,
            public_key,
            secret_key,
            environment,
        } => match exporter(&base_url, &public_key, &secret_key) {
            Ok(exporter) => {
                let provider = SdkTracerProvider::builder()
                    .with_batch_exporter(exporter)
                    .with_resource(Resource::builder().with_service_name("mxr").build())
                    .build();
                opentelemetry::global::set_tracer_provider(provider.clone());
                mxr_llm::trace::set_shared_attributes(
                    environment,
                    env!("CARGO_PKG_VERSION").to_string(),
                );
                tracing::info!("LLM tracing to the Langfuse at {base_url}");
                Some(LlmTracing { provider })
            }
            Err(error) => {
                tracing::warn!("LLM tracing is off: couldn't start the exporter: {error}");
                None
            }
        },
    }
}

impl LlmTracing {
    /// Send what is still buffered. Blocks on the export, so it runs off
    /// the async workers.
    pub(crate) async fn shutdown(self) {
        let provider = self.provider;
        match tokio::task::spawn_blocking(move || provider.shutdown()).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!("LLM tracing didn't flush on shutdown: {error}"),
            Err(error) => tracing::warn!("LLM tracing shutdown task failed: {error}"),
        }
    }
}

fn settings(var: impl Fn(&str) -> Option<String>) -> Settings {
    let var = |name: &str| var(name).filter(|value| !value.trim().is_empty());
    let (public_key, secret_key) = match (var("LANGFUSE_PUBLIC_KEY"), var("LANGFUSE_SECRET_KEY")) {
        (None, None) => return Settings::Off,
        (Some(public_key), Some(secret_key)) => (public_key, secret_key),
        _ => {
            return Settings::Refused(
                "set both LANGFUSE_PUBLIC_KEY and LANGFUSE_SECRET_KEY".to_string(),
            )
        }
    };
    let Some(base_url) = var("LANGFUSE_BASE_URL").or_else(|| var("LANGFUSE_HOST")) else {
        return Settings::Refused(
            "LANGFUSE_BASE_URL isn't set; mxr sends traces only to a Langfuse on this machine, \
             e.g. http://localhost:3000"
                .to_string(),
        );
    };
    let base_url = base_url.trim().trim_end_matches('/').to_string();
    if !mxr_llm::is_loopback_endpoint(&base_url) {
        return Settings::Refused(format!(
            "{base_url} isn't on this machine. Traces hold your mail, so mxr sends them only to \
             a Langfuse on localhost"
        ));
    }
    Settings::On {
        base_url,
        public_key,
        secret_key,
        environment: var("LANGFUSE_TRACING_ENVIRONMENT"),
    }
}

fn exporter(base_url: &str, public_key: &str, secret_key: &str) -> Result<SpanExporter, String> {
    // reqwest's blocking client starts its own runtime, which panics when
    // built (or dropped) on an async worker.
    let client = std::thread::spawn(|| {
        reqwest::blocking::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(EXPORT_TIMEOUT)
            .build()
    })
    .join()
    .map_err(|_| "the HTTP client thread panicked".to_string())?
    .map_err(|error| error.to_string())?;
    let credentials = STANDARD.encode(format!("{public_key}:{secret_key}"));
    let headers = HashMap::from([
        ("Authorization".to_string(), format!("Basic {credentials}")),
        // Without it, Langfuse v4 shows OTel spans up to ten minutes late.
        ("x-langfuse-ingestion-version".to_string(), "4".to_string()),
    ]);
    SpanExporter::builder()
        .with_http()
        .with_http_client(client)
        .with_protocol(Protocol::HttpBinary)
        // Set here, it wins over any OTEL_EXPORTER_OTLP_* endpoint variable.
        .with_endpoint(format!("{base_url}/api/public/otel/v1/traces"))
        .with_timeout(EXPORT_TIMEOUT)
        .with_headers(headers)
        .build()
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{settings, Settings};

    fn with(vars: &[(&str, &str)]) -> Settings {
        settings(|name| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| (*value).to_string())
        })
    }

    const KEYS: [(&str, &str); 2] = [
        ("LANGFUSE_PUBLIC_KEY", "pk-lf-1"),
        ("LANGFUSE_SECRET_KEY", "sk-lf-1"),
    ];

    #[test]
    fn no_keys_is_off_without_a_warning() {
        assert_eq!(
            with(&[("LANGFUSE_BASE_URL", "http://localhost:3000")]),
            Settings::Off
        );
    }

    #[test]
    fn a_local_langfuse_is_used() {
        let mut vars = KEYS.to_vec();
        vars.push(("LANGFUSE_BASE_URL", "http://127.0.0.1:3000/"));
        vars.push(("LANGFUSE_TRACING_ENVIRONMENT", "development"));
        assert_eq!(
            with(&vars),
            Settings::On {
                base_url: "http://127.0.0.1:3000".to_string(),
                public_key: "pk-lf-1".to_string(),
                secret_key: "sk-lf-1".to_string(),
                environment: Some("development".to_string()),
            }
        );
    }

    #[test]
    fn langfuse_host_is_read_when_base_url_is_unset() {
        let mut vars = KEYS.to_vec();
        vars.push(("LANGFUSE_HOST", "http://localhost:3000"));
        assert!(
            matches!(with(&vars), Settings::On { base_url, .. } if base_url == "http://localhost:3000")
        );
    }

    #[test]
    fn langfuse_cloud_and_lookalike_hosts_are_refused() {
        for url in [
            "https://cloud.langfuse.com",
            "https://us.cloud.langfuse.com",
            "http://localhost.evil.example:3000",
            "http://localhost@evil.example",
            "http://192.168.1.20:3000",
        ] {
            let mut vars = KEYS.to_vec();
            vars.push(("LANGFUSE_BASE_URL", url));
            assert!(
                matches!(with(&vars), Settings::Refused(reason) if reason.contains("isn't on this machine")),
                "{url} should be refused"
            );
        }
    }

    #[test]
    fn keys_without_a_url_are_refused_rather_than_sent_to_a_default() {
        assert!(
            matches!(with(&KEYS), Settings::Refused(reason) if reason.contains("LANGFUSE_BASE_URL"))
        );
    }

    #[test]
    fn one_key_alone_is_refused() {
        assert!(matches!(
            with(&[("LANGFUSE_PUBLIC_KEY", "pk"), ("LANGFUSE_BASE_URL", "http://localhost:3000")]),
            Settings::Refused(reason) if reason.contains("both")
        ));
    }
}
