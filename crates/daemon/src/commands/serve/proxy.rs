//! Reconcile only the exact Tailscale HTTPS port owned by this profile.
use super::state::Hosting;
use serde_json::Value;

pub(super) async fn command(program: &str, args: &[String]) -> anyhow::Result<String> {
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(15),
        tokio::process::Command::new(program)
            .args(args)
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("{program} timed out"))??;
    anyhow::ensure!(
        output.status.success(),
        "{program} failed; inspect its status and permissions (exit {})",
        output.status
    );
    Ok(String::from_utf8(output.stdout)?)
}

pub(super) async fn status() -> anyhow::Result<Value> {
    Ok(serde_json::from_str(
        &command(
            "tailscale",
            &["serve".into(), "status".into(), "--json".into()],
        )
        .await?,
    )?)
}

pub(super) fn port(config: &Hosting) -> anyhow::Result<u16> {
    url::Url::parse(&config.public_origin)?
        .port_or_known_default()
        .ok_or_else(|| anyhow::anyhow!("HTTPS port missing"))
}

pub(super) fn target(config: &Hosting) -> String {
    format!("http://{}", config.listen)
}

pub(super) fn inspect(value: &Value, config: &Hosting) -> anyhow::Result<bool> {
    let port = port(config)?.to_string();
    let tcp = value.get("TCP").and_then(|v| v.get(&port));
    let origin = url::Url::parse(&config.public_origin)?;
    let expected_address = format!("{}:{port}", origin.host_str().unwrap_or_default());
    anyhow::ensure!(value.is_object(), "Unrecognized Serve configuration shape");
    anyhow::ensure!(
        value
            .get("AllowFunnel")
            .and_then(|f| f.get(&expected_address))
            .and_then(Value::as_bool)
            != Some(true),
        "The selected HTTPS origin has Funnel enabled; private mxr hosting will not adopt it"
    );
    if let Some(foreground) = value.get("Foreground").and_then(Value::as_object) {
        for session in foreground.values() {
            let occupied = session.get("TCP").and_then(|v| v.get(&port)).is_some()
                || session
                    .get("Web")
                    .and_then(Value::as_object)
                    .is_some_and(|web| {
                        web.keys().any(|address| {
                            address.rsplit_once(':').map(|(_, p)| p) == Some(port.as_str())
                        })
                    });
            anyhow::ensure!(!occupied,"The selected HTTPS port is owned by a foreground Serve session; choose another port");
        }
    }
    let mut matching = false;
    if let Some(web) = value.get("Web").and_then(Value::as_object) {
        for (address, entry) in web {
            if address.rsplit_once(':').map(|(_, p)| p) != Some(port.as_str()) {
                continue;
            }
            anyhow::ensure!(
                address == &expected_address,
                "Serve HTTPS port belongs to another hostname; inspect its mapping before retrying"
            );
            let handlers = entry
                .get("Handlers")
                .and_then(Value::as_object)
                .ok_or_else(|| anyhow::anyhow!("Unrecognized Serve handler configuration"))?;
            anyhow::ensure!(handlers.len() == 1 && handlers.get("/").and_then(|h| h.get("Proxy")).and_then(Value::as_str) == Some(target(config).as_str()), "HTTPS port {port} is already owned by another Serve handler; choose another public port");
            anyhow::ensure!(
                config.mapping_installed,
                "An existing mapping has no mxr ownership record; choose another public port"
            );
            matching = true;
        }
    }
    anyhow::ensure!(
        tcp.is_none()
            || (matching
                && tcp.and_then(|t| t.get("HTTPS")).and_then(Value::as_bool) == Some(true)),
        "Serve port {port} conflicts with another TCP mapping"
    );
    Ok(matching)
}

pub(super) async fn prerequisites(config: &Hosting) -> anyhow::Result<()> {
    let status: Value =
        serde_json::from_str(&command("tailscale", &["status".into(), "--json".into()]).await?)?;
    anyhow::ensure!(
        status.get("BackendState").and_then(Value::as_str) == Some("Running"),
        "Tailscale must be installed and logged in before setup"
    );
    let host = status
        .pointer("/Self/DNSName")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim_end_matches('.');
    let origin = url::Url::parse(&config.public_origin)?;
    anyhow::ensure!(
        !host.is_empty() && origin.host_str() == Some(host),
        "--public-origin must use this node's Tailscale DNS name ({host})"
    );
    anyhow::ensure!(status.get("CertDomains").and_then(Value::as_array).is_some_and(|domains|domains.iter().any(|domain|domain.as_str()==Some(host))),"Enable HTTPS certificates for this node in the Tailscale admin console before setup; mxr will not change tailnet policy");
    inspect(&self::status().await?, config)?;
    Ok(())
}

pub(super) async fn publish(config: &Hosting) -> anyhow::Result<()> {
    if inspect(&status().await?, config)? {
        return Ok(());
    }
    command(
        "tailscale",
        &[
            "serve".into(),
            "--bg".into(),
            format!("--https={}", port(config)?),
            target(config),
        ],
    )
    .await?;
    anyhow::ensure!(
        inspect(&status().await?, config)?,
        "Serve did not retain the owned mapping"
    );
    Ok(())
}

pub(super) async fn remove(config: &Hosting) -> anyhow::Result<()> {
    if !inspect(&status().await?, config)? {
        return Ok(());
    }
    command(
        "tailscale",
        &[
            "serve".into(),
            format!("--https={}", port(config)?),
            "off".into(),
        ],
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_other_handlers_and_an_unrecorded_matching_proxy() {
        let mut config: Hosting = serde_json::from_value(serde_json::json!({"version":1,"executable":"/bin/mxr","config_dir":"/cfg","data_dir":"/data","socket":"/sock","instance":"f","public_origin":"https://host.ts.net:8443","listen":"127.0.0.1:18741","workspace":{"workspace_id":"w","recovery_generation":"g"},"enabled":true,"daemon_enabled":true,"mapping_installed":false,"service_installed":false})).unwrap();
        let owned = serde_json::json!({"TCP":{"8443":{"HTTPS":true}},"Web":{"host.ts.net:8443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:18741"}}}}});
        assert!(inspect(&owned, &config).is_err());
        config.mapping_installed = true;
        assert!(inspect(&owned, &config).unwrap());
        let other = serde_json::json!({"TCP":{"443":{"HTTPS":true}},"Web":{"host.ts.net:443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:9999"}}}}});
        assert!(!inspect(&other, &config).unwrap());
        let conflict = serde_json::json!({"TCP":{"8443":{"TCPForward":"127.0.0.1:33"}}});
        assert!(inspect(&conflict, &config).is_err());
        let foreground = serde_json::json!({"Foreground":{"session":{"TCP":{"8443":{"HTTPS":true}},"Web":{"host.ts.net:8443":{"Handlers":{"/":{"Proxy":"http://127.0.0.1:18741"}}}}}}});
        assert!(inspect(&foreground, &config).is_err());
        let public = serde_json::json!({"AllowFunnel":{"host.ts.net:8443":true}});
        assert!(inspect(&public, &config).is_err());
    }
}
