use super::*;

/// 〔条 66〕那个值**不在 monitor 进程里**：本模块生产段里唯一的 `static` 是死亡账那一张
/// （`ledger()`），原来那张「推进来的生效值表」连同推它的命令一起没了。
///
/// ⚠ 这是 `E2` 的 monitor 那一半：退出臂那一问（`kill_on_exit_now` → `exit_policy_call`）的路径上
/// 缓存点数 == 登记的 0。**带正控**：扫描器对死亡账那一张必须看得见（它是真 `static`），
/// 否则下面那条「只有一个」是在一台瞎了的尺子上成立的。
#[test]
fn the_monitor_keeps_no_copy_of_the_exit_value() {
    let prod = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/backend_policy.rs"
    ));
    let statics: Vec<&str> = prod
        .lines()
        .map(str::trim_start)
        .filter(|l| l.starts_with("static "))
        .collect();
    assert_eq!(
        statics.len(),
        1,
        "`backend_policy.rs` 生产段的 `static` 不是恰好一个（死亡账那张）：{statics:?}\n\
         多出来的多半是那个值的副本回来了 ——：不缓存、不在启动时读一次存内存。"
    );
    assert!(
        statics[0].contains("Health"),
        "唯一那个 `static` 不是死亡账那张（正控失败）：{}",
        statics[0]
    );
    for f in ["fn kill_on_exit_now(", "async fn exit_policy_call("] {
        let at = guard_core::find_pinned(&prod, f).unwrap_or_else(|e| panic!("切不出 `{f}`：{e}"));
        let body = &prod[at..at + prod[at..].find("\n}\n").expect("函数没有收尾")];
        for c in [
            "static ",
            "OnceLock",
            "OnceCell",
            "Mutex",
            "thread_local",
            "Atomic",
        ] {
            assert!(
                !body.contains(c),
                "`{f}` 体内出现了 `{c}` —— 那一问的路径上有了记忆（E2 防的正是这个）"
            );
        }
        assert!(
            body.contains("exit-policy-read") || body.contains(".call("),
            "`{f}` 体内既不发那条命令也不 `.call(` —— 它不再去问了，那它的答案从哪来？"
        );
    }
}

#[test]
fn the_exit_answer_is_read_from_the_backend_shape_and_nothing_else() {
    assert_eq!(
        kill_from_answer(&json!({ "killOnExit": true, "state": "chosen" })),
        Some(true)
    );
    assert_eq!(
        kill_from_answer(&json!({ "killOnExit": false, "state": "unreadable" })),
        Some(false)
    );
    for bad in [json!({}), json!({ "killOnExit": "true" }), json!(null)] {
        assert_eq!(
            kill_from_answer(&bad),
            None,
            "形状不对的应答 {bad} 被读出了一个值 —— 退出臂会拿一个编的答案去收进程"
        );
    }
}

// 「空 origin 必须拒」那一条（驱动 Tauri 命令 `set_backend_exit_policy`〔散文墓碑〕里的 `origin.route`）随命令退役：
//   设置页经通道问；空白名那一档由通道那一跳 `chan/webview.rs::chan_call` 的 `Origin::route` 当场按「用法错」拒（判据在 `webview_tests`）。

/// 没有通道的那台机器：退出臂那一问**明说**问不到，按缺省（不结束）办，不编一个值。
/// 界面那两问（问 / 写）改走通道之后，这里只剩退出臂那一问（monitor 自己的事）。
#[test]
fn without_a_channel_nothing_is_made_up() {
    let o = Origin("这台机从来没连上过-b2".into());
    let r = tauri::async_runtime::block_on(exit_policy_call(
        &o,
        "exit-policy-read",
        serde_json::json!({}),
        std::time::Duration::from_secs(1),
    ));
    assert!(r.is_err(), "没通道却答出了一个值：{r:?}");
    assert!(!kill_on_exit_now(&o), "问不到必须按缺省（不结束）办");
}

// 这里原来有三条跨语言逐字对拍（退出行为那张表 · 按清单派生的「每张表都对拍」·
//   读数那几句「只有一个家」的旧版）。文案表立起来之后两侧读**同一条表项**，Rust 副本删了，对拍随之删。
// 随后留下的那条「两侧说读数时用的是不是同一批 key」也退役了：
//   读数的三档判定只剩 [`health_face`] 一份（TS 那份删了，`judgment-single-home` 的 J21 钉它零实现），
//   只剩一侧再比就是恒真。接班的是下面那几条行为判据（三档逐格 · 金样 · 真命令接线）。

/// ★★ ② 退出行为与崩溃读数的那几句话**只有一个家：文案表**。
///
/// 人群：前端那两份（`backend-policy.ts` 取文 · `backend-section.ts` 画格子）＋ Rust 这两份
/// （本文件的被测模块 · `backend_control.rs`）。表里 `backendPolicy.*` 的每一句都不许在人群里以原文出现 ——
/// 出现 = 抄了第二份，下一次改文案一定只改到一处。
#[test]
fn the_backend_policy_copy_has_exactly_one_home() {
    let homes: Vec<(&str, String)> = vec![
        (
            "src/frontend/ui/backend-policy.ts",
            guard_core::strip_comment_lines(include_str!(
                "../../../src/frontend/ui/backend-policy.ts"
            )),
        ),
        (
            "src/frontend/ui/settings/backend-section.ts",
            guard_core::strip_comment_lines(include_str!(
                "../../../src/frontend/ui/settings/backend-section.ts"
            )),
        ),
        (
            "backend_control.rs",
            guard_core::production_code(include_str!(
                "../../../src/frontend/shell/src/backend_control.rs"
            )),
        ),
        (
            "backend_policy.rs",
            guard_core::production_code(include_str!(
                "../../../src/frontend/shell/src/backend_policy.rs"
            )),
        ),
    ];
    for (name, src) in &homes {
        // `backend-policy.ts` 的退出行为取文口搬走后只剩 `LOCAL_ORIGIN`（剥完 ~430 字节）⇒ 地板 500 → 300。
        assert!(
            src.len() > 300,
            "{name} 只读到 {} 字节 —— 人群坏了",
            src.len()
        );
    }
    let table: Value = serde_json::from_str(include_str!("../../../src/shared/copy/table.json"))
        .expect("文案表解不开");
    let lines: Vec<(String, String)> = table["entries"]
        .as_object()
        .expect("文案表没有 entries")
        .iter()
        // 健康那六句的 key 随判定归了本文件的面（`rsBackendPolicy.health.*`），原文一字没改。
        .filter(|(k, _)| {
            k.starts_with("backendPolicy.") || k.starts_with("rsBackendPolicy.health.")
        })
        .map(|(k, v)| (k.clone(), v["zh"].as_str().unwrap_or_default().to_string()))
        .collect();
    // 反空真：退出行为 4 句 + 读数 6 句（含 ⓘ 与 [详情] 那两句）。
    assert_eq!(
        lines.len(),
        10,
        "表里 backendPolicy.* ＋ rsBackendPolicy.health.* 不是 10 条：{lines:?}"
    );
    for (key, zh) in &lines {
        let hit: Vec<&str> = homes
            .iter()
            .filter(|(_, src)| src.contains(zh.as_str()))
            .map(|(n, _)| *n)
            .collect();
        assert!(
            hit.is_empty(),
            "`{key}` 那句原文又出现在 {hit:?} 里 —— 它的家是文案表，抄一份就是两个家"
        );
    }
}

// ── `K-P3` `KP3B`：四件事分得开 ──────────────────────────────────────

