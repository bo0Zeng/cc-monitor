//! cc-monitor 自己那份 known_hosts：一个「主机 ＋ 口」一行，换了钥匙换那一行，内容没变不写；主机名照 OpenSSH 的写法。

use super::*;

#[test]
fn one_line_per_host_and_port_replaced_in_place_and_unchanged_means_no_write() {
    assert_eq!(host_pattern("pi.local", 22), "pi.local");
    assert_eq!(host_pattern("192.0.2.1", 2222), "[192.0.2.1]:2222");
    assert_eq!(host_pattern("[::1]", 2200), "[::1]:2200");
    let a = merged("", "[h]:2222", "ssh-ed25519 AAAA1").unwrap();
    assert_eq!(a, "[h]:2222 ssh-ed25519 AAAA1\n");
    assert_eq!(
        merged(&a, "[h]:2222", "ssh-ed25519 AAAA1"),
        None,
        "同一把不写"
    );
    let b = merged(&a, "other", "ssh-rsa BBBB").unwrap();
    assert_eq!(b, "[h]:2222 ssh-ed25519 AAAA1\nother ssh-rsa BBBB\n");
    // 换钥匙 ⇒ 那一行原地换掉，别的行不动。
    assert_eq!(
        merged(&b, "[h]:2222", "ssh-ed25519 AAAA2").unwrap(),
        "[h]:2222 ssh-ed25519 AAAA2\nother ssh-rsa BBBB\n"
    );
    // 「[h]:2222」与「h」是两台。
    assert_eq!(
        merged(&b, "h", "ssh-ed25519 CCCC").unwrap(),
        "[h]:2222 ssh-ed25519 AAAA1\nother ssh-rsa BBBB\nh ssh-ed25519 CCCC\n"
    );
}
