//! Native public-client OAuth. Credentials never belong in a job or repository.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::{
    ffi::{CStr, CString},
    fs::File,
    io::{Read, Write},
    os::unix::{
        fs::MetadataExt,
        io::{AsRawFd, FromRawFd},
    },
    path::{Component, Path},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    Storage,
    Busy,
    SignedOut,
    InvalidToken,
    Permission,
    Callback,
    Timeout,
    Denied,
    Network,
    Reauthorize,
    InvalidResponse,
}
impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Storage => "credential storage is unavailable or unsafe",
            Self::Busy => "another authentication operation is in progress",
            Self::SignedOut => "ChatGPT is not signed in",
            Self::InvalidToken => "ID token validation failed",
            Self::Permission => "ChatGPT subscription permission was not granted",
            Self::Callback => "authorization callback rejected",
            Self::Timeout => "authorization callback timed out",
            Self::Denied => "authorization was declined",
            Self::Network => "authentication service request failed",
            Self::Reauthorize => "refresh outcome is uncertain or expired; sign in again",
            Self::InvalidResponse => "authentication service response rejected",
        })
    }
}
impl std::error::Error for AuthError {}
type Result<T> = std::result::Result<T, AuthError>;
const MAX_JSON: u64 = 1024 * 1024;

/// Not Debug: even internal state must not accidentally print credentials.
pub struct AuthStore {
    dir: File,
    #[cfg(test)]
    test_base: Option<String>,
    #[cfg(test)]
    callback_timeout: Duration,
}
#[derive(Serialize, Deserialize)]
struct Saved {
    host_id: String,
    #[serde(default)]
    registration: Option<Registration>,
    #[serde(default)]
    credentials: Option<Credentials>,
}
#[derive(Serialize, Deserialize)]
struct Registration {
    client_id: String,
    subject: String,
}
#[derive(Serialize, Deserialize)]
struct Credentials {
    access_token: String,
    refresh_token: String,
    id_token: String,
    scopes: Vec<String>,
    expires_at: u64,
    earliest_refresh_at: u64,
    nonce: String,
    #[serde(default)]
    refresh_pending: bool,
}
/// Contains no tokens, email, subject, client ID, or host identifier.
#[derive(Debug, Serialize)]
pub struct AuthStatus {
    pub signed_in: bool,
    pub needs_login: bool,
    pub expires_at: Option<u64>,
    pub account_label: Option<String>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LoginMode {
    Continue,
    NewAccount,
}
/// Deliberately not Serialize/Display. Debug is always redacted.
pub struct AccessToken(String);
impl AccessToken {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Debug for AccessToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AccessToken([REDACTED])")
    }
}
const ISSUER: &str = "https://auth.openai.com";
const AUTHORIZE: &str = "https://auth.openai.com/api/accounts/authorize";
const TOKEN: &str = "https://auth.openai.com/api/accounts/oauth/token";
const JWKS: &str = "https://auth.openai.com/.well-known/jwks.json";
const RESOURCE: &str = "https://api.openai.com/v1";
const SCOPE: &str = "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
fn now() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_secs())
        .map_err(|_| AuthError::InvalidToken)
}
fn subscription(scopes: &[String]) -> Result<()> {
    if ["chatgpt.tokens.use.direct", "resource.invoke"]
        .iter()
        .all(|needed| scopes.iter().any(|s| s == needed))
    {
        Ok(())
    } else {
        Err(AuthError::Permission)
    }
}

