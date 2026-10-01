//! 驾驶舱读面（名册 · 收件箱）迁到界面经通道直接问那台后端 —— monitor 这边那一整套 shell 读与它的判据（坏行解析 · 自述头 ·
//! 本机 `bash` 解析 · 超时不留孤儿 · 命令模板一处构造）随生产代码一起删了〔散文墓碑〕；坏行契约的判据住后端 `tests/backend/control/cc_bus_tests.rs`。
//! 这里只剩：id 规则的再导出（`INVARIANTS §47`）· 唯一 quote 的往返性质 · monitor 零写面 · 界面只经一处说。

use super::*;

// ===== id 校验：`--help` 这条是真实盘面数据，不是构造的边角 =====
#[test]
fn rejects_leading_dash_ids_from_real_disk() {
    // 盘上真实存在 `~/.cc-bus/inbox/--help.jsonl`（188 字节）与 `282.jsonl`。
    assert!(
        !is_valid_bus_id("--help"),
        "--help 必须被拒（会被当成 flag）"
    );
    assert!(!is_valid_bus_id("-x"));
    assert!(!is_valid_bus_id(""));
    // 纯数字是合法的（`282` 虽然是误用产生的，但它本身不构成注入面）
    assert!(is_valid_bus_id("282"));
}

#[test]
fn accepts_real_ids() {
    for id in ["proj_cc", "cc-9d66c46d", "KVM_cc", "EasyTier_cc", "x_y_cc"] {
        assert!(is_valid_bus_id(id), "{id} 应合法");
    }
}

#[test]
fn rejects_shell_metachars_and_control() {
    for bad in [
        "a b", "a;rm", "a$(x)", "a\nb", "a\tb", "a/b", "a.b", "a:b", "a*",
    ] {
        assert!(!is_valid_bus_id(bad), "{bad:?} 应被拒");
    }
}

// **断言方式的两个教训，都写在这里免得再犯**：
//  ① 第一版我写 `assert!(!cmd.contains("; rm -rf ~;"))` —— 错的。正确逃逸的结果本来
//     就**包含**那个危险子串，只是它落在单引号内、完全惰性。断言"危险子串不出现"是在
//     检查一个错误的性质。真正要证的是「这一整坨仍是**一个** shell 词，内容逐字等于
//     原文」→ 用**往返还原**证。
//  ② 第二版我把断言打在 `is_valid_bus_id` 这个谓词上，结果把 `cc_bus_send` 里那句
//     校验整个删掉，测试**照样全绿**（失效模式③：门禁太窄）。所以现在一律打在
//     `build_*_cmd` 这些**真正构造命令的函数**上。

/// POSIX 单引号形态的最小逆运算：把 `shell_quote` 的产物还原回原文。
/// 只认它产出的那一种形状；遇到**裸单引号**返回 None——那正是"能逃出去"的标志。
fn unquote_posix(q: &str) -> Option<String> {
    let b = q.as_bytes();
    if b.len() < 2 || b[0] != b'\'' || b[b.len() - 1] != b'\'' {
        return None;
    }
    let esc = "'\\''"; // 单引号 反斜杠 单引号 单引号
    let mut out = String::new();
    let mut rest = &q[1..q.len() - 1];
    loop {
        match rest.find('\'') {
            None => {
                out.push_str(rest);
                return Some(out);
            }
            Some(i) => {
                out.push_str(&rest[..i]);
                if !rest[i..].starts_with(esc) {
                    return None;
                }
                out.push('\'');
                rest = &rest[i + esc.len()..];
            }
        }
    }
}

#[test]
fn unquote_helper_itself_rejects_unescaped_quotes() {
    // 守住这个测试助手本身：它若把裸引号也"还原"了，下面几条就全成了摆设
    assert_eq!(unquote_posix("'a'b'"), None);
    assert_eq!(unquote_posix("noquotes"), None);
    assert_eq!(unquote_posix("'ok'").as_deref(), Some("ok"));
}

