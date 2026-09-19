use super::*;
use std::collections::HashMap;

// TMUX_LS_FMT: name\tpath\tcommand\tattached\twindows\t@ccm_sid（6 列真 TAB 分隔）
fn raw(name: &str, cmd: &str, sid: &str) -> String {
    format!("{name}\t/p\t{cmd}\t0\t1\t{sid}")
}

#[test]
fn tmux_origin_for_sid_matches_ccm_sid_column_command_agnostic() {
    // @ccm_sid==目标 + command=bash（claude 已退）→ 命中 origin
    let by = HashMap::from([("A".to_string(), raw("cc-x", "bash", "target-full"))]);
    assert_eq!(
        tmux_origin_for_sid(&by, "target-full"),
        Some("A".to_string())
    );
    // command-agnostic：command 仍是 claude（帧陈旧）也命中——**变异锚点**：改看 command 则此断言红
    let by2 = HashMap::from([("A".to_string(), raw("cc-x", "claude", "target-full"))]);
    assert_eq!(
        tmux_origin_for_sid(&by2, "target-full"),
        Some("A".to_string())
    );
}

#[test]
fn tmux_origin_for_sid_only_ccm_sid_column_not_name_or_command() {
    // sid 只作 name/command 出现、@ccm_sid 列是别的 → 不命中（不误把 name/command 当 sid）
    let by = HashMap::from([("A".to_string(), raw("target", "target", "other-sid"))]);
    assert_eq!(tmux_origin_for_sid(&by, "target"), None);
}

#[test]
fn tmux_origin_for_sid_no_tmux_empty_missing() {
    let by = HashMap::from([
        ("A".to_string(), "NO_TMUX".to_string()),
        ("B".to_string(), String::new()),
    ]);
    assert_eq!(tmux_origin_for_sid(&by, "x"), None);
}

#[test]
fn tmux_origin_for_sid_multi_origin_routes() {
    let by = HashMap::from([
        ("A".to_string(), raw("cc-a", "bash", "sid-a")),
        ("B".to_string(), raw("cc-b", "bash", "sid-b")),
    ]);
    assert_eq!(tmux_origin_for_sid(&by, "sid-b"), Some("B".to_string()));
    assert_eq!(tmux_origin_for_sid(&by, "sid-z"), None);
}

#[test]
fn classify_removed_some_is_idle_none_is_archive() {
    // audit-fixes F03.2（D 审计②）：分流映射的变异锚点——把 Some/None 两臂写反则本测红。
    assert_eq!(
        classify_removed(Some("pi".to_string()), RemovalCause::Gone),
        RemovedDisposition::Idle {
            origin: "pi".to_string()
        }
    );
    assert_eq!(
        classify_removed(None, RemovalCause::Gone),
        RemovedDisposition::Archive
    );
}

