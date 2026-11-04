use super::*;

/// P2s-Y1（acceptor: 机检）：**三个口各只有一条命令，且都吃 `origin`**。
///
/// 防的是「本机一套名字、远端另一套名字」—— 那样两侧就长出两套语义，
/// 正是 `C1` 排除掉的做法（也是 #58 门⑤ 要防的形态）。
#[test]
fn the_three_ports_are_one_command_each_and_all_take_origin() {
    let src = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/backend_control.rs"
    ));
    const PORTS: &[&str] = &["backend_status", "backend_start", "backend_stop"];
    for p in PORTS {
        // `backend_stop` 本机那一支要等（SIGTERM → ≤ 约 10 秒 → 强杀）⇒ 它是 `async`（同步命令跑在主线程上）。
        // 〔INVARIANTS §10〕`backend_start` 本机那一支也要等（起进程 · 连口 · 读 hello · 等绑上口）⇒ 同样 `async`。
        // 〔INVARIANTS §10〕`backend_status` 本机那一支要拿句柄表的锁 ⇒ 也是 `async`，本体住 `backend_status_now`
        //   （命令本身只有一行 `spawn_blocking`）⇒ 那一口切本体那一份。
        guard_core::find_pinned(&src, &format!("pub async fn {p}(origin: String)"))
            .unwrap_or_else(|e| panic!("`{p}` 不是恰好一条 `async` 命令（{e}）"));
        let sig = if *p == "backend_status" {
            "fn backend_status_now(\n    origin: String,".to_string()
        } else {
            format!("pub async fn {p}(origin: String)")
        };
        let at = guard_core::find_pinned(&src, &sig).unwrap_or_else(|e| {
            panic!(
                "`{p}` 不是「恰好一处、且第一个参数是 origin」的形状（{e}）。\n\
                     ★ 三个口必须**各只有一条命令**且都按 origin 分派 —— 本机一套、远端一套\n\
                     就是两套语义（`C1` 明说排除这条路）。"
            )
        });
        // ★★ **逐口切体，不数全局**。
        //
        // 第一版写的是「`is_local(&origin)` 全局出现 >= 2 处」。变异实测：
        // 把 `backend_stop` 里那一支整个摘掉，**判据照样绿** —— 因为另外两口还各有一处，
        // 计数仍然 >= 2。⇒ 计数型自检在「人群有多个成员」时天然逮不到「某一个成员塌了」。
        let body: String = src[at..]
            .lines()
            .skip(1)
            // ⚠ 收尾行**不写字面量右花括号** —— 本仓有判据用「花括号配平」剥测试段
            // （剥法的唯一住址是 `guard_core::production_source`），源码里多一个孤立的右花括号会让它**提前闭合**（`b'…'` 的字符字面量也算，我第一次「修」时就还带着一个），
            // 测试段整段泄漏进「生产段」⇒ 别的判据当场误报（08-11 实测：单写者守卫红了）。
            .take_while(|l| *l != "\u{7d}")
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            body.len() > 30,
            "`{p}` 切出来的函数体只有 {} 字节 —— 切错了，本条在空转",
            body.len()
        );
        guard_core::find_pinned(&body, "is_local(&origin)").unwrap_or_else(|e| {
            panic!(
                "`{p}` 里没有恰好一处 `is_local(&origin)` 分派（{e}）——\n\
                     ⇒ 这一口没接上本机那侧（或接了两次），`C1`「本地要和远端一样」在这口上落空。"
            )
        });
    }
}

/// ★★ **「说成功其实没成」的形态**〔D 阶段补审 08-11，A6〕：`backend_start` 把 `Resolved::Missing{reason}` 当 `Ok(reason)` ⇒
/// 「没内嵌后端」「释放失败」两种**真失败**在前端只走 `console.info`，**一个 toast 都不弹**。
///
/// 本条钉：本机那支必须有 `Failed` ⇒ `Err` 那一格（三态各有去处，不靠 `reason` 字符串猜）。
#[test]
fn starting_reports_failure_as_failure() {
    let src = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/backend_control.rs"
    ));
    let at = guard_core::find_pinned(&src, "pub async fn backend_start(origin: String)")
        .expect("起口不在了");
    let body: String = src[at..]
        .lines()
        .skip(1)
        .take_while(|l| *l != "\u{7d}")
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        body.len() > 200,
        "切出来的体只有 {} 字节 —— 切错了",
        body.len()
    );

    guard_core::find_pinned(&body, "StartOutcome::Failed").unwrap_or_else(|e| {
        panic!(
            "本机「起」没有把 `Failed` 单独接出来（{e}）。\n\
                 三种结局（起了 / 已经在跑 / 起不来）必须各有去处 ——\n\
                 全塞进 `Ok(reason)` 的话，真失败在前端只走 `console.info`，一个 toast 都不弹。"
        )
    });
    assert!(
        // 那句失败的话进了文案表：`Err(format!(…))` 换成了 `Err(copy_text(…))`，句子带详情时是 `Err(Said::…)`，都算「回 Err」。
        body.contains("Err(format!")
            || body.contains("Err(copy_text(")
            || body.contains("Err(Said::"),
        "本机「起」的失败那一格不回 `Err` —— 那就是把失败说成了成功。"
    );
}

/// 「起」成了只回成功本身，不带一句话：那句话没有读者（界面只看成败），带了就是一句不进文案表的硬编码。
#[test]
fn starting_answers_success_without_a_sentence() {
    let r: Result<(), cmd_wire::failed::Failed> =
        tauri::async_runtime::block_on(backend_start(" ".into()));
    assert!(r.is_err(), "空 origin 被放过了");
}

#[test]
fn an_empty_origin_is_refused_by_every_port() {
    for r in [
        tauri::async_runtime::block_on(backend_status("  ".into())).map(|_| ()),
        // `backend_start` 也改成 `async` ⇒ 同样就地跑完它。
        tauri::async_runtime::block_on(backend_start(" ".into())).map(|_| ()),
        // `backend_stop` 改成 `async`（本机那一支要等）⇒ 就地跑完它。
        tauri::async_runtime::block_on(backend_stop("".into())).map(|_| ()),
    ] {
        assert!(
            r.is_err(),
            "空 origin 被放过了 —— 那会造出一档谁都读不到的「全局」，设了没反应且不报错"
        );
    }
}

#[test]
fn an_unknown_remote_origin_says_so_instead_of_pretending() {
    let e = tauri::async_runtime::block_on(backend_stop("从没注册过的机器".into()))
        .unwrap_err()
        .said;
    assert!(
        copy_core::copy_matches("rsBackendControl.handle.missing", &e),
        "对不认识的 origin 应当明说没有把手，而不是返回一句像成功的话：{e}"
    );
}

/// 远端那台在不在只有一个源：本机后端推来的那台成品（界面照 `MachineState` 画）。壳的状态口对远端不表态（`channel: null`），
/// 不拿自己记的那台 hello 另答一份 —— 两个源里只要一个不更新，界面就一处说在线、一处说离线。
#[test]
fn the_status_port_does_not_answer_whether_a_remote_is_up() {
    let ledger = crate::backend_policy::DeathLedger::default();
    let st = backend_status_now("某远端".into(), &ledger).expect("远端那一问照样答得出");
    assert_eq!(
        serde_json::to_value(&st).unwrap()["channel"],
        serde_json::Value::Null,
        "远端那一格壳又在答在不在"
    );
}
