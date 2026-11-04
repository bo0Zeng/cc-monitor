//! 拨号核心钥匙那一口（`ssh-dial/env.rs`）的判据：本 crate 不读盘，PEM 文本 ⇒ 钥匙只在 `key_from_pem` 一处。
use super::*;

/// 没装宿主那一套时（[`Env::bare`]）钥匙那一格不碰盘：盘上真有一把 OpenSSH 私钥、路径照写，也回读不出那一句。
#[test]
fn the_bare_env_never_reads_a_key_file() {
    let dir = std::env::temp_dir().join(format!("ccm-dial-bare-key-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let at = dir.join("id_t");
    std::fs::write(&at, test_pem().as_bytes()).unwrap();
    let shown = at.to_string_lossy().into_owned();
    let said = match Env::bare().keys.credential(&shown) {
        Err(s) => s.said,
        Ok(_) => panic!("没装宿主那一套也读盘拿到了钥匙"),
    };
    assert_eq!(
        said,
        copy_text("beConnect.auth.keyLoad", &[("path", &shown)])
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

/// 宿主读好的 PEM 文本由拨号核心那一处解：同一份文本解得出钥匙；坏文本回读不出那一句（路径照交来的那一形）、解码原话进 `raw`。
#[test]
fn a_pem_handed_in_by_the_host_decodes_in_the_core() {
    let pem = test_pem();
    assert!(matches!(
        key_from_pem("~/.ssh/id_t", &pem, None),
        Ok(Credential::Key(_))
    ));
    let bad = match key_from_pem("~/.ssh/id_t", "not a key", None) {
        Err(s) => s,
        Ok(_) => panic!("坏文本解成了钥匙"),
    };
    assert_eq!(bad.said, key_unreadable("~/.ssh/id_t"));
    assert!(bad.raw.is_some(), "解码原话没带上");
}

/// 一把 OpenSSH 格式的私钥文本（固定种子，判据现造）。
fn test_pem() -> String {
    PrivateKey::from(russh::keys::ssh_key::private::Ed25519Keypair::from_seed(
        &[7; 32],
    ))
    .to_openssh(russh::keys::ssh_key::LineEnding::LF)
    .unwrap()
    .to_string()
}
