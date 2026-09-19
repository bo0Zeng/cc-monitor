use super::*;

/// ★★★ **G4：看得见「代码还在但走不到」** —— 本工作区最后一条、也最难的那条。
///
/// # 它治的病
///
/// Phase G 的分层变异抽样里，A1（`usage.rs`）与 A3（`local_accounts.rs`）**都存活**：
/// 在调用前插一句 `if true { return … }`，**调用那行文字还在** ⇒
/// 所有扫源码的守卫/登记表照样绿。那一族只能证明「这行写在那儿」，
/// **不能证明它会被执行**。
///
/// # 三个候选都不是答案，而根因也不是「缺判据形态」
///
/// 摸底先量了一件事：**当时那两个调用方（用量聚合 ＋ `list_local_session_accounts`）
/// 一个测试都没有驱动过**（`grep` 实测 0 处）。⇒ 三个候选
/// （真二进制 e2e 断言副作用 / 覆盖率门槛 / `#[cfg(test)]` 计数探针）
/// **全都要先有「一个真驱动那条路的测试」才谈得上**，而那一步才是缺的那步。
/// ⇒ 补上那一步之后，**探针根本不用新造** ——
///
/// # ★ 探针是定框 §5 自己给的：**诚实降级的 tagged 返回 + `reason`**
///
/// 定框 §5 逐字：「拿不到依赖」是**诚实降级**（tagged 返回 + `reason`），不是 `Err`。
/// ⇒ 在**没有 sidecar** 的环境里（单测就是这种环境）：
///
/// | 真走了那条路 | 被短路了 |
/// |---|---|
/// | 拿到 `NoBackend` ⇒ **带 `reason`** 的诚实降级 | 「空但成功」——**给不出 reason** |
///
/// **`reason` 的有无，就是「这条路真的跑了」的证据。** 它不需要新工具链、
/// 不侵入生产代码、也不需要真二进制 —— 那条纪律本身就是探针。
///
/// ⚠ 诚实说清它**不能**做什么：它证明的是「调用发生了且拿到了后端不在的答复」，
/// **不证明** happy path 正确（那要真 sidecar，属 e2e）。⇒ 它只杀「短路」这一类，
/// 而那正是 A1/A3 那一类。
#[tokio::test]
async fn a_short_circuit_cannot_fake_the_honest_degrade() {
    // 前提自检：本测试环境**必须**没有 sidecar，否则下面两条会走 happy path 而空转。
    let probe = run_query(
        env!("CCM_TARGET_TRIPLE"),
        &["--list-accounts"],
        &*crate::spawn_managed::local_backend_one_shot_query(),
    );
    assert!(
        matches!(probe, QueryOutcome::NoBackend(_)),
        "测试环境里居然找得到 sidecar —— 本条的前提不成立，两条断言会空转。\n\
             （若哪天单测环境真带 sidecar，本条要改成显式指一个不存在的 target triple）"
    );

    // 账号查询：诚实降级必须 available=false **且带 error 理由**。
    //    短路（`return Ok(Default::default())`）给出的是 error=None ⇒ 区分得开。
    // 〔`设计/50`：这里原先还有第二个调用点「② 用量聚合」，
    //   随用量 ② 轴整轴退役 ⇒ 本条今天只剩一个调用点。**性质没降**：它买的是
    //   「短路装不出诚实降级」，一个调用点就足以买到；降的是覆盖面，如实记在这儿。〕
    let r = crate::local_accounts::list_local_session_accounts()
        .await
        .expect("这条路的诚实降级是 Ok(available=false)，不该是 Err");
    assert!(!r.available, "没有 sidecar 却报 available=true");
    assert!(
        r.error.is_some(),
        "`list_local_session_accounts` 返回了「空但成功」——\n\
             没有 sidecar 时它**必须说出理由**（定框 §5：tagged 返回 + reason）。\n\
             ⇒ 拿不出 reason 就意味着**那条查询根本没发生**（被短路了），\n\
             而扫源码的守卫看不见这种错：调用那行文字还在。"
    );
}

