//! 控制面判据里引到上层的那几条（命令表 `stream::inbound::REGISTRY` · 进后端的那个词 `stream::entry::is_backend_word` ·
//! 帧面宿主交的账号清单成品 `faces::launch_account_face::list_at`）：控制面是独立的包（`backend-control`）、够不到上层
//! ⇒ 挂在主 crate（`tests/backend/cross/control.rs`）。其余控制面的判据住各自模块底下。

/// 命令表里那一条登记的码（排好）。
fn codes_of(op: &str) -> Vec<String> {
    let mut v: Vec<String> = crate::stream::inbound::REGISTRY
        .iter()
        .find(|s| s.name == op)
        .unwrap_or_else(|| panic!("后端登记表里没有 `{op}`"))
        .codes
        .iter()
        .map(|c| c.to_string())
        .collect();
    v.sort();
    v
}

/// 金样里那一条写的码（排好）。
fn golden_codes(g: &serde_json::Value) -> Vec<String> {
    let mut v: Vec<String> = g["codes"]
        .as_array()
        .expect("金样缺 codes")
        .iter()
        .map(|c| c.as_str().expect("码不是字符串").to_string())
        .collect();
    v.sort();
    v
}

// ───────── cc-bus ─────────

/// **跨语言金样**里 cc-bus 那几条的拒绝码 == 后端登记的（`tests/__fixtures__/cc-bus-read.golden.json` · `cc-bus-control.golden.json`）；
/// 成品本身由 `control/cc_bus_tests.rs` 那两条金样判据比。少一档多一档，界面那张「码 → 一句话」的表就漏一档或多一档。
#[test]
fn the_bus_goldens_codes_are_the_registered_ones() {
    let read: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/cc-bus-read.golden.json"))
            .expect("金样读不出来");
    assert_eq!(
        golden_codes(&read["inbox"]),
        codes_of("bus-inbox"),
        "金样 `bus-inbox` 的拒绝码与后端登记的不相等"
    );
    let control: serde_json::Value = serde_json::from_str(include_str!(
        "../../__fixtures__/cc-bus-control.golden.json"
    ))
    .expect("金样读不出来");
    assert_eq!(
        golden_codes(&control["bus-list"]),
        codes_of("bus-list"),
        "金样里 `bus-list` 的拒绝码与后端登记的不相等"
    );
}

/// 总线的帧命令：名单 · 读收件箱 · 发信 · 两阶段读两跳 · 未读数。界面不发信（人要说话就在会话里说）。
#[test]
fn the_bus_commands_are_the_inbox_and_its_ops() {
    let mut got: Vec<&str> = crate::stream::inbound::REGISTRY
        .iter()
        .map(|s| s.name)
        .filter(|n| n.starts_with("bus-"))
        .collect();
    got.sort_unstable();
    assert_eq!(
        got,
        [
            "bus-commit",
            "bus-inbox",
            "bus-list",
            "bus-peek",
            "bus-send",
            "bus-unread"
        ]
    );
}

// ───────── 终端管理 L1 ─────────

/// 终端管理那三条的**跨语言金样**（`tests/__fixtures__/terminals.golden.json`）里写的码 == 后端登记的（照登记的先后）；
/// 成品本身由 `control/terminals_tests.rs::the_products_match_the_golden` 比。命令级码与线上的原因码一个写法：不带短横。
#[test]
fn the_terminal_goldens_codes_are_the_registered_ones() {
    let in_order = |name: &str| -> Vec<String> {
        crate::stream::inbound::REGISTRY
            .iter()
            .find(|s| s.name == name)
            .unwrap_or_else(|| panic!("注册表里没有 {name}"))
            .codes
            .iter()
            .map(|c| c.to_string())
            .collect()
    };
    let g: serde_json::Value =
        serde_json::from_str(include_str!("../../__fixtures__/terminals.golden.json"))
            .expect("金样读不懂");
    for name in ["terminals-list", "terminal-preview", "terminal-input"] {
        assert_eq!(
            serde_json::to_value(in_order(name)).unwrap(),
            g[name]["codes"],
            "{name} 的码与登记的不相等"
        );
    }
    for code in in_order("terminal-input")
        .iter()
        .chain(in_order("terminals-list").iter())
    {
        assert!(!code.contains('-'), "命令级码 `{code}` 带短横");
    }
}