fn random() -> Result<String> {
    let mut bytes = [0; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| AuthError::Storage)?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
fn host_id() -> Result<String> {
    let mut b = [0; 16];
    SystemRandom::new()
        .fill(&mut b)
        .map_err(|_| AuthError::Storage)?;
    b[6] = (b[6] & 15) | 64;
    b[8] = (b[8] & 63) | 128;
    let h: String = b.iter().map(|v| format!("{v:02x}")).collect();
    Ok(format!(
        "urn:uuid:{}-{}-{}-{}-{}",
        &h[..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..]
    ))
}
fn cstr(s: &str) -> Result<CString> {
    CString::new(s).map_err(|_| AuthError::Storage)
}
fn from_fd(fd: i32) -> Result<File> {
    if fd < 0 {
        Err(AuthError::Storage)
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}
fn safe_file(f: &File) -> Result<()> {
    let m = f.metadata().map_err(|_| AuthError::Storage)?;
    if !m.is_file()
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o777 != 0o600
        || m.nlink() != 1
    {
        return Err(AuthError::Storage);
    }
    Ok(())
}
impl AuthStore {
    /// Uses the OS account home, not a job-controlled environment variable.
    pub fn new() -> Result<Self> {
        let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
        let mut result = std::ptr::null_mut();
        let mut buffer = vec![0u8; 65536];
        let rc = unsafe {
            libc::getpwuid_r(
                libc::geteuid(),
                &mut entry,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                &mut result,
            )
        };
        if rc != 0 || result.is_null() || entry.pw_dir.is_null() {
            return Err(AuthError::Storage);
        }
        let home = unsafe { CStr::from_ptr(entry.pw_dir) }
            .to_str()
            .map_err(|_| AuthError::Storage)?;
        Self::at(Path::new(home).join(".config/zeus"))
    }
    /// Explicit trusted-caller override (tests/administration). Never wire to job input.
    /// Requires an absolute path, without symlinks or parent traversal.
    pub fn at(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.is_absolute() {
            return Err(AuthError::Storage);
        }
        let mut dir = from_fd(unsafe {
            libc::open(
                cstr("/")?.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        })?;
        let components: Vec<_> = path.components().collect();
        if components.len() < 2 {
            return Err(AuthError::Storage);
        }
        for component in components.iter().skip(1) {
            let Component::Normal(name) = component else {
                return Err(AuthError::Storage);
            };
            let name = cstr(name.to_str().ok_or(AuthError::Storage)?)?;
            let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
            let mut fd = unsafe { libc::openat(dir.as_raw_fd(), name.as_ptr(), flags) };
            if fd < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT) {
                if unsafe { libc::mkdirat(dir.as_raw_fd(), name.as_ptr(), 0o700) } < 0
                    && std::io::Error::last_os_error().raw_os_error() != Some(libc::EEXIST)
                {
                    return Err(AuthError::Storage);
                }
                fd = unsafe { libc::openat(dir.as_raw_fd(), name.as_ptr(), flags) };
            }
            dir = from_fd(fd)?;
        }
        let m = dir.metadata().map_err(|_| AuthError::Storage)?;
        if m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o777 != 0o700 {
            return Err(AuthError::Storage);
        }
        let store = Self {
            dir,
            #[cfg(test)]
            test_base: None,
            #[cfg(test)]
            callback_timeout: Duration::from_secs(300),
        };
        let _lock = store.lock()?;
        if store.open("auth.json")?.is_none() {
            store.save(&Saved {
                host_id: host_id()?,
                registration: None,
                credentials: None,
            })?;
        }
        let saved = store.load()?;
        if !saved.host_id.starts_with("urn:uuid:") || saved.host_id.len() != 45 {
            return Err(AuthError::Storage);
        }
        Ok(store)
    }
    fn open(&self, name: &str) -> Result<Option<File>> {
        let fd = unsafe {
            libc::openat(
                self.dir.as_raw_fd(),
                cstr(name)?.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        if fd < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT) {
            return Ok(None);
        }
        let file = from_fd(fd)?;
        safe_file(&file)?;
        Ok(Some(file))
    }
    fn lock(&self) -> Result<File> {
        let m = self.dir.metadata().map_err(|_| AuthError::Storage)?;
        if m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o777 != 0o700 {
            return Err(AuthError::Storage);
        }
        let file = from_fd(unsafe {
            libc::openat(
                self.dir.as_raw_fd(),
                cstr("session.lock")?.as_ptr(),
                libc::O_RDWR
                    | libc::O_CREAT
                    | libc::O_NOFOLLOW
                    | libc::O_NONBLOCK
                    | libc::O_CLOEXEC,
                0o600,
            )
        })?;
        safe_file(&file)?;
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                return Ok(file);
            }
            if std::io::Error::last_os_error().raw_os_error() != Some(libc::EWOULDBLOCK) {
                return Err(AuthError::Storage);
            }
            if Instant::now() >= deadline {
                return Err(AuthError::Busy);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    fn load(&self) -> Result<Saved> {
        let file = self.open("auth.json")?.ok_or(AuthError::Storage)?;
        let mut bytes = Vec::new();
        file.take(MAX_JSON + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| AuthError::Storage)?;
        if bytes.len() as u64 > MAX_JSON {
            return Err(AuthError::Storage);
        }
        serde_json::from_slice(&bytes).map_err(|_| AuthError::Storage)
    }
    fn save(&self, saved: &Saved) -> Result<()> {
        // Check existing target too; never silently replace a symlink/hardlink.
        self.open("auth.json")?;
        let bytes = serde_json::to_vec(saved).map_err(|_| AuthError::Storage)?;
        if bytes.len() as u64 > MAX_JSON {
            return Err(AuthError::Storage);
        }
        let name = cstr(&format!(".auth-{}.key", random()?))?;
        let mut file = from_fd(unsafe {
            libc::openat(
                self.dir.as_raw_fd(),
                name.as_ptr(),
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        })?;
        let outcome = (|| {
            safe_file(&file)?;
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .map_err(|_| AuthError::Storage)?;
            if unsafe {
                libc::renameat(
                    self.dir.as_raw_fd(),
                    name.as_ptr(),
                    self.dir.as_raw_fd(),
                    cstr("auth.json")?.as_ptr(),
                )
            } != 0
            {
                return Err(AuthError::Storage);
            }
            self.dir.sync_all().map_err(|_| AuthError::Storage)
        })();
        if outcome.is_err() {
            unsafe {
                libc::unlinkat(self.dir.as_raw_fd(), name.as_ptr(), 0);
            }
        }
        outcome
    }
}
impl AuthStore {
    #[cfg(test)]
    pub fn for_test(path: &Path, base: &str, timeout: Duration) -> Result<Self> {
        let u = url::Url::parse(base).map_err(|_| AuthError::Network)?;
        if u.scheme() != "http" || u.host_str() != Some("127.0.0.1") {
            return Err(AuthError::Network);
        }
        let mut store = Self::at(path)?;
        store.test_base = Some(base.to_owned());
        store.callback_timeout = timeout;
        Ok(store)
    }
    fn endpoint(&self, official: &str) -> String {
        #[cfg(test)]
        if let Some(base) = &self.test_base {
            return format!("{}{}", base, url::Url::parse(official).unwrap().path());
        }
        official.to_owned()
    }
    fn callback_limit(&self) -> Duration {
        #[cfg(test)]
        {
            self.callback_timeout
        }
        #[cfg(not(test))]
        {
            Duration::from_secs(300)
        }
    }
}
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .try_proxy_from_env(false)
        .redirects(0)
        .timeout(Duration::from_secs(20))
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(10))
        .timeout_write(Duration::from_secs(10))
        .build()
}
fn response<T: serde::de::DeserializeOwned>(
    result: std::result::Result<ureq::Response, ureq::Error>,
) -> Result<T> {
    // Never surface request URLs, headers, tokens, remote bodies, or library errors.
    let result = result.map_err(|_| AuthError::Network)?;
    if result.status() != 200 {
        return Err(AuthError::Network);
    }
    let mut bytes = Vec::new();
    result
        .into_reader()
        .take(MAX_JSON + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| AuthError::Network)?;
    if bytes.len() as u64 > MAX_JSON {
        return Err(AuthError::InvalidResponse);
    }
    serde_json::from_slice(&bytes).map_err(|_| AuthError::InvalidResponse)
}
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    #[serde(default)]
    id_token: Option<String>,
    token_type: String,
    expires_in: u64,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    earliest_refresh_at: Option<u64>,
}
#[derive(Deserialize)]
struct JwtHeader {
    alg: String,
    kid: String,
    #[serde(default)]
    crit: Vec<String>,
    b64: Option<bool>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Audience {
    One(String),
    Many(Vec<String>),
}
#[derive(Deserialize)]
struct Claims {
    iss: String,
    aud: Audience,
    sub: String,
    exp: u64,
    iat: u64,
    nonce: Option<String>,
    azp: Option<String>,
    nbf: Option<u64>,
    at_hash: Option<String>,
}
#[derive(Deserialize)]
struct Jwk {
    kty: String,
    kid: String,
    alg: Option<String>,
    #[serde(rename = "use")]
    usage: Option<String>,
    key_ops: Option<Vec<String>>,
    n: Option<String>,
    e: Option<String>,
}
#[derive(Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}
fn id_claims(
    store: &AuthStore,
    token: &str,
    client: &str,
    nonce: &str,
    require_nonce: bool,
    access: &str,
) -> Result<Claims> {
    use ring::digest::{digest, SHA256};
    use ring::signature::{RsaPublicKeyComponents, RSA_PKCS1_2048_8192_SHA256};
    let parts: Vec<_> = token.split('.').collect();
    if parts.len() != 3 || token.len() > 32768 {
        return Err(AuthError::InvalidToken);
    }
    let decode = |part: &str| {
        URL_SAFE_NO_PAD
            .decode(part)
            .map_err(|_| AuthError::InvalidToken)
    };
    let header: JwtHeader =
        serde_json::from_slice(&decode(parts[0])?).map_err(|_| AuthError::InvalidToken)?;
    if header.alg != "RS256" || !header.crit.is_empty() || header.b64 == Some(false) {
        return Err(AuthError::InvalidToken);
    }
    let keys: Jwks = response(agent().get(&store.endpoint(JWKS)).call())?;
    let candidates: Vec<_> = keys.keys.iter().filter(|k| k.kid == header.kid).collect();
    if candidates.len() != 1 {
        return Err(AuthError::InvalidToken);
    }
    let key = candidates[0];
    if key.kty != "RSA"
        || key.alg.as_deref().is_some_and(|a| a != "RS256")
        || key.usage.as_deref().is_some_and(|v| v != "sig")
        || key
            .key_ops
            .as_ref()
            .is_some_and(|v| !v.iter().any(|s| s == "verify"))
    {
        return Err(AuthError::InvalidToken);
    }
    let n = decode(key.n.as_deref().ok_or(AuthError::InvalidToken)?)?;
    let e = decode(key.e.as_deref().ok_or(AuthError::InvalidToken)?)?;
    let signature = decode(parts[2])?;
    RsaPublicKeyComponents { n: &n, e: &e }
        .verify(
            &RSA_PKCS1_2048_8192_SHA256,
            format!("{}.{}", parts[0], parts[1]).as_bytes(),
            &signature,
        )
        .map_err(|_| AuthError::InvalidToken)?;
    let claims: Claims =
        serde_json::from_slice(&decode(parts[1])?).map_err(|_| AuthError::InvalidToken)?;
    let (audience, multiple) = match &claims.aud {
        Audience::One(a) => (a == client, false),
        Audience::Many(a) => (a.iter().any(|v| v == client), a.len() > 1),
    };
    let time = now()?;
    if claims.iss != ISSUER
        || !audience
        || (multiple && claims.azp.as_deref() != Some(client))
        || claims.azp.as_deref().is_some_and(|a| a != client)
        || claims.exp <= time
        || claims.iat > time.saturating_add(60)
        || claims.nbf.is_some_and(|n| n > time)
        || claims.sub.is_empty()
        || claims.sub.len() > 256
        || (require_nonce && claims.nonce.is_none())
        || claims.nonce.as_deref().is_some_and(|n| n != nonce)
    {
        return Err(AuthError::InvalidToken);
    }
    if claims.at_hash.as_deref().is_some_and(|h| {
        h != URL_SAFE_NO_PAD.encode(&digest(&SHA256, access.as_bytes()).as_ref()[..16])
    }) {
        return Err(AuthError::InvalidToken);
    }
    Ok(claims)
}
fn credentials(
    reply: TokenResponse,
    nonce: String,
    old: Option<&Credentials>,
) -> Result<Credentials> {
    let scopes: Vec<String> = match reply.scope {
        Some(s) => s.split_whitespace().map(str::to_owned).collect(),
        None => old.map(|c| c.scopes.clone()).ok_or(AuthError::Permission)?,
    };
    subscription(&scopes)?;
    if !scopes.iter().any(|s| s == "offline_access")
        || !reply.token_type.eq_ignore_ascii_case("Bearer")
        || reply.expires_in == 0
        || reply.expires_in > 86400
        || !opaque_token(&reply.access_token)
        || !opaque_token(&reply.refresh_token)
    {
        return Err(AuthError::InvalidResponse);
    }
    let expires_at = now()?
        .checked_add(reply.expires_in)
        .ok_or(AuthError::InvalidResponse)?;
    let earliest_refresh_at = reply.earliest_refresh_at.unwrap_or(0);
    if earliest_refresh_at > expires_at {
        return Err(AuthError::InvalidResponse);
    }
    Ok(Credentials {
        access_token: reply.access_token,
        refresh_token: reply.refresh_token,
        id_token: reply
            .id_token
            .or_else(|| old.map(|v| v.id_token.clone()))
            .ok_or(AuthError::InvalidToken)?,
        scopes,
        expires_at,
        earliest_refresh_at,
        nonce,
        refresh_pending: false,
    })
}
fn opaque_token(s: &str) -> bool {
    !s.is_empty() && s.len() <= 65536 && s.bytes().all(|b| b.is_ascii_graphic())
}
fn issued_client(s: &str) -> bool {
    s != "dynamic_agent_client"
        && !s.is_empty()
        && s.len() <= 256
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn summary(saved: &Saved) -> AuthStatus {
    let usable = saved
        .credentials
        .as_ref()
        .is_some_and(|c| !c.refresh_pending && subscription(&c.scopes).is_ok());
    AuthStatus {
        signed_in: usable,
        needs_login: !usable,
        expires_at: saved.credentials.as_ref().map(|c| c.expires_at),
        account_label: saved.registration.as_ref().map(|r| {
            format!(
                "chatgpt-{}",
                &URL_SAFE_NO_PAD.encode(ring::digest::digest(
                    &ring::digest::SHA256,
                    r.client_id.as_bytes()
                ))[..12]
            )
        }),
    }
}
pub fn status(store: &AuthStore) -> Result<AuthStatus> {
    let _lock = store.lock()?;
    Ok(summary(&store.load()?))
}
/// Local logout only. Remote revocation is not confirmed; host/registration survive.
pub fn logout(store: &AuthStore) -> Result<()> {
    let _lock = store.lock()?;
    let mut saved = store.load()?;
    saved.credentials = None;
    store.save(&saved)
}

fn callback(
    listener: &std::net::TcpListener,
    redirect: &str,
    state: &str,
    registered: Option<&str>,
    limit: Duration,
) -> Result<(String, String)> {
    let deadline = Instant::now() + limit;
    loop {
        if Instant::now() >= deadline {
            return Err(AuthError::Timeout);
        }
        match listener.accept() {
            Ok((mut socket, peer)) => {
                // BSD/macOS accept inherits O_NONBLOCK. Clear it so read/write
                // deadlines work even when the peer sends headers later.
                socket
                    .set_nonblocking(false)
                    .map_err(|_| AuthError::Callback)?;
                if !peer.ip().is_loopback() {
                    return Err(AuthError::Callback);
                }
                let result = (|| {
                    let mut bytes = Vec::new();
                    let mut byte = [0];
                    while !bytes.ends_with(b"\r\n\r\n") {
                        let left = deadline.saturating_duration_since(Instant::now());
                        if left.is_zero() {
                            return Err(AuthError::Timeout);
                        }
                        socket
                            .set_read_timeout(Some(left.min(Duration::from_secs(2))))
                            .map_err(|_| AuthError::Callback)?;
                        if socket.read(&mut byte).map_err(|_| AuthError::Callback)? == 0
                            || bytes.len() >= 16384
                        {
                            return Err(AuthError::Callback);
                        }
                        bytes.push(byte[0]);
                    }
                    let request = std::str::from_utf8(&bytes).map_err(|_| AuthError::Callback)?;
                    let mut lines = request.split("\r\n");
                    let first: Vec<_> = lines
                        .next()
                        .ok_or(AuthError::Callback)?
                        .split_whitespace()
                        .collect();
                    if first.len() != 3
                        || first[0] != "GET"
                        || first[2] != "HTTP/1.1"
                        || !first[1].starts_with("/auth/callback?")
                        || first[1].contains('#')
                    {
                        return Err(AuthError::Callback);
                    }
                    let origin = url::Url::parse(redirect).map_err(|_| AuthError::Callback)?;
                    let expected_host =
                        format!("127.0.0.1:{}", origin.port().ok_or(AuthError::Callback)?);
                    let hosts: Vec<_> = lines
                        .filter_map(|l| l.split_once(':'))
                        .filter(|(k, _)| k.eq_ignore_ascii_case("host"))
                        .collect();
                    if hosts.len() != 1 || hosts[0].1.trim() != expected_host {
                        return Err(AuthError::Callback);
                    }
                    let url = url::Url::parse(&format!("http://{expected_host}{}", first[1]))
                        .map_err(|_| AuthError::Callback)?;
                    let mut query = std::collections::HashMap::new();
                    for (k, v) in url.query_pairs() {
                        if query.insert(k.into_owned(), v.into_owned()).is_some() {
                            return Err(AuthError::Callback);
                        }
                    }
                    if query.get("state").map(String::as_str) != Some(state) {
                        return Err(AuthError::Callback);
                    }
                    if query.contains_key("error") {
                        return Err(AuthError::Denied);
                    }
                    let client = match (registered, query.get("client_id")) {
                        (Some(expected), Some(got)) if expected != got => {
                            return Err(AuthError::Callback)
                        }
                        (Some(expected), _) => expected.to_owned(),
                        (None, Some(got)) => got.clone(),
                        _ => return Err(AuthError::Callback),
                    };
                    if !issued_client(&client) {
                        return Err(AuthError::Callback);
                    }
                    let code = query
                        .remove("code")
                        .filter(|s| opaque_token(s))
                        .ok_or(AuthError::Callback)?;
                    Ok((code, client))
                })();
                let _ = socket.set_write_timeout(Some(Duration::from_millis(200)));
                // Constant, non-reflecting page: never echo callback values or claim login success before validation.
                let body = "Return to Zeus to check sign-in status.";
                let status = if result.is_ok() {
                    "200 OK"
                } else {
                    "400 Bad Request"
                };
                let _=socket.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes());
                return result;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10))
            }
            Err(_) => return Err(AuthError::Callback),
        }
    }
}
/// Bind first, then give the trusted UI a URL to open in a system browser.
/// Do not log the URL. This minimal flow omits optional ID-token/email hints,
/// so even returning URLs contain no retained tokens. Continue pins identity;
/// NewAccount explicitly replaces the single registration only after validation.
pub fn login<F>(store: &AuthStore, mode: LoginMode, present: F) -> Result<AuthStatus>
where
    F: FnOnce(&str) -> Result<()>,
{
    let _lock = store.lock()?;
    let mut saved = store.load()?;
    let registered = if mode == LoginMode::Continue {
        saved.registration.as_ref()
    } else {
        None
    };
    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|_| AuthError::Callback)?;
    listener
        .set_nonblocking(true)
        .map_err(|_| AuthError::Callback)?;
    let redirect = format!(
        "http://127.0.0.1:{}/auth/callback",
        listener
            .local_addr()
            .map_err(|_| AuthError::Callback)?
            .port()
    );
    let (state, nonce, verifier) = (random()?, random()?, random()?);
    let challenge = URL_SAFE_NO_PAD.encode(ring::digest::digest(
        &ring::digest::SHA256,
        verifier.as_bytes(),
    ));
    let mut url = url::Url::parse(&store.endpoint(AUTHORIZE)).map_err(|_| AuthError::Network)?;
    let client = registered
        .map(|r| r.client_id.as_str())
        .unwrap_or("dynamic_agent_client");
    {
        let mut q = url.query_pairs_mut();
        for (k, v) in [
            ("client_id", client),
            ("ext_agent_host_id", saved.host_id.as_str()),
            ("response_type", "code"),
            ("redirect_uri", redirect.as_str()),
            ("scope", SCOPE),
            ("resource", RESOURCE),
            ("state", state.as_str()),
            ("nonce", nonce.as_str()),
            ("code_challenge_method", "S256"),
            ("code_challenge", challenge.as_str()),
        ] {
            q.append_pair(k, v);
        }
        if registered.is_none() {
            q.append_pair("agent_name_hint", "Zeus");
        }
    }
    present(url.as_str())?;
    let (code, client) = callback(
        &listener,
        &redirect,
        &state,
        registered.map(|r| r.client_id.as_str()),
        store.callback_limit(),
    )?;
    let reply: TokenResponse = response(agent().post(&store.endpoint(TOKEN)).send_form(&[
        ("grant_type", "authorization_code"),
        ("client_id", &client),
        ("code", &code),
        ("code_verifier", &verifier),
        ("redirect_uri", &redirect),
        ("resource", RESOURCE),
    ]))?;
    let claims = id_claims(
        store,
        reply.id_token.as_deref().ok_or(AuthError::InvalidToken)?,
        &client,
        &nonce,
        true,
        &reply.access_token,
    )?;
    if registered.is_some_and(|r| r.subject != claims.sub) {
        return Err(AuthError::InvalidToken);
    }
    let tokens = credentials(reply, nonce, None)?;
    saved.registration = Some(Registration {
        client_id: client,
        subject: claims.sub,
    });
    saved.credentials = Some(tokens);
    store.save(&saved)?;
    Ok(summary(&saved))
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SubscriptionModel {
    pub id: String,
    pub display_name: String,
}
/// Current account-specific subscription catalog, not the API-key data[] shape.
/// Visibility filtering preserves server order; never supplies invented defaults.
pub fn list_models(store: &AuthStore) -> Result<Vec<SubscriptionModel>> {
    #[derive(Deserialize)]
    struct Catalog {
        models: Vec<Model>,
    }
    #[derive(Deserialize)]
    struct Model {
        visibility: String,
        slug: Option<String>,
        display_name: Option<String>,
    }
    let token = access_token(store)?;
    let catalog: Catalog = response(
        agent()
            .get(&store.endpoint("https://api.openai.com/v1/models"))
            .set("Authorization", &format!("Bearer {}", token.as_str()))
            .call(),
    )?;
    if catalog.models.len() > 4096 {
        return Err(AuthError::InvalidResponse);
    }
    catalog
        .models
        .into_iter()
        .filter(|m| m.visibility == "list")
        .map(|m| {
            let id = m.slug.ok_or(AuthError::InvalidResponse)?;
            let display_name = m.display_name.ok_or(AuthError::InvalidResponse)?;
            if id.is_empty()
                || id.len() > 256
                || id.chars().any(|c| c.is_whitespace() || c.is_control())
                || display_name.is_empty()
                || display_name.len() > 512
                || display_name.chars().any(char::is_control)
            {
                return Err(AuthError::InvalidResponse);
            }
            Ok(SubscriptionModel { id, display_name })
        })
        .collect()
}

pub fn access_token(store: &AuthStore) -> Result<AccessToken> {
    let _lock = store.lock()?;
    let mut saved = store.load()?;
    let c = saved.credentials.as_mut().ok_or(AuthError::SignedOut)?;
    subscription(&c.scopes)?;
    if c.refresh_pending {
        return Err(AuthError::Reauthorize);
    }
    let time = now()?;
    if c.expires_at > time.saturating_add(60)
        || (time < c.earliest_refresh_at && time < c.expires_at)
    {
        return Ok(AccessToken(c.access_token.clone()));
    }
    if time < c.earliest_refresh_at {
        return Err(AuthError::Reauthorize);
    }
    // Durably journal BEFORE sending a rotating credential. A crash, ambiguous
    // network outcome, invalid replacement, or failed final write must never
    // cause reuse of the previous refresh token. Recovery is explicit login.
    c.refresh_pending = true;
    store.save(&saved)?;
    let c = saved.credentials.as_ref().ok_or(AuthError::SignedOut)?;
    let registration = saved.registration.as_ref().ok_or(AuthError::Reauthorize)?;
    let reply: TokenResponse = response(agent().post(&store.endpoint(TOKEN)).send_form(&[
        ("grant_type", "refresh_token"),
        ("client_id", &registration.client_id),
        ("refresh_token", &c.refresh_token),
        ("resource", RESOURCE),
    ]))
    .map_err(|_| AuthError::Reauthorize)?;
    if let Some(id) = &reply.id_token {
        let claims = id_claims(
            store,
            id,
            &registration.client_id,
            &c.nonce,
            false,
            &reply.access_token,
        )?;
        if claims.sub != registration.subject {
            return Err(AuthError::InvalidToken);
        }
    }
    let replacement = credentials(reply, c.nonce.clone(), Some(c))?;
    let token = AccessToken(replacement.access_token.clone());
    saved.credentials = Some(replacement);
    store.save(&saved)?;
    Ok(token)
}