#[test]
fn quote_roundtrip_is_the_real_property() {
    for evil in [
        "hi'; rm -rf ~; echo '",
        "$(id)",
        "`whoami`",
        "a\nb",
        "中文 带空格",
        "'",
        "''",
    ] {
        let q = shell_quote_core::posix_quote(evil);
        assert_eq!(
            unquote_posix(&q).as_deref(),
            Some(evil),
            "逃逸后必须能逐字还原（说明它仍是一个完整的 shell 词）: {q}"
        );
    }
}

// ════════════════════════════════════════════════════════════════════════
// cc-bus 写面迁到界面之后：monitor 里一条路都不剩 · 界面只经一处说 · id 规则两份对拍
// ════════════════════════════════════════════════════════════════════════

/// ★★**monitor 里 cc-bus 写面一条路都不剩**（零命中 ＋ 正控）。
///
/// 守的要求：「迁到通道之后，业务解释是不是**只有一个家**」—— 发消息 / 收掉 / 派生 / 广播的解释
/// 今天只住 `src/frontend/ui/cc-bus-control.ts`（广播的挑人住后端）；monitor 里再长出一条拼 `cc-send` / `cc-kill` / `cc-spawn` /
/// `cc-broadcast` shell 串的路，就是 `K-R98` / `K-R112` / BS1b 一条条删掉的那几条 SSH 路回来了（它们纯按名字、没有身份核对）。
/// 帧命令名那一格由 `frame_query_tests` 的 `CHANNELED_ELSEWHERE`（monitor 生产段零字面量）管，本条管 shell 串那几种形态。
/// 正控：同一识别器在后端 `control/cc_bus.rs` 的生产段上认得出它真在转调的 `cc-send`。
#[test]
fn the_monitor_has_no_cc_bus_write_path_any_more() {
    let root = crate::guard_support::repo_root();
    // 现拼，免得本文件自己被别的扫描收进人群。
    let verbs: Vec<String> = ["send", "kill", "spawn", "broadcast"]
        .iter()
        .map(|v| format!("cc-{v}"))
        .collect();
    // 唯一豁免：部署器 `cc_bus_deploy.rs` 把这几份脚本**装到盘上**（字节表 + 装之前核 ccm 够不够新的那几句话），
    // 不调用它们。判的是**两向相等**：生产段里提到这几个名字的 monitor 文件集合 == {部署器}。
    // 别的文件一出现就是多一个元素；部署器不再提（比如字节表挪走了）就是少一个 —— 豁免过期，也红。
    let deployer = "cc_bus_deploy.rs";
    let mut who = std::collections::BTreeSet::new();
    let mut files = 0usize;
    for (p, one_file) in
        guard_core::scan_tree_excluding(&root.join("src/frontend/shell/src"), &["rs"], &[])
    {
        files += 1;
        let prod = guard_core::strip_comment_lines(&guard_core::production_code(&one_file));
        for v in &verbs {
            if guard_core::contains_word(&prod, v) {
                who.insert(format!(
                    "{} ({v})",
                    p.file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default()
                ));
            }
        }
    }
    assert!(files > 100, "只扫到 {files} 份 monitor 源码 —— 遍历坏了");
    // 部署器那份字节表随装 cc-bus 进了本机后端（`src/backend/assets/cc_bus_install.rs`）⇒ 豁免过期、人群清零。
    let _ = deployer;
    let only_deployer: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    assert_eq!(
        who, only_deployer,
        "monitor 生产段里提到 cc-bus 写面脚本名的文件 ≠ {{部署器 × 四个名字}} —— 多出来的那个就是在 monitor 里长回的一条\
         拼 shell 串的路（它只许经后端的 `bus-*` 原语，界面经 `src/frontend/ui/cc-bus-control.ts` 一处说）；少了就是豁免过期"
    );
    let backend = guard_core::production_code(
        &std::fs::read_to_string(root.join("src/backend/control/cc_bus.rs"))
            .expect("读后端 cc_bus.rs"),
    );
    assert!(
        guard_core::contains_word(&backend, &verbs[0]),
        "正控失败：后端 `control/cc_bus.rs` 的生产段里认不出 `cc-send` —— 识别器瞎了，上面的零命中不可信"
    );
}