// ════════════════════════════════════════════════════════════════════════
// 〔C4e 问 2〕过门的命令，登记表里的码必须盖住门会回的码
// ════════════════════════════════════════════════════════════════════════

/// 一段生产源码里「错误元组 / 错误出口」的码：`(` 之后（隔空白）紧跟一个蛇形字面量、再跟 `,`。
fn tl2_error_codes_in(src: &str) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    for (i, _) in src.match_indices('(') {
        let after = src[i + 1..].trim_start();
        let Some(rest) = after.strip_prefix('"') else {
            continue;
        };
        let Some(end) = rest.find('"') else { continue };
        let lit = &rest[..end];
        if rest[end + 1..].trim_start().starts_with(',')
            && lit.contains('_')
            && lit.chars().all(|c| c.is_ascii_lowercase() || c == '_')
        {
            out.insert(lit.to_string());
        }
    }
    out
}

/// `gate.rs` 生产段里 `fn <name>(` 的函数体（到顶层 `\n}` 为止）。
fn tl2_fn_body<'a>(src: &'a str, name: &str) -> &'a str {
    let at = src
        .find(&format!("fn {name}("))
        .unwrap_or_else(|| panic!("gate.rs 里找不到 `fn {name}(`"));
    let tail = &src[at..];
    &tail[..tail.find("\n}").expect("函数没收尾")]
}

/// 过门的命令里，把门的某个码当场翻成自己的那几格：`(命令, 门的码, 翻成什么 · 为什么)`。翻的那一处在实现里（变异：删掉那一处翻码，
/// 那个码就会原样上线、而 `codes` 没列它 —— 本条照登记放过，那一格自己的行为判据钉着翻码）。
const GATE_CODES_RECAST: &[(&str, &str, &str)] = &[(
    "terminal-end",
    "no_tmux",
    "`kill.rs::end_terminal`：只有名单里有 tmux 那一行才过三道门，过门那一刻 tmux 起不来 ⇒ `kill_failed`；\
     不声明 `no_tmux`：没 tmux 的机器上托管那一行照结束（`kill_tests` 钉着）",
)];

