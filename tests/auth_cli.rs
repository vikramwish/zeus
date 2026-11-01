use std::process::Command;

#[test]
fn help_describes_subscription_and_model_commands_without_connecting() {
    let output = Command::new(env!("CARGO_BIN_EXE_zeus"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("auth login chatgpt"));
    assert!(help.contains("auth status chatgpt"));
    assert!(help.contains("auth logout chatgpt"));
    assert!(help.contains("models chatgpt"));
}

#[test]
fn unsupported_subscription_is_not_silently_replaced_with_an_api_key() {
    let output = Command::new(env!("CARGO_BIN_EXE_zeus"))
        .args(["auth", "login", "unsupported-provider"])
        .env("OPENAI_API_KEY", "deliberately-not-a-real-key")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("deliberately-not-a-real-key"));
}