/// ★★**界面说 cc-bus 那五条只经一处**：`chan.call(origin, "bus-…"` 只住 `src/frontend/ui/cc-bus-control.ts`，
/// 各恰好一处。
///
/// 守的要求：同上一条（业务解释只有一个家）—— 先核 id / 正文 / 派生形状、按形状收、逐态说人话，这几件只写在那一份里；
/// 别处直接 `chan.call(…, "bus-send", …)` 就是绕过它们的第二条路。两向相等：出现这些字面量的文件集合 == `{src/cc-bus-control.ts}`，
/// 那一份里处数恒等（五条各 1）。
#[test]
fn the_front_end_speaks_the_bus_ops_only_through_one_module() {
    let root = crate::guard_support::repo_root();
    let ops = [
        "bus-list",
        "bus-send",
        "bus-kill",
        "bus-spawn",
        "bus-broadcast",
        // 驾驶舱读面那两条。
        "bus-state",
        "bus-inbox",
    ];
    let mut homes: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        Default::default();
    let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
    let mut scanned = 0usize;
    for (p, text) in guard_core::scan_tree_excluding(&root.join("src"), &["ts"], &[]) {
        scanned += 1;
        let rel = p
            .strip_prefix(&root)
            .unwrap_or(&p)
            .to_string_lossy()
            .replace('\\', "/");
        let prod = guard_core::strip_comment_lines(&text);
        for op in ops {
            let needle = format!("chan.call(origin, \"{op}\"");
            let c = prod.matches(needle.as_str()).count();
            if c > 0 {
                homes.entry(op.to_string()).or_default().insert(rel.clone());
                *counts.entry(op.to_string()).or_default() += c;
            }
        }
    }
    assert!(scanned > 100, "只扫到 {scanned} 份前端源码 —— 遍历坏了");
    for op in ops {
        assert_eq!(
            homes.get(op).cloned().unwrap_or_default().into_iter().collect::<Vec<_>>(),
            vec!["src/frontend/ui/cc-bus-control.ts".to_string()],
            "`{op}` 的 `chan.call` 出现在 `src/frontend/ui/cc-bus-control.ts` 之外（或那一份里没有了）—— 界面说它的家不止一个"
        );
        assert_eq!(
            counts.get(op).copied(),
            Some(1),
            "`{op}` 在那一份里不是恰好一处"
        );
    }
}

/// ★**agent id 的规则对金样**：monitor [`is_valid_bus_id`]（读收件箱那一条）读跨语言金样
/// `cc-bus-control.golden.json` 的 `ids`。
///
/// 界面那一份（`src/frontend/ui/cc-bus-control.ts` 里的 TS 副本）删了，实现搬进共享 crate（`shell_quote_core::bus_id_ok`），
/// 这里的 [`is_valid_bus_id`] 是它的再导出；后端 `bus-*` 入口用同一个函数判同一份 `ids`
/// （`tests/backend/control/cc_bus_tests.rs::bus_ids_are_judged_here_before_they_reach_cc_bus`）。本条留着：读收件箱这一侧对金样。
#[test]
fn the_bus_id_rule_agrees_with_the_shared_samples() {
    let g: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            crate::guard_support::repo_root().join("tests/__fixtures__/cc-bus-control.golden.json"),
        )
        .expect("读金样"),
    )
    .expect("金样不是 JSON");
    let take = |k: &str| -> Vec<String> {
        g["ids"][k]
            .as_array()
            .unwrap_or_else(|| panic!("金样缺 ids.{k}"))
            .iter()
            .map(|v| v.as_str().expect("id 不是字符串").to_string())
            .collect()
    };
    let (ok, bad) = (take("ok"), take("bad"));
    assert!(
        !ok.is_empty() && !bad.is_empty(),
        "金样的 ids 空了 —— 下面是空转"
    );
    for id in &ok {
        assert!(is_valid_bus_id(id), "{id:?} 该放行");
    }
    for id in &bad {
        assert!(!is_valid_bus_id(id), "{id:?} 该拒");
    }
}
