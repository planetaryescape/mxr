use crate::cli::WebAction;
use mxr_web::{bind_listener, WebServerConfig};
use nix::errno::Errno;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const START_TIMEOUT: Duration = Duration::from_secs(5);
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

pub struct Args {
    pub action: Option<WebAction>,
    pub host: String,
    pub port: u16,
    pub print_url: bool,
    pub no_open: bool,
    pub strict_port: bool,
    pub remote_host: Option<String>,
    pub foreground: bool,
    pub detached_child: bool,
}

pub async fn run(args: Args) -> anyhow::Result<()> {
    if args.detached_child && !args.foreground {
        anyhow::bail!("internal error: --detached-child requires --foreground");
    }

    if let Some(action) = args.action {
        return match action {
            WebAction::Stop => stop_local().await,
        };
    }

    if let Some(remote) = args.remote_host {
        if args.foreground || args.detached_child {
            anyhow::bail!("--foreground is only supported for the local web bridge");
        }
        return run_remote(remote, args.print_url, args.no_open);
    }

    if args.foreground {
        run_local_foreground(args).await
    } else {
        run_local_detached(args).await
    }
}

async fn run_local_detached(args: Args) -> anyhow::Result<()> {
    let host = parse_host(&args.host)?;
    let fallback_host = display_host(host);

    if let Some(pid) = live_web_pid() {
        if let Some(port) = read_web_port() {
            let host = read_web_host().unwrap_or(fallback_host);
            let (bridge_url, display_url) = local_bridge_url(&host, port)?;
            present_launch_url(&bridge_url, &display_url, args.print_url, args.no_open);
            return Ok(());
        }

        eprintln!("mxr web bridge pid {pid} is missing its port file; restarting it");
        terminate_web_pid(pid).await?;
        clear_web_state();
    }

    clear_web_state();
    let child_pid = spawn_detached_web_child(&args)?;
    let (host, port) = wait_for_detached_web_ready(child_pid, &fallback_host).await?;
    let (bridge_url, display_url) = local_bridge_url(&host, port)?;
    present_launch_url(&bridge_url, &display_url, args.print_url, args.no_open);
    Ok(())
}

