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
    let scoped_ref = scoped_password_ref(&password_ref);
    let password = mxr_keychain::get_password(&scoped_ref, &username)?;
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
    config.validate()?;
    if !auth_required {
        return Ok(config);
    }
    let scoped_ref = scoped_password_ref(&password_ref);
    let password = mxr_keychain::get_password(&scoped_ref, &username)?;
    Ok(config.with_password(password))
}

pub(crate) fn gmail_auth(
    client_id: String,
    client_secret: String,
    token_ref: String,
) -> mxr_provider_gmail::auth::GmailAuth {
    mxr_provider_gmail::auth::GmailAuth::new(client_id, client_secret, token_ref).with_storage(
        mxr_config::token_dir(),
        mxr_config::gmail_oauth_keychain_service(),
    )
}

pub(crate) fn outlook_auth(
    client_id: String,
    token_ref: String,
    tenant: mxr_provider_outlook::OutlookTenant,
) -> mxr_provider_outlook::OutlookAuth {
    mxr_provider_outlook::OutlookAuth::new(client_id, token_ref, tenant)
        .with_token_root(mxr_config::token_dir())
}

pub(crate) fn scoped_password_ref(password_ref: &str) -> String {
    mxr_config::scoped_credential_service(password_ref)
}

#[cfg(test)]
pub(crate) fn scoped_password_ref_for_test(password_ref: &str) -> String {
    scoped_password_ref(password_ref)
}

#[cfg(test)]
mod tests {
    #[test]
    fn password_refs_are_scoped_to_runtime_identity_before_keychain_lookup() {
        temp_env::with_var("MXR_INSTANCE", Some("mxr-dev"), || {
            assert_eq!(
                super::scoped_password_ref_for_test("mxr/work-imap"),
                "mxr-dev/mxr/work-imap"
            );
        });
    }
}
