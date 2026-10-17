//! 机器表里固化的主机指纹换进那份库（下一版删）：对得上的直接删格 · 对不上的删那几行、写成待核 · 没行的写成待核；
//! 机器表删掉那两格、读得回；再做一次不变样。

use super::*;

fn ep(h: &str, p: u16) -> Endpoint {
    Endpoint {
        host: h.into(),
        port: p,
    }
}

fn key(seed: u8) -> (String, String) {
    let k = russh::keys::PrivateKey::from(
        russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&[seed; 32]),
    );
    let line = k.public_key().to_openssh().unwrap();
    let two = line
        .split_whitespace()
        .take(2)
        .collect::<Vec<_>>()
        .join(" ");
    (two, kh::fingerprint(k.public_key()))
}

#[test]
fn matching_rows_stay_mismatching_rows_go_and_the_rest_become_pending() {
    let (ka, fa) = key(1);
    let (kb, _) = key(2);
    let (_, fb) = key(3);
    let text = format!("a {ka}\nb {kb}\nother {kb}\n");
    let pinned = vec![
        (vec![ep("a", 22)], fa.clone()),
        (vec![ep("b", 22)], fb.clone()),
        (vec![ep("c", 2222), ep("c2", 22)], "SHA256:c".to_string()),
    ];
    let after = library_after(&text, &pinned);
    assert_eq!(
        after,
        format!(
            "a {ka}\nother {kb}\n#cc-monitor-pending-fingerprint b {fb}\n#cc-monitor-pending-fingerprint [c]:2222,c2 SHA256:c\n"
        )
    );
    assert!(library_holds(&after, &pinned));
    assert!(!library_holds(&text, &pinned), "量具：换形之前不该判成已换");
    assert_eq!(library_after(&after, &pinned), after, "再做一次不变样");
}

#[test]
fn the_table_loses_both_cells_and_the_library_holds_every_pin() {
    let home = std::env::temp_dir().join(format!("ccm-hkm-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join(".cc-monitor")).unwrap();
    let own = crate::platform::fs::Own::issue_at(&home);
    let (ka, fa) = key(1);
    std::fs::write(home.join(".cc-monitor/known_hosts"), format!("a {ka}\n")).unwrap();
    let table = serde_json::json!({"version": 1, "hosts": [
        {"label": "甲", "host": "a", "user": "u", "hostKeyFingerprint": fa, "hostKeyPinnedAt": "2026-01-01"},
        {"label": "乙", "host": "b", "port": 2200, "user": "u", "hostKeyFingerprint": "SHA256:b", "hostKeyPinnedAt": ""},
        {"label": "丙", "host": "c", "user": "u"}
    ]});
    let file = home.join(".cc-monitor/machines.json");
    std::fs::write(&file, table.to_string()).unwrap();
    assert!(
        matches!(machines::read_at(&file), machines::Loaded::Unreadable(_)),
        "量具：旧形状新表读不懂"
    );
    let lib = crate::dial::known_hosts::writer_at(&own);
    assert_eq!(migrate_in(&own, &lib), Moved::Done(2));
    let machines::Loaded::Present(hosts) = machines::read_at(&file) else {
        panic!("换形后机器表读不回");
    };
    assert_eq!(
        hosts
            .iter()
            .map(|h| h.name().to_string())
            .collect::<Vec<_>>(),
        ["甲", "乙", "丙"]
    );
    assert!(!std::fs::read_to_string(&file).unwrap().contains("hostKey"));
    assert_eq!(
        std::fs::read_to_string(home.join(".cc-monitor/known_hosts")).unwrap(),
        format!("a {ka}\n#cc-monitor-pending-fingerprint [b]:2200 SHA256:b\n")
    );
    assert_eq!(migrate_in(&own, &lib), Moved::Nothing);
    let _ = std::fs::remove_dir_all(&home);
}
