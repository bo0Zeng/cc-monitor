//! `dial/links.rs`：池里的连接读数归到机器表里的那一台，写成「怎么连过去的」（成品 `route`）与每条连接那一句（进复制详情）；
//! 那两样怎么写在 `ssh_dial::pool::way_of`。池快照是判据造的（不拨号）：身份照生产那一处现算（拨号请求 → `ssh_dial::pool::identity`），
//! 期望逐项手写。

use super::*;
use copy_core::copy_text;
use ssh_dial::pool::{LinkFacts, Way, Wire};

fn host(v: serde_json::Value) -> Host {
    ssh_dial::machine::loose(&v).expect("机器表那一行读不动")
}

/// `compressed` ⇒ 这一跳远（闸开着：远的那一跳 SSH 层压）。
fn wire(endpoint: &str, compressed: bool, bytes_in: u64, bytes_out: u64) -> Wire {
    Wire {
        endpoint: endpoint.to_string(),
        far: compressed,
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
    copy_text(
        "beLinks.row.line",
        &[
            ("endpoint", place),
            (
                "packing",
                &copy_text(
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

fn jump_line(place: &str, compressed: bool, bytes_in: u64, bytes_out: u64) -> String {
    copy_text(
        "beLinks.row.jumpLine",
        &[("line", &line(place, compressed, bytes_in, bytes_out))],
    )
}

fn jumped(chain: &[&str]) -> String {
    copy_text(
        "beLinks.route.jump",
        &[("chain", &chain.join(&copy_text("beProfile.chain.arrow", &[])))],
    )
}

fn table() -> Vec<Host> {
    vec![
        host(
            serde_json::json!({"label": "box", "host": "box.test", "user": "u", "addresses": ["box.test", "box.lan:2222"]}),
        ),
        host(serde_json::json!({"label": "edge", "host": "edge.test", "user": "u"})),
        host(
            serde_json::json!({"label": "deep", "host": "deep.test", "user": "u", "jump": "edge"}),
        ),
        host(serde_json::json!({"label": "far", "host": "far.test", "user": "u", "jump": "deep"})),
        host(serde_json::json!({"label": "idle", "host": "idle.test", "user": "u"})),
    ]
}

/// ★ 身份对上的进那台；对不上的哪台都不进。直连连着、不经代理 ⇒ 没有 `route`，每条连接一句；
/// 经跳板 ⇒ `route` 写跳板名（多跳外到内），连没连上都写；直连又没连接的那台不出现。
#[test]
fn each_machine_gets_how_it_is_reached_and_one_line_per_link() {
    let hosts = table();
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
    let want: Links = [
        (
            "box".to_string(),
            Way {
                route: None,
                hops: vec![
                    line("box.lan:2222", true, 2048, 512),
                    line("box.test:22", false, 10, 20),
                ],
            },
        ),
        (
            "deep".to_string(),
            Way {
                route: Some(jumped(&["edge"])),
                hops: vec![
                    line("deep.test:22", true, 3000, 100),
                    jump_line("edge.test:22", false, 5000, 900),
                ],
            },
        ),
        (
            "far".to_string(),
            Way {
                route: Some(jumped(&["edge", "deep"])),
                hops: vec![],
            },
        ),
    ]
    .into_iter()
    .collect();
    assert_eq!(got, want);
}

/// ★ 常驻流要不要那台压（`ssh_dial::pool::far_of`）：那台在池里的连接全近 ⇒ 近；有一条远 ⇒ 远；
/// 一条都没有 · 身份对不上 ⇒ 远（答不上来宁可压）。跳板那几条不算（只运隧道）。
#[test]
fn the_resident_feed_asks_the_pool_whether_the_hop_is_far() {
    use ssh_dial::pool::far_of;
    let hosts = table();
    let req = |name: &str| {
        crate::dial::dial_named(&hosts, name, "stream", serde_json::json!({})).expect("组请求")
    };
    let one = |link: Wire, jumps: Vec<Wire>| LinkFacts { link, jumps };
    let snap = vec![
        (
            key_of(&hosts, "box"),
            vec![
                one(wire("box.lan:2222", false, 1, 1), vec![]),
                one(wire("box.test:22", false, 1, 1), vec![]),
            ],
        ),
        (
            key_of(&hosts, "deep"),
            vec![
                one(wire("deep.test:22", false, 1, 1), vec![]),
                one(
                    wire("deep.test:22", true, 1, 1),
                    vec![wire("edge.test:22", false, 1, 1)],
                ),
            ],
        ),
        (
            key_of(&hosts, "edge"),
            vec![one(
                wire("edge.test:22", false, 1, 1),
                vec![wire("x.test:22", true, 1, 1)],
            )],
        ),
    ];
    assert!(!far_of(&snap, &req("box")), "全近 ⇒ 近");
    assert!(far_of(&snap, &req("deep")), "有一条远 ⇒ 远");
    assert!(!far_of(&snap, &req("edge")), "跳板那几条不算");
    assert!(far_of(&snap, &req("idle")), "没连接 ⇒ 远");
    assert!(far_of(&[], &req("box")), "空快照 ⇒ 远");
}

/// ★ 经代理只有连上了才知道：那几条里有一条经代理 ⇒ `route` 说经代理（直连 ⇒ 只这一句；经跳板 ⇒ 接在跳板后面）；
/// 那一条的那一句地址那一格照「经代理」那一条写。
#[test]
fn a_proxied_link_says_so_in_the_route_and_in_its_line() {
    let hosts = table();
    let mut proxied = wire("box.test:22", false, 10, 20);
    proxied.via_proxy = true;
    let mut outer = wire("edge.test:22", false, 5000, 900);
    outer.via_proxy = true;
    let snap = vec![
        (
            key_of(&hosts, "box"),
            vec![LinkFacts {
                link: proxied,
                jumps: vec![],
            }],
        ),
        (
            key_of(&hosts, "deep"),
            vec![LinkFacts {
                link: wire("deep.test:22", true, 3000, 100),
                jumps: vec![outer],
            }],
        ),
    ];
    let got = by_machine(&hosts, &snap);
    let place = |e: &str| copy_text("beLinks.route.proxy", &[("endpoint", e)]);
    assert_eq!(
        got["box"],
        Way {
            route: Some(copy_text("beLinks.route.proxyOnly", &[])),
            hops: vec![line(&place("box.test:22"), false, 10, 20)],
        }
    );
    assert_eq!(
        got["deep"],
        Way {
            route: Some(copy_text("beLinks.route.jumpProxy", &[("chain", "edge")])),
            hops: vec![
                line("deep.test:22", true, 3000, 100),
                jump_line(&place("edge.test:22"), false, 5000, 900),
            ],
        }
    );
}