/// 四种死法各造一形。**用证据造，不用变体名造** —— 这是 `KP3B` 的承重点：
/// 「不许是「源码里出现了四个枚举名」（有名字 ≠ 接上了）」。
fn four_shapes() -> Vec<(&'static str, DeathEvidence)> {
    vec![
        (
            "从来没起来（Adopt::None / 起不来）",
            DeathEvidence {
                outcome: Outcome::NeverSpawned,
                handshake: Handshake::NeverSpoke,
                reader: ReaderEnd::CleanEof,
                start_failure: Some((
                    "没内嵌后端，exe 旁边也没有".to_string(),
                    vec![std::path::PathBuf::from("/甲/乙")],
                )),
            },
        ),
        (
            "被拒了（2026-07-09 那个 exit 2：一个字节都没输出、没有 hello）",
            DeathEvidence {
                outcome: Outcome::Exited(2),
                handshake: Handshake::NeverSpoke,
                reader: ReaderEnd::CleanEof,
                start_failure: None,
            },
        ),
        (
            "崩了（说过话之后被信号打死）",
            DeathEvidence {
                outcome: Outcome::Signalled(9),
                handshake: Handshake::Spoke,
                reader: ReaderEnd::CleanEof,
                start_failure: None,
            },
        ),
        (
            "读坏了（B1：InvalidData 与 EOF 同路）",
            DeathEvidence {
                outcome: Outcome::Exited(0),
                handshake: Handshake::Spoke,
                reader: ReaderEnd::Broken("stream did not contain valid UTF-8".to_string()),
                start_failure: None,
            },
        ),
    ]
}

/// ★★ `KP3B` 正题：四形 ⇒ 四个**互不相同**的判定，四条**两两不同**的话。
///
/// 死值验逐字要的是「把两种合成一种 ⇒ 红」。本条就是那一刀的落点：
/// 让 `verdict` 把任意两形判成同一格，判定集合就从 4 掉到 3，当场红。
#[test]
fn the_four_deaths_are_told_apart_by_evidence_not_by_name() {
    let shapes = four_shapes();
    // 反空真：四形都得在，少一形下面的去重计数会靠人群变小而恒绿。
    assert_eq!(shapes.len(), 4, "夹具只剩 {} 形 —— 人群坏了", shapes.len());
    let mut kinds: Vec<String> = Vec::new();
    let mut copies: Vec<String> = Vec::new();
    for (what, ev) in &shapes {
        let d = verdict(ev).unwrap_or_else(|| {
            panic!("「{what}」被判成了「不是一次死亡」—— 它不上账，也就没有任何一行记它")
        });
        kinds.push(death_kind(&d));
        copies.push(death_detail(&d));
    }
    let mut uniq = kinds.clone();
    uniq.sort_unstable();
    uniq.dedup();
    assert_eq!(
        uniq.len(),
        4,
        "四形只判出 {} 种判定：{kinds:?}\n\
             ★ `§0-2` 那两条真事故同一根：**死亡判据分不清崩了 / 拒绝了 / 读坏了**。\n\
             合成一格 = 那条根原样复发，而它的下游是「重起把一次确定性失败放大」。",
        uniq.len()
    );
    // 四条话两两不同（`KP3B` 逐字：「四条文案两两不同」）。
    for i in 0..copies.len() {
        for j in (i + 1)..copies.len() {
            assert_ne!(
                copies[i], copies[j],
                "第 {i} 形与第 {j} 形说的是同一句话 —— 那两格在用户眼里就没分开"
            );
        }
    }
    // 每一条只摆证据（逐字相等，不是「≥ 30 字」那道地板 —— 那道地板要的正是处方）。
    assert_eq!(
        copies,
        vec![
            "没内嵌后端，exe 旁边也没有；找过 1 处：/甲/乙",
            "它一个字节都没说就退出了",
            "说过话之后异常终止",
            "monitor 这一侧读出错：stream did not contain valid UTF-8",
        ],
        "账行的详情不是只摆证据的那四句"
    );
}

/// ★★ `B1` 那一刀：**读坏了不许被计成一次崩溃。**
///
/// 来历（`local_backend.rs` 逐字）：一个坏字节让 `InvalidData` 与 EOF 走同一条路
/// ⇒ 消费者返回 = 判死 ⇒ 记一次「崩溃」，三次之后整个进程周期不再起来，
/// 日志写「崩了 3 次」——**一个错误的诊断**。
#[test]
fn a_broken_reader_is_never_counted_as_a_crash() {
    let ev = DeathEvidence {
        outcome: Outcome::Signalled(9), // ⚠ 连「它真的被打死了」都不改这一格的答案
        handshake: Handshake::Spoke,
        reader: ReaderEnd::Broken("invalid utf-8".to_string()),
        start_failure: None,
    };
    let d = verdict(&ev).expect("读坏了也是一件要上账的事");
    assert_eq!(
        death_kind(&d),
        copy_core::copy_static!("rsBackendPolicy.death.misread"),
        "读端出错被判成了别的 —— 而这一维说的是**我们这一侧**，不是那个进程"
    );
    let origin = "kp3-读坏了不算崩-甲";
    let mut sink = CapturingSink::default();
    record_death(origin, &ev, &mut sink).expect("要记一笔");
    let h = health(origin);
    assert_eq!(
        (h.crashed, h.misread),
        (0, 1),
        "读坏了被加进了「崩了」那一格（crashed={}, misread={}）——\n\
             那正是「崩了 3 次」那条错误诊断的来历。",
        h.crashed,
        h.misread
    );
}

/// ★ **负例**：正常收工不上账。少了它，`verdict` 恒 `Some(..)` 也照样绿，
/// 而那会让每一次干净退出都在账上留一行「崩了」。
#[test]
fn a_clean_exit_is_not_a_death() {
    let ev = DeathEvidence {
        outcome: Outcome::Exited(0),
        handshake: Handshake::Spoke,
        reader: ReaderEnd::CleanEof,
        start_failure: None,
    };
    assert_eq!(
        verdict(&ev),
        None,
        "`exit 0` + 说过话 + 干净 EOF 被判成了一次死亡 —— 那是一次正常收工"
    );
    let origin = "kp3-正常收工-甲";
    let mut sink = CapturingSink::default();
    assert!(
        record_death(origin, &ev, &mut sink).is_none(),
        "正常收工也上账了"
    );
    assert!(
        sink.lines.is_empty(),
        "正常收工往落点写了东西：{:?}",
        sink.lines
    );
    assert_eq!(health(origin).seen(), 0, "正常收工被记进了读数");
}

/// ★ 起不来那一支的原因**原样转来**，不另写一份
/// （`local_backend_host.rs` 那一族逐字的纪律：「两份措辞迟早对不上」）。
#[test]
fn the_start_failure_reason_is_passed_through_verbatim() {
    // 中性夹具串：**不取自任何路径或夹具名**（`brief` 第 12 条那一族 ——
    // 断言用的子串取自夹具名字时，判据会靠名字恒真）。
    let reason = "拿不到 attach token（那个文件是空的）⇒ 拒绝起一个不设防的口";
    let ev = DeathEvidence {
        outcome: Outcome::NeverSpawned,
        handshake: Handshake::NeverSpoke,
        reader: ReaderEnd::CleanEof,
        start_failure: Some((reason.to_string(), vec![std::path::PathBuf::from("/丙/丁")])),
    };
    let d = verdict(&ev).expect("起不来是一件要上账的事");
    match &d {
        Death::NeverStarted {
            reason: got,
            looked_at,
        } => {
            assert_eq!(got, reason, "那句原因被改写过了 —— 两份措辞迟早对不上");
            assert_eq!(looked_at.len(), 1, "找过的地方丢了");
        }
        other => panic!("判成了 {other:?}，而证据说它从来没起来"),
    }
    assert!(
        death_detail(&d).contains(reason),
        "那句原因没被带到账上那一行 —— 账上只剩一个分类名，读的人拿不到下一步"
    );
}

// ── `K-P3` `KP3A`：一本真的会被写下来的账 ────────────────────────────

/// 记下每一行的落点（**只在测试里**）—— 让「那一行真的交给了落点」可判。
#[derive(Default)]
struct CapturingSink {
    lines: Vec<String>,
}
impl DeathSink for CapturingSink {
    fn write_line(&mut self, line: &str) -> Result<(), String> {
        self.lines.push(line.to_string());
        Ok(())
    }
}

/// 一个**拒收**的落点（目录不可写那一形的替身）。
struct RefusingSink;
impl DeathSink for RefusingSink {
    fn write_line(&mut self, _line: &str) -> Result<(), String> {
        Err("落点拒收：那个目录不可写".to_string())
    }
}

