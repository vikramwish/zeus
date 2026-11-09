use zeus::workspace::Workspace;

fn source() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    std::fs::create_dir_all(source.join("config")).unwrap();
    std::fs::write(
        source.join("app.py"),
        "print('fine')\nTOKEN_NAME = 'sk-'  # short mention only\n",
    )
    .unwrap();
    std::fs::write(
        source.join("settings.py"),
        "OPENAI_API_KEY = \"sk-abcdefghijklmnopqrstuvwxyz0123456789\"\nDEBUG = True\n",
    )
    .unwrap();
    std::fs::write(
        source.join("deploy.pem.txt"),
        "-----BEGIN RSA PRIVATE KEY-----\nMIIEfixture\n-----END RSA PRIVATE KEY-----\n",
    )
    .unwrap();
    std::fs::write(
        source.join("aws.cfg"),
        "aws_access_key_id = AKIAIOSFODNN7EXAMPLE\n",
    )
    .unwrap();
    std::fs::write(
        source.join("config/.netrc"),
        "machine x login y password z\n",
    )
    .unwrap();
    std::fs::write(source.join("config/.npmrc"), "//registry/:_authToken=abc\n").unwrap();
    std::fs::write(source.join("cert.p12"), b"\x30\x82binary").unwrap();
    std::fs::write(source.join("terraform.tfstate"), "{}").unwrap();
    (temp, source)
}

#[test]
fn credential_filenames_are_excluded_from_snapshots() {
    let (temp, source) = source();
    let workspace = Workspace::snapshot(&source, &temp.path().join("candidate")).unwrap();
    let skipped = workspace.inventory.skipped.clone();
    for name in [
        "config/.netrc",
        "config/.npmrc",
        "cert.p12",
        "terraform.tfstate",
    ] {
        assert!(
            skipped.iter().any(|s| s == name),
            "{name} not skipped: {skipped:?}"
        );
        assert!(
            !temp.path().join("candidate").join(name).exists(),
            "{name} copied"
        );
        assert!(workspace.read(name).is_err());
    }
    assert!(workspace
        .inventory
        .entries
        .iter()
        .any(|e| e.path == "app.py"));
}

#[test]
fn secret_like_content_is_copied_but_never_shown_to_the_model() {
    let (temp, source) = source();
    let workspace = Workspace::snapshot(&source, &temp.path().join("candidate")).unwrap();
    // The verifier may still need these files, so the candidate keeps them.
    for name in ["settings.py", "deploy.pem.txt", "aws.cfg"] {
        assert!(
            temp.path().join("candidate").join(name).is_file(),
            "{name} missing from candidate"
        );
        let error = workspace.read(name).unwrap_err();
        assert!(error.contains("secret-like"), "{name}: {error}");
        assert!(!error.contains("sk-abcdefghijklmnopqrstuvwxyz0123456789"));
        assert!(!error.contains("AKIAIOSFODNN7EXAMPLE"));
        let entry = workspace
            .inventory
            .entries
            .iter()
            .find(|e| e.path == name)
            .unwrap();
        assert_eq!(entry.kind, "secret-like-text", "{name}");
    }
    // Ordinary text, including a short mention of a prefix, stays readable.
    let read = workspace.read("app.py").unwrap();
    assert!(read.content.contains("fine"));
    assert_eq!(
        workspace
            .inventory
            .entries
            .iter()
            .find(|e| e.path == "app.py")
            .unwrap()
            .kind,
        "text"
    );
}

#[test]
fn search_skips_secret_like_files_with_a_reason_and_no_excerpt() {
    let (temp, source) = source();
    let workspace = Workspace::snapshot(&source, &temp.path().join("candidate")).unwrap();
    let result = workspace.search("sk-", 50).unwrap();
    let matches = result["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1, "{result}");
    assert_eq!(matches[0]["path"], "app.py");
    let skipped = result["skipped"].as_array().unwrap();
    let settings = skipped
        .iter()
        .find(|s| s["path"] == "settings.py")
        .unwrap_or_else(|| panic!("{result}"));
    assert!(settings["reason"].as_str().unwrap().contains("secret-like"));
    assert!(!result
        .to_string()
        .contains("sk-abcdefghijklmnopqrstuvwxyz0123456789"));
}
