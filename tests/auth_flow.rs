#![allow(dead_code)]
#[path = "../src/auth.rs"]
mod auth;

use auth::{logout, status, AuthStore};
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use ring::signature::{RsaKeyPair, RSA_PKCS1_SHA256};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::os::unix::fs::PermissionsExt;
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

// Generated test-only RSA key. No real service or user credential is used.
const TEST_KEY: &str = "MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQC2uSTt9F1qWkKMPwn/6xqlotWfjQe/0fuHQmzNk6jTfrNkNyt3CpbT0c2xhezGylzbRJZE45CNRGiFY7OdfSYk6HVZwdniIITGY6di+PWRb7G1tH9PB7km9BNTeXvhus0tm99JfKyqGIIYi/CxDkheglA9JEDzXKLnRqMBMstYmv8Chby0Xn7Cto/xfy/jnk9WhyGUMbbVri/DTZZK5KLKtrTpz5Pm/zs+gF4rN7aOzTt5ftXmEhIbBCUKiag4S+4/ryoPwM6vHSFNGUNaMNhL+Z7YVnw11HdUZe5CeXj7/TBUhoAgMO0TOPdRdodZfhLJ1YrdBpHIpKZvBZTD4yVtAgMBAAECggEAXaQQauoanZBYHmuIAmFILK6Ab93kpMdF8hF2lPwhPxHleWxUmBMTLMaOdXW+3DmE0NIbeH5u7Baswf9QY1tpmuyOh0szEBG+ObWEaNJRdxEYr5gqziOVHYNkDBlsBUioMcF58r0z94/LzCybUwGQ0O5TlKHpr05AJeorS0fush8O4w4dcyCiW6BYcT8aAxDhE4l+p50Ua8XttqFo2stEebpDsuy67UKxEyul86eWqHppmAk0+x30aelUE2wnQOt/JThVAWgEgARyqRft+bOXjxaVhbubj6SWVMAz5WX6sLBsekYLWHh0QvHQJYMY1D9xI0ZFyfMmDIe4Xm9WkHrHAQKBgQDeHC/h7kW1a/Cc+M86aqMR9xgQ1o/QLNO8JpW8grNlo/3hZbolHdq1TPigsY/EC4lNvzKdOyqOXJramCvlrfwOIBJD/oDhteEpQVsKM31xYqJFN/BE3AZ4QRAZPQkU7OMNW7WMEDJ2sWRS6teYBWmjKWa21KrpqKdWeiLwD1qooQKBgQDSmnirRyPzB9EfbWb9RsZPV8L0QXQD1Llj69+/1ZtFAoLCMG8lv+hOGqd8WzN+GwvQMiDT8e+0WBev+BLUDEfLoSYgo3d2P+KSAMceK9tRTg9oKtc6iufpgnp9dc6KNfOir0H+bL4j+aotlz270ouysmFQRfA+j0Ku1jshOSZNTQKBgQCN4x/LirVJYj0Bagin3rtxnJHpH8gbSqewSE3bcPs9cPecZ6n0UBAJRbSGoyeeK2h5HC0vOD7FVu8tuYxBW1XriB1jBlA64D+IZVHNQHtgVYwn/8YjlxMi+x1XoiANGA8r12wwn6A9JrFaLt+vI7YKH0aYgV71iWHs4nUzOfvGQQKBgEi/Txtq8YILmzMN6VtnzSyvFgqlaVvBaWCYo6tyL7ZAzcYiUOJrTeVc06njHwBae1QpXKmZksFrBJOmT2qr60jtm2v+y08vdiYU2rMTVyjYm/3RfpMcVEuW5JTd0sBvLn9tppAqI+vtW9lvusVcTyEk+PRvlDMn7VLTcY1iXlapAoGBAK8pOCZwGJ12TvT3Oo6DKwp/Qh0zghSFTCPAb//3HyObTMM11Ft4gmj+k+Nng2Wl7CfEycRSQKJqYJbtyqLaflqLp6ytC6LGHYQT6LSNgHJ+8kCH5Edthhl4gRSedpcYzwnhsGHaaOzm2r3PMnyn1q+vRfQwwTyHPiWwUySyPljp";
const MODULUS: &str = "B6B924EDF45D6A5A428C3F09FFEB1AA5A2D59F8D07BFD1FB87426CCD93A8D37EB364372B770A96D3D1CDB185ECC6CA5CDB449644E3908D44688563B39D7D2624E87559C1D9E22084C663A762F8F5916FB1B5B47F4F07B926F41353797BE1BACD2D9BDF497CACAA1882188BF0B10E485E82503D2440F35CA2E746A30132CB589AFF0285BCB45E7EC2B68FF17F2FE39E4F5687219431B6D5AE2FC34D964AE4A2CAB6B4E9CF93E6FF3B3E805E2B37B68ECD3B797ED5E612121B04250A89A8384BEE3FAF2A0FC0CEAF1D214D19435A30D84BF99ED8567C35D4775465EE427978FBFD305486802030ED1338F7517687597E12C9D58ADD0691C8A4A66F0594C3E3256D";
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
fn sign(claims: Value) -> String {
    let key = RsaKeyPair::from_pkcs8(&STANDARD.decode(TEST_KEY).unwrap()).unwrap();
    let input = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","kid":"fixture"}"#),
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap())
    );
    let mut sig = vec![0; key.public().modulus_len()];
    key.sign(
        &RSA_PKCS1_SHA256,
        &ring::rand::SystemRandom::new(),
        input.as_bytes(),
        &mut sig,
    )
    .unwrap();
    format!("{input}.{}", URL_SAFE_NO_PAD.encode(sig))
}
#[derive(Default)]
struct FixtureState {
    authorize: HashMap<String, String>,
    grants: Vec<HashMap<String, String>>,
    fault: String,
    model_auth: String,
    storage_root: Option<std::path::PathBuf>,
}
struct Fixture {
    base: String,
    state: Arc<Mutex<FixtureState>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Fixture {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let state = Arc::new(Mutex::new(FixtureState::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (s, done) = (state.clone(), stop.clone());
        let thread = std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                let Ok((mut socket, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut byte = [0];
                while !bytes.ends_with(b"\r\n\r\n") {
                    if socket.read(&mut byte).unwrap_or(0) == 0 {
                        break;
                    }
                    bytes.push(byte[0]);
                }
                let headers = String::from_utf8(bytes).unwrap();
                if headers.is_empty() {
                    continue;
                }
                let len: usize = headers
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|v| v.trim().parse().unwrap())
                    })
                    .unwrap_or(0);
                let mut body = vec![0; len];
                socket.read_exact(&mut body).unwrap();
                let target = headers
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap();
                let url = url::Url::parse(&format!("http://fixture{target}")).unwrap();
                let mut s = s.lock().unwrap();
                let mut code = 200;
                let mut extra = String::new();
                let body = match url.path() {
                    "/api/accounts/authorize" => {
                        s.authorize = url
                            .query_pairs()
                            .map(|(k, v)| (k.into_owned(), v.into_owned()))
                            .collect();
                        let mut cb = url::Url::parse(&s.authorize["redirect_uri"]).unwrap();
                        let state = if s.fault == "state" {
                            "wrong"
                        } else {
                            &s.authorize["state"]
                        };
                        cb.query_pairs_mut()
                            .append_pair("state", state)
                            .append_pair("code", "test-code");
                        if s.fault == "denied" {
                            cb.query_pairs_mut().append_pair("error", "access_denied");
                        }
                        if s.fault == "duplicate_state" {
                            cb.query_pairs_mut().append_pair("state", state);
                        }
                        if s.authorize["client_id"] == "dynamic_agent_client"
                            && s.fault != "missing_client"
                        {
                            cb.query_pairs_mut()
                                .append_pair("client_id", "oaiapp_fixture");
                        }
                        if s.fault == "changed_client" {
                            cb.query_pairs_mut()
                                .append_pair("client_id", "oaiapp_other");
                        }
                        code = 302;
                        extra = format!("Location: {cb}\r\n");
                        String::new()
                    }
                    "/api/accounts/oauth/token" => {
                        let form: HashMap<String, String> = url::form_urlencoded::parse(&body)
                            .map(|(k, v)| (k.into_owned(), v.into_owned()))
                            .collect();
                        assert_eq!(form["client_id"], "oaiapp_fixture");
                        assert_eq!(form["resource"], "https://api.openai.com/v1");
                        let refresh = form["grant_type"] == "refresh_token";
                        if !refresh {
                            assert_eq!(form["redirect_uri"], s.authorize["redirect_uri"]);
                            assert_eq!(
                                URL_SAFE_NO_PAD
                                    .encode(Sha256::digest(form["code_verifier"].as_bytes())),
                                s.authorize["code_challenge"]
                            );
                        }
                        s.grants.push(form);
                        if s.fault == "write_failure" && refresh {
                            std::fs::set_permissions(
                                s.storage_root.as_ref().unwrap().join("auth.json"),
                                std::fs::Permissions::from_mode(0o644),
                            )
                            .unwrap();
                        }
                        if s.fault == "disconnect" && refresh {
                            continue;
                        }
                        if s.fault == "refresh_error" && refresh {
                            code = 400;
                            json!({"error":"invalid_grant","error_description":"DO-NOT-LEAK"})
                                .to_string()
                        } else {
                            let mut claims = json!({"iss":"https://auth.openai.com", "aud":"oaiapp_fixture", "sub":"subject-a", "exp":now()+3600,"iat":now(),"nonce":s.authorize["nonce"],"email":"private@example.test"});
                            match s.fault.as_str() {
                                "nonce" => claims["nonce"] = json!("wrong"),
                                "expired" => claims["exp"] = json!(now() - 1),
                                "issuer" => claims["iss"] = json!("https://attacker.invalid"),
                                "audience" => claims["aud"] = json!("other-client"),
                                "identity" => claims["sub"] = json!("subject-b"),
                                _ => (),
                            }
                            let mut id = sign(claims);
                            if s.fault == "signature" {
                                let at = id.rfind('.').unwrap() + 1;
                                id.replace_range(
                                    at..at + 1,
                                    if &id[at..at + 1] == "A" { "B" } else { "A" },
                                );
                            }
                            let scope = if s.fault == "scope" {
                                "openid profile email offline_access resource.invoke"
                            } else {
                                "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct"
                            };
                            json!({"access_token":if refresh {"access-rotated"} else {"access-initial"},"refresh_token":if refresh {"refresh-rotated"} else {"refresh-initial"},"id_token":id,"token_type":"Bearer","scope":scope,"expires_in":3600,"earliest_refresh_at":now()}).to_string()
                        }
                    }
                    "/.well-known/jwks.json" => {
                        let n: Vec<u8> = (0..MODULUS.len())
                            .step_by(2)
                            .map(|i| u8::from_str_radix(&MODULUS[i..i + 2], 16).unwrap())
                            .collect();
                        json!({"keys":[{"kty":"RSA","use":"sig","alg":"RS256","kid":"fixture","n":URL_SAFE_NO_PAD.encode(n),"e":"AQAB"}]}).to_string()
                    }
                    "/v1/models" => {
                        s.model_auth = headers
                            .lines()
                            .find(|l| l.to_ascii_lowercase().starts_with("authorization:"))
                            .unwrap_or("")
                            .to_owned();
                        match s.fault.as_str() {
                            "catalog_oversized" => "x".repeat(1024*1024+1),
                            "catalog_redirect" => { code=302; extra="Location: http://127.0.0.1:1/must-not-follow\r\n".into(); "DO-NOT-LEAK".into() },
                            "catalog_invalid" => "{\"data\":[]}".into(),
                            _ => json!({"models":[{"visibility":"list","slug":"fixture-z","display_name":"Fixture Z"},{"visibility":"hidden","slug":"hidden","display_name":"Hidden"},{"visibility":"list","slug":"fixture-a","display_name":"Fixture A"}]}).to_string()
                        }
                    }
                    _ => {
                        code = 404;
                        "{}".to_owned()
                    }
                };
                let response = format!("HTTP/1.1 {code} Test\r\nContent-Length: {}\r\nContent-Type: application/json\r\n{extra}Connection: close\r\n\r\n{body}",body.len());
                let _ = socket.write_all(response.as_bytes());
            }
        });
        Self {
            base,
            state,
            stop,
            thread: Some(thread),
        }
    }
    fn login(
        &self,
        store: &AuthStore,
        mode: auth::LoginMode,
    ) -> Result<auth::AuthStatus, auth::AuthError> {
        let mut browser = None;
        let result = auth::login(store, mode, |url| {
            let url = url.to_owned();
            browser = Some(std::thread::spawn(move || {
                let _ = ureq::AgentBuilder::new()
                    .try_proxy_from_env(false)
                    .timeout(Duration::from_secs(5))
                    .build()
                    .get(&url)
                    .call();
            }));
            Ok(())
        });
        if let Some(browser) = browser {
            browser.join().unwrap();
        }
        result
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}

