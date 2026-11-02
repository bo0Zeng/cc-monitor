//! 第三方 API key 的明文只往下传登记过的那几跳；monitor 一侧零读零写（本机那一份的写者是本机常驻后端）。
//!
//! 调用面数的是具名出口方法的调用点，不是「明文」这个概念：经别的路径带出去（自定义类型、`Deref`）这里看不见，
//! 那一格由 `creds-core` 定义面的判据兜（有几个出口 · 类型没有第二个交出内层串的 impl）。
//! monitor 够不到凭据文件的写半边由编译器挡：写半边在 `harden` feature 后面，monitor 不开（`creds_guard.rs` 钉 manifest）。

use super::{equal, none, rule};

/// 定义面那张表（`creds-core` 判据 `INNER_FIELD_USERS`）住在哪。
const DEFINITION_TABLE: &str = "tests/common/creds-core/lib_tests.rs";

/// 扫哪几棵树：这就是「每个出口恰好一处调用」的分母（通信层也在里面：门上比对只经只回布尔的那一口）。
const PLAINTEXT_SCAN_TREES: &[&str] = &[
    "src/frontend/shell/src",
    "src/common",
    "src/comms",
    "src/backend",
];

/// 装秘密的类型的唯一住址：`(文件, 类型名)`。
const SECRET_HOLDER: (&str, &str) = ("src/common/creds-core/src/lib.rs", "SecretKey");

/// 长得像装秘密的类型、装的却不是秘密的那几个：`(文件, 类型名, 装的是什么)`。加一行就是在放宽。
const NOT_SECRET_HOLDERS: &[(&str, &str, &str)] = &[(
    "src/backend/dial/mod.rs",
    "Held",
    "活流计数的把手：装那台的机器名，丢掉时把计数减一",
)];

/// 生产段里每一个「字段私有、只包一个 `String`、不派生 `Debug`」的元组结构的名字 —— 装秘密的类型那副样子
/// （派生 `Debug` 会把内层原样印出来，装秘密的不会派生它）。属性取的是上一个 item 收尾到 `struct` 之间那一段。
fn string_wrappers_without_derived_debug(prod: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (i, _) in prod.match_indices("struct ") {
        let rest = &prod[i + "struct ".len()..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let shape: String = rest[name.len()..]
            .chars()
            .take_while(|c| *c != ';' && *c != '{')
            .filter(|c| !c.is_whitespace())
            .collect();
        if name.is_empty() || shape != "(String)" {
            continue;
        }
        let head_from = prod[..i].rfind([';', '}', '{']).map_or(0, |k| k + 1);
        let head = &prod[head_from..i];
        let derives_debug = head.match_indices("derive(").any(|(d, _)| {
            let inner = &head[d + "derive(".len()..];
            let inner = &inner[..inner.find(')').unwrap_or(inner.len())];
            inner
                .split(',')
                .any(|t| t.trim() == "Debug" || t.trim().ends_with("::Debug"))
        });
        if !derives_debug {
            out.push(name);
        }
    }
    out
}

/// 通信层从 `creds-core` 叫得出的：`(文件, 跟在 `creds_core::` 后面的那一段)`。只有装钥匙的类型（门上那两把）。
const COMMS_NAMES_FROM_CREDS_CORE: &[(&str, &str)] = &[("src/comms/outward/door.rs", "SecretKey")];

/// 通信层依赖 `creds-core` 只为装钥匙的那个类型：鉴权头的形状（`store::AuthStyle`）· 凭据文件的读写与权限都是上游选择与宿主的事。
/// 依赖边开着，编译器不再挡「通信层叫出 `creds_core::store::…`」⇒ 这里数：通信层生产段里每一处 `creds_core::<名字>`
/// 与登记的两向相等；`comms-outward` 引它不开 `harden`（权限那一半连编都编不进来）。
#[test]
fn the_comms_layer_names_nothing_from_creds_core_but_the_key_type() {
    let named = |prod: &str| -> Vec<String> {
        prod.match_indices("creds_core::")
            .map(|(i, m)| {
                prod[i + m.len()..]
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '{')
                    .collect()
            })
            .collect()
    };
    assert_eq!(
        named("use creds_core::store::AuthStyle;\nuse creds_core::{perm, SecretKey};\n"),
        vec!["store".to_string(), "{perm".to_string()],
        "合成样本上认法不对"
    );
    rule(|repo| {
        let found = repo
            .under(&["src/comms"], &["rs"])
            .into_iter()
            .flat_map(|f| named(f.prod()).into_iter().map(|n| (f.rel.clone(), n)));
        equal(
            "通信层从 `creds-core` 只许叫出装钥匙的 `SecretKey`",
            found,
            COMMS_NAMES_FROM_CREDS_CORE
                .iter()
                .map(|(f, n)| (f.to_string(), n.to_string())),
        );
        let manifest = repo.file("src/comms/outward/Cargo.toml").text();
        let dep: Vec<&str> = manifest
            .lines()
            .filter(|l| l.trim_start().starts_with("creds-core"))
            .collect();
        assert_eq!(dep.len(), 1, "`comms-outward` 的清单里 `creds-core` 那一行：{dep:?}");
        assert!(
            !dep[0].contains("features"),
            "`comms-outward` 引 `creds-core` 开了 feature：{} —— 通信层只要那个类型，不碰凭据文件",
            dep[0]
        );
    });
}