/// 要求住址：`INVARIANTS §34`（tmux 破坏性 / 半破坏性命令三道门）· `INVARIANTS §42`（线上契约文档与代码不许漂）。
///
/// C4e 交上来的缺口逐字：「`launch` 那条后端登记表的 `codes` **没列 `wrong_owner`**，而 `control/launch.rs::run` 经 `gate::admit` 真会回它」。
/// 病根是「码表手写、没人从门那一侧核」。本条从**门的源码**派生：`admit` / `admit_destructive` 各自会回哪些码（连它们调的 `probe`），
/// 再从 `inbound::REGISTRY` 的每一格找出「实现模块调了哪道门」，那一格的 `codes` 必须 ⊇ 那道门的码。
/// 异源：码集合读 `gate.rs`，调用关系读各模块与 `stream/inbound/` 的源码，被比的是运行期的 `REGISTRY`。
#[test]
fn every_command_that_passes_the_gate_lists_the_gates_codes() {
    let gate = guard_core::production_code(include_str!("../../../src/backend/control/gate.rs"));
    let probe = tl2_error_codes_in(tl2_fn_body(&gate, "probe"));
    let codes_of = |f: &str| -> std::collections::BTreeSet<String> {
        let body = tl2_fn_body(&gate, f);
        let mut c = tl2_error_codes_in(body);
        if body.contains("probe(") {
            c.extend(probe.iter().cloned());
        }
        c
    };
    let doors = [
        ("admit", codes_of("admit")),
        ("admit_destructive", codes_of("admit_destructive")),
    ];
    assert!(
        doors[0].1.contains("wrong_owner") && doors[1].1.contains("too_many_windows"),
        "门的码读错了：{doors:?}"
    );
    // 窗口门只给破坏性动作：往一个多窗口会话里打字不删任何东西，非破坏性的 `admit` 不许回窗口数那一码。
    assert!(
        !doors[0].1.contains("too_many_windows"),
        "非破坏性的 `admit` 会回 `too_many_windows` 了 —— 送键会被多窗口误拒：{doors:?}"
    );

    let families = guard_core::addr::backend_registry_sources();
    let root = guard_core::addr::backend_src_root();
    let mut checked = Vec::new();
    // typed 那几行（`typed::<档>::<crate::control::<模块>::<类型>>(…)`）：命令名从那份文件里 `impl Command for <类型>` 的 `const NAME` 读。
    let mut typed_cells: Vec<(String, String)> = Vec::new();
    for (_, prod) in &families {
        for (at, _) in prod.match_indices("super::typed::") {
            let rest = &prod[at..];
            let Some(lt) = rest.find("::<") else { continue };
            let after = &rest[lt + 3..];
            let path = &after[..after.find('>').unwrap_or(after.len())];
            let Some(tail) = path.strip_prefix("crate::control::") else {
                continue;
            };
            let segs: Vec<&str> = tail.split("::").collect();
            let (Some(module), Some(ty)) = (segs.first(), segs.last()) else {
                continue;
            };
            let Ok(src) =
                std::fs::read_to_string(root.join("control").join(format!("{module}.rs")))
            else {
                continue;
            };
            let Some(name) = guard_core::typed_command_name(&src, ty) else {
                continue;
            };
            typed_cells.push((name, format!("crate::control::{module}::{ty}")));
        }
    }
    for spec in crate::stream::inbound::REGISTRY {
        let anchor = format!("name: \"{}\",", spec.name);
        let typed_cell = typed_cells
            .iter()
            .find(|(n, _)| n == spec.name)
            .map(|(_, c)| c.as_str());
        let cell = match families
            .iter()
            .find_map(|(_, prod)| prod.find(&anchor).map(|at| (prod, at)))
        {
            Some((family, at)) => {
                let cell = &family[at..];
                &cell[..cell.find("\n    },").unwrap_or(cell.len())]
            }
            None => match typed_cell {
                Some(c) => c,
                None => continue,
            },
        };
        for (i, _) in cell.match_indices("crate::control::") {
            let module: String = cell[i + "crate::control::".len()..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            let Ok(src) =
                std::fs::read_to_string(root.join("control").join(format!("{module}.rs")))
            else {
                continue;
            };
            let prod = guard_core::production_code(&src);
            for (door, want) in &doors {
                if !prod.contains(&format!("gate::{door}(")) {
                    continue;
                }
                let mut have: std::collections::BTreeSet<String> =
                    spec.codes.iter().map(|s| s.to_string()).collect();
                // 照实现说过的翻码：那一格把门的这几个码换成了自己的（翻成什么、为什么住 `GATE_CODES_RECAST`）。
                for (cmd, code, _) in GATE_CODES_RECAST {
                    if *cmd == spec.name {
                        have.insert((*code).to_string());
                    }
                }
                let missing: Vec<_> = want.difference(&have).collect();
                assert!(
                    missing.is_empty(),
                    "`{}` 的实现（control/{module}.rs）过 `gate::{door}`，那道门会回 {missing:?}，而 `inbound::REGISTRY` 那一格的 `codes` 没列 —— \
                     界面按登记的码逐码说人话，漏一个就是那一句永远说不出来（C4e 问 2 那一形）",
                    spec.name
                );
                checked.push((spec.name, *door));
            }
        }
    }
    checked.sort();
    checked.dedup();
    // 正控 ＋ 人群：今天登记表里直接过门的恰好是 terminal-end（admit_destructive，tmux 那一形）；`admit` 由 `session-open` · 换号重启经
    //   `launch::run` 在里头过（码各自翻成自己的），`terminal-input` 那一路过的是 `admit_with`。多了少了都要回来看一眼。
    assert_eq!(
        checked,
        vec![("terminal-end", "admit_destructive")],
        "过门的命令变了"
    );
}

// ───────── ccm ─────────

mod ccm {
    use crate::ccm_line::argv::{self, Parsed};
    use crate::ccm_line::SUBCOMMAND_WORD;
    use crate::control::ccm::plan::Env;
    use crate::control::ccm::*;

    // 回包写成线上那一份、入参经 typed 那一处收（同分派）：判据照线上形比（盖住 `ccm::*` 里的同名口）。
    fn answer_print(
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, crate::common::said::Said> {
        let req: CcmPrintReq = crate::wire::typed::take(args)
            .map_err(|f| crate::common::said::Said::new("bad_args", f.said))?;
        crate::control::ccm::answer_print(&req, crate::faces::launch_account_face::list_at)
            .map(|r| serde_json::to_value(r).unwrap())
    }

    /// 〔「路由不看 argv0」〕要求：「没有 `--` ⇒ 整行原样交 claude」＋「route 里没有 base ≠ ccm 那一支」。
    /// 分流先看被叫成什么、再看参数：叫 `ccm` / 后端本名 ⇒ 零参数是「起一个 claude」，打头的 `--` 紧跟后端词才进后端；
    /// 被叫成别的名字（`~/.cc-monitor/bin/<名>` 那条链接）⇒ 等同 `ccm @<名> …`，参数一个都不进后端。
    /// 回环：pane 里把自己再叫一次（[`self_invocation`] ＋ 内层参数）叫得回 ccm 本身、参数一个不多一个不少、配置不叠第二遍。
    #[test]
    fn routing_reads_the_name_it_was_called_by_then_the_argv() {
        let v = |xs: &[&str]| xs.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let route =
            |a0: &str, args: &[String]| route(a0, args, crate::stream::entry::is_backend_word);
        for me in [
            "ccm",
            "/x/ccm",
            "C:\\x\\ccm.exe",
            "/x/cc-monitor-backend",
            "cc-monitor-backend-4.1.2",
        ] {
            assert_eq!(
                route(me, &[]),
                Entry::Ccm(vec![]),
                "零参数不再是流模式（{me}）"
            );
            assert_eq!(
                route(me, &v(&["--stream"])),
                Entry::Ccm(v(&["--stream"])),
                "没有打头的 `--` 就交 claude"
            );
            assert_eq!(
                route(me, &v(&["--", "--stream", "--tail-only"])),
                Entry::Backend(v(&["--stream", "--tail-only"]))
            );
            assert_eq!(
                route(me, &v(&["--", "--ping"])),
                Entry::Backend(v(&["--ping"]))
            );
            assert_eq!(
                route(me, &v(&["--", "--ccm-tmux"])),
                Entry::Ccm(v(&["--", "--ccm-tmux"]))
            );
        }
        assert_eq!(
            route(
                "/home/u/.cc-monitor/bin/teamcct",
                &v(&["你好", "--", "--ccm-print"])
            ),
            Entry::Ccm(v(&["@teamcct", "你好", "--", "--ccm-print"]))
        );
        assert_eq!(
            route("teamcct", &v(&["--", "--ping"])),
            Entry::Ccm(v(&["@teamcct", "--", "--ping"])),
            "被叫成配置名的那一趟不进后端"
        );
        let inner = v(&["--tail-only", "--", "--cwd", "/p", "--ccm-agent", "claude"]);
        for (a0, me) in [
            ("/x/ccm", "/x/ccm"),
            ("/x/cc-monitor-backend", "/x/cc-monitor-backend"),
            ("/x/teamcct", "/x/ccm"),
            ("teamcct", "ccm"),
        ] {
            let mut again = self_invocation(&v(&[a0, "--", "--ccm-tmux=n"]));
            assert_eq!(again, v(&[me]), "「怎么叫我」（{a0}）");
            again.extend(inner.iter().cloned());
            assert_eq!(
                match route(&again[0], &again[1..]) {
                    Entry::Ccm(v) => Some(v),
                    Entry::Backend(_) => None,
                },
                Some(inner.clone()),
                "从 {a0} 进来的在 pane 里叫不回 ccm"
            );
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    // 帧命令 `ccm-print`：别名预览
    // 要求：「`ccm --print` 不跑、吐出等价的一行 shell ⇒ 生成器旁边显示**这条别名实际会执行什么**，
    // 是真验证，不是前端拼串」。
    // ═══════════════════════════════════════════════════════════════════════

    /// **C1：预览与 `ccm --print` 是同一个计划函数。**
    ///
    /// 源码那一半：`run` 与 [`answer_print`] 都经 [`plan_of`]（`plan::build(` 全文件恰好一处由
    /// `the_name_avoidance_has_exactly_one_source_and_the_plan_settles_it` 钉着），预览那一处交的是预览环境、不继承账号。
    /// 行为那一半（异源）：同一组参数，`answer_print` 的 `line` == 手搭一份「家目录里的新终端」环境、
    /// 直接走 `argv::parse_layered` → `plan::build` → `plan::render` 的产物（直路 · 容器路 · 显式不带账号三形）。
    #[test]
    fn the_alias_preview_is_the_same_plan_as_ccm_print() {
        let prod =
            guard_core::production_code(include_str!("../../../src/backend/control/ccm/mod.rs"));
        for pin in [
            "plan_of(&o, env, true, accounts)",
            "plan_of(&o, Env::for_preview(), false, accounts)",
        ] {
            guard_core::find_pinned(&prod, pin).unwrap_or_else(|e| {
                panic!("`{pin}` 不是恰好一处（{e}）—— 预览与 `--print` 不再走同一个计划函数")
            });
        }
        let home = std::env::var("HOME").unwrap_or_default();
        for args in [
            // 别名的预置参数就是一条 argv（`<交给 claude 的…> -- <ccm 的…>`）。
            // 直路的启动器在计划里照这台 `PATH` 找 ⇒ 给一个绝对路径，判据不随这台装没装 claude 漂移。
            vec![
                "--",
                "--cwd",
                "/p",
                "--ccm-agent",
                "claude",
                "--launcher",
                "/bin/sh",
            ],
            vec!["--", "--ccm-tmux=w5alias-preview-probe", "--cwd", "/p"],
            vec![
                "--model",
                "m",
                "--",
                "--base",
                "--cwd",
                "/q",
                "--launcher",
                "/bin/sh",
            ],
        ] {
            let got = answer_print(&serde_json::json!({ "args": args }))
                .unwrap_or_else(|e| panic!("{args:?} 预览被拒：{e:?}"));
            let a: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            let o = match argv::test_support::parse(&a).expect("该解析得动") {
                Parsed::Opts(o) => o,
                other => panic!("{other:?}"),
            };
            let pick =
                |k: &str, d: String| std::env::var(k).ok().filter(|v| !v.is_empty()).unwrap_or(d);
            let e = Env {
                home: home.clone(),
                pwd: home.clone(),
                accts_manifest: plan::accts_manifest_under(&home),
                ccm_env: pick("CCM_ENV", argv::Defaults::ENV.to_string()),
                account_env: crate::agents::account_env_of(&o.agent)
                    .unwrap_or_default()
                    .to_string(),
                base_url_env: crate::agents::base_url_env_of(&o.agent)
                    .unwrap_or_default()
                    .to_string(),
                self_argv: vec!["ccm".into()],
                ..Default::default()
            };
            // 账号清单成品照这台机器的真值（预览要的就是这一点：「这台机器上敲这条别名」）—— 与真跑同一个入口交的那一份。
            let table = || crate::faces::launch_account_face::list_at(&e.home, &o.agent);
            let mut plan = plan::build(&o, &e, &table, None).expect("该算得出计划");
            // 直路的启动器在计划里换成要起的那个文件，同真跑那一处（找法只一处）。
            if let plan::Plan::Direct(d) = &mut plan {
                launcher_resolved(d, &|p| crate::platform::shell::agent_launch(p, None))
                    .expect("启动器找得到");
            }
            assert_eq!(
                got["line"].as_str().expect("line"),
                plan::render(&plan),
                "{args:?}：预览与同一语境下的 `--print` 不是同一行"
            );
        }
    }

    /// **C2：预览的语境逐格写死**（「从这台机器家目录里的一个新终端敲这条别名」）。各格一刀：
    /// 叫的是 `ccm` · cwd = home · 不在 tmux 里 · 没有继承来的中转地址 / 启动号 / 账号目录。
    #[test]
    fn the_alias_preview_speaks_for_a_fresh_terminal_at_home() {
        let e = Env::for_preview();
        assert_eq!(
            e.self_argv,
            vec![SUBCOMMAND_WORD.to_string()],
            "别名叫的是 `ccm`"
        );
        assert_eq!(e.pwd, e.home, "不给 --cwd 的别名在家目录里敲");
        assert!(e.tmux.is_none(), "新终端不在 tmux 里");
        assert!(e.inherited_config_dir.is_none(), "账号目录变量不继承");
        assert!(
            e.inherited_base_url.is_none(),
            "常驻后端进程身上的中转地址不是那个终端的"
        );
        // 行为：不给 --cwd ⇒ 落在家目录；容器路内层叫回的是 `ccm`。
        let line = |args: &[&str]| -> String {
            answer_print(&serde_json::json!({ "args": args })).expect("该答得出")["line"]
                .as_str()
                .expect("line")
                .to_string()
        };
        let home = std::env::var("HOME").unwrap_or_default();
        assert!(
            line(&["--", "--ccm-agent", "claude", "--launcher", "/bin/sh"])
                .contains(&plan::qarg(&home)),
            "不给 --cwd 却没落在家目录"
        );
        let boxed = line(&["--", "--ccm-tmux=w5alias-preview-probe", "--cwd", "/p"]);
        assert!(
            boxed.contains("'ccm' '--' '--cwd'"),
            "容器路内层没有叫回 `ccm`：{boxed}"
        );
    }

    /// `ccm-print` 的两种拒：形状不对 ⇒ `bad_args`；ccm 自己拒 / 不起会话的那一形 ⇒ `refused`，原话带回。
    #[test]
    fn the_alias_preview_refuses_in_the_words_of_ccm() {
        let code = |v: serde_json::Value| match answer_print(&v) {
            Ok(_) => "ok".to_string(),
            Err(e) => e.code_or_fallback().to_string(),
        };
        assert_eq!(code(serde_json::json!({})), "bad_args");
        assert_eq!(code(serde_json::json!({ "args": [1] })), "bad_args");
        assert_eq!(
            code(serde_json::json!({ "args": vec!["x"; PRINT_MAX_WORDS + 1] })),
            "bad_args"
        );
        assert_eq!(
            code(serde_json::json!({ "args": ["--", "--ccm-help"] })),
            "refused"
        );
        // 未知旗标交给 claude、不拒 ⇒ 拿一条 ccm 自己的组合规则当「拒」的样本。
        let e = answer_print(&serde_json::json!({ "args": ["--", "--detach"] }))
            .expect_err("--detach 不带 --tmux 该被拒");
        assert_eq!(e.code.as_deref(), Some("refused"));
        let said = e.said;
        let want = match argv::test_support::parse(&["--".to_string(), "--detach".to_string()]) {
            Err(argv::Die(m)) => m,
            other => panic!("{other:?}"),
        };
        assert_eq!(said, want, "拒的那句不是 ccm 自己的原话");
        assert_eq!(
            code(serde_json::json!({ "args": ["--", "--cwd", "/p", "--launcher", "/bin/sh"] })),
            "ok"
        );
        // 右边认不得 ⇒ ccm 的原话拒（不猜）。
        assert_eq!(
            code(serde_json::json!({ "args": ["--", "--model", "m"] })),
            "refused"
        );
    }
}

// ───────── 落点：传上去的那一份 ─────────

#[test]
fn p4_the_staged_name_routes_to_the_backend() {
    let name = ssh_dial::sftp::staged_name(deploy::STAGED_STEM);
    let argv0 = format!("/home/x/{}/{name}", relay_route_core::STAGING_DIR_REL);
    let args: Vec<String> = vec!["--".into(), "--place-self".into()];
    assert_eq!(
        crate::control::ccm::route(&argv0, &args, crate::stream::entry::is_backend_word),
        crate::control::ccm::Entry::Backend(vec!["--place-self".into()])
    );
}
