//! 要求：部署那一族的契约（键 · 戳格式 · 答话形状 · 路径）；「读它字节里那段身份戳，不跑它」。
//!
//! 判定那几格（身份判定 · 取样解释 · 只升不降 · 旧入口 · 旧落点 · 表 A / 表 B 的判序）随判定搬进后端 `tests/backend/control/deploy_plan_tests.rs`；
//! 这里只剩戳的格式（字节自报 · 序键）；问那台的扫描命令与它回话的解读随判定搬进后端 `tests/backend/deploy/lib_tests.rs`。界标由参数交（[`Marks`]）⇒ 用一对**占位界标**（规矩只看「两个界标之间 `[[:alnum:]_.-]+`」）。

use super::*;

/// 占位界标（真界标是本 crate 的 `STAMP_OPEN` / `STAMP_CLOSE`）。
const M: Marks<'static> = Marks {
    open: "<<id:",
    close: ":id>>",
};

// ═══ 戳的序键（「只升不降」那条判定住后端）══════════════════════════════

/// B1a：序键手写表 —— 合法形 · 多位代号 · 缺字母 · 大写 · 缺名 · 缺前缀。
#[test]
fn hx2_build_order_reads_generation_and_letter_and_refuses_other_shapes() {
    let cases: &[(&str, Option<(u32, u8)>)] = &[
        ("p1a-history", Some((1, b'a'))),
        ("p3m-ssh-zlib", Some((3, b'm'))),
        ("p2z-relay-in-resident", Some((2, b'z'))),
        ("p12c-x", Some((12, b'c'))),
        ("p3-x", None),
        ("p3M-x", None),
        ("p3m", None),
        ("p3m-", None),
        ("3m-x", None),
        ("sr1b-id", None),
        ("", None),
    ];
    for (id, want) in cases {
        assert_eq!(build_order(id), *want, "{id:?}");
    }
}

/// B1a 后半：另一份相对手上这一版 —— 逐字相同 ⇒ 同；两边都解得出才分新旧；同序不同名 · 任一边解不出 ⇒ 不可比。
#[test]
fn hx2_relation_is_newer_or_older_only_when_both_orders_parse_and_differ() {
    use Relation::{Incomparable, Newer, Older, Same};
    let cases: &[(&str, &str, Relation)] = &[
        ("p3m-a", "p3m-a", Same),
        ("junk", "junk", Same),
        ("p3m-b", "p3n-a", Older),
        ("p3z-b", "p4a-a", Older),
        ("p3n-b", "p3m-a", Newer),
        ("p12a-b", "p9z-a", Newer),
        ("p3m-b", "p3m-a", Incomparable),
        ("junk", "p3n-a", Incomparable),
        ("p1a-x", "junk", Incomparable),
    ];
    for (theirs, mine, want) in cases {
        assert_eq!(
            build_relation(theirs, mine),
            *want,
            "{theirs:?} 对 {mine:?}"
        );
    }
}

// B1b（出过的每一个 `BUILD_ID` 都有序、历史表爬升）住后端 `deploy_plan_tests.rs`：它读的是后端源码里的历史表，
//   放在后端那一侧不跨两半（`cross_half_edge_registry`）。

/// 手上一份字节自报的身份：与远端那条扫描同一条规矩（恰好一个才是身份 · 重复去重 · 空身份不收 · 0 字节是 Empty）。
#[test]
fn the_bytes_answer_their_own_identity_by_the_same_rule_as_the_remote_scan() {
    let stamp = |id: &str| format!("{}{id}{}", M.open, M.close);
    assert_eq!(identity_of_bytes(b"", M), RemoteIdentity::Empty);
    assert_eq!(identity_of_bytes(b"no stamp", M), RemoteIdentity::NoStamp);
    assert_eq!(
        identity_of_bytes(
            format!("xx{}yy{}", stamp("p9a-s"), stamp("p9a-s")).as_bytes(),
            M
        ),
        RemoteIdentity::Stamp("p9a-s".into())
    );
    assert_eq!(
        identity_of_bytes(format!("{}{}", stamp("b2"), stamp("a1")).as_bytes(), M),
        RemoteIdentity::Ambiguous(vec!["a1".into(), "b2".into()])
    );
    assert_eq!(
        identity_of_bytes(stamp("").as_bytes(), M),
        RemoteIdentity::NoStamp
    );
}