/// 装一串秘密、不让它进日志、只经具名的口取明文 —— 这件事只许 `creds_core::SecretKey` 一个类型做。
/// 别处再长一个同样形状的类型，就是同一条安全性质的第二个实现，`creds-core` 定义面那几条判据看不见它。
/// 认的是形状（私有 `String` 字段 · 不派生 `Debug`），不是名字；手写遮蔽的 `Debug` 与干脆没有 `Debug` 都算。
#[test]
fn only_creds_core_defines_a_type_that_wraps_a_secret_string() {
    let planted = "#[derive(Clone)]\npub struct Token(String);\n#[derive(Debug, Clone)]\npub struct Name(String);\n";
    assert_eq!(
        string_wrappers_without_derived_debug(planted),
        vec!["Token".to_string()],
        "合成样本上认法不对：派生了 `Debug` 的不算，没派生的算"
    );
    rule(|repo| {
        let found = repo
            .under(PLAINTEXT_SCAN_TREES, &["rs"])
            .into_iter()
            .flat_map(|f| {
                string_wrappers_without_derived_debug(f.prod())
                    .into_iter()
                    .map(|n| (f.rel.clone(), n))
            });
        let registered = std::iter::once(SECRET_HOLDER)
            .chain(NOT_SECRET_HOLDERS.iter().map(|(f, n, _)| (*f, *n)))
            .map(|(f, n)| (f.to_string(), n.to_string()));
        equal(
            "装一串秘密的类型（私有 `String` 字段 · 不派生 `Debug`）只许是 `creds_core::SecretKey`",
            found,
            registered,
        );
    });
}

/// 定义面标 `Handling::HandsOut("<唯一调用点住的文件>")` 的每一行 ⇒ `(方法名, 那份文件)`。
fn plaintext_exit_sites(core: &str) -> Vec<(String, String)> {
    let at = crate::find_pinned(core, "const INNER_FIELD_USERS:").expect("切不出定义面那张表");
    let table = core[at..]
        .find("\n];")
        .map(|i| &core[at..at + i])
        .expect("切不出表体（找不到 `];` 收尾）");
    let mark = "Handling::HandsOut(\"";
    let mut out = Vec::new();
    for (i, _) in table.match_indices(mark) {
        let before = &table[..i];
        let name = before
            .rfind('"')
            .and_then(|q_end| {
                before[..q_end]
                    .rfind('"')
                    .map(|q_start| &before[q_start + 1..q_end])
            })
            .expect("`HandsOut` 前面切不出方法名");
        let rest = &table[i + mark.len()..];
        let home = &rest[..rest.find('"').expect("`HandsOut(\"…` 没有收尾引号")];
        out.push((name.to_string(), home.to_string()));
    }
    out
}

#[test]
fn the_plaintext_exits_are_called_from_exactly_one_place_each_across_all_three_trees() {
    rule(|repo| {
        let sites = plaintext_exit_sites(repo.file(DEFINITION_TABLE).text());
        assert!(
            sites.len() >= 2,
            "定义面里只读出 {} 个 `HandsOut` —— 取法坏了，本条会零命中地绿：{sites:?}",
            sites.len()
        );
        let mut found: Vec<(String, String)> = Vec::new();
        for f in repo.under(PLAINTEXT_SCAN_TREES, &["rs"]) {
            let prod = f.prod();
            for (name, _) in &sites {
                let needle = format!("{name}(");
                for (i, _) in prod.match_indices(&needle) {
                    // 定义不是出口，调用才是。
                    if !prod[..i].trim_end().ends_with("fn") {
                        found.push((name.clone(), f.rel.clone()));
                    }
                }
            }
        }
        let n = found.len();
        let want = sites.len();
        equal(
            "明文出口的调用点（每个恰好一处，住定义面登记的那份）",
            found,
            sites,
        );
        assert_eq!(n, want, "有出口的调用点不止一处");
    });
}

/// 界面经通道 `apikey-key-set` 交那台的后端；monitor 只转不透明字节，一处都不许叫出那条写口。
#[test]
fn hx2_the_monitor_names_no_plaintext_key_on_the_way_to_the_backend() {
    let needles = [
        format!("write_apikey_{}_key", "credentials"),
        format!("\"apikey-{}-set\"", "key"),
    ];
    let named = |prod: &str| -> usize {
        prod.lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .map(|l| needles.iter().filter(|n| l.contains(n.as_str())).count())
            .sum()
    };
    rule(|repo| {
        let hits: Vec<&str> = repo
            .crate_tree("src/frontend/shell", &["rs"])
            .into_iter()
            .filter(|f| named(f.prod()) > 0)
            .map(|f| f.rel.as_str())
            .collect();
        // 正控：后端那条帧命令的宿主里认得出。
        let control = named(repo.file("src/backend/faces/accounts_face.rs").prod());
        none("monitor 生产段又能叫出明文 key 的写口", hits, control);
        equal(
            "前端发 apikey-key-set 的地方",
            repo.under(&["src/frontend/ui"], &["ts"])
                .into_iter()
                .filter(|f| !f.rel.starts_with("src/frontend/ui/generated/"))
                .flat_map(|f| {
                    let n = f.text().matches(needles[1].as_str()).count();
                    (n > 0).then(|| (f.rel.clone(), n))
                }),
            [("src/frontend/ui/apikey-reads.ts".to_string(), 1)],
        );
    });
}