/// ★★ `KP3A`①：非零退出 / 异常终止**必须在账上留一行，带退出状态**。
///
/// # 非空对照（`KP3A`② 那一刀的落点）
///
/// 把 [`record_death`] 里 `sink.write_line(&line)` 那一行摘掉 ⇒ **本条当场红**
/// （落点一行都没收到）。少了它，「有一本账」这句话只靠一个函数名成立。
#[test]
fn every_abnormal_exit_leaves_one_line_carrying_its_exit_status() {
    let cases: Vec<(&str, DeathEvidence, &str)> = vec![
        (
            "非零退出",
            DeathEvidence {
                outcome: Outcome::Exited(2),
                handshake: Handshake::NeverSpoke,
                reader: ReaderEnd::CleanEof,
                start_failure: None,
            },
            "码=2",
        ),
        (
            "异常终止",
            DeathEvidence {
                outcome: Outcome::Signalled(11),
                handshake: Handshake::Spoke,
                reader: ReaderEnd::CleanEof,
                start_failure: None,
            },
            "信号 11",
        ),
    ];
    assert_eq!(cases.len(), 2, "夹具少了一形");
    for (what, ev, status) in &cases {
        let origin = format!("kp3-账上留一行-{what}");
        let mut sink = CapturingSink::default();
        let rec = record_death(&origin, ev, &mut sink)
            .unwrap_or_else(|| panic!("「{what}」没上账 —— 那就没有任何东西在记它"));
        assert_eq!(
            sink.lines.len(),
            1,
            "「{what}」交给落点的行数是 {}（应恰好 1）",
            sink.lines.len()
        );
        assert_eq!(
            sink.lines[0], rec.line,
            "交给落点的那一行与回给调用方的不是同一行"
        );
        assert!(
            rec.line.contains(status),
            "「{what}」那一行里没有退出状态 `{status}`：{}\n\
                 ⇒ 账上只剩一个分类名，而 `KP3A`① 要的是「带退出状态与时刻」。",
            rec.line
        );
        assert!(
            rec.line.contains(&origin),
            "那一行没说是哪台机：{}",
            rec.line
        );
        assert!(
            rec.sink_error.is_none(),
            "落点好着却报了错：{:?}",
            rec.sink_error
        );
    }
}

/// ★★ `KP3A` 死值验：**账写不进去要出声，不许静默。**
///
/// 两半缺一不可：① 那句拒收**原样**回到调用方手上（`#[must_use]` 让它吞不掉）；
/// ② 落点坏了**不许把读数一起带走** —— 进程内那张表照样更新，
/// 否则「写不进去」会顺带把界面上「崩过几次」清成 0，那是第二次静默。
#[test]
fn a_refusing_ledger_is_never_silent() {
    let origin = "kp3-写不进去-甲";
    let ev = DeathEvidence {
        outcome: Outcome::Signalled(9),
        handshake: Handshake::Spoke,
        reader: ReaderEnd::CleanEof,
        start_failure: None,
    };
    let rec = record_death(origin, &ev, &mut RefusingSink).expect("要记一笔");
    let why = rec.sink_error.as_deref().unwrap_or("");
    assert!(
        why.contains("拒收"),
        "落点拒收了，而调用方手上什么都没有（sink_error={:?}）——\n\
             那就是「没有任何东西在记它崩没崩」原地复发。",
        rec.sink_error
    );
    assert!(
        !rec.line.is_empty(),
        "拒收的时候连那一行本身都没给 —— 调用方连就地喊一声都做不到"
    );
    assert_eq!(
        health(origin).crashed,
        1,
        "落点坏了把读数也一起带走了 —— 那是同一件事上的第二次静默"
    );
}

/// ★★ `KP3C`：那句「无人监护」后面接得上一个**真读数**，而默认档是**答不出来**。
///
/// ★ 这一条是 `§0-1` 那一格的落点：今天不是「它没崩过」，是「没有任何东西在记」。
/// 把默认档写成「没崩过」⇒ 本条当场红。
#[test]
fn the_reading_defaults_to_unknown_not_to_clean() {
    let never_touched = health("kp3-从来没被记过的一台机");
    assert_eq!(never_touched.seen(), 0, "这台机的夹具串被别的测试用过了");
    let face = health_face(&never_touched);
    assert_eq!(face.state, HealthState::Unknown);
    assert_eq!(
        face.summary,
        crate::copy_table::copy_text("rsBackendPolicy.health.unknown", &[]),
        "一条记录都没有的机器被说成了别的 —— `§0-1` 逐字：\n\
             「今天不是『它没崩过』，是『没有任何东西在记它崩没崩』…… \
             这两句话差得很远，不许混用」。"
    );
    // 崩过之后读数要跟着走，且带得出次数与最后那一行。
    let origin = "kp3-读数跟着走-甲";
    let ev = DeathEvidence {
        outcome: Outcome::Signalled(6),
        handshake: Handshake::Spoke,
        reader: ReaderEnd::CleanEof,
        start_failure: None,
    };
    let mut sink = CapturingSink::default();
    record_death(origin, &ev, &mut sink).expect("要记一笔");
    record_death(origin, &ev, &mut sink).expect("要记第二笔");
    let face = health_face(&health(origin));
    let said = face.summary.clone();
    assert!(
        said.contains("×2") && said.contains("信号 6"),
        "读数没带出次数与最后那一行：{said}"
    );
    assert_eq!(
        face.state,
        HealthState::Crashed,
        "记了两笔崩溃，那一格却不是「崩过」—— 那句话就永远只是一句静态承诺了"
    );
    // 占位符必须真的被填掉（漏一个 `replace` 会把 `{crashed}` 原样端到用户眼前）。
    for ph in ["{crashed}", "{last}", "{misread}"] {
        assert!(!said.contains(ph), "读数里还留着占位符 `{ph}`：{said}");
    }
}

/// ★ **反空真**：生产落点真的造得出来、真的接一行。
///
/// 少了这一条，下面那条源码判据在一棵**根本没有 `MonitorLog`** 的树上照样能红得对、
/// 却证不了那个类型今天还在（`brief` 第 9 条那一族：报「有牙」要说清射程）。
#[test]
fn the_production_sink_takes_a_line_without_complaining() {
    let origin = "kp3-生产落点-甲";
    let ev = DeathEvidence {
        outcome: Outcome::Exited(3),
        handshake: Handshake::NeverSpoke,
        reader: ReaderEnd::CleanEof,
        start_failure: None,
    };
    let rec = record_death(origin, &ev, &mut MonitorLog).expect("要记一笔");
    assert!(
        rec.sink_error.is_none(),
        "生产落点拒收了：{:?}",
        rec.sink_error
    );
    assert!(
        rec.line.contains("码=3"),
        "生产落点收到的那一行没带退出状态：{}",
        rec.line
    );
    assert_eq!(health(origin).refused, 1, "读数没跟上");
}

/// ★ 落点那一半**真的把行交给了日志**（否则 `MonitorLog` 是个 no-op，
/// 而「真的会被写下来」这句话就只剩一个类型名）。
///
/// ⚠ 这一条读的是**源码文本**：`tracing` 没有返回值，一次真调用与一次 no-op
/// 在行为面上无法区分（要区分得装一个 capture subscriber，那是另一件事的成本）。
/// **如实登记，不假装这是行为判据。**
#[test]
fn the_production_sink_really_hands_the_line_to_the_log() {
    let prod = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/backend_policy.rs"
    ));
    guard_core::pin_line(&prod, "tracing::error!(\"{line}\");").unwrap_or_else(|why| {
        panic!(
            "{why}\n\
                 ⇒ `MonitorLog` 不再把那一行交给日志了。\n\
                 那本账**真的会被写下来**靠的就是这一行：真正碰盘的是\n\
                 `logging.rs::build_rolling_appender`（`write_site_registry::WRITE_SITES` 里\n\
                 已登记的那条），而时刻由 `fmt` 的默认 timer 打（带日期）。\n\
                 换成一个不打时刻的落点，账上就再问不出「它上一次崩在哪一天」——\n\
                 `~/.cc-monitor/bin/wrap.log` 那两行就是那个样子。"
        )
    });
}