#[test]
fn dynamic_login_verifies_pkce_jwt_and_persists_only_validated_tokens() {
    let tmp = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(tmp.path()).unwrap().join("auth");
    let mock = Fixture::new();
    let store = AuthStore::for_test(&root, &mock.base, Duration::from_secs(5)).unwrap();
    let result = mock.login(&store, auth::LoginMode::Continue).unwrap();
    assert!(result.signed_in);
    assert_eq!(
        auth::access_token(&store).unwrap().as_str(),
        "access-initial"
    );
    let redacted = format!("{result:?} {:?}", auth::access_token(&store).unwrap());
    for secret in [
        "access-initial",
        "refresh-initial",
        "private@example.test",
        "subject-a",
    ] {
        assert!(!redacted.contains(secret));
    }
    let state = mock.state.lock().unwrap();
    assert_eq!(state.authorize["agent_name_hint"], "Zeus");
    assert_eq!(state.authorize["client_id"], "dynamic_agent_client");
    assert_eq!(state.authorize["code_challenge_method"], "S256");
    assert_eq!(state.grants.len(), 1);
}

#[test]
fn rejects_untrusted_login_results_without_overwriting_credentials() {
    for fault in [
        "state",
        "duplicate_state",
        "denied",
        "missing_client",
        "nonce",
        "expired",
        "issuer",
        "audience",
        "signature",
        "scope",
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap().join("auth");
        let mock = Fixture::new();
        let store = AuthStore::for_test(&root, &mock.base, Duration::from_secs(5)).unwrap();
        mock.state.lock().unwrap().fault = fault.into();
        let result = mock.login(&store, auth::LoginMode::Continue);
        assert!(result.is_err(), "accepted {fault}");
        assert!(!status(&store).unwrap().signed_in);
        let saved = std::fs::read_to_string(root.join("auth.json")).unwrap();
        assert!(!saved.contains("access-initial"));
        assert!(!saved.contains("refresh-initial"));
        if ["state", "duplicate_state", "denied", "missing_client"].contains(&fault) {
            assert!(mock.state.lock().unwrap().grants.is_empty());
        }
    }
}