/// 三态各自的判定口径。
///
/// ⚠ **本条的名字是个全称命题**（"zero is the **only** success"），
/// 所以用例面**不许只挑几个好数**〔G1 补，Phase G 变异抽样 B1 的产物〕：
/// 原先只测了 `Some(0)` / `Some(2)` / `None` —— **恰恰漏了 `Some(1)`**，
/// 而那是最常见的失败码。变异 `code.map(|c| if c == 1 { 0 } else { c })`
/// 把 1 悄悄映射成 0，本条**照样绿**。
/// ★ 覆盖面**第②格·用例面**：判据声称全称，用例却有洞，洞还开在最常走的那个值上。
#[test]
fn exit_zero_is_the_only_success_and_stderr_survives_failure() {
    assert_eq!(
        classify(Some(0), "line1\nline2\n".into(), String::new()),
        QueryOutcome::Ok("line1\nline2\n".into())
    );
    // ★ `Some(1)`：最常见的失败码，也是原先唯一没测的那个（见头注）。
    assert_eq!(
        classify(Some(1), "half\n".into(), "nope\n".into()),
        QueryOutcome::Failed {
            code: Some(1),
            stderr: "nope\n".into()
        }
    );
    // ★ 全称命题要按全称验：**除 0 以外一个都不许算成功**。
    //   逐个跑一遍常见退出码，别只挑样本 —— 「只挑几个好数」正是 B1 活下来的形状。
    for c in [1i32, 2, 3, 42, 101, 126, 127, 130, 255, -1] {
        assert!(
            matches!(
                classify(Some(c), "x\n".into(), "e\n".into()),
                QueryOutcome::Failed { .. }
            ),
            "退出码 {c} 被判成了非 Failed —— 而本条的名字声称「只有 0 是成功」"
        );
    }
    // ⚠ 退出码非 0 时 stdout 里可能**也有内容**（daemon 边写边失败），但那不是成功。
    assert_eq!(
        classify(Some(2), "partial\n".into(), "boom\n".into()),
        QueryOutcome::Failed {
            code: Some(2),
            stderr: "boom\n".into()
        }
    );
    // 被信号杀掉：`code()` 是 None，仍是失败而不是「后端不在」。
    assert_eq!(
        classify(None, String::new(), String::new()),
        QueryOutcome::Failed {
            code: None,
            stderr: String::new()
        }
    );
}

/// ★ **「后端不在」必须与「查询失败」分得开** —— 压成一个 `Err` 就是让上层猜。
///
/// 这条钉的是**类型上的可区分性**，不是某个字符串：`Failed` 那支带得出退出码，
/// `NoBackend` 那支带得出「找过哪些路径」。
#[test]
fn no_backend_is_not_a_failed_query() {
    let missing = run_query(
        "x86_64-unknown-linux-gnu-does-not-exist",
        &["--list-accounts"],
        &*crate::spawn_managed::local_backend_one_shot_query(),
    );
    match &missing {
        QueryOutcome::NoBackend(reason) => {
            // 理由里必须能看出「找过哪儿」，否则 UI 只能说一句「不可用」。
            assert!(
                reason.contains("cc-monitor-remote") || reason.contains("找过"),
                "「后端不在」的理由里看不出找过哪些路径：{reason}"
            );
        }
        other => panic!(
            "开发树里没有 sidecar，本条应当走 `NoBackend`，实得 {other:?}\n\
                 ⚠ 如果这台机器上**确实**有 sidecar（比如刚跑过发版构建），\
                 那本条会误报 —— 那时该把它改成注入一个不存在的 triple，而不是放宽断言。"
        ),
    }
    assert!(!matches!(missing, QueryOutcome::Failed { .. }));
}