// 这里原来有一条逐行钉 TS 三档的源码判据；三档搬进 [`health_face`]、TS 那份删了，它随之退役。
//   接班：下面两条（判准逐格 · 成品金样）＋ `judgment-single-home` 的 J21（TS 零实现）。

/// 造一份读数（只有四个计数与短摘要要紧；账行那一格给个占位，界面从来不读它）。
fn reading(
    crashed: u32,
    refused: u32,
    never_started: u32,
    misread: u32,
    brief: Option<&str>,
) -> Health {
    Health {
        crashed,
        refused,
        never_started,
        misread,
        last: brief.map(|b| format!("账行（只落日志）：{b}")),
        last_brief: brief.map(str::to_string),
    }
}

/// ★★ **那条区分的判准**逐格钉死：全零 ⇒ 无记录；四个计数**任一**非零 ⇒ 不是无记录；
/// 只有 `crashed` 非零才是崩过。
///
/// 五格缺一不可：只钉全零那一格的话，把判准写成 `crashed == 0` 照样绿 ——
/// 而那会把一台记到过 2 次读坏了的机器说成「答不出来」（可它明明有账）；
/// 四个计数各自 = 1 那四格就是这条判准的全部内容（「判准是『账上一条记录都没有』，不是 `crashed === 0`」）。
#[test]
fn the_health_face_is_judged_by_the_whole_ledger_not_by_crashes_alone() {
    let cells: [(&str, Health, HealthState); 5] = [
        ("全零", reading(0, 0, 0, 0, None), HealthState::Unknown),
        (
            "只崩过 1 次",
            reading(1, 0, 0, 0, Some("崩了，signal 9")),
            HealthState::Crashed,
        ),
        (
            "只被拒 1 次",
            reading(0, 1, 0, 0, Some("被拒了，exit 2")),
            HealthState::Clean,
        ),
        (
            "只没起来 1 次",
            reading(0, 0, 1, 0, Some("从来没起来，没有退出状态")),
            HealthState::Clean,
        ),
        (
            "只读坏 1 次",
            reading(0, 0, 0, 1, Some("读坏了，退出状态未知")),
            HealthState::Clean,
        ),
    ];
    for (what, h, want) in &cells {
        let face = health_face(h);
        assert_eq!(face.state, *want, "「{what}」判成了 {:?}", face.state);
        // 每一档给界面的东西也跟着档走：无记录 ⇒ 只有 ⓘ；没异常退出 ⇒ 都没有；异常退出过 ⇒ 只有 `[详情]`。
        match want {
            HealthState::Unknown => assert!(
                face.why.is_some() && face.detail.is_none(),
                "「{what}」是无记录，却没有 ⓘ、或给了 [详情]：{face:?}"
            ),
            HealthState::Clean => assert!(
                face.why.is_none() && face.detail.is_none(),
                "「{what}」没异常退出过，却挂着 ⓘ 或 [详情]：{face:?}"
            ),
            _ => assert!(
                face.why.is_none() && face.detail.is_some(),
                "「{what}」记到过事，却挂着「无记录」的 ⓘ、或没有 [详情]：{face:?}"
            ),
        }
        for s in [Some(&face.summary), face.why.as_ref(), face.detail.as_ref()]
            .into_iter()
            .flatten()
        {
            assert!(
                !s.contains('{') && !s.contains('〔'),
                "「{what}」那一格留着占位符或缺键：{s}"
            );
        }
    }
}

/// 金样：`tests/__fixtures__/backend-health.golden.json`（TS 解码器读同一份，`settings/backend-section.vitest.ts`）。
fn health_golden() -> Vec<(String, Health, Value)> {
    let g: Value = serde_json::from_str(include_str!(
        "../../__fixtures__/backend-health.golden.json"
    ))
    .expect("金样解不开");
    let cases = g["cases"].as_array().expect("金样没有 cases");
    cases
        .iter()
        .map(|c| {
            let n = |k: &str| {
                c["counts"][k]
                    .as_u64()
                    .unwrap_or_else(|| panic!("金样缺计数 {k}")) as u32
            };
            (
                c["name"].as_str().expect("金样缺 name").to_string(),
                reading(
                    n("crashed"),
                    n("refused"),
                    n("neverStarted"),
                    n("misread"),
                    c["lastBrief"].as_str(),
                ),
                c["face"].clone(),
            )
        })
        .collect()
}