fn expire(root: &std::path::Path) {
    let path = root.join("auth.json");
    let mut v: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    v["credentials"]["expires_at"] = json!(now() - 1);
    v["credentials"]["earliest_refresh_at"] = json!(0);
    std::fs::write(path, serde_json::to_vec(&v).unwrap()).unwrap();
}
#[test]
fn refresh_rotates_atomically_and_serializes_concurrent_callers() {
    let tmp = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(tmp.path()).unwrap().join("auth");
    let mock = Fixture::new();
    let store = AuthStore::for_test(&root, &mock.base, Duration::from_secs(5)).unwrap();
    mock.login(&store, auth::LoginMode::Continue).unwrap();
    expire(&root);
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                assert_eq!(
                    auth::access_token(&store).unwrap().as_str(),
                    "access-rotated"
                )
            });
        }
    });
    let saved: Value =
        serde_json::from_slice(&std::fs::read(root.join("auth.json")).unwrap()).unwrap();
    assert_eq!(saved["credentials"]["refresh_token"], "refresh-rotated");
    assert_eq!(saved["credentials"]["refresh_pending"], false);
    assert!(saved["credentials"]["expires_at"].as_u64().unwrap() > now());
    let s = mock.state.lock().unwrap();
    assert_eq!(s.grants.len(), 2);
    assert_eq!(s.grants[1]["refresh_token"], "refresh-initial");
    assert!(!s.grants[1].contains_key("scope"));
    drop(s);
    expire(&root);
    assert_eq!(
        auth::access_token(&store).unwrap().as_str(),
        "access-rotated"
    );
    assert_eq!(
        mock.state.lock().unwrap().grants[2]["refresh_token"],
        "refresh-rotated"
    );
}
#[test]
fn failed_refresh_is_redacted_and_never_replays_a_possibly_rotated_token() {
    for fault in ["refresh_error", "signature", "scope", "identity"] {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap().join("auth");
        let mock = Fixture::new();
        let store = AuthStore::for_test(&root, &mock.base, Duration::from_secs(5)).unwrap();
        mock.login(&store, auth::LoginMode::Continue).unwrap();
        expire(&root);
        mock.state.lock().unwrap().fault = fault.into();
        let err = auth::access_token(&store).unwrap_err();
        assert!(!err.to_string().contains("DO-NOT-LEAK"));
        assert_eq!(
            auth::access_token(&store).unwrap_err(),
            auth::AuthError::Reauthorize
        );
        assert!(status(&store).unwrap().needs_login);
        assert_eq!(mock.state.lock().unwrap().grants.len(), 2);
    }
}

