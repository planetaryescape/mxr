pub(crate) fn imap_config_with_credentials(
    host: String,
    port: u16,
    username: String,
    password_ref: String,
    auth_required: bool,
    use_tls: bool,
) -> anyhow::Result<mxr_provider_imap::config::ImapConfig> {
    let config = mxr_provider_imap::config::ImapConfig::new(
        host,
        port,
        username.clone(),
        password_ref.clone(),
        auth_required,
        use_tls,
    );
    if !auth_required {
        return Ok(config);
    }
    let password = mxr_keychain::get_password(&password_ref, &username)?;
    Ok(config.with_password(password))
}

pub(crate) fn smtp_config_with_credentials(
    host: String,
    port: u16,
    username: String,
    password_ref: String,
    auth_required: bool,
    use_tls: bool,
) -> anyhow::Result<mxr_provider_smtp::config::SmtpConfig> {
    let config = mxr_provider_smtp::config::SmtpConfig::new(
        host,
        port,
        username.clone(),
        password_ref.clone(),
        auth_required,
        use_tls,
    );
    if !auth_required {
        return Ok(config);
    }
    let password = mxr_keychain::get_password(&password_ref, &username)?;
    Ok(config.with_password(password))
}