/// ★★ 成品的线上形状两侧同一份：生产的 [`health_face`] 序列化 == 金样（逐格，键集恰好四个）。
///
/// 反空真：金样四形、三档都在（少一档，下面那条相等只在剩下的几档上成立）。
#[test]
fn the_health_face_matches_the_cross_language_golden() {
    let golden = health_golden();
    assert_eq!(golden.len(), 4, "金样不是四形 —— 人群坏了");
    let states: std::collections::BTreeSet<String> = golden
        .iter()
        .map(|(_, _, f)| f["state"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(
        states.into_iter().collect::<Vec<_>>(),
        vec!["clean", "crashed", "unknown"],
        "金样没盖全三档"
    );
    for (name, h, want) in &golden {
        let got = serde_json::to_value(health_face(h)).expect("成品序列化不了");
        let keys: Vec<&str> = got
            .as_object()
            .expect("成品不是对象")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys.len(), 4, "「{name}」的成品键集不是四个：{keys:?}");
        assert_eq!(
            &got, want,
            "「{name}」：生产现产的成品与金样对不上 —— 改了文案就照这份更新金样，改了形状就两侧一起改。\n现产：{got}"
        );
    }
}

/// ★★ `K-P1 KPY6` 的另一半：**那条无条件禁令今天是假的，四处一处都不许留着。**
///
/// 翻面之前那句话住四处（`P2s-Y5`）：`backend_policy.rs` · `backend_control.rs` ·
/// `backend-policy.ts` · `settings/backend-section.ts`。它逐字是
/// 「**UI 文案不许写「backend 继续运行」**」，依据是「backend 153ms 内自己走」。
///
/// `K-P1` 之后那个依据**只在没脱离的那一支成立** ⇒ 这条**无条件**禁令是一句半假的全称。
/// 留在四处中的任何一处，下一个人读到的就是「界面永远不许说继续跑」——
/// 而那正好会把 `K14` 背书的那半（「如实说继续跑，无人监护」）读成违规。
///
/// ⚠ 本条只查**那一句的字面**。它逮不到「换个说法写同一条无条件禁令」——
/// 如实登记，不假装机检覆盖了它。
#[test]
fn the_unconditional_ban_is_gone_from_all_four_homes() {
    const HOMES: &[(&str, &str)] = &[
        (
            "backend_policy.rs",
            include_str!("../../../src/frontend/shell/src/backend_policy.rs"),
        ),
        (
            "backend_control.rs",
            include_str!("../../../src/frontend/shell/src/backend_control.rs"),
        ),
        (
            "src/frontend/ui/backend-policy.ts",
            include_str!("../../../src/frontend/ui/backend-policy.ts"),
        ),
        (
            "src/frontend/ui/settings/backend-section.ts",
            include_str!("../../../src/frontend/ui/settings/backend-section.ts"),
        ),
    ];
    // 针**运行时拼**：本条自己的散文里就有这几个字，写成一个整串的话它会命中自己
    //（同族自指陷阱本仓记过多次）。
    //
    // ⚠⚠ **08-26 死值验当场逮到这条针是错的**：第一版拼的是 `不许写后端继续运行`，
    //   而原句是 `不许写「backend 继续运行」`——**中间隔着一对直角引号**。
    //   ⇒ 那一版**永远匹配不上原句**，本条是个安慰剂：把原句原样放回 `backend_control.rs`，
    //   实测 `5 passed; 0 failed`（rc=0），**一点都不红**。
    //   ★ 这就是「acceptor 必须先证明它会失败」买到的东西：它红之前，我以为它有牙。
    let needle = format!(
        "{}{}{}backend 继续运行{}",
        "不许", "写", '\u{300c}', '\u{300d}'
    );
    // ⚠ `.rs` 那两份**只看 `#[cfg(test)]` 之前那一段** —— 判据自己的解释性散文
    //   （包括本条的头注）住在测试段里，不切掉的话本条会**命中它自己、恒红**。
    //   ⚠ 这里不能用 `production_code`：它把 `//` 开头的行整行剥掉，
    //   而这条判据要查的**正是**那些头注散文（`//!` 也是 `//` 开头）。
    let before_tests = |s: &str| s.split("#[cfg(test)]").next().unwrap_or(s).to_string();
    let mut left: Vec<&str> = Vec::new();
    for (name, src) in HOMES {
        // 抽取器自检：四份都得真读到，切完也不能只剩个壳。
        assert!(
            src.len() > 500,
            "{name} 只读到 {} 字节 —— 人群坏了",
            src.len()
        );
        let scan = if name.ends_with(".rs") {
            before_tests(src)
        } else {
            (*src).to_string()
        };
        assert!(
            scan.len() > 400,
            "{name} 切完只剩 {} 字节 —— 切法坏了，本条在空转",
            scan.len()
        );
        if scan.contains(&needle) {
            left.push(name);
        }
    }
    assert!(
        left.is_empty(),
        "这几处还留着那条**无条件**禁令：{left:?}\n\
             它的依据（「monitor 一退它 153ms 内自己走」）在 `K-P1` 之后**只对没脱离的那一支成立**。\n\
             ⇒ 留着它 = 下一个人会把 `K14` 背书的那半（如实说「继续跑，无人监护」）读成违规。\n\
             四处要**同一拍**改，这正是「不许只改一处」那条 DoD 的落点。"
    );
}

// ── `K-P3b`：接了几处就是几处 ────────────────────────────────────────

/// 把一段（函数体）从生产段里切出来：从 `head` 那一行的下一行起，
/// 到第一行**恰好是右花括号**为止。形状抄 `local_backend_host_tests.rs::body_of`。
///
/// ⚠ 收尾行**不写字面量右花括号** —— 本仓有判据用「花括号配平」剥测试段，
/// 源码里多一个孤立的右花括号会让它提前闭合（`local_backend.rs` 那处逐字记过）。
fn body_after(prod: &str, head: &str) -> String {
    let at = guard_core::find_pinned(prod, head).unwrap_or_else(|e| {
        panic!("切不出 `{head}`（{e}）—— 它改名或搬家了，来改 `DEATH_RECORD_SITES`")
    });
    prod[at..]
        .lines()
        .skip(1)
        .take_while(|l| *l != "\u{7d}")
        .collect::<Vec<_>>()
        .join("\n")
}

/// 只有本判据读这张表 ⇒ 它住判据旁边，不住产品代码（住在那边时它是生产段里的一条死常量）。
/// ★★ `K-P3b KP3W3`：[`record_death`] 的**生产调用点逐处点名**。
///
/// 形状照 `src/backend/readonly_guard.rs::ALLOWED` 那种
/// 「**逐处点名 + 相等**」，不是地板 —— 地板在变大方向上是瞎的
/// （那张表的报错文案逐字：「不许改回地板」）。
///
/// # 它守的是什么
///
/// `K-P3` 第一档交付时这个数是 **0**：判据、账、文案全买了，**一个消费者都没有**
/// （`K-P3` `§3-5` 第一行如实登记）。本件把它接成 3 处；
/// [`the_death_ledger_is_wired_at_exactly_these_sites`] 让「接了几处就是几处」
/// 变成一条相等断言 —— 摘掉任何一处**都会点名是哪一处少了**。
///
/// `(文件, 那一处的宿主函数头, 它记的是哪条路)`
///
/// ⚠ 第二列是**函数头整行的前缀**，用来把那一处的函数体切出来单独数 ——
/// 只数全局总数的话，「某一处塌了、另一处多了一次」会互相抵消（本仓 `backend_control.rs`
/// 那条「逐口切体，不数全局」的头注为同一形栽过一次）。
const DEATH_RECORD_SITES: &[(&str, &str, &str)] = &[
    (
        "local_backend_host.rs",
        "fn note_detached_death(",
        "脱离路：`attach_stream` 的流断了 ⇒ `reap_detached` 那条收尸线程 `wait()` 回来那一拍",
    ),
    (
        "local_backend_host.rs",
        "fn backend_supervise_events(",
        "监护路：backend 那个 `on_event` **闭包**收到 `Exited` 那一拍。\
         它抽成一个返回闭包的函数，只为让 `KP3W3` 那三只假后端能跑**同一个闭包**\
         —— 内联的闭包测试够不着，那条行为判据就只能退回读源码",
    ),
    (
        "local_backend_host.rs",
        "fn note_never_started(",
        "起不来：`start_local_backend` 返回 `StartOutcome::Failed` 那一个出口",
    ),
];

/// ★★ `KP3W3` 的「数」那一格：[`record_death`] 的生产调用点 == [`DEATH_RECORD_SITES`]。
///
/// **相等，不是地板** —— 地板在变大方向上是瞎的（`readonly_guard::ALLOWED` 那张表的
/// 报错文案逐字：「不许改回地板」）。
///
/// 两格一起判，缺一格就漏一种：
/// ① **总数**：整棵 `src/frontend/shell/src` 的生产段里恰好这么多处；
/// ② **逐处点名**：每一处的宿主函数体内**恰好一处** ——
///    只数总数的话，「某一处塌了、另一处多记了一次」会互相抵消
///    （`backend_control.rs` 那条「逐口切体，不数全局」为同一形栽过一次）。
///
/// ⚠ **这两行先前是纯旧话，而且与它下面 20 几行的注释互相矛盾**：
/// 原文是「人群里**没有本文件自己**：`scan_tree!` 按 `file!()` 摘掉调用者那一份，
/// 而 [`record_death`] 的定义与它自己的单测都住这儿 —— 不摘就恒有命中」。
/// 那一刀**在这一处不生效**（判据由 `#[path]` 挂载 ⇒ `file!()` 是带 `..` 的折返路径
/// ⇒ 后缀比不命中）；而剖分之后 [`record_death`] 的**定义**住 `backend_policy.rs`、
/// 它自己的单测住本判据文件，**两者已经不同住一份**。
/// ⇒ 今天的真实口径：人群 = `src/frontend/shell/src` 整棵树（`backend_policy.rs` **在里面**），
/// 而「定义不算调用点」是下面那个 `call_sites` 闭包按语法排掉的 —— 不是靠摘文件。
///
/// # ⚠ 诚实边界：它数的是**源码文本**，不是「那一行真的会跑到」
///
/// 〔09-05 `7u` 那一刀当场量出来的，不是想出来的〕：把三处接线一起退成
/// 「形状对、恒答一张脸」（每处开头加一句 `if true { … return; }`，
/// 调用点原文留着），**本条一个字都不会说** —— 那一趟实测本条绿着，
/// 而红的是三条**行为**判据（`three_fake_backends…` · `the_never_started_reason…` ·
/// `the_consumer_reports_what_it_observed…`）。
///
/// ⇒ 本条买的是「**那几行还在、而且只在这几处**」，买不到「它们走得到」。
/// 走得到那一半由上面那三条行为判据管；两条合起来才闭合，单独任何一条都不够。
/// **别把本条读成「接线还活着」。**
#[test]
fn the_death_ledger_is_wired_at_exactly_these_sites() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files: Vec<(std::path::PathBuf, String)> = guard_core::scan_tree!(&root, &["rs"]);
    assert!(
        files.len() >= 20,
        "只扫到 {} 份 `.rs` —— 抽取坏了，本条会零命中地绿",
        files.len()
    );
    let mut scanned = 0usize;
    let mut hits: Vec<(String, usize)> = Vec::new();
    // 🔴 〔搬树 2026-09-18 ·  条 73〕**只数调用点，不数定义。**
    //
    // 上一版靠 `scan_tree!` 的 `file!()` 自摘把 `backend_policy.rs` 摘出人群 ——
    // 当年本条就住在它的 `#[cfg(test)]` 段里，而那份文件恰好是 `record_death` 的**定义家**。
    // 剖分之后自摘落空，`pub fn record_death(` 这一处**定义**被当成第四个调用点数了进来
    // （读数逐字：「生产调用点是 4 处（登记 3 处）。实得：[("backend_policy.rs", 1), …]」）。
    //
    // ⚠ 修法**不是**把那份文件整个排掉（那会连它里面真有的调用点一起瞎掉），
    //   而是把「定义」与「调用」分开 —— 口径抄 `creds_store_tests.rs` 那条同族判据：
    //   「『定义』不是一个出口，『调用』才是；两者混在一个数里，那个数就同时装了两件事」。
    //   ⇒ 人群反而**比搬树前更大**：`backend_policy.rs` 的其余代码现在也进了扫描面。
    let call_sites = |prod: &str| -> usize {
        prod.match_indices("record_death(")
            .filter(|(i, _)| !prod[..*i].trim_end().ends_with("fn"))
            .count()
    };
    for (path, raw) in &files {
        let prod = guard_core::production_code(raw);
        scanned += prod.len();
        let n = call_sites(&prod);
        if n > 0 {
            hits.push((
                path.file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                n,
            ));
        }
    }
    assert!(
        scanned > 200_000,
        "剥完只剩 {scanned} 字节可扫 —— 剥过头了，本条在空转"
    );
    // ⚠⚠ **逐处点名排在总数前面，这个次序是有意的**〔09-05 死值验当场逼出来的〕。
    //   先跑总数那一条时，摘掉任意一处 `record_death` 印出来的是
    //   「生产调用点是 2 处（登记 3 处）。实得：[("local_backend_host.rs", 2)]」——
    //   它说得出**少了一处**，说不出**少的是哪一处**（三处都在同一个文件里）。
    //   ⇒ 先红的该是**说得出病在哪**的那句诊断，而不是要人再去查一遍的记账话。
    //   （形状抄 `local_backend_host_tests.rs::the_user_actionable_start_failures_all_reach_the_user`
    //   头注那一段：「②③ 排在 ④ 前面是有意的」。）
    for (file, head, why) in DEATH_RECORD_SITES {
        let raw: &str = match *file {
            "local_backend_host.rs" => {
                include_str!("../../../src/frontend/shell/src/local_backend_host.rs")
            }
            other => panic!("`DEATH_RECORD_SITES` 里出现了没接语料的文件：{other}"),
        };
        let prod = guard_core::production_code(raw);
        let body = body_after(&prod, head);
        assert!(
            body.len() > 100,
            "`{file}` 的 `{head}` 切出来只有 {} 字节 —— 切错了，这一格在空转",
            body.len()
        );
        let n = body.matches("record_death(").count();
        assert_eq!(
            n, 1,
            "`{file}` 的 `{head}` 体内 `record_death(` 有 {n} 处（该恰好 1 处）。\n\
                 这一处记的是：{why}\n\
                 ★ 0 处 = **这条路的死亡从此没人记**；而只数总数的话，\n\
                 「这一处塌了、另一处多记了一次」会互相抵消，谁都不出声。"
        );
    }
    let total: usize = hits.iter().map(|(_, n)| *n).sum();
    assert_eq!(
        total,
        DEATH_RECORD_SITES.len(),
        "`record_death` 的生产调用点是 {total} 处（登记 {} 处）。实得：{hits:?}\n\
             ★ **接了几处就是几处**。变少 = 某一条观测路又回到了「看得见它没了、却没人记」，\n\
             那正是 `K-P3` `§3-5` 第一行登记的那一格（当时这个数是 0）；\n\
             变多 = 有第四条路开始记账（上面逐处那一圈只看登记过的三处，\n\
             **第四处它一个字都不会说**）⇒ 回来把它写进 `DEATH_RECORD_SITES`，\n\
             并说清它记的是哪条路。",
        DEATH_RECORD_SITES.len()
    );
}