#[test]
fn account_catalog_uses_subscription_bearer_and_keeps_visible_server_order() {
    let tmp = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(tmp.path()).unwrap().join("auth");
    let mock = Fixture::new();
    let store = AuthStore::for_test(&root, &mock.base, Duration::from_secs(5)).unwrap();
    assert!(auth::list_models(&store).is_err());
    mock.login(&store, auth::LoginMode::Continue).unwrap();
    let models = auth::list_models(&store).unwrap();
    assert_eq!(
        models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
        vec!["fixture-z", "fixture-a"]
    );
    assert_eq!(models[0].display_name, "Fixture Z");
    assert!(mock
        .state
        .lock()
        .unwrap()
        .model_auth
        .ends_with("Bearer access-initial"));
}
#[test]
fn returning_login_pins_identity_and_logout_preserves_registration_without_tokens() {
    let tmp = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(tmp.path()).unwrap().join("auth");
    let mock = Fixture::new();
    let store = AuthStore::for_test(&root, &mock.base, Duration::from_secs(5)).unwrap();
    mock.login(&store, auth::LoginMode::Continue).unwrap();
    let original = std::fs::read(root.join("auth.json")).unwrap();
    let original_host = mock.state.lock().unwrap().authorize["ext_agent_host_id"].clone();
    for fault in ["identity", "changed_client", "scope", "signature"] {
        mock.state.lock().unwrap().fault = fault.into();
        assert!(mock.login(&store, auth::LoginMode::Continue).is_err());
        assert_eq!(std::fs::read(root.join("auth.json")).unwrap(), original);
    }
    mock.state.lock().unwrap().fault.clear();
    mock.login(&store, auth::LoginMode::Continue).unwrap();
    {
        let s = mock.state.lock().unwrap();
        assert_eq!(s.authorize["client_id"], "oaiapp_fixture");
        assert!(!s.authorize.contains_key("agent_name_hint"));
        assert_eq!(s.authorize["ext_agent_host_id"], original_host);
    }
    logout(&store).unwrap();
    assert_eq!(
        auth::access_token(&store).unwrap_err(),
        auth::AuthError::SignedOut
    );
    let saved = std::fs::read_to_string(root.join("auth.json")).unwrap();
    assert!(!saved.contains("access-initial"));
    assert!(!saved.contains("refresh-initial"));
    assert!(!saved.contains("eyJ"));
    assert!(saved.contains("oaiapp_fixture"));
    mock.login(&store, auth::LoginMode::Continue).unwrap();
    assert_eq!(
        mock.state.lock().unwrap().authorize["client_id"],
        "oaiapp_fixture"
    );
    mock.state.lock().unwrap().fault = "identity".into();
    mock.login(&store, auth::LoginMode::NewAccount).unwrap();
    assert_eq!(
        mock.state.lock().unwrap().authorize["client_id"],
        "dynamic_agent_client"
    );
}
#[test]
fn callback_has_a_deadline_and_does_not_leak_attempt_values() {
    let tmp = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(tmp.path()).unwrap().join("auth");
    let mock = Fixture::new();
    let store = AuthStore::for_test(&root, &mock.base, Duration::from_millis(50)).unwrap();
    let start = std::time::Instant::now();
    let result = auth::login(&store, auth::LoginMode::Continue, |_| Ok(()));
    assert_eq!(result.unwrap_err(), auth::AuthError::Timeout);
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(mock.state.lock().unwrap().grants.is_empty());
}
#[test]
fn storage_rejects_symlinks_hardlinks_permissions_and_concurrent_lock() {
    use std::os::unix::{fs::symlink, io::AsRawFd};
    let tmp = tempfile::tempdir().unwrap();
    let dir = std::fs::canonicalize(tmp.path()).unwrap();
    let root = dir.join("auth");
    let store = AuthStore::at(&root).unwrap();
    symlink(&root, dir.join("alias")).unwrap();
    assert!(AuthStore::at(dir.join("alias")).is_err());
    let path = root.join("auth.json");
    let real = dir.join("saved");
    std::fs::rename(&path, &real).unwrap();
    symlink(&real, &path).unwrap();
    assert!(status(&store).is_err());
    assert!(logout(&store).is_err());
    std::fs::remove_file(&path).unwrap();
    std::fs::hard_link(&real, &path).unwrap();
    assert!(status(&store).is_err());
    std::fs::remove_file(&path).unwrap();
    std::fs::rename(&real, &path).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(status(&store).is_err());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("session.lock"))
        .unwrap();
    assert_eq!(
        unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    assert_eq!(status(&store).unwrap_err(), auth::AuthError::Busy);
    drop(lock);
    assert!(!status(&store).unwrap().signed_in);
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(status(&store).is_err());
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn failed_rotation_commit_or_lost_response_requires_login_after_restart() {
    for fault in ["write_failure", "disconnect"] {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap().join("auth");
        let mock = Fixture::new();
        let store = AuthStore::for_test(&root, &mock.base, Duration::from_secs(5)).unwrap();
        mock.login(&store, auth::LoginMode::Continue).unwrap();
        expire(&root);
        {
            let mut s = mock.state.lock().unwrap();
            s.fault = fault.into();
            s.storage_root = Some(root.clone());
        }
        assert!(auth::access_token(&store).is_err());
        std::fs::set_permissions(
            root.join("auth.json"),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        drop(store);
        let store = AuthStore::for_test(&root, &mock.base, Duration::from_secs(5)).unwrap();
        assert_eq!(
            auth::access_token(&store).unwrap_err(),
            auth::AuthError::Reauthorize
        );
        assert_eq!(mock.state.lock().unwrap().grants.len(), 2);
        assert!(!std::fs::read_to_string(root.join("auth.json"))
            .unwrap()
            .contains("refresh-rotated"));
    }
}
#[test]
fn catalog_transport_is_bounded_redacted_and_rejects_other_schemas() {
    let tmp = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(tmp.path()).unwrap().join("auth");
    let mock = Fixture::new();
    let store = AuthStore::for_test(&root, &mock.base, Duration::from_secs(5)).unwrap();
    mock.login(&store, auth::LoginMode::Continue).unwrap();
    for fault in ["catalog_oversized", "catalog_redirect", "catalog_invalid"] {
        mock.state.lock().unwrap().fault = fault.into();
        let error = auth::list_models(&store).unwrap_err();
        assert!(!error.to_string().contains("DO-NOT-LEAK"));
        assert!(!error.to_string().contains("access-initial"));
    }
}

#[test]
fn accepted_callback_waits_for_delayed_bytes_instead_of_inheriting_nonblocking() {
    let tmp = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(tmp.path()).unwrap().join("auth");
    let mock = Fixture::new();
    let store = AuthStore::for_test(&root, &mock.base, Duration::from_secs(1)).unwrap();
    let mut browser = None;
    let result = auth::login(&store, auth::LoginMode::Continue, |u| {
        let url = url::Url::parse(u).unwrap();
        let params: HashMap<String, String> = url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        let redirect = url::Url::parse(&params["redirect_uri"]).unwrap();
        let state = params["state"].clone();
        let port = redirect.port().unwrap();
        browser = Some(std::thread::spawn(move || {
            let mut socket = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            std::thread::sleep(Duration::from_millis(100));
            let _=socket.write_all(format!("GET /auth/callback?state={state}&error=access_denied HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n").as_bytes());
        }));
        Ok(())
    });
    browser.unwrap().join().unwrap();
    assert_eq!(result.unwrap_err(), auth::AuthError::Denied);
}

#[test]
fn private_store_has_stable_host_and_redacted_signed_out_status() {
    let tmp = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(tmp.path())
        .unwrap()
        .join("credentials");
    let store = AuthStore::at(&root).unwrap();
    let first: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("auth.json")).unwrap()).unwrap();
    let host = first["host_id"].as_str().unwrap();
    assert!(host.starts_with("urn:uuid:"));
    assert_eq!(host.len(), 45);
    assert_eq!(
        std::fs::metadata(&root).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(root.join("auth.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(!status(&store).unwrap().signed_in);
    logout(&store).unwrap();
    drop(store);
    let store = AuthStore::at(&root).unwrap();
    assert!(!status(&store).unwrap().signed_in);
    let second: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("auth.json")).unwrap()).unwrap();
    assert_eq!(first["host_id"], second["host_id"]);
}
