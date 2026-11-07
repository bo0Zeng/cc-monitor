use super::*;

const ED: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILQCC8lGudKQHbZ2zZuQG78N9fMA/7+ta9ABfB4Y7pAy t";
const EC: &str = "ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBNVPflvnRiV4KzyMCaIDRxk6eINfhT6IOrOG4SEOrYVALTND5Xb4MjkIaVfXIczcP3pg1Q5Q3ras3DnAv9Czelw= t";
const RSA: &str = "ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAAAgQDS43W3W6qjX2fRFLQ+OfS09MndTxwcJU6gieI3DRKffKInfOxNRpr20dhaeKVuK5Hh4uHeeiArihT2SFuI5i0DhubdqyOK9ETka84EYdTf3SgIJPw0O5KSqEjPUowCNBU/LLbI/KVd1mHVmQvseXh5ejsU6sb1ihynKKkztj8AWQ== t";

fn sep() -> String {
    copy_core::copy_text("kit.text.sep", &[])
}

/// 三种算法各出那个名字；指纹是拨号核心那一份整串（`ssh-keygen -lf` 同一串）切成 前 4 … 后 2。
#[test]
fn each_algorithm_gets_its_name_and_the_short_fingerprint() {
    // 整串（ssh-keygen -lf）：wVTBShlYkkoB8WD0hGJe9lQkqJo1i208w88daYI/LiA
    assert_eq!(key_line(ED), format!("Ed25519{}SHA256:wVTB…iA", sep()));
    // trdeHNfUjwwkV7yIi6LQS3ZVkIsTI/077L8y5i7KkUU
    assert_eq!(key_line(EC), format!("ECDSA P-256{}SHA256:trde…UU", sep()));
    // xJ/T/viS7wJ+ey8sDri3n42/kJynn4oO9gG4byLwdG0
    assert_eq!(key_line(RSA), format!("RSA{}SHA256:xJ/T…G0", sep()));
}

/// 切的是拨号核心那一份整串的头尾，不另算。
#[test]
fn the_short_form_cuts_the_dial_core_fingerprint() {
    let key = russh::keys::PublicKey::from_openssh(ED).unwrap();
    let full = ssh_dial::known_hosts::fingerprint(&key);
    let body = full.strip_prefix("SHA256:").unwrap();
    let line = key_line(ED);
    let short = line.rsplit(&sep()).next().unwrap();
    assert_eq!(
        short,
        format!("SHA256:{}…{}", &body[..4], &body[body.len() - 2..])
    );
}

/// 读不出的一行 ⇒ 「公钥无法读取」那一句，不是空串。
#[test]
fn an_unreadable_line_says_so() {
    let unreadable = copy_core::copy_text("mobIdentity.row.unreadable", &[]);
    assert!(!unreadable.is_empty());
    assert_eq!(key_line(""), unreadable);
    assert_eq!(key_line("ssh-ed25519 notbase64 t"), unreadable);
}