/// ★ **F01b → P3 刀 0 → 刀 1：这张表的三代裁定，都留在这里。**
///
/// # 名字换过一次（原 `the_local_path_is_safe_only_because_local_sids_never_enter_the_tmux_cache`）
///
/// 那个名字现在**主动误导** —— 本地 sid **已经进表了**（刀 1 故意让它进的）。
/// 换名不是换判据：下面三条性质是原来那两条的**超集**。
///
/// # 三代的账
///
/// | 代 | 本地路径为什么安全 | 本条钉什么 |
/// |---|---|---|
/// | F01b | **巧合**：本地 sid 进不了这张表 ⇒ `find_tmux_origin_for_sid` 恒 `None` | 写入点只在远端 + 本地不产 `Superseded` |
/// | 刀 0 | 本地**自己判得出** `Superseded` | 锚点翻正：本地**确实**产 `Superseded` |
/// | 刀 1 | 本地进表了，靠的是刀 0 那条真保证 | 写入口**唯一** + 键的取值域只有 origin |
///
/// ★ 每一代都是**上一代的失败信息叫我来改的** ——
/// F01b 那条逐字写着「回 F01b 重新裁定」，这是它多写那三句话的全部价值。
///
/// # 为什么钉「唯一写入口」而不是「每处写入都按远端标签做键」
///
/// 后者是上一代的形态，它今天**必然红**（本机那处的键就不是远端标签）。
/// 但真正要防的东西没变：**这张表的键必须是 origin**，不许是裸 sid、不许是常量。
/// ⇒ 收成一个写入口 + 钉住那个口的调用方，比「逐处看键长什么样」更硬。
#[test]
fn the_tmux_cache_has_one_writer_and_only_origin_keys() {
    let src = guard_core::production_code(include_str!("../../src/bridge/src/ssh_source.rs"));
    // 抽取器自检：真的抽到了那张表。
    let touches = src.matches("tmux_raw_registry()").count();
    assert!(
        touches >= 3,
        "只抽到 {touches} 处 `tmux_raw_registry()` —— 抽取器坏了，本条会零命中地绿"
    );

    // ① 写口唯一 —— **写包括清**〔D 阶段补审 08-11 订正〕。
    //
    // 原版只数 `insert`。而 `.remove(` 同样是写者：远端断连处早就在清表，
    // 判据看不见它 ⇒ 它自陈要防的「**一边清一边不清**」当时**就是事实**
    //（远端清、本机从来不清 ⇒ `<local>` 那份原文永久陈旧 ⇒ 灰点）。
    // ⇒ 三种写法一起数，允许的家有两个：`record_tmux_raw`（写）与 `forget_tmux_raw`（清）。
    const WRITE_VERBS: &[&str] = &["insert", "remove", "clear"];
    let mut writes = 0usize;
    for seg in src.split("tmux_raw_registry()").skip(1) {
        let head = &seg[..seg.len().min(160)];
        if WRITE_VERBS.iter().any(|v| head.contains(v)) {
            writes += 1;
        }
    }
    assert_eq!(
        writes, 2,
        "`tmux_raw_registry` 的写口有 {writes} 处 —— 只许两处：\n\
             `record_tmux_raw`（写）与 `forget_tmux_raw`（清）。\n\
             多一处就意味着有人绕过这两个口各写各的：一边存原文一边存解析后的、一边清一边不清。"
    );
    // ⚠ 这一段第一版是**空转**的：写成 `src.find(…).unwrap_or(usize::MAX)` 再比大小 ——
    // 找不到时 `usize::MAX > at` 恒真 ⇒ 断言永远过。改成**按行切函数体**再看。
    let at = guard_core::find_pinned(&src, "pub(crate) fn record_tmux_raw(")
        .expect("唯一写入口 `record_tmux_raw` 不在了 —— 名字改了就来改本条");
    let body: String = src[at..]
        .lines()
        .skip(1)
        // ⚠ 收尾行**不写字面量右花括号** —— 本仓有判据用「花括号配平」剥测试段
        // （`ssh_source::strip_cfg_test`），源码里多一个孤立的右花括号会让它**提前闭合**（`b'…'` 的字符字面量也算，我第一次「修」时就还带着一个），
        // 测试段整段泄漏进「生产段」⇒ 别的判据当场误报（08-11 实测：单写者守卫红了）。
        .take_while(|l| *l != "\u{7d}")
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        body.len() > 20,
        "`record_tmux_raw` 切出来的函数体只有 {} 字节 —— 切错了，本条在空转",
        body.len()
    );
    guard_core::find_pinned(&body, "tmux_raw_registry()").unwrap_or_else(|e| {
        panic!(
            "写入口 `record_tmux_raw` 里没有恰好一处 `tmux_raw_registry()`（{e}）——\n\
                 那么上面数出来的写口在别的地方。"
        )
    });
    // ② 清除口也必须在它自己的家里。
    let forget_at = guard_core::find_pinned(&src, "pub(crate) fn forget_tmux_raw(")
        .expect("清除口 `forget_tmux_raw` 不在了 —— 没有它，断连/停机之后那份原文就是陈旧证据");
    let forget_body: String = src[forget_at..]
        .lines()
        .skip(1)
        .take_while(|l| *l != "\u{7d}")
        .collect::<Vec<_>>()
        .join("\n");
    guard_core::find_pinned(&forget_body, "tmux_raw_registry()")
        .expect("`forget_tmux_raw` 里没有恰好一处 `tmux_raw_registry()` —— 切错了或它不再清那张表");

    // ③ **两侧都要清** —— 远端断连清了、本机不清，就是补审逮到的那个真 bug。
    let lb = guard_core::production_code(include_str!(
        "../../src/bridge/src/backend/control/local_backend.rs"
    ));
    guard_core::find_pinned(&lb, "forget_tmux_raw(").unwrap_or_else(|e| {
        panic!(
            "本机那条路不清 `tmux_raw_registry`（{e}）。\n\
                 远端断连早就清了（Batch9-F28 那处）；本机若只摘入方向 client 不清这张表，\n\
                 停掉本机 daemon 之后 `<local>` 那份 `tmux ls` 原文**永久留着**，\n\
                 成了「tmux 还在」的陈旧证据 ⇒ `classify_removed(Some(_), Gone)` = `Idle`\n\
                 = 那个「永远消不掉、也 attach 不上的灰点」（F01b 那个 bug）。"
        )
    });

    // ② 键的取值域：调用方只许传远端标签或本机那个常量。
    let local_origin = crate::inbound_client::LOCAL_ORIGIN;
    for f in [
        guard_core::production_code(include_str!("../../src/bridge/src/ssh_source.rs")),
        guard_core::production_code(include_str!(
            "../../src/bridge/src/backend/control/local_backend.rs"
        )),
    ] {
        for seg in f.split("record_tmux_raw(").skip(1) {
            let head = &seg[..seg.len().min(120)];
            let ok = head.contains("host_label")
                || head.contains("origin_label")
                || head.contains("LOCAL_ORIGIN")
                || head.contains("origin: &str"); // 定义那一行
            assert!(
                ok,
                "有人拿一个既不是远端标签也不是 `{local_origin}` 的键写这张表：\n{head}\n\
                     ⇒ 键的取值域一破，`find_tmux_origin_for_sid` 就会按一个没人认识的 origin 返回值，\n\
                     而下游 `classify_removed` 拿它当「有 tmux 格子」用。"
            );
        }
    }

    // ③ 本地进表的**前置**必须还在：本地要判得出 `Superseded`。
    let sm = guard_core::production_code(include_str!("../../src/bridge/src/session_map.rs"));
    let verb = format!("RemovedSid::{}", "superseded");
    assert!(
        sm.contains(verb.as_str()),
        "`session_map.rs` 不产 `Superseded` 了，而本地 sid **已经在这张表里**。\n\
             ⇒ `/branch` 会走 `(Some(<local>), Gone)` = `Idle` ⇒ 「永远消不掉、也 attach 不上的灰点」回来。\n\
             这正是 F01b 当年那个 bug。刀 0 是刀 1 的硬前置，**不许只回退刀 0**。"
    );
}