async fn run_local_foreground(args: Args) -> anyhow::Result<()> {
    let host = parse_host(&args.host)?;

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
    let auth_token = load_auth_token()?;

    let bridge_cfg = mxr_config::load_config()
        .map(|c| c.bridge)
        .unwrap_or_default();
    let config = WebServerConfig::new(mxr_config::socket_path(), auth_token.clone())
        .with_cors_allowlist(bridge_cfg.cors_allowlist.clone())
        .with_host_allowlist(bridge_cfg.host_allowlist.clone())
        .with_auto_local_token(bridge_cfg.auto_local_token);

    let display_host = display_host(addr.ip());
    if args.detached_child {
        write_web_state(&display_host, addr.port())?;
    }
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

async fn stop_local() -> anyhow::Result<()> {
    let Some(pid) = live_web_pid() else {
        clear_web_state();
        println!("mxr web bridge is not running");
        return Ok(());
    };

    terminate_web_pid(pid).await?;
    clear_web_state();
    println!("stopped mxr web bridge (pid {pid})");
    Ok(())
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

fn parse_host(host: &str) -> anyhow::Result<IpAddr> {
    host.parse()
        .map_err(|error| anyhow::anyhow!("invalid host `{host}`: {error}"))
}

fn display_host(host: IpAddr) -> String {
    if host.is_unspecified() {
        "127.0.0.1".to_string()
    } else {
        host.to_string()
    }
}

fn load_auth_token() -> anyhow::Result<String> {
    match std::env::var("MXR_WEB_BRIDGE_TOKEN") {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        _ => mxr_config::read_or_create_bridge_token().map_err(|error| {
            anyhow::anyhow!(
                "failed to read or create bridge token at {}: {error}",
                mxr_config::bridge_token_path().display()
            )
        }),
    }
}

fn local_bridge_url(host: &str, port: u16) -> anyhow::Result<(String, String)> {
    let auth_token = load_auth_token()?;
    let display_url = format!("http://{host}:{port}");
    Ok((format!("{display_url}/#token={auth_token}"), display_url))
}

fn present_launch_url(bridge_url: &str, display_url: &str, print_url: bool, no_open: bool) {
    if print_url || no_open {
        println!("{bridge_url}");
        return;
    }

    eprintln!("opening mxr web at {display_url}");
    if let Err(error) = open::that(bridge_url) {
        eprintln!("failed to open browser: {error}; visit {bridge_url} manually");
    }
}

fn spawn_detached_web_child(args: &Args) -> anyhow::Result<u32> {
    let exe = std::env::current_exe()?;
    let mut command = std::process::Command::new(exe);
    command
        .arg("web")
        .arg("--foreground")
        .arg("--detached-child")
        .arg("--no-open")
        .arg("--host")
        .arg(&args.host)
        .arg("--port")
        .arg(args.port.to_string())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    if args.strict_port {
        command.arg("--strict-port");
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    command
        .spawn()
        .map(|child| child.id())
        .map_err(|error| anyhow::anyhow!("failed to start mxr web bridge: {error}"))
}

async fn wait_for_detached_web_ready(
    pid: u32,
    fallback_host: &str,
) -> anyhow::Result<(String, u16)> {
    let deadline = Instant::now() + START_TIMEOUT;
    while Instant::now() < deadline {
        if read_web_pid() == Some(pid) {
            if let Some(port) = read_web_port() {
                let host = read_web_host().unwrap_or_else(|| fallback_host.to_string());
                return Ok((host, port));
            }
        }

        if !process_is_alive(pid) {
            clear_web_state();
            anyhow::bail!("mxr web bridge exited before it became ready");
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let _ = terminate_web_pid(pid).await;
    clear_web_state();
    anyhow::bail!("timed out waiting for mxr web bridge to start")
}

async fn terminate_web_pid(pid: u32) -> anyhow::Result<()> {
    send_signal(pid, Signal::SIGTERM)?;
    if !wait_for_process_exit(pid, STOP_TIMEOUT).await {
        send_signal(pid, Signal::SIGKILL)?;
        if !wait_for_process_exit(pid, Duration::from_secs(1)).await {
            anyhow::bail!("mxr web bridge pid {pid} did not exit cleanly");
        }
    }
    Ok(())
}

async fn wait_for_process_exit(pid: u32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !process_is_alive(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    !process_is_alive(pid)
}

fn send_signal(pid: u32, signal: Signal) -> anyhow::Result<()> {
    match kill(Pid::from_raw(pid as i32), Some(signal)) {
        Ok(()) | Err(Errno::ESRCH) => Ok(()),
        Err(error) => Err(anyhow::anyhow!(
            "failed to send {signal:?} to mxr web bridge pid {pid}: {error}"
        )),
    }
}

fn process_is_alive(pid: u32) -> bool {
    match kill(Pid::from_raw(pid as i32), None) {
        Ok(()) | Err(Errno::EPERM) => true,
        Err(Errno::ESRCH) => false,
        Err(_) => false,
    }
}

fn web_pid_path() -> PathBuf {
    mxr_config::data_dir().join("web.pid")
}

fn web_port_path() -> PathBuf {
    mxr_config::data_dir().join("web.port")
}

fn web_host_path() -> PathBuf {
    mxr_config::data_dir().join("web.host")
}

fn read_web_pid() -> Option<u32> {
    std::fs::read_to_string(web_pid_path())
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn read_web_port() -> Option<u16> {
    std::fs::read_to_string(web_port_path())
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn read_web_host() -> Option<String> {
    let host = std::fs::read_to_string(web_host_path()).ok()?;
    let host = host.trim();
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

fn live_web_pid() -> Option<u32> {
    let pid = read_web_pid()?;
    if process_is_web_bridge(pid) {
        Some(pid)
    } else {
        clear_web_state();
        None
    }
}

fn process_is_web_bridge(pid: u32) -> bool {
    if !process_is_alive(pid) {
        return false;
    }

    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output();
    let Ok(output) = output else {
        return false;
    };
    if !output.status.success() {
        return false;
    }

    let command = String::from_utf8_lossy(&output.stdout);
    command.contains(" web ") && command.contains("--detached-child")
}

fn write_web_state(host: &str, port: u16) -> std::io::Result<()> {
    write_atomic(&web_host_path(), &format!("{host}\n"))?;
    write_atomic(&web_port_path(), &format!("{port}\n"))?;
    write_atomic(&web_pid_path(), &format!("{}\n", std::process::id()))?;
    Ok(())
}

fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn clear_web_state() {
    let _ = std::fs::remove_file(web_pid_path());
    let _ = std::fs::remove_file(web_port_path());
    let _ = std::fs::remove_file(web_host_path());
}
