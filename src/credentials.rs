pub(crate) fn callback(
    url: &str,
    username_from_url: Option<&str>,
    allowed_types: &git2::CredentialType,
    repo: &git2::Repository,
) -> Result<git2::Cred, git2::Error> {
    if allowed_types.is_ssh_key() {
        if let Some(username) = username_from_url {
            return git2::Cred::ssh_key_from_agent(username);
        } else {
            return Err(git2::Error::from_str("No username for SSH key auth"));
        }
    }
    // Try credential helpers for HTTPS/HTTP
    if allowed_types.is_user_pass_plaintext() {
        if let Ok(config) = repo.config() {
            if let Ok(cred) = git2::Cred::credential_helper(&config, url, username_from_url) {
                return Ok(cred);
            }
        }
    }
    // Fallback to default
    git2::Cred::default()
}