/// ★★ `KP3W3`：**监护器自己一笔都不许记。**
///
/// 同一个 `supervise_with_stdio` 今天有两种客户：backend 与中转。
/// 记账落进监护器体内 ⇒ 中转的死会记到「这台机的后端」头上，
/// 而那本账的定义就是「这台机的后端」的账（`§0a` 逐字）。
/// ⇒ 接线必须落在**客户这一侧**的 `on_event` 上。
#[test]
fn the_supervisor_itself_never_records_a_death() {
    let prod = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/local_backend.rs"
    ));
    assert!(
        prod.len() > 10_000,
        "剥完只剩 {} 字节 —— 本条在空转",
        prod.len()
    );
    let body = body_after(&prod, "pub fn supervise_with_stdio(");
    assert!(
        body.len() > 1_000,
        "`supervise_with_stdio` 切出来只有 {} 字节 —— 切错了",
        body.len()
    );
    assert_eq!(
        body.matches("record_death(").count(),
        0,
        "`supervise_with_stdio` 体内出现了 `record_death(` ——\n\
             ★ 它同时监护后端与中转 ⇒ 中转的死会被记进「这台机的后端」那本账。\n\
             ⇒ 记账落在客户那一侧的 `on_event`（`local_backend_host::backend_supervise_events`）。"
    );
    // 整组「调后端的客户端 ＋ 监护」（从前的 `backend/` 目录，今天逐个点名：`backend_client_guard_tests.rs::GUARDED`）也是 0 ——
    //   判与记都不在那一半（`B1` 那次误诊正是判断落在 backend 层的产物）。
    let files: Vec<(std::path::PathBuf, String)> = crate::backend_client_guard::guarded_sources();
    assert!(
        files.len() >= 5 && files.iter().all(|(_, s)| !s.is_empty()),
        "只读到 {} 份那一组的文件 —— 抽取坏了，本条在空转",
        files.len()
    );
    let mut offenders: Vec<String> = Vec::new();
    for (path, raw) in &files {
        if guard_core::production_code(raw).contains("record_death(") {
            offenders.push(path.display().to_string());
        }
    }
    assert!(
        offenders.is_empty(),
        "`backend/` 的生产段里出现了 `record_death(`：{offenders:?}\n\
             ⇒ 判与记该在宿主层。`backend/` 那半**只搬证据**（`SuperviseEvent::Exited` 的\n\
             `status` / `witness` 两个字段就是它搬的全部）。"
    );
}

/// ★ `K-P3b`：**「根本没有读端」不是一次干净 EOF，也永远不是「读坏了」。**
///
/// 死值验：把 `verdict` 里 `ReaderEnd::NotObserved` 那一臂改成
/// `return Some(Death::Misread { .. })` ⇒ 下面两格一起红。
#[test]
fn a_reader_that_never_existed_is_neither_a_clean_eof_nor_a_misread() {
    assert_ne!(
        ReaderEnd::NotObserved,
        ReaderEnd::CleanEof,
        "「根本没有读端」与「干净 EOF」变成同一个值了 —— \
             那句「管子关了 / 它走了」对一条从来没有过管子的路是假话"
    );
    // ① 从来没起来：这一维让开，判定由剩下两维给出。
    let never = DeathEvidence {
        outcome: Outcome::NeverSpawned,
        handshake: Handshake::NeverSpoke,
        reader: ReaderEnd::NotObserved,
        start_failure: Some(("拿不到那一份".to_string(), Vec::new())),
    };
    assert_eq!(
        death_kind(&verdict(&never).expect("起不来是一件要上账的事")),
        copy_core::copy_static!("rsBackendPolicy.death.neverStarted"),
        "没有读端被判成了别的 —— 「读坏了」说的是我们这一侧读**出了错**，\
             而那条路上连读端都没有过"
    );
    // ② 非空对照：同一个 `NotObserved` 配一个真的异常终止 ⇒ 仍然判「崩了」，
    //    这一维不许把判定拽走。
    let signalled = DeathEvidence {
        outcome: Outcome::Signalled(9),
        handshake: Handshake::Spoke,
        reader: ReaderEnd::NotObserved,
        start_failure: None,
    };
    assert_eq!(
        death_kind(&verdict(&signalled).expect("被信号打死要上账")),
        copy_core::copy_static!("rsBackendPolicy.death.crashed"),
        "`NotObserved` 把一次真的异常终止判成了别的格"
    );
}

