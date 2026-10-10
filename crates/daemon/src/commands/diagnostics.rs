pub(crate) fn selected_target_metadata() -> anyhow::Result<(serde_json::Value, bool)> {
    let configured =
        std::env::var(mxr_transport::DAEMON_ADDR_ENV).is_ok_and(|value| !value.trim().is_empty());
    let address = crate::server::resolve_daemon_addr()?;
    let (kind, display) = match address {
        mxr_transport::TransportAddr::Unix(path) => ("unix", format!("unix://{}", path.display())),
        mxr_transport::TransportAddr::Tcp(addr) => ("tcp", format!("tcp://{addr}")),
        mxr_transport::TransportAddr::Cmd(_) => ("cmd", "cmd://<command hidden>".to_string()),
    };
    Ok((
        serde_json::json!({"kind": kind, "display": display, "configured": configured}),
        configured,
    ))
}

pub(super) fn client_local_metadata() -> anyhow::Result<serde_json::Value> {
    Ok(serde_json::json!({
        "runtime_instance": mxr_config::app_instance_name(),
        "config_path": mxr_config::config_file_path(),
        "data_dir": mxr_config::data_dir(),
        // MXR_DAEMON_ADDR may select a different Unix socket. This field
        // always describes the client's own default profile.
        "socket_path": mxr_config::socket_path(),
    }))
}