/// ★ S0：`Superseded` 恒归档 —— **且不看 tmux 快照**。
///
/// 这条钉的是用户 2026-07-30 实测的那个 bug：`/branch` 之后原 tab 变成一个永远
/// 消不掉、也 attach 不上的灰点。四象限里出问题的就是 `(Some(origin), Superseded)`
/// 这一格 —— 快照说「tmux 还在」（对的，格子确实在），但那一格已经改挂新 sid 了。
#[test]
fn superseded_always_archives_even_when_tmux_snapshot_still_shows_the_sid() {
    // ★ 关键格：快照有它、但它是被顶替的 ⇒ 必须归档，不能灰点。
    assert_eq!(
        classify_removed(Some("pi".to_string()), RemovalCause::Superseded),
        RemovedDisposition::Archive
    );
    // 快照里没有时当然也归档（这格两个 cause 同答案，单独列出来是为了说明
    // Superseded 的判定**与快照无关**，不是碰巧和 Gone 一致）。
    assert_eq!(
        classify_removed(None, RemovalCause::Superseded),
        RemovedDisposition::Archive
    );
    // 反向对照：同一份「快照里有」的输入，Gone 仍然是灰点 —— 证明上面第一条不是
    // 因为把灰点分支整个删了才绿的（那种"修法"会把真正的 idle-tmux 功能砸掉）。
    assert_eq!(
        classify_removed(Some("pi".to_string()), RemovalCause::Gone),
        RemovedDisposition::Idle {
            origin: "pi".to_string()
        }
    );
}