// ── 两个同名 `LOCAL_ORIGIN` 的钉子 ⇒ 合了 ──────
//
// ⚠ **这一段原本是两条，删掉了一条 —— 那条是我重复造的。**
//   跨语言那条（Rust `inbound_client::LOCAL_ORIGIN` ↔ TS `backend-policy.ts`）
//   **仓里本来就有**：`tests/frontend/shell/inbound_client_tests.rs::
//   the_local_origin_is_the_same_string_on_both_sides`。
//   我一度判它「不存在」，依据是一条 `grep … | head -4` 的输出 ——
//   🔴 **拿一个截断过的人群下「不存在」的判，正是本仓反复治的那个病。**
//
// 剩下那一条原名 `the_two_same_named_local_origin_constants_stay_deliberately_different`〔散文墓碑〕，
// 钉的是「两者**刻意不同**：`backend-policy.ts` 的是 backend origin，`accounts.ts` 的 `"__local__"` 是账号面自己的标记
// ——合并是一次设计变更，回去看」。它守的是「合并之前别顺手改」，不是「永远不合」。
// C4b 读了（一个 Origin 类型、账号面本机与远端同一条路）裁「合」：`"__local__"` 装的就是
// 「哪台机器」本身（它只是本机那条命令结果的缓存键），账号面没有第二个概念 ⇒ 这一次就是它等的那次设计变更。
// ⇒ 本条改成钉**合了之后**的形状；TS 那一侧按语法树的零命中住 `tests/frontend/ui/ipc/commands.vitest.ts`（「本机只有一个表示」）。

/// 账号面**不再有自己的本机表示**：`accounts.ts` 生产段里零处 `LOCAL_ORIGIN` 声明、零处 `"__local__"`；
/// 本机那个值的家恰好一个（`backend-policy.ts` 的 `LOCAL_ORIGIN = "<local>"`，正控）。
///
/// 与 TS 那条（`commands.vitest.ts`，按语法树扫全树的字符串字面量）**异源**：本条按文本读两份指名的文件，
/// 专钉「账号面那一份不许再声明一个同名常量」—— 换个值再声明一次，TS 那条的字面量扫描认不出它的用意。
#[test]
fn the_account_face_has_no_local_origin_of_its_own() {
    let policy = include_str!("../../../src/frontend/ui/backend-policy.ts");
    let accounts = include_str!("../../../src/frontend/ui/accounts.ts");
    let needle = "export const LOCAL_ORIGIN = ";
    let prod = |src: &str| guard_core::strip_comment_lines(src);
    // 正控：本机那个值恰好一个家，值是 `<local>`。
    let policy_prod = prod(policy);
    assert_eq!(
        policy_prod.matches(needle).count(),
        1,
        "`backend-policy.ts` 里 `{needle}` 应恰好 1 处 —— 抽取器坏了，或本机那个值的家挪了"
    );
    let value = policy_prod
        .split_once(needle)
        .and_then(|(_, rest)| rest.split_once(';'))
        .map(|(v, _)| v.trim().trim_matches('"').to_string())
        .expect("抠不出值");
    assert_eq!(value, "<local>", "backend origin 的哨兵值变了");
    // 账号面：零声明、零旧值。
    let accounts_prod = prod(accounts);
    assert_eq!(
        accounts_prod.matches(needle).count(),
        0,
        "`accounts.ts` 又声明了一个自己的 `LOCAL_ORIGIN` —— 账号缓存键与 `AccountsState.origin` 用 `ipc/origin.ts` 那一个"
    );
    assert!(
        !accounts_prod.contains("\"__local__\""),
        "`accounts.ts` 生产段里又出现了 `\"__local__\"` —— 那是已退役的第二种本机写法"
    );
    // 阴性对照：剥法真的在跑（注释里讲历史不算）。
    assert!(
        !prod("// 旧写法 \"__local__\"\n").contains("__local__"),
        "剥注释没剥掉 —— 上面那条零命中可能是空转"
    );
}

// ── 后端停止产 markdown 与设计论证 ─────────

/// 那五种形状里，**后端这一侧**能产出来的四种（markdown · 源码住址 · 日志行格式 · 设计论证）。
/// 与 `tests/frontend/ui/settings/ui-copy-discipline.vitest.ts::SHAPES` 同义；这里只认字面，不做语义判断。
fn ui_copy_violations(s: &str) -> Vec<&'static str> {
    let mut out = Vec::new();
    if s.contains("**") {
        out.push("markdown 标记");
    }
    if s.contains(".rs") || s.contains("::") {
        out.push("源码住址");
    }
    if ["[死亡账]", "origin=", "判定=", "退出状态="]
        .iter()
        .any(|n| s.contains(n))
    {
        out.push("日志行格式");
    }
    if ["下一步：", "放大器", "本条不推翻", "如实登记", "判不了"]
        .iter()
        .any(|n| s.contains(n))
    {
        out.push("设计论证 / 下一步处方");
    }
    out
}

/// ★★ **进界面的那一格**（`backend_status` 的 `health` 成品：`summary` · `why` · `detail` 三句，
/// 「崩过」那一句接的是 `Health::last_brief`）全干净，
/// 而**进日志的那一行**（`ledger_line`）照旧带着日志行格式 —— 两边的分工是这条判据的全部内容。
///
/// 反空真：尺子先在一段合成的脏文本上四种全逮到（正控），再在 `ledger_line` 上逮到日志行格式
/// （它**本来就该**是日志行 —— 若那边也零命中，说明尺子瞎了，而不是后端干净了）。
#[test]
fn what_reaches_the_settings_panel_carries_no_markdown_no_argument_no_log_format() {
    let dirty = "**下一步：**看 `backend_policy.rs` 里的 [死亡账] origin=x —— 重起是放大器";
    let mut got = ui_copy_violations(dirty);
    got.sort_unstable();
    assert_eq!(
        got,
        vec![
            "markdown 标记",
            "日志行格式",
            "源码住址",
            "设计论证 / 下一步处方"
        ],
        "正控没全逮到 —— 下面的「零命中」是在一把瞎了的尺子上成立的"
    );
    let shapes = four_shapes();
    assert_eq!(shapes.len(), 4, "夹具少了一形");
    for (what, ev) in &shapes {
        let d = verdict(ev).expect("四形都是死亡");
        let brief = last_brief(&d);
        assert_eq!(
            ui_copy_violations(&brief),
            Vec::<&str>::new(),
            "「{what}」进界面的那一格犯了：{brief}"
        );
        // 拼法走文案表（不在代码里写死「，」）。
        assert_eq!(
            brief,
            crate::copy_table::copy_text(
                "rsBackendPolicy.death.brief",
                &[
                    ("kind", death_kind(&d).as_str()),
                    ("ended", exit_status(&d).as_str())
                ]
            ),
            "「{what}」的短摘要没走文案表"
        );
        // 判定词与退出状态那句人话都在；裸码不上这一格（它进死亡账那一行 ＋ 复制详情）。
        assert!(
            brief.contains(&death_kind(&d)) && brief.contains(&exit_status(&d)),
            "「{what}」的短摘要丢了判定或退出状态：{brief}"
        );
        if let Some(code) = exit_code(&d) {
            assert!(
                !brief.contains(&code),
                "「{what}」的短摘要带了裸码 {code}：{brief}"
            );
            assert!(
                ledger_line("o", &d).contains(&format!("码={code}")),
                "「{what}」的码没进账行：{}",
                ledger_line("o", &d)
            );
        }
        // 进日志的那四条话也不许再带 markdown / 论证（它们不进界面，但要后端「停止产」）。
        let copy = death_detail(&d);
        assert!(
            !copy.contains("**") && !copy.contains("放大器") && !copy.contains(".rs"),
            "「{what}」的日志话里还有 markdown / 论证 / 源码住址：{copy}"
        );
        // 反空真：日志那一行**就是**日志行格式。
        assert!(
            ui_copy_violations(&ledger_line("o", &d)).contains(&"日志行格式"),
            "账行不再是日志行格式了 —— 尺子或账行其一坏了"
        );
    }
    // 成品里每一句都进界面：金样四形（三档）逐句扫。
    let golden = health_golden();
    assert_eq!(golden.len(), 4, "金样少了一形");
    for (name, h, _) in &golden {
        let face = health_face(h);
        for s in [Some(&face.summary), face.why.as_ref(), face.detail.as_ref()]
            .into_iter()
            .flatten()
        {
            assert_eq!(
                ui_copy_violations(s),
                Vec::<&str>::new(),
                "「{name}」那一格的成品犯了：{s}"
            );
        }
    }
}

