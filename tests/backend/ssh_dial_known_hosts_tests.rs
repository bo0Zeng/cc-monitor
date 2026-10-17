//! 信哪把主机钥匙的规矩（拨号核心 `known_hosts.rs`）：一台一行写全部地址 · 任一地址命中都算 · 首见才追加 · 忘掉删这台全部 · 待核的指纹。

use super::*;

fn ep(host: &str, port: u16) -> Endpoint {
    Endpoint {
        host: host.to_string(),
        port,
    }
}

fn key(seed: u8) -> String {
    let k = russh::keys::PrivateKey::from(
        russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&[seed; 32]),
    );
    let line = k.public_key().to_openssh().unwrap();
    line.split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn host_names_follow_openssh_and_one_line_names_every_address() {
    assert_eq!(host_pattern("pi.local", 22), "pi.local");
    assert_eq!(host_pattern("192.0.2.1", 2222), "[192.0.2.1]:2222");
    assert_eq!(host_pattern("[::1]", 2200), "[::1]:2200");
    let m = [ep("a", 22), ep("b", 2222), ep("a", 22)];
    assert_eq!(names(&m), "a,[b]:2222");
}

/// 首见追加一行写全部地址；之后用这台任一地址都读得回那一把；别的台不算；再记（哪怕换了一把）不动。
#[test]
fn first_seen_appends_once_and_any_address_reads_it_back() {
    let m = [ep("a", 22), ep("b", 2222)];
    let t = remembered("other ssh-rsa AAAA\n", &m, &key(1)).unwrap();
    assert_eq!(t, format!("other ssh-rsa AAAA\na,[b]:2222 {}\n", key(1)));
    for one in [ep("a", 22), ep("b", 2222)] {
        let got = known(&t, std::slice::from_ref(&one));
        assert_eq!(got.len(), 1, "{one:?}");
        assert_eq!(
            got[0].to_openssh().unwrap().split_whitespace().nth(1),
            key(1).split_whitespace().nth(1)
        );
    }
    assert!(known(&t, &[ep("b", 22)]).is_empty(), "口不同是另一台");
    assert!(known(&t, &[ep("c", 22)]).is_empty());
    assert_eq!(
        remembered(&t, &[ep("b", 2222)], &key(2)),
        None,
        "有行就不动"
    );
}

/// 忘掉删这台的全部行（几种类型几行都删、连同待核的指纹），别的台留着；没有可删的 ⇒ 不写。
#[test]
fn forgetting_drops_every_line_of_that_machine_only() {
    let t = format!(
        "a {}\n[a]:2 {}\nb {}\n#cc-monitor-pending-fingerprint a SHA256:x\n",
        key(1),
        key(2),
        key(3)
    );
    let m = [ep("a", 22), ep("a", 2)];
    assert_eq!(forgotten(&t, &m).unwrap(), format!("b {}\n", key(3)));
    assert_eq!(forgotten(&format!("b {}\n", key(3)), &m), None);
    assert_eq!(forgotten(&format!("a {}\n", key(1)), &m).unwrap(), "");
}

/// 注释 · 标记行 · 散列过的主机名不算这台的钥匙。
#[test]
fn comments_markers_and_hashed_names_are_not_keys() {
    let t = format!("# a {k}\n@revoked a {k}\n|1|abc= {k}\n", k = key(1));
    assert!(known(&t, &[ep("a", 22)]).is_empty());
}

/// 待核的指纹：写一行注释、按任一地址读得回；记下整把时随之去掉；同一台不写第二行。
#[test]
fn a_pending_fingerprint_lives_until_the_whole_key_is_remembered() {
    let m = [ep("a", 22), ep("b", 22)];
    let t = with_pending("", &m, " SHA256:fp ").unwrap();
    assert_eq!(pending(&t, &[ep("b", 22)]).as_deref(), Some("SHA256:fp"));
    assert!(known(&t, &m).is_empty(), "待核那一行不是钥匙");
    assert_eq!(with_pending(&t, &m, "SHA256:other"), None);
    let after = remembered(&t, &[ep("a", 22)], &key(1)).unwrap();
    assert_eq!(after, format!("a {}\n", key(1)));
    assert_eq!(pending(&after, &m), None);
}