/// ★ S0 跨语言双写点：monitor 认的字面量必须与 daemon 发的逐字一致。
///
/// 照本仓既有纪律（`TMUX_LS_FMT` / 观测取值那几条）：**读另一侧的源文件 + 锚定那一行**。
/// 漂了的表现是**静默失效**——monitor 认不出 `cause`、退回 `Gone`、灰点 bug 悄悄复活，
/// 而两侧各自的测试都是绿的。
#[test]
fn removal_cause_wire_literal_stays_in_sync() {
    let daemon_wire = include_str!("../../src/backend/wire.rs");
    // 反向自检：真读到了那个文件，且它确实是那个 enum 所在的文件。
    assert!(daemon_wire.len() > 2000, "没读到 daemon wire.rs");
    assert!(
        daemon_wire.contains("pub enum RemovalCause"),
        "daemon 侧 RemovalCause 不在预期文件里，双写点锚点已失效"
    );
    // daemon 用 `#[serde(rename_all = "snake_case")]` + 变体名 `Superseded`
    // ⇒ 线上就是 "superseded"。两个锚点都钉住，任一侧改名都红。
    assert!(
        daemon_wire.contains(r#"#[serde(rename_all = "snake_case")]"#),
        "daemon 侧 RemovalCause 的 serde 命名策略变了，线上字面量可能已不是 snake_case"
    );
    assert_eq!(REMOVAL_CAUSE_SUPERSEDED, "superseded");
    assert!(
        daemon_wire.contains("    Superseded,"),
        "daemon 侧变体名 Superseded 变了 ⇒ 线上字面量跟着变，monitor 会认不出"
    );
}

#[test]
fn idle_registry_mark_clear_snapshot() {
    mark_idle("f032_origX", "f032_sidX");
    assert!(snapshot_idle_for_origin("f032_origX").contains("f032_sidX"));
    assert!(snapshot_idle_by_origin().get("f032_origX").is_some());
    clear_idle("f032_sidX"); // 幂等 + 跨 origin
    assert!(!snapshot_idle_for_origin("f032_origX").contains("f032_sidX"));
    clear_idle("f032_sidX"); // 再清一次不报错
}

#[test]
fn remote_idle_single_writer_guard() {
    // §24bis 机器护栏（Phase G / full-audit Agent1「重要」结构化）：REMOTE_IDLE 唯一写者 =
    // lib.rs 的 remote-session-emitter。`mark_idle`/`clear_idle` 是 pub fn、全 crate 可达——
    // 单写者此前靠注释约定、`cargo check` 抓不住（同 §8「漏 manage 带病 5 版本」失败类）。本测把
    // 约定机器化：扫 src/bridge 生产源码（剥 cfg(test) 块 + 跳注释/定义行），断言对 mark_idle/
    // clear_idle 的**调用**只出现在 lib.rs。emitter 之外新增写者 → 本测红。
    fn strip_cfg_test(src: &str) -> String {
        // 括号配平剥掉 `#[cfg(test)]` 修饰的块（同 daemon readonly_guard 的证明过的做法）。
        let mut out = String::new();
        let mut rest = src;
        while let Some(pos) = rest.find("#[cfg(test)]") {
            out.push_str(&rest[..pos]);
            let after = &rest[pos..];
            match after.find('{') {
                Some(brace) => {
                    let b = after.as_bytes();
                    let (mut depth, mut end) = (0i32, brace);
                    while end < after.len() {
                        match b[end] {
                            b'{' => depth += 1,
                            b'}' => {
                                depth -= 1;
                                if depth == 0 {
                                    end += 1;
                                    break;
                                }
                            }
                            _ => {}
                        }
                        end += 1;
                    }
                    rest = &after[end..];
                }
                None => rest = &after["#[cfg(test)]".len()..],
            }
        }
        out.push_str(rest);
        out
    }
    fn is_comment(l: &str) -> bool {
        let t = l.trim_start();
        t.starts_with("//") || t.starts_with('*') || t.starts_with("/*")
    }
    let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut stack = vec![src_dir];
    let mut offenders: Vec<String> = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read src dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let fname = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            let prod = strip_cfg_test(&std::fs::read_to_string(&path).expect("read rs"));
            for line in prod.lines() {
                if is_comment(line) {
                    continue;
                }
                let t = line.trim_start();
                // 跳过定义行（mark_idle/clear_idle 定义在 ssh_source.rs）。
                if t.starts_with("pub fn mark_idle")
                    || t.starts_with("fn mark_idle")
                    || t.starts_with("pub fn clear_idle")
                    || t.starts_with("fn clear_idle")
                {
                    continue;
                }
                if (line.contains("mark_idle(") || line.contains("clear_idle("))
                    && fname != "lib.rs"
                {
                    offenders.push(format!("{fname}: {}", t.trim_end()));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "§24bis 违规：REMOTE_IDLE 写者 mark_idle/clear_idle 只准 lib.rs 的 remote-session-emitter 调用；\
             发现 emitter 之外的调用点（如确需，先想清楚是否破坏单写者不变量）：{offenders:?}"
    );
}

#[test]
fn reaper_tracked_unions_announced_and_idle() {
    let announced = ["live-a".to_string(), "live-b".to_string()].into_iter();
    let idle = std::collections::HashSet::from(["idle-c".to_string()]);
    let tracked = reaper_tracked(announced, &idle);
    assert!(tracked.contains("live-a"));
    assert!(tracked.contains("live-b"));
    // **变异锚点**：idle sid 必须在 tracked 里——否则 idle→archived 无产出者=灰灯卡死。
    // 若把实现改成「只 announced 不并 idle」，此断言红。
    assert!(tracked.contains("idle-c"));
    assert_eq!(tracked.len(), 3);
}

#[test]
fn reaper_tracked_empty_idle_is_just_announced() {
    let announced = ["live-a".to_string()].into_iter();
    let tracked = reaper_tracked(announced, &std::collections::HashSet::new());
    assert_eq!(
        tracked,
        std::collections::HashSet::from(["live-a".to_string()])
    );
}
