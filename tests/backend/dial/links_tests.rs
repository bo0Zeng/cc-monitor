//! `dial/links.rs`：池里的连接读数归到机器表里的那一台、写成成品的那几项（`MachineState.links`；那几项怎么写在 `ssh_dial::pool::rows_of`）。
//! 池快照是判据造的（不拨号）：身份照生产那一处现算（拨号请求 → `ssh_dial::pool::identity`），期望逐项手写。

use super::*;
use ssh_dial::pool::{LinkFacts, Wire};

fn host(v: serde_json::Value) -> Host {
    ssh_dial::machine::loose(&v).expect("机器表那一行读不动")
}

fn wire(endpoint: &str, compressed: bool, bytes_in: u64, bytes_out: u64) -> Wire {
    Wire {
        endpoint: endpoint.to_string(),
        compressed,
        via_proxy: false,
        bytes_in,
        bytes_out,
    }
}

/// 那台的身份：与池放置那一刻同一个算法（组请求 → identity）。
fn key_of(hosts: &[Host], name: &str) -> String {
    let req =
        crate::dial::dial_named(hosts, name, "capture", serde_json::json!({})).expect("组请求");
    ssh_dial::pool::identity(&req)
}

fn line(place: &str, compressed: bool, bytes_in: u64, bytes_out: u64) -> String {
    copy_core::copy_text(
        "beLinks.row.line",
        &[
            ("endpoint", place),
            (
                "packing",
                &copy_core::copy_text(
                    if compressed {
                        "beLinks.pack.compressed"
                    } else {
                        "beLinks.pack.plain"
                    },
                    &[],
                ),
            ),
            ("in", &copy_core::size_text(bytes_in)),
            ("out", &copy_core::size_text(bytes_out)),
        ],
    )
}

/// ★ 身份对上的进那台；对不上的哪台都不进；没连接的那台不出现。经跳板那一条：目标在前，跳板跟在后面（外在先）。
#[test]
fn pool_links_land_on_the_machine_whose_identity_they_carry() {
    let hosts = vec![
        host(
            serde_json::json!({"label": "box", "host": "box.test", "user": "u", "addresses": ["box.test", "box.lan:2222"]}),
        ),
        host(serde_json::json!({"label": "edge", "host": "edge.test", "user": "u"})),
        host(
            serde_json::json!({"label": "deep", "host": "deep.test", "user": "u", "jump": "edge"}),
        ),
        host(serde_json::json!({"label": "idle", "host": "idle.test", "user": "u"})),
    ];
    let snap = vec![
        (
            key_of(&hosts, "box"),
            vec![
                LinkFacts {
                    link: wire("box.lan:2222", true, 2048, 512),
                    jumps: vec![],
                },
                LinkFacts {
                    link: wire("box.test:22", false, 10, 20),
                    jumps: vec![],
                },
            ],
        ),
        (
            key_of(&hosts, "deep"),
            vec![LinkFacts {
                link: wire("deep.test:22", true, 3000, 100),
                jumps: vec![wire("edge.test:22", false, 5000, 900)],
            }],
        ),
        (
            "[\"someone-else\"]".to_string(),
            vec![LinkFacts {
                link: wire("other.test:22", false, 1, 1),
                jumps: vec![],
            }],
        ),
    ];
    let got = by_machine(&hosts, &snap);
    let row = |endpoint: &str, jump: bool, compressed: bool, i: u64, o: u64| MachineLink {
        endpoint: endpoint.to_string(),
        jump,
        compressed,
        via_proxy: false,
        bytes_in: i,
        bytes_out: o,
        said: if jump {
            copy_core::copy_text(
                "beLinks.row.jumpLine",
                &[("line", &line(endpoint, compressed, i, o))],
            )
        } else {
            line(endpoint, compressed, i, o)
        },
    };
    let want: Links = [
        (
            "box".to_string(),
            vec![
                row("box.lan:2222", false, true, 2048, 512),
                row("box.test:22", false, false, 10, 20),
            ],
        ),
        (
            "deep".to_string(),
            vec![
                row("deep.test:22", false, true, 3000, 100),
                row("edge.test:22", true, false, 5000, 900),
            ],
        ),
    ]
    .into_iter()
    .collect();
    assert_eq!(got, want);
}

/// ★ 写好的那一句：地址 · 压没压 · 进出字节都在里面（字照文案表，大小照 `size_text`）；直连那条不说经代理。
#[test]
fn the_line_says_where_whether_packed_and_the_bytes() {
    let r = LinkFacts {
        link: wire("h:22", true, 1536, 100),
        jumps: vec![],
    }
    .rows();
    assert_eq!(r.len(), 1);
    let said = &r[0].said;
    for part in [
        "h:22".to_string(),
        copy_core::copy_text("beLinks.pack.compressed", &[]),
        copy_core::size_text(1536),
        copy_core::size_text(100),
    ] {
        assert!(said.contains(&part), "那一句「{said}」里没有「{part}」");
    }
    assert!(!said.contains('〔'), "那一句用了文案表里没有的键：{said}");
    assert!(!r[0].via_proxy);
    assert_eq!(
        *said,
        line("h:22", true, 1536, 100),
        "直连那条的地址那一格就是地址本身"
    );
}

/// ★ 经代理：那一项带上 `via_proxy`，那一句的地址那一格照「经代理」那一条写（跳板那一项同样）；没经的那一项照旧。
#[test]
fn a_proxied_hop_says_so_in_its_line() {
    let mut outer = wire("edge.test:22", false, 5000, 900);
    outer.via_proxy = true;
    let r = LinkFacts {
        link: wire("deep.test:22", true, 3000, 100),
        jumps: vec![outer],
    }
    .rows();
    assert_eq!(r.len(), 2);
    assert!(!r[0].via_proxy && r[1].via_proxy, "{r:?}");
    let place = copy_core::copy_text("beLinks.route.proxy", &[("endpoint", "edge.test:22")]);
    assert!(
        !place.contains('〔'),
        "「经代理」那一条不在文案表里：{place}"
    );
    assert_ne!(place, "edge.test:22", "「经代理」那一条什么也没加");
    assert_eq!(
        r[1].said,
        copy_core::copy_text(
            "beLinks.row.jumpLine",
            &[("line", &line(&place, false, 5000, 900))]
        )
    );
    assert_eq!(r[0].said, line("deep.test:22", true, 3000, 100));
}
