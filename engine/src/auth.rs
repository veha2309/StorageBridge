use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use parking_lot::Mutex;
use rand::RngCore;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::TcpListener,
    path::PathBuf,
    time::{Duration, Instant},
};
use url::Url;

#[derive(Clone, Serialize, Deserialize)]
pub struct OAuthClient {
    pub client_id: String,
    pub client_secret: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct Tokens {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
    #[serde(default)]
    account_id: String,
}
#[derive(Deserialize)]
struct TokenReply {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: i64,
}

pub struct Auth {
    root: PathBuf,
    tokens: Mutex<Option<Tokens>>,
    pub login_state: Mutex<String>,
    http: Client,
}
fn random() -> String {
    let mut bytes = [0; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
impl Auth {
    #[cfg(test)]
    pub(crate) fn for_test(root: PathBuf) -> Self {
        Self {
            root,
            tokens: Mutex::new(Some(Tokens {
                access_token: "fixture-token".into(),
                refresh_token: "fixture-refresh".into(),
                expires_at: i64::MAX,
                account_id: "fixture-account".into(),
            })),
            login_state: Mutex::new("idle".into()),
            http: Client::new(),
        }
    }
    pub(crate) fn storage_path(&self) -> &std::path::Path {
        &self.root
    }
    pub fn new(root: PathBuf) -> Result<Self> {
        let saved = load_secret(&root.join("tokens.dpapi"))
            .ok()
            .and_then(|s| serde_json::from_slice(&s).ok());
        Ok(Self {
            root,
            tokens: Mutex::new(saved),
            login_state: Mutex::new("idle".into()),
            http: Client::builder().timeout(Duration::from_secs(30)).build()?,
        })
    }
    pub fn connected(&self) -> bool {
        self.tokens
            .try_lock()
            .map(|tokens| tokens.is_some())
            .unwrap_or(true)
    }
    pub fn configure(&self, json: &str) -> Result<()> {
        let document: serde_json::Value = serde_json::from_str(json)?;
        let client: OAuthClient = serde_json::from_value(
            document
                .get("installed")
                .context("Choose a Google Desktop app OAuth client JSON file")?
                .clone(),
        )?;
        anyhow::ensure!(
            client.client_id.ends_with(".apps.googleusercontent.com"),
            "Invalid Google client ID"
        );
        save_secret(
            &self.root.join("client.dpapi"),
            &serde_json::to_vec(&client)?,
        )
    }
    pub fn configured(&self) -> bool {
        self.root.join("client.dpapi").exists()
    }
    fn client(&self) -> Result<OAuthClient> {
        Ok(serde_json::from_slice(
            &load_secret(&self.root.join("client.dpapi"))
                .context("Import your Google desktop OAuth client JSON first")?,
        )?)
    }
    fn save(&self, tokens: Tokens) -> Result<()> {
        save_secret(
            &self.root.join("tokens.dpapi"),
            &serde_json::to_vec(&tokens)?,
        )?;
        *self.tokens.lock() = Some(tokens);
        Ok(())
    }
    pub fn bearer(&self) -> Result<String> {
        let mut guard = self.tokens.lock();
        let tokens = guard.as_mut().context("Sign in to Google to connect")?;
        if tokens.expires_at > chrono::Utc::now().timestamp() + 60 {
            return Ok(tokens.access_token.clone());
        }
        let client = self.client()?;
        let response = self
            .http
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("client_id", client.client_id.as_str()),
                ("client_secret", client.client_secret.as_str()),
                ("refresh_token", tokens.refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()?;
        if !response.status().is_success() {
            bail!(
                "Google session expired or refresh failed (HTTP {}); sign in again",
                response.status()
            );
        }
        let reply: TokenReply = response.json()?;
        tokens.access_token = reply.access_token;
        tokens.expires_at = chrono::Utc::now().timestamp() + reply.expires_in;
        save_secret(
            &self.root.join("tokens.dpapi"),
            &serde_json::to_vec(tokens)?,
        )?;
        Ok(tokens.access_token.clone())
    }
    pub fn login(&self) -> Result<()> {
        let client = self.client()?;
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let redirect = format!(
            "http://127.0.0.1:{}/callback",
            listener.local_addr()?.port()
        );
        let verifier = random();
        let state = random();
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let mut url = Url::parse("https://accounts.google.com/o/oauth2/v2/auth")?;
        url.query_pairs_mut().extend_pairs([
            ("client_id", client.client_id.as_str()),
            ("redirect_uri", redirect.as_str()),
            ("response_type", "code"),
            ("scope", "https://www.googleapis.com/auth/drive"),
            ("access_type", "offline"),
            ("prompt", "consent"),
            ("state", state.as_str()),
            ("code_challenge", challenge.as_str()),
            ("code_challenge_method", "S256"),
        ]);
        webbrowser::open(url.as_str()).context("Could not open the system browser")?;
        let deadline = Instant::now() + Duration::from_secs(180);
        let code = loop {
            anyhow::ensure!(Instant::now() < deadline, "Sign-in timed out; try again");
            match listener.accept() {
                Ok((mut stream, _)) => {
                    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
                    let mut bytes = [0; 8192];
                    let n = match stream.read(&mut bytes) {
                        Ok(n) => n,
                        Err(_) => continue,
                    };
                    let request = String::from_utf8_lossy(&bytes[..n]);
                    let target = request
                        .lines()
                        .next()
                        .unwrap_or("")
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or("");
                    let Ok(callback) = Url::parse(&format!("http://localhost{target}")) else {
                        continue;
                    };
                    let pairs: std::collections::HashMap<_, _> =
                        callback.query_pairs().into_owned().collect();
                    if callback.path() != "/callback" || pairs.get("state") != Some(&state) {
                        let _ = stream.write_all(b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\nContent-Length: 0\r\n\r\n");
                        continue;
                    }
                    let body = "You can return to Storage Bridge.";
                    let _ = write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    if pairs.contains_key("error") {
                        bail!("Google sign-in was cancelled or denied");
                    }
                    break pairs
                        .get("code")
                        .context("Google did not return an authorization code")?
                        .clone();
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(100))
                }
                Err(e) => return Err(e.into()),
            }
        };
        let response = self
            .http
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("client_id", client.client_id.as_str()),
                ("client_secret", client.client_secret.as_str()),
                ("code", code.as_str()),
                ("redirect_uri", redirect.as_str()),
                ("grant_type", "authorization_code"),
                ("code_verifier", verifier.as_str()),
            ])
            .send()?;
        anyhow::ensure!(
            response.status().is_success(),
            "Google token exchange failed (HTTP {})",
            response.status()
        );
        let reply: TokenReply = response.json()?;
        let identity_response = self
            .http
            .get("https://www.googleapis.com/drive/v3/about")
            .bearer_auth(&reply.access_token)
            .query(&[("fields", "user(permissionId)")])
            .send()?;
        anyhow::ensure!(
            identity_response.status().is_success(),
            "Could not verify Google Drive account identity"
        );
        let identity: serde_json::Value = identity_response.json()?;
        let account_id = identity["user"]["permissionId"]
            .as_str()
            .context("Google did not return an account identity")?
            .to_string();
        let binding = self.root.join("account.dpapi");
        if binding.exists() {
            anyhow::ensure!(
                load_secret(&binding)? == account_id.as_bytes(),
                "This cache belongs to a different Google account. Sign in with the original account."
            );
        }
        if let Some(old) = self.tokens.lock().as_ref() {
            anyhow::ensure!(
                old.account_id.is_empty() || old.account_id == account_id,
                "This cache belongs to a different Google account. Sign in with the original account."
            );
        }
        save_secret(&binding, account_id.as_bytes())?;
        self.save(Tokens {
            access_token: reply.access_token,
            refresh_token: reply
                .refresh_token
                .context("No refresh token returned; grant offline access")?,
            expires_at: chrono::Utc::now().timestamp() + reply.expires_in,
            account_id,
        })
    }
}
pub fn save_secret(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    #[cfg(windows)]
    {
        let encrypted = crate::platform::protect(bytes, false)?;
        let staging = path.with_extension("tmp");
        std::fs::write(&staging, encrypted)?;
        std::fs::OpenOptions::new()
            .write(true)
            .open(&staging)?
            .sync_all()?;
        // Windows rename cannot replace an existing file; MoveFileEx is atomic.
        crate::platform::replace_file(&staging, path)?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (path, bytes);
        bail!("Credential storage is not implemented for this platform")
    }
}
pub(crate) fn load_secret(path: &std::path::Path) -> Result<Vec<u8>> {
    #[cfg(windows)]
    {
        crate::platform::protect(&std::fs::read(path)?, true)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        bail!("Credential storage is not implemented for this platform")
    }
}
