use mxr_web::{bind_listener, WebServerConfig};
use std::net::IpAddr;

pub struct Args {
    pub host: String,
    pub port: u16,
    pub print_url: bool,
    pub no_open: bool,
    pub strict_port: bool,
    pub remote_host: Option<String>,
}

pub async fn run(args: Args) -> anyhow::Result<()> {
    if let Some(remote) = args.remote_host {
        return run_remote(remote, args.print_url, args.no_open);
    }
    run_local(args).await
}

async fn run_local(args: Args) -> anyhow::Result<()> {
    let host: IpAddr = args
        .host
        .parse()
        .map_err(|error| anyhow::anyhow!("invalid host `{}`: {error}", args.host))?;

    // Retry on EADDRINUSE unless the caller explicitly asked for strict
    // port binding. The default is retry — a colliding port shouldn't
    // be the difference between the user seeing their inbox and not.
    let listener = bind_listener(host, args.port, !args.strict_port)
        .await
        .map_err(|error| {
            anyhow::anyhow!(
                "failed to bind {}:{}: {error}{}",
                args.host,
                args.port,
                if args.strict_port {
                    "\n(pass `--strict-port` only when you really need that exact port)"
                } else {
                    ""
                }
            )
        })?;
    let addr = listener.local_addr()?;
    if addr.port() != args.port {
        eprintln!(
            "configured port {} was in use; bound {} instead",
            args.port,
            addr.port()
        );
    }
    if let Err(err) = mxr_config::write_bridge_port(addr.port()) {
        eprintln!("warning: could not write bridge-port file ({err})");
    }

    // Token precedence: env override > persisted file > generate-and-persist.
    let auth_token = match std::env::var("MXR_WEB_BRIDGE_TOKEN") {
        Ok(value) if !value.trim().is_empty() => value,
        _ => mxr_config::read_or_create_bridge_token().map_err(|error| {
            anyhow::anyhow!(
                "failed to read or create bridge token at {}: {error}",
                mxr_config::bridge_token_path().display()
            )
        })?,
    };

    let bridge_cfg = mxr_config::load_config()
        .map(|c| c.bridge)
        .unwrap_or_default();
    let config = WebServerConfig::new(mxr_config::socket_path(), auth_token.clone())
        .with_cors_allowlist(bridge_cfg.cors_allowlist.clone())
        .with_host_allowlist(bridge_cfg.host_allowlist.clone())
        .with_auto_local_token(bridge_cfg.auto_local_token);

    let display_host = if addr.ip().is_unspecified() {
        "127.0.0.1".to_string()
    } else {
        addr.ip().to_string()
    };
    let bridge_url = format!("http://{display_host}:{}/#token={auth_token}", addr.port());

    if args.print_url {
        println!("{bridge_url}");
        eprintln!(
            "mxr web bridge listening on http://{display_host}:{}",
            addr.port()
        );
    } else {
        eprintln!(
            "mxr web bridge listening on http://{display_host}:{} (token cached at {})",
            addr.port(),
            mxr_config::bridge_token_path().display()
        );
        if !args.no_open {
            let url = bridge_url.clone();
            tokio::spawn(async move {
                if let Err(error) = open::that(&url) {
                    eprintln!("failed to open browser: {error}; visit {url} manually");
                }
            });
        } else {
            println!("{bridge_url}");
        }
    }

    mxr_web::serve(listener, config)
        .await
        .map_err(anyhow::Error::from)
}

fn run_remote(remote: String, print_url: bool, no_open: bool) -> anyhow::Result<()> {
    let host_only = remote.split(':').next().unwrap_or(&remote).to_string();
    let path = mxr_config::remote_bridge_token_path(&host_only);
    let token = std::fs::read_to_string(&path)
        .map_err(|error| {
            anyhow::anyhow!(
                "failed to read remote bridge token at {}: {error}\n\nPlace the token there with mode 0600 to authorize this client.",
                path.display()
            )
        })?
        .trim()
        .to_string();
    if token.is_empty() {
        anyhow::bail!("remote bridge token at {} is empty", path.display());
    }

    let scheme = if remote.starts_with("localhost") || remote.starts_with("127.") {
        "http"
    } else {
        "https"
    };
    let bridge_url = format!("{scheme}://{remote}/#token={token}&remote={remote}");

    if print_url || no_open {
        println!("{bridge_url}");
    } else {
        eprintln!("opening browser to {scheme}://{remote}");
        if let Err(error) = open::that(&bridge_url) {
            eprintln!("failed to open browser: {error}; visit {bridge_url} manually");
        }
    }
    Ok(())
}
