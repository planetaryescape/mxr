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
    pub auto_port: bool,
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
    validate_local_bind_host(host)?;
    let fallback_host = display_host(host);

    if let Some((host, port)) = existing_bridge(&fallback_host, args.port).await {
        let (bridge_url, display_url) = local_bridge_url(&host, port)?;
        present_launch_url(&bridge_url, &display_url, args.print_url, args.no_open);
        return Ok(());
    }

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
    validate_local_bind_host(host)?;

    let listener = bind_listener(host, args.port, should_retry_port(&args))
        .await
        .map_err(|error| {
            anyhow::anyhow!(bind_error_message(
                &args.host,
                args.port,
                args.auto_port,
                &error
            ))
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
    let bridge_url = format!("http://{display_host}:{}", addr.port());

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

fn validate_local_bind_host(host: IpAddr) -> anyhow::Result<()> {
    if host.is_loopback() {
        Ok(())
    } else {
        anyhow::bail!(
            "local web bridge bind address {host} is non-loopback; public/LAN bridge serving is reserved for future TLS remote mode"
        )
    }
}

fn display_host(host: IpAddr) -> String {
    match host {
        IpAddr::V4(ip) if ip == std::net::Ipv4Addr::LOCALHOST => "mxr.localhost".to_string(),
        IpAddr::V4(ip) if ip.is_unspecified() => "127.0.0.1".to_string(),
        IpAddr::V6(ip) if ip == std::net::Ipv6Addr::LOCALHOST => "[::1]".to_string(),
        IpAddr::V6(ip) => format!("[{ip}]"),
        IpAddr::V4(ip) => ip.to_string(),
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
    let display_url = format!("http://{host}:{port}");
    Ok((display_url.clone(), display_url))
}

fn should_retry_port(args: &Args) -> bool {
    args.auto_port && !args.strict_port
}

async fn existing_bridge(fallback_host: &str, requested_port: u16) -> Option<(String, u16)> {
    let mut probed_cached_port = None;
    if let Some(port) = mxr_config::read_bridge_port() {
        probed_cached_port = Some(port);
        if probe_bridge_health("127.0.0.1", port).await {
            let host = display_host(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
            return Some((host, port));
        }
        // The cached port no longer responds. Clear it, then also check the
        // configured port below: a previous probe may have cleared this file
        // while the daemon bridge stayed healthy and kept the stable port.
        mxr_config::clear_bridge_port();
    }

    if probed_cached_port != Some(requested_port)
        && probe_bridge_health("127.0.0.1", requested_port).await
    {
        let _ = mxr_config::write_bridge_port(requested_port);
        return Some((fallback_host.to_string(), requested_port));
    }

    None
}

fn bind_error_message(host: &str, port: u16, auto_port: bool, error: &std::io::Error) -> String {
    let mut message = format!("failed to bind {host}:{port}: {error}");
    if let Some(owner) = port_owner_description(port) {
        message.push_str(&format!("\nport {port} appears to be used by {owner}"));
    }
    if !auto_port {
        message.push_str(
            "\nmxr web uses a fixed local URL by default; stop the process using this port or pass `--auto-port` to try the next available port.",
        );
    }
    message
}

fn port_owner_description(port: u16) -> Option<String> {
    lsof_port_owner(port).or_else(|| ss_port_owner(port))
}

async fn probe_bridge_health(host: &str, port: u16) -> bool {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let Ok(Ok(mut stream)) = tokio::time::timeout(
        Duration::from_millis(500),
        tokio::net::TcpStream::connect((host, port)),
    )
    .await
    else {
        return false;
    };
    let request =
        format!("GET /api/v1/health HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n");
    if tokio::time::timeout(
        Duration::from_millis(500),
        stream.write_all(request.as_bytes()),
    )
    .await
    .ok()
    .and_then(Result::ok)
    .is_none()
    {
        return false;
    }
    let mut buf = [0_u8; 512];
    let Ok(Ok(read)) =
        tokio::time::timeout(Duration::from_millis(500), stream.read(&mut buf)).await
    else {
        return false;
    };
    let status = std::str::from_utf8(&buf[..read]).unwrap_or_default();
    (status.starts_with("HTTP/1.1 200") || status.starts_with("HTTP/1.0 200"))
        && status.contains("mxr-bridge")
}

fn lsof_port_owner(port: u16) -> Option<String> {
    let output = std::process::Command::new("lsof")
        .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().skip(1).find_map(|line| {
        let mut parts = line.split_whitespace();
        let command = parts.next()?;
        let pid = parts.next()?;
        Some(format!("{command} (pid {pid})"))
    })
}

fn ss_port_owner(port: u16) -> Option<String> {
    let output = std::process::Command::new("ss")
        .args(["-ltnp", &format!("sport = :{port}")])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .skip(1)
        .find(|line| line.contains(&format!(":{port}")))
        .map(|line| line.trim().to_string())
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
    // Capture stderr so a child that fails to bind / connect can still
    // surface its error after the parent times out. Previously this
    // went to /dev/null, leaving "timed out waiting for mxr web bridge
    // to start" as the only signal — and no way to tell why.
    let stderr_path = web_child_stderr_path();
    let stderr_target = std::fs::File::options()
        .append(true)
        .create(true)
        .open(&stderr_path)
        .map_or_else(|_| std::process::Stdio::null(), std::process::Stdio::from);
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
        .stderr(stderr_target);

    if args.strict_port {
        command.arg("--strict-port");
    }
    if args.auto_port {
        command.arg("--auto-port");
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

fn web_child_stderr_path() -> PathBuf {
    mxr_config::data_dir().join("logs").join("mxr-web.log")
}

/// Read the last few lines of the detached child's stderr log so a
/// startup failure can be surfaced to the user instead of swallowed as
/// a bare timeout. Returns `None` if the log is missing or empty.
fn read_recent_web_child_stderr() -> Option<String> {
    let contents = std::fs::read_to_string(web_child_stderr_path()).ok()?;
    let trimmed = contents.trim_end();
    if trimmed.is_empty() {
        return None;
    }
    let tail: Vec<&str> = trimmed.lines().rev().take(8).collect();
    if tail.is_empty() {
        None
    } else {
        Some(tail.into_iter().rev().collect::<Vec<_>>().join("\n"))
    }
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
    let detail = read_recent_web_child_stderr()
        .map(|tail| format!("\nLast detached-child stderr lines:\n{tail}"))
        .unwrap_or_default();
    anyhow::bail!(
        "timed out waiting for mxr web bridge to start. Check `{}` for the child's logs; common cause is the bridge port being held by another process — try `mxr web --auto-port` or change `[bridge].port` in `~/.config/mxr/config.toml`.{detail}",
        web_child_stderr_path().display()
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn default_loopback_launch_url_uses_mxr_localhost_without_token_fragment() {
        let host = display_host(IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(host, "mxr.localhost");

        temp_env::with_var("MXR_WEB_BRIDGE_TOKEN", Some("test-token"), || {
            let (launch_url, display_url) = local_bridge_url(&host, 42829).unwrap();
            assert_eq!(display_url, "http://mxr.localhost:42829");
            assert_eq!(launch_url, display_url);
        });
    }

    #[test]
    fn local_foreground_fails_fast_when_fixed_port_is_busy() {
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let temp = tempfile::TempDir::new().unwrap();
        let config_dir = temp.path().join("config");
        let data_dir = temp.path().join("data");
        let socket_path = temp.path().join("mxr.sock");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        temp_env::with_vars(
            [
                ("MXR_CONFIG_DIR", Some(config_dir)),
                ("MXR_DATA_DIR", Some(data_dir)),
                ("MXR_SOCKET_PATH", Some(socket_path)),
                ("MXR_WEB_BRIDGE_TOKEN", Some(temp.path().join("token"))),
            ],
            || {
                let result = runtime.block_on(async {
                    tokio::time::timeout(
                        Duration::from_secs(2),
                        run_local_foreground(Args {
                            action: None,
                            host: "127.0.0.1".into(),
                            port,
                            print_url: false,
                            no_open: true,
                            auto_port: false,
                            strict_port: false,
                            remote_host: None,
                            foreground: true,
                            detached_child: false,
                        }),
                    )
                    .await
                });

                let error = result
                    .expect("fixed-port conflict should fail, not hang")
                    .expect_err("fixed-port conflict should return an error");
                let message = error.to_string();
                assert!(message.contains(&format!("127.0.0.1:{port}")));
                assert!(message.contains("--auto-port"));
            },
        );
    }

    #[test]
    fn local_web_rejects_non_loopback_bind_hosts() {
        assert!(validate_local_bind_host(IpAddr::V4(Ipv4Addr::LOCALHOST)).is_ok());
        let error = validate_local_bind_host(IpAddr::V4(Ipv4Addr::UNSPECIFIED)).unwrap_err();
        assert!(
            error.to_string().contains("non-loopback"),
            "non-loopback bind rejection should explain the safety boundary: {error}"
        );
    }

    #[test]
    fn strict_port_conflict_message_points_to_auto_port_escape_hatch() {
        let error = std::io::Error::new(std::io::ErrorKind::AddrInUse, "address in use");
        let message = bind_error_message("127.0.0.1", 42829, false, &error);

        assert!(message.contains("127.0.0.1:42829"));
        assert!(message.contains("--auto-port"));
        assert!(message.contains("fixed local URL"));
    }

    #[tokio::test]
    async fn bridge_health_probe_detects_running_local_bridge() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0_u8; 1024];
            let _ = stream.read(&mut buf).await.unwrap();
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 24\r\n\r\n{\"service\":\"mxr-bridge\"}",
                )
                .await
                .unwrap();
        });

        assert!(probe_bridge_health("127.0.0.1", addr.port()).await);
        server.await.unwrap();
    }

    #[tokio::test]
    async fn bridge_health_probe_rejects_non_mxr_healthy_service() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0_u8; 1024];
            let _ = stream.read(&mut buf).await.unwrap();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok")
                .await
                .unwrap();
        });

        assert!(!probe_bridge_health("127.0.0.1", addr.port()).await);
        server.await.unwrap();
    }

    #[test]
    fn existing_bridge_reuses_requested_port_when_cache_is_missing() {
        let temp = tempfile::TempDir::new().unwrap();
        let config_dir = temp.path().join("config");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        temp_env::with_var("MXR_CONFIG_DIR", Some(&config_dir), || {
            runtime.block_on(async {
                let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                    .await
                    .unwrap();
                let port = listener.local_addr().unwrap().port();
                let server = spawn_health_server(listener, true);
                let host = display_host(IpAddr::V4(Ipv4Addr::LOCALHOST));

                let result = existing_bridge(&host, port).await;
                assert_eq!(result, Some((host, port)));
                assert_eq!(mxr_config::read_bridge_port(), Some(port));
                server.await.unwrap();
            });
        });
    }

    #[test]
    fn existing_bridge_falls_back_from_stale_cache_to_requested_port() {
        let temp = tempfile::TempDir::new().unwrap();
        let config_dir = temp.path().join("config");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        temp_env::with_var("MXR_CONFIG_DIR", Some(&config_dir), || {
            runtime.block_on(async {
                let stale_listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                    .await
                    .unwrap();
                let stale_port = stale_listener.local_addr().unwrap().port();
                let stale_server = spawn_health_server(stale_listener, false);
                let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
                    .await
                    .unwrap();
                let port = listener.local_addr().unwrap().port();
                let server = spawn_health_server(listener, true);
                mxr_config::write_bridge_port(stale_port).unwrap();
                let host = display_host(IpAddr::V4(Ipv4Addr::LOCALHOST));

                let result = existing_bridge(&host, port).await;
                assert_eq!(result, Some((host, port)));
                assert_eq!(mxr_config::read_bridge_port(), Some(port));
                stale_server.await.unwrap();
                server.await.unwrap();
            });
        });
    }

    fn spawn_health_server(
        listener: tokio::net::TcpListener,
        mxr_bridge: bool,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};

            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0_u8; 1024];
            let _ = stream.read(&mut buf).await.unwrap();
            let body = if mxr_bridge {
                r#"{"service":"mxr-bridge"}"#
            } else {
                r#"{"service":"other"}"#
            };
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        })
    }
}