/// ★ **前提触发器 —— 已经触发过一次，这是它的后继形态**（F10b 第一批，2026-08-04）。
///
/// # 原形是什么、为什么换
///
/// 原形：断言本模块**零生产调用方**，一有调用方就红并喊「回去把棘轮往下拧」。
/// F10b 第一批（`usage.rs` 改走 `--usage`）时它**确实红了**，而且红得对 ——
/// 棘轮当场从 11 拧到 10、`usage.rs` 那条登记删掉。
/// 〔`设计/50`：`usage.rs` 与 `--usage` 今天都不存在了 —— **上面这段是考古**，
///  留着是因为它解释的是本条**为什么长这个样子**，不是在描述今天的盘面。〕
///
/// ⇒ 换成后继形态：**每一个调用方都必须是「已退役」的那批**。
/// 判定不靠手写清单：拿本模块的生产调用方集合，与
/// `local_read_surface_registry::REGISTERED` 里**还挂着 `reader`** 的文件集合求交 ——
/// **交集必须为空**。一个文件既在调后端、又还记在「未退役」账上，那就是假账。
///
/// ⚠ **这不是降强度**：原形只能红**一次**（第一个调用方），此后永远绿；
/// 后继形态对**每一批迁移**都有效，而且它钉的是更难的那件事
/// （「棘轮跟着动了」而不只是「有人调了」）。
///
/// ⚠ 判定用的是**遍历 `src/` 生产段找 `run_query(`**，不是手写清单。
#[test]
fn every_caller_of_this_transport_is_already_off_the_read_surface_ledger() {
    let src_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    // 运行时拼，免得命中本文件自己。
    let verb = format!("run_{}(", "query");
    let mut callers = Vec::new();
    let mut stack = vec![src_root.clone()];
    let mut scanned = 0usize;
    let mut nested = 0usize;
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if p.extension().is_some_and(|x| x == "rs") {
                let raw = std::fs::read_to_string(&p).unwrap_or_default();
                let prod = guard_core::production_code(&raw);
                scanned += prod.len();
                if p.parent() != Some(src_root.as_path()) {
                    nested += 1;
                }
                // 本文件自己的定义行不算调用方。
                let rel = p
                    .strip_prefix(&src_root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                if rel.ends_with("local_query.rs") {
                    continue;
                }
                if prod.contains(verb.as_str()) {
                    callers.push(rel);
                }
            }
        }
    }
    // 中间量自检：遍历必须真的**递归**过。
    //
    // ⚠ **第一版只断言「扫到的字节数 > 200 000」，而那条挡不住「不递归」** ——
    // `src/` 顶层本身就有 60+ 个平铺 `.rs`（含 6000 行的 `ssh_source.rs`），
    // 字节地板光靠顶层就满足了。变异 Y4（把 `stack.push(p)` 删掉、只走一层）
    // **从那一版里活着走了出去**，而本条的头注恰恰声称它能逮住「遍历坏了」。
    // ⇒ 改成直接钉「递归发生了」：扫到的文件里必须有**嵌套**的
    //   （实测嵌套 17 个，分布在 `src/adapter`、`src/backend`、`src/backend/control`）。
    assert!(
        nested > 10,
        "只扫到 {nested} 个嵌套目录下的文件 —— 遍历没有递归，本条在空转\
             （实测嵌套 17 个：`adapter/` `backend/` `backend/control/`）"
    );
    assert!(
        scanned > 200_000,
        "剥完生产段只扫到 {scanned} 字节 —— 连顶层都没读到，遍历彻底坏了"
    );
    // 后继形态：调用方**不许**还挂在「未退役」账上。
    // ⚠ 账本的真相源是那个模块自己的 `REGISTERED`，这里**不抄一份文件名单** ——
    // 读它的源码把还标着 `reader` 的文件名抽出来（同一个数/同一张表只有一个家，定框 §4）。
    let ledger = std::fs::read_to_string(src_root.join("local_read_surface_registry.rs"))
        .expect("读不到 local_read_surface_registry.rs");
    let mut still_on_ledger: Vec<&String> = Vec::new();
    for c in &callers {
        // 登记表里的键是 `src/<rel>`。
        let key = format!("\"src/{c}\"");
        if let Some(at) = ledger.find(key.as_str()) {
            // 该条目的类别就在文件名之后不远处；只要它还标着 reader 就算「未退役」。
            // 🔴 〔`K-R97` 09-12 实测〕**按字节切窗口之前要先落到字符边界上**：
            // 登记表里全是中文说明，`at + 120` 十有八九落在一个汉字中间，
            // 那时切片当场 panic（逐字 `end byte index … is not a char boundary`）。
            // ⚠ 这不是本件改坏的，是本条**一直**踩在一颗只由字节偏移决定的雷上 ——
            // 上一处 `"src/…"` 的位置一变，雷就换个地方埋，而它此前从没被踩到过。
            let mut end = (at + 120).min(ledger.len());
            while !ledger.is_char_boundary(end) {
                end -= 1;
            }
            let window = &ledger[at..end];
            if window.contains("\"reader\"") {
                still_on_ledger.push(c);
            }
        }
    }
    assert!(
        still_on_ledger.is_empty(),
        "这些文件**既在调本机后端、又还挂在「未退役」账上**：{still_on_ledger:?}\n\
             ⇒ 那是假账，而且是最坏的那种：它让下一个人以为工作量还在。\n\
             正解：把它那条登记删掉，并把 `local_read_surface_registry` 的递减棘轮往下拧一格\n\
             （那个数只有一个家 —— `every_registered_file_declares_what_kind_of_read_it_is`，\n\
             别在别处抄第二份）。"
    );
    // 反向锚点：抽取器真的读到了账本，否则上面那条零命中地绿。
    assert!(
        ledger.contains("\"reader\"") && ledger.len() > 3000,
        "账本只读到 {} 字节或里面没有 `reader` —— 抽取坏了，本条在空转",
        ledger.len()
    );
}
