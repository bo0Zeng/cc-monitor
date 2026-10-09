//! Linux 那一份「已建立的 TCP 连接表 ＋ 进程表」：`/proc/net/tcp*` 的行怎么认（合成文本、期望手写），
//! 再在这台上真开一条本机连接，看它真被认到本进程名下、链从本进程起。

use super::*;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, TcpListener, TcpStream};

const TCP4: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:0035 00000000:0000 0A 00000000:00000000 00:00000000 00000000   101        0 11111 1 0000000000000000 100 0 0 10 0
   1: 0502000A:F3CF 0902000A:0016 01 00000000:00000000 02:00000A1C 00000000  1000        0 22222 2 0000000000000000 20 4 30 10 -1
   2: 0502000A:F3D0 0902000A:0016 06 00000000:00000000 03:00000A1C 00000000  1000        0 0 3 0000000000000000 20 4 30 10 -1
";

const TCP6: &str = "  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0000000000000000FFFF00000502000A:C351 0000000000000000FFFF00001400000A:0016 01 00000000:00000000 00:00000000 00000000  1000        0 33333 1 0000000000000000 20 4 0 10 -1
   1: B80D0120000000000000000001000000:D431 B80D0120000000000000000002000000:0016 01 00000000:00000000 00:00000000 00000000  1000        0 44444 1 0000000000000000 20 4 0 10 -1
";

/// ★ 只认已建立（state 01）那几行；地址按内核打的「每 32 位一个本机序整数」还原，端口是十六进制；带出 socket 的 inode。
#[test]
fn proc_net_tcp_rows_read_back_to_addresses_ports_and_inodes() {
    let v4 = parse_proc_net_tcp(TCP4, Family::V4);
    assert_eq!(
        v4,
        vec![TcpLine {
            local: IpAddr::V4(Ipv4Addr::new(10, 0, 2, 5)),
            local_port: 0xF3CF,
            remote: IpAddr::V4(Ipv4Addr::new(10, 0, 2, 9)),
            remote_port: 22,
            inode: 22222,
        }],
        "监听（0A）与 TIME_WAIT（06）那两行不算"
    );
    let v6 = parse_proc_net_tcp(TCP6, Family::V6);
    assert_eq!(
        v6,
        vec![
            TcpLine {
                local: "::ffff:10.0.2.5".parse::<Ipv6Addr>().unwrap().into(),
                local_port: 0xC351,
                remote: "::ffff:10.0.0.20".parse::<Ipv6Addr>().unwrap().into(),
                remote_port: 22,
                inode: 33333,
            },
            TcpLine {
                local: "2001:db8::1".parse::<Ipv6Addr>().unwrap().into(),
                local_port: 0xD431,
                remote: "2001:db8::2".parse::<Ipv6Addr>().unwrap().into(),
                remote_port: 22,
                inode: 44444,
            },
        ]
    );
    // 认不出的行丢掉，不整份失败。
    assert!(
        parse_proc_net_tcp("garbage\n   0: zz:zz yy:yy 01 x x x x 0 0 5\n", Family::V4).is_empty()
    );
}

/// ★ `/proc/<pid>/stat` 那一行 ⇒ 父进程号 · 名字 · 启动时刻；名字里带空格与括号也认得对（找最后一个右括号）。
#[test]
fn proc_stat_reads_parent_name_and_start_even_with_odd_names() {
    let line = "4242 (my (odd) term) S 4200 4242 4242 34816 4242 4194304 1 0 0 0 0 0 0 0 20 0 1 0 987654 1000 100";
    assert_eq!(
        parse_stat(line),
        Some(StatLine {
            ppid: 4200,
            name: "my (odd) term".into(),
            start: 987654,
        })
    );
    assert_eq!(parse_stat("4242 no-parens S 1"), None);
}

/// ★ `socket:[N]` 那种链接 ⇒ N；别的链接（文件 · 管道 · anon_inode）不是。
#[test]
fn only_socket_links_name_an_inode() {
    assert_eq!(socket_inode("socket:[22222]"), Some(22222));
    assert_eq!(socket_inode("pipe:[22222]"), None);
    assert_eq!(socket_inode("/dev/pts/3"), None);
    assert_eq!(socket_inode("anon_inode:[eventfd]"), None);
}

/// ★ 真机：本进程开一条本机连接 ⇒ 这一趟的连接表里，四元组全等那一行的拥有者是本进程；进程表里有本进程，父进程也在。
#[test]
fn a_live_loopback_connection_is_owned_by_this_process() {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let c = TcpStream::connect(l.local_addr().unwrap()).unwrap();
    let (_s, _) = l.accept().unwrap();
    let (la, ra) = (c.local_addr().unwrap(), c.peer_addr().unwrap());
    let raw = connection_and_process_tables().expect("这台的 /proc 读得到");
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let me = std::process::id();
    let hit = v["tcp"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| {
            r["la"] == la.ip().to_string()
                && r["lp"] == la.port()
                && r["ra"] == ra.ip().to_string()
                && r["rp"] == ra.port()
        })
        .unwrap_or_else(|| panic!("连接表里没有 {la} → {ra}：{raw:.400}"));
    assert_eq!(hit["pid"], me, "拥有者认成了别人");
    let procs = v["proc"].as_array().unwrap();
    let mine = procs
        .iter()
        .find(|p| p["pid"] == me)
        .expect("进程表里有本进程");
    assert!(mine["start"].as_u64().unwrap() > 0);
    let ppid = mine["ppid"].as_u64().unwrap();
    assert!(procs.iter().any(|p| p["pid"] == ppid), "父进程也在表里");
}