/// ★ 读数里那一格**接的是短摘要，不是账行**：记一笔之后 [`health_face`] 格子里那一句
/// 没有日志行格式（原来 `HEALTH_CRASHED` 的 `{last}` 被整条账行填满，截图 2 那一段就是它）。
#[test]
fn the_crash_reading_quotes_the_brief_not_the_ledger_line() {
    let origin = "st2-读数接短摘要-甲";
    // 一个闭集外的码（`0xC0000006`）：读数里说「原因不明」，裸码不上句子（它在账行 `码=` 与复制详情里）。
    let ev = DeathEvidence {
        outcome: Outcome::Exited(-1073741818),
        handshake: Handshake::Spoke,
        reader: ReaderEnd::CleanEof,
        start_failure: None,
    };
    let mut sink = CapturingSink::default();
    let rec = record_death(origin, &ev, &mut sink).expect("要记一笔");
    let h = health(origin);
    assert_eq!(
        h.last.as_deref(),
        Some(rec.line.as_str()),
        "账行那一格没跟上"
    );
    let d = verdict(&ev).expect("是死亡");
    assert_eq!(h.last_brief.as_deref(), Some(last_brief(&d).as_str()));
    let said = health_face(&h).summary;
    assert!(
        said.contains(&exit_status(&d)) && !said.contains("1073741818"),
        "读数里没有退出状态那句人话，或带了裸码：{said}"
    );
    assert!(
        rec.line.contains("码=-1073741818 (0xC0000006)"),
        "账行没带码：{}",
        rec.line
    );
    assert_eq!(
        ui_copy_violations(&said),
        Vec::<&str>::new(),
        "读数里混进了账行 / markdown：{said}"
    );
}

/// ★★ 接线那一半：**真命令** `backend_status` 回的 `health` 就是 [`health_face`] 的成品，
/// 原料（四个计数 · 短摘要 · 账行）一格都不上线。
///
/// 原来这里是一条钉拼 JSON 那一行的源码判据；远端 origin 那一支不碰通道也不碰进程，
/// 真命令在测试里跑得动 ⇒ 换成行为判据。
#[test]
fn the_status_command_hands_the_panel_the_finished_face() {
    let origin = "pb1-状态接成品-甲";
    let ev = DeathEvidence {
        outcome: Outcome::Signalled(9),
        handshake: Handshake::Spoke,
        reader: ReaderEnd::CleanEof,
        start_failure: None,
    };
    let mut sink = CapturingSink::default();
    let _ = record_death(origin, &ev, &mut sink).expect("要记一笔");
    let st = tauri::async_runtime::block_on(crate::backend_control::backend_status(origin.into()))
        .expect("查状态");
    let got = st.get("health").expect("状态里没有 health 那一格").clone();
    let want = serde_json::to_value(health_face(&health(origin))).expect("成品序列化不了");
    assert_eq!(
        got, want,
        "`backend_status` 交给界面的不是 `health_face` 的成品"
    );
    assert_eq!(
        got["state"], "crashed",
        "记了一笔崩溃，交出去的却不是「崩过」那一档：{got}"
    );
    let keys: std::collections::BTreeSet<&str> = got
        .as_object()
        .expect("health 不是对象")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        ["detail", "state", "summary", "why"].into_iter().collect(),
        "health 那一格的键不是成品那四个 —— 原料（计数 / 短摘要 / 账行）又上线了，界面就能再判一遍"
    );
    // 没记过的那一台：同一条命令回「无记录」那一档（远端今天恒是这一档 —— 那是真话，不是缺席）。
    let quiet = tauri::async_runtime::block_on(crate::backend_control::backend_status(
        "pb1-没记过的一台".into(),
    ))
    .expect("查状态");
    assert_eq!(quiet["health"]["state"], "unknown");
}

// ── 死亡账说人话：`0xC000013A` ─────────────────────────

/// 被控制台事件杀死的那一种，那一格说**人话**，不说裸码。
///
/// ⚠ 两侧异源：码写成**字面量** `-1073741510`（`ExitStatus::code()` 在 Windows 上真给的那个 `i32`），
/// 不从 [`STATUS_CONTROL_C_EXIT`] 推；那句话逐字抄，不引用常量。
/// 常量写错一位（或按数值换算成了别的数）⇒ 这里当场红。
#[test]
fn a_console_ctrl_kill_is_said_in_words_not_as_a_bare_code() {
    let said: &str = copy_core::copy_static!("rsBackendPolicy.death.consoleCtrl");
    let crashed = Death::Crashed {
        how: Outcome::Exited(-1073741510),
    };
    assert_eq!(
        last_brief(&crashed),
        crate::copy_table::copy_text(
            "rsBackendPolicy.death.brief",
            &[
                (
                    "kind",
                    copy_core::copy_static!("rsBackendPolicy.death.crashed")
                ),
                ("ended", said)
            ]
        )
    );
    let refused = Death::Refused { code: -1073741510 };
    assert_eq!(
        last_brief(&refused),
        crate::copy_table::copy_text(
            "rsBackendPolicy.death.brief",
            &[
                (
                    "kind",
                    copy_core::copy_static!("rsBackendPolicy.death.refused")
                ),
                ("ended", said)
            ]
        )
    );
    for d in [&crashed, &refused] {
        let brief = last_brief(d);
        assert!(
            !brief.contains("1073741510") && !brief.contains("exit "),
            "说了人话还把裸码带上了：{brief}"
        );
        assert_eq!(
            ui_copy_violations(&brief),
            Vec::<&str>::new(),
            "那句人话自己带了界面禁用的形状：{brief}"
        );
        // 日志那一行同一个来源：也说人话（码另起一格 `码=`）。
        assert!(
            ledger_line("o", d).contains(&format!("退出状态={said}")),
            "日志那一行没跟上：{}",
            ledger_line("o", d)
        );
    }
}

/// 邻格对照：**只认闭集里那几个码**（控制台事件 · 访问越界）。差一的码、POSIX 的码说「原因不明」，
/// 裸码只进 [`exit_code`]（死亡账那一行 ＋ 复制详情）—— 防「一律换成同一句人话」那种写法让上一条也绿。
#[test]
fn only_the_known_codes_get_words_the_neighbours_say_unknown() {
    let av = Death::Crashed {
        how: Outcome::Exited(-1073741819),
    };
    assert_eq!(
        exit_status(&av),
        copy_text("rsBackendPolicy.death.accessViolation", &[])
    );
    assert_eq!(exit_code(&av).as_deref(), Some("-1073741819 (0xC0000005)"));
    for code in [-1073741509, -1073741511, -1073741818, 1, 2, 255] {
        let d = Death::Crashed {
            how: Outcome::Exited(code),
        };
        let unknown = copy_text("reason.io.unknown", &[]);
        assert_eq!(exit_status(&d), unknown, "码 {code} 被说成了别的");
        assert_eq!(
            exit_status(&Death::Refused { code }),
            unknown,
            "被拒那一臂的码 {code} 被说成了别的"
        );
        let raw = exit_code(&d).expect("有码却没交出码");
        assert!(raw.starts_with(&code.to_string()), "码 {code} 交成了 {raw}");
    }
    let sig = Death::Crashed {
        how: Outcome::Signalled(9),
    };
    // 「signal N」那一截也走文案表（不在代码里写死英文）。
    assert_eq!(
        exit_status(&sig),
        crate::copy_table::copy_text("rsBackendPolicy.status.signal", &[("sig", "9")])
    );
    assert_eq!(exit_status(&sig), "信号 9");
}
