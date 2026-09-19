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

// ===== 真实脏数据：目录名含换行，把一条记录劈成 2+3 字段两行 =====
#[test]
fn survives_embedded_newline_in_dir_real_sample() {
    let text = "good_cc\t/tmp/a\t2026-07-18T19:00:00-07:00\t任务\n\
                    x_y_cc\t/tmp/tmp.o6LGcLq9Qq/x\n\
                    y\t2026-07-18T19:29:07-07:00\t\n\
                    other_cc\t/tmp/b\t2026-07-18T20:00:00-07:00\t\n";
    let (rows, skipped) = parse_spawned_tsv(text);
    // 两条好行必须活下来——坏行不能连累它们
    assert_eq!(rows.len(), 2, "好行应全部解出，实得 {rows:?}");
    assert_eq!(rows[0].id, "good_cc");
    assert_eq!(rows[1].id, "other_cc");
    assert_eq!(skipped, 2, "两条畸形行应被计数");
}

// ===== 真实脏数据：任务文本含换行，产生 0/1 字段行 =====
#[test]
fn survives_multiline_task_text_real_sample() {
    let text = "a_cc\t/tmp/a\t2026-07-18T19:00:00-07:00\t背景:android-terminal\n\
                    (aterm,手机 SSH 终端 App)这边准备接进\n\
                    \n\
                    b_cc\t/tmp/b\t2026-07-18T21:00:00-07:00\t\n";
    let (rows, skipped) = parse_spawned_tsv(text);
    assert_eq!(rows.len(), 2);
    assert_eq!(skipped, 1, "空行不计入 skipped，只有那一条有内容的坏行算");
}

#[test]
fn many_bad_lines_still_yield_all_good_rows() {
    // 坏行很多时也不能整体失败。（原名叫 "majority"，但真实盘面是 5/15=33%，
    // 并非多数派——名字与事实不符会误导后来人，已改名。这里构造 8 条纯属压力形态。）
    let mut text = String::new();
    for i in 0..7 {
        text.push_str(&format!(
            "ok{i}_cc\t/tmp/{i}\t2026-07-18T19:00:00-07:00\tt\n"
        ));
    }
    for i in 0..8 {
        text.push_str(&format!("broken{i}\n"));
    }
    let (rows, skipped) = parse_spawned_tsv(&text);
    assert_eq!(rows.len(), 7);
    assert_eq!(skipped, 8);
}

#[test]
fn garbage_id_rows_are_skipped_not_rendered() {
    let text = "--help\t/tmp/x\t2026-07-18T19:00:00-07:00\tt\n\
                    good_cc\t/tmp/y\t2026-07-18T19:00:00-07:00\tt\n";
    let (rows, skipped) = parse_spawned_tsv(text);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "good_cc");
    assert_eq!(skipped, 1, "--help 这行必须被跳过，不能渲染进驾驶舱");
}

// ===== agents.tsv：结构干净，但校验仍要在 =====
#[test]
fn parses_clean_agents_tsv() {
    let text = "cc-9d66c46d\tcc-9d66c46d:0.0\t2026-07-28T11:48:32-07:00\n\
                    KVM_cc\tKVM_cc:0.0\t2026-07-18T07:26:31-07:00\n";
    let (rows, skipped) = parse_agents_tsv(text);
    assert_eq!(rows.len(), 2);
    assert_eq!(skipped, 0);
    assert_eq!(rows[1].registered_at, "2026-07-18T07:26:31-07:00");
}

#[test]
fn bad_timestamp_does_not_drop_the_row() {
    // 时间戳坏掉不该让这个 agent 从驾驶舱消失——UI 标"时间未知"即可
    let text = "a_cc\ta_cc:0.0\tnot-a-timestamp\n";
    let (rows, skipped) = parse_agents_tsv(text);
    assert_eq!(rows.len(), 1);
    assert_eq!(skipped, 0);
    assert_eq!(rows[0].registered_at, "not-a-timestamp");
}

#[test]
fn empty_and_missing_input_are_not_errors() {
    assert_eq!(parse_agents_tsv(""), (vec![], 0));
    assert_eq!(parse_spawned_tsv("\n\n\n"), (vec![], 0));
}

// ===== 合并读回的切分 =====
#[test]
fn splits_combined_payload() {
    let raw = format!("AGENTS\n{CC_BUS_SPLIT_MARKER}\nSPAWNED\n");
    let (a, b) = split_combined(&raw, CC_BUS_SPLIT_MARKER);
    assert_eq!(a.trim(), "AGENTS");
    assert_eq!(b.trim(), "SPAWNED");
}

#[test]
fn missing_marker_degrades_gracefully() {
    let (a, b) = split_combined("only one file", CC_BUS_SPLIT_MARKER);
    assert_eq!(a, "only one file");
    assert_eq!(b, "");
}

/// 造一份自述头。测试自己拼，是为了让下面每一格喂的**逐字**是什么一眼看得见。
fn head_line(home: u8, cat: u8) -> String {
    format!("{CC_BUS_HEAD_MARKER} home={home} cat={cat}\n")
}

/// 🔴🔴 **本件的正题判据：「读不到」不许长成「一个都没有」。**
///
/// # 它为什么存在（**云端有读数，不是设想**）
///
/// 09-10 云端 `windows-latest` 那趟 `cargo test` 1323 过 1 红，红的那条报的是
/// 「`bash -lc 'sleep 2.8416'` 只花了 120.7456ms 就回来了」——**那个壳里外部命令跑不起来**。
/// 而 `CC_BUS_CAT_CMD` 的主体正是两次 `cat`，归同一个 `PATH`。
///
/// 本机现打（`bash -lc`，两种情形各跑一趟，`cmp` 逐字节比）：
/// · `cat` 缺席、两张表**真的在** → stdout 逐字 `"\n@@CCMON-CCBUS-SPLIT@@\n"`，rc=0；
/// · `cat` 在、`$CC_BUS_HOME` **整个不存在** → stdout **一模一样**，rc=0。
/// ⇒ 修之前，这两件事在解析器眼里是同一份输入 ⇒ 驾驶舱两次都写
/// 「这台机器上没有登记过的 cc-bus agent」。**后者对，前者是假话。**
///
/// # 🔴 铁律 12：**修之前它一定红** —— 静态推演（跑不了测试，所以逐格推）
///
/// 把 [`interpret_cc_bus_read`] 换回本件之前那三行
/// （`split_combined` → `parse_agents_tsv` → `parse_spawned_tsv`，出口是裸 `CcBusState`），
/// 喂下面 ② 那格的输入 `"\n@@CCMON-CCBUS-SPLIT@@\n"`：
/// 1. `split_combined` 命中标记 ⇒ `a == "\n"`、`s == "\n"`；
/// 2. `parse_agents_tsv("\n")`：`text.lines()` 给出一行 `""`，`row_fields` 判
///    「`trim().is_empty()` 且不含 `\t`」⇒ `None` ⇒ `continue` ⇒ `(vec![], 0)`；
/// 3. `parse_spawned_tsv("\n")` 同理；
/// 4. 出口 `CcBusState { agents: [], spawned: [], skipped: 0 }` —— **是 `Ok` 不是 `Err`**
///    ⇒ ② 那格的 `is_err()` **红**。①③⑥ 同理（都拿得到一个空 `CcBusState`）。
/// ⇒ 本判据**不是恒绿**，它逐格咬住的正是被修掉的那个行为。
///
/// # 反向：**不许恒红**
///
/// ④「真的没装」与 ⑤「真有 agent」两格要求 `Ok`，且 ⑤ 要求数得出行 ——
/// 一个「凡是零个就报错」的糊涂修法会在 ④ 红。这两格是本判据的非空对照。
#[test]
fn a_read_that_could_not_happen_is_never_rendered_as_an_empty_roster() {
    // ① 那条命令根本没跑起来（`bash` 是个存根 / 壳不认这段语法）⇒ 空串
    let e1 = interpret_cc_bus_read("<local>", "").expect_err("空输出必须是错，不是零个");
    assert!(e1.contains("没有跑起来"), "错误没说清是哪一步：{e1}");

    // ② **今天生产上 `cat` 缺席时逐字节的输出**（本机实测过的那一串）
    let e2 = interpret_cc_bus_read("<local>", "\n@@CCMON-CCBUS-SPLIT@@\n")
        .expect_err("只有分隔标记 = 自述头没回来 = 读不到");
    assert!(e2.contains("没有跑起来"), "{e2}");

    // ③ 头回来了，而它自己说这个壳里没有 `cat`
    let raw3 = format!("{}\n{CC_BUS_SPLIT_MARKER}\n", head_line(1, 0));
    let e3 = interpret_cc_bus_read("<local>", &raw3).expect_err("cat=0 必须是错");
    assert!(e3.contains("没有 `cat`"), "{e3}");
    assert!(
        e3.contains("不是"),
        "错误里必须写明它不是「一个 agent 都没有」，否则用户读到的还是同一句：{e3}"
    );

    // ④ **非空对照**：cc-bus 真的没装 —— 这是一种**合法状态**，不许顺手判成错。
    let raw4 = format!("{}\n{CC_BUS_SPLIT_MARKER}\n", head_line(0, 1));
    let st4 = interpret_cc_bus_read("<local>", &raw4).expect("没装是状态不是错误");
    assert_eq!(st4.agents.len(), 0);
    assert_eq!(st4.spawned.len(), 0);
    assert_eq!(st4.skipped, 0, "自述头不许被当成坏行计进 skipped");

    // ⑤ **非空对照**：真读到了 —— 证明这把尺子不是恒红，且头被干净地摘掉了。
    let raw5 = format!(
        "{}a_cc\ta_cc:0.0\t2026-01-01\n{CC_BUS_SPLIT_MARKER}\nb_cc\t/d\t2026-01-01\ttask\n",
        head_line(1, 1)
    );
    let st5 = interpret_cc_bus_read("<local>", &raw5).expect("正常读回不该报错");
    assert_eq!(st5.agents.len(), 1, "头没摘干净或行被吃了");
    assert_eq!(st5.agents[0].id, "a_cc");
    assert_eq!(st5.spawned.len(), 1);
    assert_eq!(st5.skipped, 0, "自述头串进了解析器");

    // ⑥ 只回来半份（流被掐 / 输出被截）⇒ 拒收，不拿半份清单当完整的用。
    let raw6 = format!("{}a_cc\ta_cc:0.0\t2026-01-01\n", head_line(1, 1));
    let e6 = interpret_cc_bus_read("<local>", &raw6).expect_err("半份必须拒收");
    assert!(e6.contains("半份"), "{e6}");

    // ⑦ 头在、但字段读不出来 ⇒ 整个头作废，**不许悄悄当成 `false`/`true`**。
    let bad_head = format!("{CC_BUS_HEAD_MARKER} home=? cat=1");
    let raw7 = format!("{bad_head}\n\n{CC_BUS_SPLIT_MARKER}\n");
    assert!(
        interpret_cc_bus_read("<local>", &raw7).is_err(),
        "读不懂的自述头被当成了一次成功的读"
    );
}

/// ★ **断言落在使用处**：`read_cc_bus_state` 必须**经** [`interpret_cc_bus_read`] 出结果。
///
/// ⚠ 这条是上面那条的搭档，缺了它上面那条就守不住 —— 本仓有过逐字的先例：
/// `build_online_cmd` 抽成纯函数、判据打在纯函数上，而生产调用点**内联复制了一份**，
/// 于是「删掉真正在跑的那句校验，整套测试照样全绿」（`check_cc_bus_agent_online` 头注
/// 逐字记着这次）。本条钉的是**接线**：命令函数里不许自己解析。
///
/// # 🔴 铁律 12：修之前它一定红（静态推演）
///
/// 本件之前，`read_cc_bus_state` 的函数体逐字含
/// `let (a, s) = split_combined(&raw, CC_BUS_SPLIT_MARKER);` 与两个 `parse_*_tsv(`，
/// 且**不含** `interpret_cc_bus_read(` ⇒ 下面第一条 `assert!(body.contains(...))` 直接红。
/// 反向变异（把解析搬回命令函数里）⇒ 第二、三条红。
#[test]
fn the_cockpit_never_renders_an_unread_roster_as_empty() {
    let code = non_test_code();
    assert_eq!(
        code.matches("fn interpret_cc_bus_read(").count(),
        1,
        "解释点不是恰好一处 —— 两条传输路一旦各解释各的，口径立刻会漂"
    );
    let at = code
        .find("pub async fn read_cc_bus_state(")
        .expect("生产段找不到读状态命令 —— 判据在空转");
    let rest = &code[at..];
    let end = rest[1..]
        .find("\npub ")
        .map(|k| k + 1)
        .unwrap_or_else(|| rest.len().min(1600));
    let body = &rest[..end];
    // 窗口自检：取错窗口的话下面三条会变成空真（本仓这两条判据自己栽过这个坑）。
    assert!(
        body.contains("CC_BUS_CAT_CMD"),
        "窗口取错了 —— 里面看不见那条命令常量，下面整条是空真。实得：{body}"
    );
    assert!(
        body.contains("interpret_cc_bus_read("),
        "读状态没有经过那道「先证明这次真的读到了」的解释 —— \n\
             空输出会被解析成零个 agent，而驾驶舱把零个渲染成「这台机器上没有 agent」。"
    );
    for inline in ["split_combined(", "parse_agents_tsv(", "parse_spawned_tsv("] {
        assert!(
            !body.contains(inline),
            "`{inline}` 又被内联进了读状态命令 —— 那正好绕过自述头那一步，\n\
                 而绕过之后的症状是**全绿**：空输入解析出零个 agent，一条断言都不会红。"
        );
    }
}

#[test]
fn never_panics_on_adversarial_input() {
    for bad in [
        "\0\0\0",
        "\t\t\t\t\t",
        "a\t\t\t",
        &"x".repeat(100_000),
        "\u{feff}a_cc\tp\tt\tx",
    ] {
        let _ = parse_agents_tsv(bad);
        let _ = parse_spawned_tsv(bad);
    }
}

// ===== 定值命令：零注入面（同 mcp.rs 那条 CMD 常量的形状）=====
#[test]
fn cat_command_is_a_constant_with_no_interpolation() {
    // origin 只用于选连接配置，绝不能出现在命令串里
    assert!(!CC_BUS_CAT_CMD.contains("{}"));
    assert!(!CC_BUS_CAT_CMD.contains("$1"));
    assert!(CC_BUS_CAT_CMD.contains(CC_BUS_SPLIT_MARKER));
    // 只读：不得出现任何写操作
    for w in [
        "rm ", "mv ", "> ", ">>", "tee ", "truncate", "chmod", "kill",
    ] {
        assert!(!CC_BUS_CAT_CMD.contains(w), "定值命令里不该有写操作 {w:?}");
    }
    // 尊重 CC_BUS_HOME，且两文件缺失时仍 rc=0
    assert!(CC_BUS_CAT_CMD.contains("CC_BUS_HOME"));
    assert!(CC_BUS_CAT_CMD.trim_end().ends_with("true"));
    // ★〔ccbus-win 09-10〕自述头：**这条命令必须自己报「我读没读得了」**。
    // 没有它，`cat` 缺席与「目录根本不存在」在 stdout 上逐字节相同（本机 `cmp` 实测）。
    assert!(
        CC_BUS_CAT_CMD.contains(CC_BUS_HEAD_MARKER),
        "读命令不再自报家门 —— 「读不到」会重新长回「一个都没有」"
    );
    assert!(
        CC_BUS_CAT_CMD.contains("command -v cat"),
        "自述头不再报 `cat` 在不在 —— 那一格正是 09-10 云端红出来的那一形"
    );
    // 自述头必须由**内建**打出来：外部命令一个都没有的壳里它也得回得来，
    // 否则「头没回来」这个读数本身就随着 PATH 一起失效了。
    assert!(
        CC_BUS_CAT_CMD.contains("printf '@@CCMON-CCBUS-HEAD@@"),
        "自述头不是用内建 `printf` 打的 —— 换成外部命令之后，\
             「头没回来」这个读数会跟着 PATH 一起失效"
    );
    assert!(
        !CC_BUS_CAT_CMD.contains("which "),
        "探 `cat` 在不在用了外部的 `which` —— 它自己也可能不在 PATH 上，\
             那是拿一个问不出来的答案去回答另一个问不出来的问题（`command -v` 是内建）"
    );
}

// ★★ `K-R112`（09-13）：**这里原来住着「在线检查：id 必须先过校验才允许拼进命令」**。〔散文墓碑〕
// 它守的是「构造那条 `tmux has-session -t '=<id>:'` 之前先校验」这个顺序 ——
// 而那条命令串本件删净了（查在线改走 `bus-list`）⇒ 它守的顺序**不存在了**。
// **校验本身没有丢**：`online_via_daemon` 开场就拒非法 id，由
// `the_online_lamp_asks_the_identity_space_and_has_nothing_else_to_ask` ＋
// `an_unknown_liveness_is_never_rendered_as_dark`（走**生产入口本体**，不是扫源码）钉着。

// ===================== B03 批二 =====================
//
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
        let q = crate::ssh_source::shell_quote(evil);
        assert_eq!(
            unquote_posix(&q).as_deref(),
            Some(evil),
            "逃逸后必须能逐字还原（说明它仍是一个完整的 shell 词）: {q}"
        );
    }
}

// ===== 校验落在构造函数上（删掉任何一处校验，这些立刻红）=====
//
// ⚠〔`K-R98` 09-13〕发消息那条**不在这里了**：它没有命令构造器可言（改走 daemon 原语）。
//   那道 id 白名单**没有丢，换了住址** —— 搬进 `send_via_daemon`，
//   由 `the_send_path_asks_the_backend_instead_of_composing_a_shell_line` 按源码钉着。
#[test]
fn builders_reject_bad_ids_at_the_call_site() {
    for bad in ["--help", "-t", "a b", "a;id", "", "a'b", "a/b"] {
        // ⚠〔`K-R112` 09-13〕`build_online_cmd` 从这张表出去了 —— 它整块删了
        //   （查在线改走 `bus-list`）。那道白名单**没有丢，换了住址**：搬进
        //   `online_via_daemon`，由 `an_unknown_liveness_is_never_rendered_as_dark` 钉着。
        assert!(build_inbox_cmd(bad).is_err(), "inbox: {bad:?} 应被拒");
    }
}

#[test]
fn inbox_cmd_is_readonly_and_bounded() {
    let c = build_inbox_cmd("proj_cc").unwrap();
    assert!(c.contains("tail -n 200"), "必须有上界: {c}");
    assert!(c.contains("CC_BUS_HOME"), "须尊重 CC_BUS_HOME: {c}");
    for w in ["rm ", "mv ", ">>", "tee ", "kill"] {
        assert!(!c.contains(w), "只读命令里不该有 {w:?}: {c}");
    }
}

#[test]
fn spawn_cmd_whitelists_tool_and_quotes_paths() {
    for bad in ["bash", "claude; id", "", "CLAUDE"] {
        assert!(
            build_spawn_cmd(bad, "/tmp", "", None).is_err(),
            "tool {bad:?} 应被拒"
        );
    }
    let dir = "/tmp/has space/and'quote";
    let task = "分析; whoami";
    let c = build_spawn_cmd("codex", dir, task, None).unwrap();
    // `--` 结束选项：dir 若是 `--new` 这类词，不加它会被 cc-spawn 的旗标循环吃掉
    assert!(c.starts_with("cc-spawn --tool codex --base -- '"));
    let rest = &c["cc-spawn --tool codex --base -- ".len()..c.len() - " 2>&1".len()];
    let (qd, qt) = rest.split_at(crate::ssh_source::shell_quote(dir).len());
    assert_eq!(unquote_posix(qd).as_deref(), Some(dir));
    assert_eq!(unquote_posix(qt.trim_start()).as_deref(), Some(task));
    // 无任务时不得留下空参数
    let c2 = build_spawn_cmd("claude", "/tmp", "", None).unwrap();
    assert_eq!(c2, "cc-spawn --tool claude --base -- '/tmp' 2>&1");
    assert!(
        build_spawn_cmd("claude", "  ", "t", None).is_err(),
        "空目录应被拒"
    );
}

// ===== L2：spawn 必须显式表态用哪个账号（B03 审计重要-5）=====

/// **不传账号 = 显式用基座**，而不是"什么都不说、让 ccm 落默认号"。
/// 原实现就是后者：从驾驶舱点两下就在 manifest 默认账号上起真 agent 烧额度，
/// 用户既没选过也不知道用了哪个号。
#[test]
fn spawn_always_states_an_account_choice() {
    let c = build_spawn_cmd("claude", "/d", "", None).unwrap();
    assert!(c.contains(" --base "), "不选账号必须显式 --base，实得: {c}");
    let c2 = build_spawn_cmd("claude", "/d", "", Some("acctz")).unwrap();
    assert!(c2.contains(" --account acctz "), "选了号要转发，实得: {c2}");
    // 两者互斥：命令里不得同时出现
    assert!(!c2.contains("--base"));
    assert!(!c.contains("--account"));
}

/// 账号名来自 manifest（我们自己维护），但**仍要过字符集**——
/// B03 审计的 `--help` 教训：盘上真会出现没人预料的 id，
/// "这是我们自己的数据"不是免检理由。
#[test]
fn account_name_is_validated_before_joining_the_command() {
    for bad in ["--base", "-x", "a b", "a;id", "", "a'b", "a/b", "$(id)"] {
        assert!(
            build_spawn_cmd("claude", "/d", "", Some(bad)).is_err(),
            "账号名 {bad:?} 必须被拒"
        );
    }
    for ok in ["z", "acct_b", "team-1", "A9"] {
        assert!(
            build_spawn_cmd("claude", "/d", "", Some(ok)).is_ok(),
            "{ok} 应合法"
        );
    }
}

// ===== inbox 解析同样守"坏行跳过并计数" =====
#[test]
fn parses_real_inbox_line() {
    let l = r#"{"id":"KVM_cc-178-31346","from":"KVM_cc","to":"cc-9d66c46d","ts":"2026-07-26T05:06:19-07:00","text":"【告知】A 大半就绪","class":"direct","hops":"1"}"#;
    let (m, sk) = parse_inbox_jsonl(l);
    assert_eq!(sk, 0);
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].from, "KVM_cc");
    assert_eq!(m[0].class, "direct");
    assert_eq!(m[0].text, "【告知】A 大半就绪");
}

#[test]
fn inbox_bad_lines_skipped_not_fatal() {
    let text = concat!(
        r#"{"from":"a","text":"ok1","ts":"t","class":"direct"}"#,
        "\n这不是 json\n\n",
        r#"{"from":"b","text":"ok2","ts":"t","class":"broadcast"}"#,
        "\n",
        r#"{"nothing":"useful"}"#,
        "\n"
    );
    let (m, sk) = parse_inbox_jsonl(text);
    assert_eq!(m.len(), 2, "好行必须全解出，实得 {m:?}");
    assert_eq!(sk, 2, "坏 json + 无有效字段各一条；空行不计");
}

// ===== B03 审计逼出来的补漏 =====

/// ★★ `K-R112`（09-13）：**阻塞-2 那条守卫翻了面 —— 它现在钉「那条命令串一处都没有」。**
///
/// 原文钉的是「在线检查**真的经过** `build_online_cmd`」（构造只有一份）。
/// 本件把查在线整条改走 `bus-list` ⇒ 那个构造器删了，**原判据的参照物不存在了**。
/// 照字面留着它只有一种活法：把它放宽成「有没有那个词」——那是本区判过多次的假绿。
/// ⇒ **同一处翻成反向锚点**：生产段里 `tmux has-session -t` 这个模板**恰好 0 次**。
/// 谁哪天在任何地方重新拼一条按名字探在线的串，这一条当场红。
#[test]
fn the_probe_by_name_template_is_gone_from_production() {
    let code = non_test_code();
    assert!(code.contains("pub async fn check_cc_bus_agent_online"));
    assert_eq!(
        code.matches("tmux has-session -t").count(),
        0,
        "生产段又出现了按名字探在线的命令串 —— 那正是 `P4f` 要治的病\n\
             （名字会被重用：敲门打进陌生人屏幕 · 「收掉 agent」杀了无辜进程）。\n\
             在线三态归 `bus-list`（登记 + 身份核过），不许在旁边再长一条按名字的路。"
    );
    // 反向自检：抽取器还够得着东西（不然上面那个 0 是空真）。
    assert!(
        code.contains("online_via_daemon"),
        "生产段连 `online_via_daemon` 都找不到 —— 抽取器坏了，上面那个 0 不作数"
    );
}

/// 取本文件的**非测试、非注释**代码。
/// **扫源码的守卫必须先剥注释**——本轮我有两条守卫栽在这上面：一条把文档注释里提到的
/// 命令名也数进去（3 != 1），另一条把错误消息里的 `format!` 当成命令构造。
/// 守卫扫错东西 = 假红，和恒绿一样坏。
/// 抠出一段源码里所有普通字符串字面量（**保留源码形态**：`\"` 与 `{{` 原样留着，
/// 这样拿它回头在源码里数出现次数才对得上）。
fn string_literals(body: &str) -> Vec<String> {
    let b: Vec<char> = body.chars().collect();
    let (mut out, mut i) = (Vec::new(), 0usize);
    while i < b.len() {
        if b[i] == '"' {
            let (mut j, mut lit) = (i + 1, String::new());
            while j < b.len() && b[j] != '"' {
                if b[j] == '\\' && j + 1 < b.len() {
                    lit.push(b[j]);
                    lit.push(b[j + 1]);
                    j += 2;
                    continue;
                }
                lit.push(b[j]);
                j += 1;
            }
            out.push(lit);
            i = j + 1;
            continue;
        }
        i += 1;
    }
    out
}

/// 一个 `format!` 模板里**最长的静态片段** —— 占位符 `{…}` 是变的，静态片段才是
/// 「这条命令长什么样」。`{{` / `}}` 是转义的花括号，算静态。
fn longest_static_run(lit: &str) -> String {
    let c: Vec<char> = lit.chars().collect();
    let (mut best, mut cur, mut i) = (String::new(), String::new(), 0usize);
    while i < c.len() {
        if c[i] == '{' && i + 1 < c.len() && c[i + 1] == '{' {
            cur.push_str("{{");
            i += 2;
            continue;
        }
        if c[i] == '}' && i + 1 < c.len() && c[i + 1] == '}' {
            cur.push_str("}}");
            i += 2;
            continue;
        }
        if c[i] == '{' {
            if cur.chars().count() > best.chars().count() {
                best = cur.clone();
            }
            cur.clear();
            while i < c.len() && c[i] != '}' {
                i += 1;
            }
            i += 1;
            continue;
        }
        cur.push(c[i]);
        i += 1;
    }
    if cur.chars().count() > best.chars().count() {
        best = cur;
    }
    best
}

/// 取 `fn <name>` 的函数体：从签名那行起，**到下一个顶格行为止**。
///
/// ⚠ 第一版写成「到下一个顶格 `fn ` 为止」，于是 `build_spawn_cmd` 的体一路吃到了
/// 它下面那个 struct 的属性里，把 `"../../src/generated"` 当成了命令模板（出现 4 次）。
/// **同一族的错第 N 次**：我以为的那个对象，与切片实际圈住的那个对象不是同一个。
/// 顶格行 = 函数自己的收尾行，或下一个顶层项 —— 两者都是正确的边界。
///
/// ⚠ 刻意**不写花括号字面量**来找收尾：本文件会被 `production_code` 一族按括号配平剥，
/// 而一个落单的右花括号会打坏那个配平（本工作区真踩过一次，红了整轮）。
fn fn_body(code: &str, name: &str) -> String {
    let start = code
        .find(&format!("fn {name}"))
        .unwrap_or_else(|| panic!("生产段里没有 fn {name} —— 抽取器坏了"));
    let mut out = Vec::new();
    for (i, line) in code[start..].lines().enumerate() {
        // ⚠ 多行签名的收尾行 `) -> Result<…> {` 也顶格 —— 它是**头的一部分**，
        // 不是边界。第一版漏了这条，`build_spawn_cmd` 的体被切在签名处、抠出空串
        // （长度自检当场报出来了 —— 自检存在的意义就在这里）。
        let top_level = !line.is_empty()
            && !line.starts_with(char::is_whitespace)
            && !line.starts_with(')');
        if i > 0 && top_level {
            break;
        }
        out.push(line);
    }
    out.join("\n")
}

/// ★★ **每条远端命令模板都只准构造一处**〔audit-0805 08-07，Phase G 第 39 件〕。
///
/// # 它补的是一个「只挡住了自己那一条」的 singleton
///
/// 隔壁 `the_probe_by_name_template_is_gone_from_production` 钉的是
/// **`tmux has-session -t` 这个字面量的处数** —— 那条是对的，
/// 但它的人群是**当初出事的那一条路**（阻塞-2：有人内联复制了一份在线检查）。
/// 另外几个构造器（inbox / spawn / 广播 / 收掉）**一个都没被这条性质覆盖**。
///
/// 08-07 实测：在生产段内联一份
/// `format!("cc-send {id} {text} 2>&1")`（绕开发消息那个构造器的 id 白名单
/// **与** `shell_quote` 引用），全仓 **973 条判据一条都不红**。
/// 而那条路把**任意用户文本**送进远端 shell —— 它曾是本模块里赌注最高的一条。
///
/// ⚠〔`K-R98` 09-13〕**那条路今天整个不在了**：发消息改走 daemon 的 `bus-send` 原语，
/// 连同它的 shell 构造器一起删净 ⇒ 本条的人群从 6 个降到 5 个。
/// 「这条路上还有没有拼出来的命令串」由
/// `the_send_path_asks_the_backend_instead_of_composing_a_shell_line` 接着守，
/// **判的是那条路，不是某个符号在不在**。
///
/// # 人群从构造器本身派生
///
/// 不手写模板清单（手写清单就是下一个「只挡住我列的那几条」）：
/// 扫出所有 `fn build_*_cmd`，从每个的字符串字面量里取**最长静态片段**
/// （占位符是变的，静态片段才是「这条命令长什么样」），要求它在生产段恰好出现一次。
#[test]
fn every_remote_command_template_is_built_in_exactly_one_place() {
    let code = non_test_code();
    // 人群 = 生产段里所有命令构造器（派生，不是手写清单）。
    let names: Vec<String> = code
        .match_indices("fn build_")
        .map(|(i, _)| {
            code[i + 3..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect::<String>()
        })
        .collect();
    // ★ 抽取器自检：抓不到构造器时下面整条空转。
    assert!(
        names.len() >= 2,
        "生产段只找到 {} 个 `build_*_cmd`（**`K-R112` 09-13 现打 2：inbox / spawn**；\
             此前是 5，online/broadcast/kill 三个随本件那三条改走 daemon 原语整块删了；\
             再往前 08-07 是 4，发消息那个由 `K-R98` 删净）\
             —— 抽取器坏了或构造器改名了，本条此刻无效：{names:?}",
        names.len()
    );

    for name in &names {
        let body = fn_body(&code, name);
        let body = body.as_str();
        // 两道语义过滤，否则挑中的是**错误消息**而不是命令模板：
        // ① 跳过错误路径那几行（这几个构造器最长的字面量其实是那句
        //    「非法 agent id（拒绝拼入命令）」，好几个构造器共用 ⇒ 出现不止 1 次；
        //    这一版第一次跑就被自己逮出来了）；
        // ② 命令模板要送进**远端 shell**，必然是 ASCII —— 带中文的一定不是它。
        // ⚠ 两道都只会把候选**变少**：过滤过头 ⇒ 下面那条长度自检当场红（不是静默变绿）。
        let prod_lines: String = body
            .lines()
            .filter(|l| !l.contains("Err("))
            .collect::<Vec<_>>()
            .join("\n");
        let run = string_literals(&prod_lines)
            .iter()
            .map(|l| longest_static_run(l))
            .filter(|s| s.is_ascii())
            .max_by_key(|s| s.chars().count())
            .unwrap_or_default();
        // 自检：片段太短就不足以标识一条命令，本条对它是空转。
        assert!(
            run.chars().count() >= 8,
            "`{name}` 里抠不出足够长的命令静态片段（实得 {:?}）—— \
                 要么它不再用 `format!` 拼命令，要么抽取器坏了。两种都要人来看一眼。",
            run
        );
        assert_eq!(
            code.matches(run.as_str()).count(),
            1,
            "命令模板 {run:?}（属于 `{name}`）在生产段出现了 {} 次，应当恰好 1 次。\n\
                 多出来的那处 = **又内联复制了一份命令构造**，而复制品不会带上构造器里的\n\
                 那几道防线（id 白名单 / `shell_quote` 引用 / 空值拒绝）。\n\
                 ⚠ 这正是阻塞-2 的形状，只是当时只在 `build_online_cmd` 那一条上补了判据。\n\
                 修法是**调用 `{name}`**，不是把这条判据放宽。",
            code.matches(run.as_str()).count()
        );
    }
}

fn non_test_code() -> String {
    let src = include_str!("../../src/bridge/src/cc_bus.rs");
    let code = src.split(concat!("#[cfg", "(test)]")).next().unwrap_or(src);
    code.lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//") && !t.starts_with('*') && !t.starts_with("/*")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn non_test_code_helper_is_sane() {
    // 守住这个助手本身：剥过头（剥成空）或没剥干净，上下两条守卫就都成了摆设
    let c = non_test_code();
    assert!(
        c.contains("pub async fn check_cc_bus_agent_online"),
        "剥过头了"
    );
    // ⚠〔`K-R112` 09-13〕锚点从 `build_online_cmd` 换成 `build_inbox_cmd` ——
    //   前者整块删了（查在线改走 `bus-list`）。换的是**参照物**，不是口径。
    assert!(c.contains("fn build_inbox_cmd"), "剥过头了");
    assert!(!c.contains("阻塞-2 原样复发"), "注释没剥干净");
    assert!(c.len() > 2000, "剩下的代码太少，守卫形同虚设");
}

/// **重要-2 的守卫**：`parse_agents_tsv` 的 id 校验此前**没有会红的断言**
/// （`never_panics_on_adversarial_input` 用 `let _ =` 丢结果，只守 panic 不守语义）。
/// 对照 `parse_spawned_tsv` 有 `garbage_id_rows_are_skipped_not_rendered` 守着——
/// 两个同构解析器只守了一个。
#[test]
fn agents_garbage_id_rows_are_skipped_not_rendered() {
    let text = "--help\thelp:0.0\t2026-07-18T07:26:31-07:00\n\
                    good_cc\tgood_cc:0.0\t2026-07-28T11:48:32-07:00\n";
    let (rows, skipped) = parse_agents_tsv(text);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "good_cc");
    assert_eq!(skipped, 1, "--help 这行必须被跳过并计数");
}

/// **重要-4a**：只含制表符的行有结构无内容 → 该算坏行，不该凭空蒸发。
#[test]
fn tab_only_line_counts_as_bad_not_vanished() {
    let (rows, skipped) = parse_spawned_tsv("\t\t\t\n");
    assert_eq!(rows.len(), 0);
    assert_eq!(skipped, 1, "有结构无内容的行必须计入 skipped");
    // 真空行仍然不计（这是既有契约，别修坏）
    let (_, sk2) = parse_spawned_tsv("\n\n   \n");
    assert_eq!(sk2, 0, "真空行不计入 skipped，否则 UI 虚报");
}

/// **重要-4b**：任务文本里有制表符时，末字段要把余下的都收回来，不能静默截断。
#[test]
fn task_with_tabs_is_not_silently_truncated() {
    let (rows, _) = parse_spawned_tsv("a_cc\t/d\t2026\tpart1\tpart2\tpart3\n");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].task, "part1\tpart2\tpart3", "多余字段不得丢");
}

/// ★ P4a-Y1：**本机那条跑的是同一条串** —— 命令只有一个构造点。
///
/// 钉「有本机分支」很容易，钉不住「它跑的是同一条串」：本机臂里自己拼一句
/// `cat ~/.cc-bus/agents.tsv` 照样绿，而那就是**第二份文件布局知识**，
/// 两份会各自漂（这个仓管这叫「一段逻辑、两种表示」）。
///
/// ⇒ 判据钉**构造点唯一**：三条读面命令各自的构造器在生产段只准出现一次
/// （定义处不算），且生产段不许长出新的 `.tsv` 字面量。
#[test]
fn the_local_read_path_runs_the_very_same_command_string() {
    let code = non_test_code();
    // ⚠〔`K-R112` 09-13〕`build_online_cmd(&id)` 出表 —— 查在线整条改走 `bus-list`，
    //   本机与远端**共用同一个函数体**（origin 是入参），比「同一条命令串」更强一格。
    for builder in ["build_inbox_cmd(&id)"] {
        assert_eq!(
            code.matches(builder).count(),
            1,
            "`{builder}` 在生产段出现了不止一次 —— 多半是本机臂自己又构了一条命令。\n\
                 本机与远端必须用**同一个 `cmd`**：构造在分支之前，分支只决定谁来跑它。"
        );
    }
    // `.tsv` 只准出现在 `CC_BUS_CAT_CMD` 那个常量里（两次：agents / spawned）。
    assert_eq!(
        code.matches(".tsv").count(),
        2,
        "生产段出现了新的 `.tsv` 字面量 —— cc-bus 的文件布局只准有一份表示，\n\
             它住在 `CC_BUS_CAT_CMD` 里。本机要读同一批文件，就跑同一条串。"
    );
}

/// ★ D 阶段补审：**超时之后，那个子进程还在不在。**
///
/// `local_shell_read` 的显式 `start_kill()` 只在**成功路径**上。超时那条路是
/// `tokio::time::timeout` 把整个 future 丢掉 —— 而 tokio 的 `Child`
/// **默认不因句柄被 drop 而杀掉子进程**（全仓 `kill_on_drop` 命中曾是 0）。
///
/// ⇒ 一次超时留一个孤儿 `bash`。这与本会话在 `#60` 上栽的那次同族：
/// **我自己留下的孤儿进程**把后面八轮实测全带偏了。那次的教训不是「以后小心」，
/// 是「留一条判据去数它」。
///
/// 本条真起进程、真等超时（阳性对照 ~0.2s ＋ 超时 ~1.4s）——
/// 起的是一个 shell 内建的空转循环，不是任何会烧额度的东西。
///
/// # 🔴 夹具换掉了 `sleep`，为什么〔ccbus-win 09-10〕
///
/// 09-09 那版用 `sleep 30.<pid>` 当阻塞体，并加了一格「先证明 `sleep` 真的阻塞」的自检。
/// **那一格 09-10 在云端 `windows-latest` 打中了**：`bash -lc 'sleep 2.8416'`
/// 只花了 120.7456ms 就回来，且 `local_shell_read` 回的是 `Ok` ——
/// 即 `bash` **起得来**，但那条 `sleep` 没有阻塞。
///
/// ⇒ 那版夹具的阻塞性**是环境的函数**（`sleep` 是外部命令，要 PATH 上有它）。
/// 本版换成 `while :; do : <marker>; done`：`while` / `:` 全是 **shell 内建**，
/// 阻塞与否只取决于 bash 的语义，不取决于这台机器上装了什么。
/// 本机实测（`/bin/bash -lc`）：进程存活、`pgrep -fc <marker>` 数到 **1**、
/// `/proc/<pid>/cmdline` 逐字是 `/bin/bash -lc while :; do : <marker>; done`
/// —— 即 bash **不会** exec 掉自己（`while` 不是 simple command），marker 留在 argv 上。
///
/// # 剩下的那一格自检：**阳性对照**
///
/// 内建阻塞体唯一会「不阻塞」的情形是 **bash 根本没跑我们这段脚本**
/// （比如 `bash` 解析到的是 `C:\Windows\System32\bash.exe` 那个没装发行版的 WSL 存根）。
/// 那一形用一条纯内建的 `printf` 探针直接量出来 —— 它同时是**产品面**的读数：
/// `CC_BUS_CAT_CMD` 走的是同一个 `local_shell_read`、同一个 `bash -lc`。
///
/// ⚠ **诚实边界**：这一格红 = 这台机器上本机 cc-bus 的读面根本用不了。
/// 那时本条守的性质（超时不漏工作进程）**在这台机器上没人守** —— 这是事实，不是可以关掉的理由。
///
/// ⚠ 空转循环会占满一个核约 1.4 秒。换来的是「阻塞性不再是环境的函数」，值这个价。
///
/// # 🔴🔴 每一格自带上限，因为**挂死比红更坏**〔ccbus-win 09-10 第四拍〕
///
/// 云端 run `34462442459`（`32d527c`，windows runner）逐字只留下两行有用的：
/// ```text
/// 09:52:09  test cc_bus::tests::a_timed_out_local_read_does_not_leave_an_orphan_behind
///           has been running for over 60 seconds
/// 10:17:16  ##[error]The operation was canceled.
/// ```
/// **跑了 25 分钟没回来，是人工取消的**（同一台机器上一趟整个 `cargo test` 只用 4m26s）。
/// 一条红的测试会告诉你哪里坏了；一条挂死的测试**什么都不说**，还把后面全部拖住。
///
/// ⇒ 本条改成**逐格设限**：每一格自己带上限，超了带着**格号**炸。
/// 病在哪当时没量到，所以这里**不猜**，只让下一趟能自己说出来。
///
/// ⚠ **逐格设限盖不住什么**：`tokio::time::timeout` 只约束 **await 点**。
/// 某一格里若是一个**同步**调用卡住（`Command::spawn` 自己 / `Path::exists`），
/// current_thread 运行时上的计时器根本没机会跑。
///
/// # 🔴🔴 那句「万一还挂，那本身就是一个读数」**兑现了**〔第五拍〕
///
/// 云端 run `34468962797`（`4016d6a`）：`fmt`/`clippy` 全过、同一个 binary 里
/// **别的测试 11:08:26 就全跑完了**，而本条 11:08:09 起、到 11:35:08 作业闸掐断 ——
/// **独自跑了 27 分钟，零输出，`【格N】` 一个都没印出来**。
/// ⇒ 没有任何一格的上限炸过 ⇒ **卡的是同步调用，不是 await**。
///
/// ⇒ 本拍改成**整条跑在裸线程上、由主线程 `recv_timeout` 收一个结论**
/// （形状抄 [`bounded`] —— 那是这几趟里唯一没被卡住的形状，不是巧合）。
/// **主线程只等一个 `Result`，它不可能被子线程里的任何同步调用拖住** ⇒
/// 无论卡在哪，本条都会在有界时间内给出**一个读数**：绿、红、或者
/// 「整条超过 N 秒没回来」。
///
/// ⚠ **为什么不是换 `flavor = "multi_thread"`**：那只能救「同步调用卡住」这一形，
/// 救不了**运行时析构**那一形 —— Windows 上子进程 stdio 走 tokio 的 blocking 池，
/// 而运行时 drop 时会等正在跑的 blocking 任务。裸线程这条把 `Runtime` 的**析构也包在
/// 上限里面**，两形一起兜住。
///
/// ⚠ 一条挂死的测试**不只是自己没读数**：它把同一趟里其余所有读数一起吃掉
/// （那三趟里「生成物必须最新」与 vendor 那格 `cargo test` 一次都没跑到）。
#[test]
fn a_timed_out_local_read_does_not_leave_an_orphan_behind() {
    // 🔴 **整条的硬上限**。四格各自的上限加起来最坏 ~130s，这里给 210s 的外框：
    //    外框先炸就说明卡在四格**之外**（解析 bash / spawn / 运行时析构）。
    const TOTAL_SECS: u64 = 210;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("ccbus-orphan-judge".to_string())
        .spawn(move || {
            let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("起不了 tokio 运行时");
                // ★★ **声明在 `rt` 之后**〔第六拍〕：局部变量逆序析构 ⇒ 它正好在
                //    `rt` 析构的**前一刻**跑，而且**正常返回与 unwind 两条路都会走到**。
                //    上一拍那句显式面包屑只在正常返回那条路上 —— 09-10 那趟走的是 panic 路，
                //    于是「断言之后花了多久」整段无从判读。
                // ⚠ `rt` 的析构必须留在这条硬上限**里面**：Windows 上子进程 stdio 走
                //    tokio 的 blocking 池，而运行时 drop 会等正在跑的 blocking 任务。
                let _crumb = DropCrumb;
                rt.block_on(orphan_judge_body());
                breadcrumb("四格全过（正文没有 panic）");
            }));
            breadcrumb("判据线程收尾（运行时已析构完）");
            let _ = tx.send(out);
        })
        .expect("起不了判据线程");
    match rx.recv_timeout(std::time::Duration::from_secs(TOTAL_SECS)) {
        Ok(Ok(())) => {}
        // 子线程里的 panic 原样抬回来 —— 失败消息与从前逐字一致。
        Ok(Err(payload)) => std::panic::resume_unwind(payload),
        Err(e) => panic!(
            "整条判据超过 {TOTAL_SECS}s 没回来（{e:?}）。\n\
                 🔴 **这是硬上限炸的，不是被测性质失败** —— 别读成「超时漏了工作进程」。\n\
                 ⇒ 去日志里找 `[ccbus-orphan]` 那几行面包屑：**最后印出来的那一行\n\
                 就是它走到的最远处**，下一行要做的事就是卡住的那一步。\n\
                 （面包屑绕开了 libtest 的输出捕获直接写 fd 2，正是为挂死这一形准备的。）"
        ),
    }
}

/// 面包屑：**绕开 libtest 的输出捕获**，直接写进程的 fd 2。
///
/// 🔴 为什么不是 `eprintln!`〔第五拍〕：`eprintln!` 走 `std::io::_eprint`，
/// 它先看 libtest 装的那个**线程局部**捕获缓冲 ⇒ 只有**测试失败或成功**时才转印得出来。
/// 而 09-10 云端那三趟正是**挂死**：既不失败也不成功，捕获里的东西一个字都到不了日志
/// —— 那三趟合起来零输出，我们只能靠推。`std::io::stderr()` 的 `Write` 不经过那一层。
///
/// ⇒ 这几行**不是给绿的时候看的**，是给「万一还挂」准备的：日志会停在
/// 最后一条面包屑上，下一步就是卡住的那一步。
///
/// ⚠ 还有第二重保险，两重是刻意叠的：本判据的正文跑在**自己起的那条裸线程**上，
/// 而 libtest 的捕获是**线程局部**的 —— 那条线程上压根没装捕获。
/// 两重都指望不上的话，我们就又回到「零输出只能靠推」那一趟了。
///
/// ⚠ 每一条都带**自测起点起的秒数**〔第六拍〕：09-10 那趟的四格全在 4.5 秒内返回，
/// 而整条炸了 210s 硬上限 —— 中间那 205 秒**只能靠云端日志的行时间戳去推**。
/// 把耗时印进行里，下一趟这笔账就不用再借外面的钟。
fn breadcrumb(what: &str) {
    use std::io::Write;
    static T0: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let t0 = T0.get_or_init(std::time::Instant::now);
    let mut err = std::io::stderr();
    let _ = writeln!(
        err,
        "[ccbus-orphan +{:.1}s] {what}",
        t0.elapsed().as_secs_f64()
    );
    let _ = err.flush();
}

/// 落在**析构那一刻**的面包屑 —— 它的全部本事就是「在 `Runtime` 被 drop 之前喊一声」。
///
/// 🔴 为什么非要一个 `Drop` 而不是一句显式调用〔第六拍〕：09-10 那趟走的是
/// **panic 那条路**，上一拍那句写在 `block_on` 之后的显式面包屑**压根没执行到**，
/// 于是「断言之后那 205 秒花在哪」整段无从判读。`Drop` 正常返回与 unwind 两条路都走得到。
struct DropCrumb;

impl Drop for DropCrumb {
    fn drop(&mut self) {
        breadcrumb("正文结束，开始析构 tokio 运行时（正常与 unwind 两条路都走这里）");
    }
}

/// 本判据的正文 —— 四格。被 [`a_timed_out_local_read_does_not_leave_an_orphan_behind`]
/// 放在裸线程上跑，好让整条有一个不可能被同步调用拖住的硬上限。
async fn orphan_judge_body() {
    // ★★ **阳性对照**：先证明这台机器的 `bash -lc` 真的跑了我们这段脚本、
    //    而且它的 stdout 真的回到了我们手上。**清一色内建**（`printf`），
    //    所以它量的是「壳活着吗」，不掺任何 PATH 的运气。
    //
    // 没有这一格时，两件完全不同的事在输出上**一模一样**：
    //   ① `bash` 是个存根 / 当场就退 ⇒ stdout 立刻 EOF ⇒ `local_shell_read` 回 `Ok("")`；
    //   ② 超时那一格真的失效了（本条要买的那一面）。
    const HELLO: &str = "CCBUS-SHELL-ALIVE";
    let probe = format!("printf %s {HELLO}");

    // ★ **先把两个同步嫌疑点拆开**〔第五拍〕：`local_shell_read` 里同步的只有两处 ——
    //   `resolve_bash()`（`env::var_os` + 最多 6 次 `Path::exists`）与 `Command::spawn()`。
    //   在这里先单独跑一次 `resolve_bash()` 并前后各留一条面包屑，
    //   下一趟即使还挂，日志也分得出是这两处里的哪一处。
    breadcrumb("格①之前：开始 resolve_bash()（同步：env::var_os + Path::exists）");
    let which = resolve_bash();
    breadcrumb(&format!("格①之前：resolve_bash() 回来了 -> {which:?}"));

    breadcrumb("进入格①·壳自检（下一步是同步的 Command::spawn）");
    // 内层 30s → 10s：一条 `printf` 回不来的话，多等 20 秒买不到任何东西。
    let alive = stage(
        "格①·壳自检",
        20,
        local_shell_read(&probe, 4096, 10, "壳自检", OnOverflow::Reject),
    )
    .await;
    breadcrumb("格①·壳自检回来了");
    // ⚠ 用 `contains` 不用逐字相等：`-l` 会过 `/etc/profile`，有的机器的 rc 会往
    //   stdout 上垫东西。垫东西不影响本格要证的事（脚本跑了、stdout 回得来），
    //   而逐字相等会把「rc 话多」误报成「壳是死的」。产品那侧同理，见 `take_head`。
    let got = alive.as_deref().unwrap_or("");
    // ★ `which` 在上面那条面包屑里已经拿到了 —— 它同时进错误消息，也同时进日志
    //   〔第二拍立、第五拍改成面包屑：挂死的时候错误消息根本印不出来〕。
    assert!(
        got.contains(HELLO),
        "【格①】阳性对照没回来（跑的是 `bash -lc '{probe}'`，全是内建）。实得：{alive:?}\n\
             解析到的 bash：{which:?}\n\
             ⇒ 这台机器上 `bash` 解析到的那个东西**根本没跑我们的脚本**\n\
             （Windows 上 `C:\\Windows\\System32\\bash.exe` 那个没装发行版的 WSL 存根\
             就是这一形）。\n\
             ⚠ 这不只是本判据的事：`CC_BUS_CAT_CMD` 走的是**同一个** `bash -lc` ——\n\
             这一格红 ⇒ 本机 cc-bus 的读面在这台机器上也读不了（那是产品面的事）。\n\
             ⚠ 同时意味着本条守的性质（超时不漏工作进程）在这台机器上**没人守**。"
    );

    // ★★ **格②：先量尺子本身，而且在起那个空转进程之前量**〔ccbus-win 09-10 第四拍〕。
    //
    // 拿一个**一定在**的针去问一次：本测试进程自己。它买两件事——
    //   ① 尺子答得出一个数（Windows 那半 09-10 之前从没真跑过，见 `count_live_processes`）；
    //   ② 它**答得及时**（那一趟 25 分钟没回来，而这条路上唯一无界的就是数进程那一格）。
    // ⚠ 顺序是刻意的：先量尺子、再起空转进程。反过来的话，尺子一卡，
    //   那个 100% 占核的空转进程就会陪着它一起烧到作业被掐。
    let me = std::env::current_exe().expect("【格②】拿不到本测试进程自己的路径");
    let stem = me.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    assert!(
        !stem.is_empty(),
        "【格②】本测试进程的文件名取不出来：{me:?}"
    );
    breadcrumb("进入格②·尺子自检（数进程，跑在它自己的裸线程上）");
    let seen_self = count_live_processes(stem, 45, "格②·尺子自检").await;
    breadcrumb(&format!("格②·尺子自检回来了 -> {seen_self}"));
    assert!(
        seen_self >= 1,
        "【格②】尺子连**本测试进程自己**都数不到（针=`{stem}`，实得 {seen_self}）。\n\
             ⇒ 下面那格的「0 个孤儿」是**空真** —— 不是没有孤儿，是尺子看不见东西。\n\
             （本仓最高频的那一类病：尺子的作用域对不上事实。）"
    );

    // ⚠ marker **不能只写在注释里**：`bash -lc '<单条 simple command>'` 会 **exec 掉自己**，
    // 于是注释从任何 cmdline 上都消失，`pgrep` 数到 0 ⇒ **判据假绿**（09-09 实测栽过一次）。
    // ⇒ 把 marker 放进那个必然存活的进程**自己的 argv** 里。
    // （`while` 循环不是 simple command，bash 不会 exec 掉自己 —— 本机现打验过。）
    let marker = format!("ccbus-orphan-{}", std::process::id());
    let cmd = format!("while :; do : {marker}; done");

    // ★★ **格③a：趁它还活着数一次**〔第七拍，反空真〕。
    //
    // 修好收尾之后格④ 会数到 0 —— 可「真的 0」与「尺子看不见 bash 子进程的 cmdline」
    // **在 `usize` 上同形**。格② 那格用 exe stem 当针，只证得了「尺子答得出一个数」，
    // **证不了它看得见一个 bash 的命令行** —— 那个缺口我第四拍就写下了，
    // 收尾一修好它就从「将来要补」变成「现在必须补」，否则格④ 会变成一条恒绿的空判据。
    //
    // ⇒ 用**同一个 marker、同一把尺子**，在探针还活着的时候先数一次，必须 ≥1。
    // ⚠ 内层上限从 1s 抬到 10s **就是为了这一格**：数一次要起 PowerShell + 问 WMI，
    //   1 秒的窗口里它多半还没问完，那时数到 0 是**尺子慢**、不是「看不见」。
    breadcrumb("进入格③·超时探针（下一步又是同步的 Command::spawn）");
    let read = local_shell_read(&cmd, 4096, 10, "超时探针", OnOverflow::Reject);
    let watch = async {
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        breadcrumb("格③a·趁它活着数一次（尺子看不看得见 bash 子进程的 cmdline）");
        count_live_processes(&marker, 45, "格③a·活着时数").await
    };
    let (r, alive_now) = stage("格③·超时探针", 45, async {
        tokio::join!(read, watch)
    })
    .await;
    breadcrumb(&format!("格③·超时探针回来了；活着时数到 {alive_now}"));
    assert!(
        alive_now >= 1,
        "【格③a】探针**明明还活着**，尺子却数到 {alive_now} 个（marker={marker}）。\n\
             ⇒ 尺子看不见 `bash -lc` 那种子进程的命令行 ⇒ **下面格④ 的「0 个孤儿」是空真**\n\
             —— 不是没有孤儿，是根本数不到。这一格就是为了不让格④ 变成恒绿的空判据。"
    );
    assert!(
        r.is_err(),
        "【格③】10 秒上限跑一个内建死循环竟然没超时 —— 本判据在空转。实得：{r:?}"
    );
    // 给 tokio 的收尸队一点时间。
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    breadcrumb("进入格④·数孤儿");
    let n = count_live_processes(&marker, 45, "格④·数孤儿").await;
    breadcrumb(&format!("格④·数孤儿回来了 -> {n}"));
    // 只在**要红的时候**才多问一次：绿的那条路上一次都不问。
    //
    // ★★ **这两条面包屑是为那 205 秒留的**〔第六拍〕：09-10 那趟四格全在 4.5s 内返回
    //    （最后一条面包屑 `格④·数孤儿回来了 -> 2`），而【格④】那条 panic 直到 205 秒
    //    之后才印出来。这中间**只有下面这一次 `describe`**，可它自己带着 45s 上限
    //    —— 两边对不上，**说明还有一件我没看见的事**。⇒ 不猜，把它夹在两条面包屑中间。
    let survivors = if n == 0 {
        String::new()
    } else {
        breadcrumb("格④·要红了，开始倒出 survivors（下一步 describe_live_processes）");
        let s = describe_live_processes(&marker, 45, "格④·倒出").await;
        breadcrumb("格④·倒出回来了");
        s
    };
    breadcrumb("格④·即将断言（若红，下一步是 panic → unwind → Runtime 析构）");
    assert_eq!(
        n, 0,
        "【格④】超时之后还留着 {n} 个子进程（marker={marker}）—— 每超时一次漏一个。\n\
             显式 `start_kill()` 只在成功路径上；超时那条要靠 `kill_on_drop(true)`。\n\
             留下来的是：\n{survivors}\n\
             ⚠ **这一格红有两种读法，别只挑顺手的那一种**〔ccbus-win 09-10 第四拍〕：\n\
             ㈠ 真漏 —— `local_shell_read` 超时那条路没把工作进程收干净（那是产品面的事）；\n\
             ㈡ **夹具的锅** —— Windows 上 `{which:?}` 若是个**外壳**（起完真 bash 自己就退），\n\
                `kill_on_drop` 杀掉的是外壳，真 bash 还在转。\n\
             ⚠ ㈡ **本轮没有任何读数**（宿主是 Linux，Windows 的进程树语义没人量过）。\n\
                分辨法就在上面那份清单里：留下来那个的 cmdline 是不是 `bash -lc while …` 本身，\n\
                它的 ppid 指向谁。**在量到之前别下结论。**"
    );
}

/// 给一格套一个自己的上限：超了带着**格号**炸，而不是把整条测试拖成挂死。
///
/// 🔴 立项理由〔ccbus-win 09-10 第四拍〕：云端那趟本条**跑了 25 分钟没回来**，
/// 日志里只有一句 "has been running for over 60 seconds"，之后是人工取消 ——
/// 而当时 `ci.yml` 一条 `timeout-minutes` 都没有 ⇒ 不取消就烧到 GitHub 的 6 小时上限。
/// **一条挂死的测试比一条红的更坏**：红的会说哪里坏了，挂死的什么都不说。
///
/// ⚠ **它约束的只有 await 点**（跑在 current_thread 运行时上）：某一格里若是
/// 同步调用卡住，计时器压根没机会跑 —— 09-10 云端那趟 27 分钟零输出，实测就是这一形。
/// ⇒ 这一层**不是**万能的兜底，它只是把「已知会 await 的那几格」变成有名有姓的红。
/// 真正的兜底在两处：数进程那一格自己的裸线程（[`count_live_processes`]），
/// 以及**整条判据外面那个硬上限**（见
/// [`a_timed_out_local_read_does_not_leave_an_orphan_behind`] 头一段）。
async fn stage<T>(label: &str, secs: u64, f: impl std::future::Future<Output = T>) -> T {
    match tokio::time::timeout(std::time::Duration::from_secs(secs), f).await {
        Ok(v) => v,
        Err(_) => panic!(
            "【{label}】超过 {secs}s 没回来 —— 本条就挂在这一格上。\n\
                 ⚠ 这是**上限炸的**，不是被测性质失败 —— 别读成「性质不成立」。"
        ),
    }
}

/// 数「cmdline 里含 `needle` 的活进程」有几个 —— **两个平台各一把尺子，量同一件事**。
///
/// POSIX 用 `pgrep -fc`；Windows 上**根本没有 `pgrep`**（Git for Windows 不带
/// procps），那边问 WMI 的 `Win32_Process`。这不是把 Windows 那半关掉，
/// 是给同一个量换一把这台机器上真存在的尺子。
///
/// 🔴 **数不出来一律红，绝不回 0**〔09-09 收紧〕：原写法是 `.unwrap_or(0)`，
/// 而「尺子坏了」与「一个孤儿都没有」在它下面**同形** —— 后者正是本判据要买的那一面。
/// （`pgrep -fc` 零命中时打印 `0`、退出码非零 ⇒ 这一形照旧解析得出，POSIX 行为逐字不变。）
///
/// ⚠ 诚实边界：Windows 那把尺子在交回本件时**没有在任何机器上跑过**
/// （宿主是 Linux，且本件不许跑测试）。它坏掉的表现是**红**，不是绿。
///
/// ⚠⚠ **09-10 那趟是它的第一次真跑**〔ccbus-win 第二拍预言、第四拍兑现〕：
/// 09-09/09-10 前两趟都在阳性对照就红了，根本走不到这里；[`resolve_bash`] 落地之后
/// 前两格过了，这一行才第一次真在 Windows 上跑 —— **然后整条测试 25 分钟没回来**。
///
/// # 🔴 上限是本函数的一部分，不是调用方的自觉〔第四拍〕
///
/// 病在哪**没量到**，所以这里不猜。但有一件事是确定的：`Command::output()`
/// **没有超时形态**（`ccm_probe::probe_with` 的头注早就逐字记着这句），
/// 而它是这条测试路径上**原先唯一无界的一格**。⇒ 上限收进来。
///
/// ⚠ **超时是「红」，不是「0 个孤儿」** —— 口径与 09-09 那次收紧一个字不差：
/// 「尺子答不上」与「一个孤儿都没有」在 `usize` 上同形，而后者正是本判据要买的那一面。
///
/// ⚠ 为什么用**裸线程 + 轮询**，而不是 `spawn_blocking` + `timeout`：
/// tokio 的运行时在 **drop 时会等正在跑的 blocking 任务跑完** ——
/// 真卡住的话，我们 panic 完照样卡在运行时析构里，又变回一条挂死的测试。
/// 裸线程漏掉就漏掉，进程退出时一起走。
///
/// ⚠ **诚实边界**：本函数**没有**验证「尺子看得见一个 `bash` 子进程的 cmdline」——
/// 它只验证「尺子答得出一个数」。调用点用「本测试进程自己」当针做了那一格自检
/// （格②），那覆盖的是「尺子整个坏了 / 卡住」，**不**覆盖「看得见 exe、看不见 bash」。
/// 要买那一格得有一台真 Windows，本轮宿主是 Linux。
async fn count_live_processes(needle: &str, secs: u64, label: &str) -> usize {
    let n = needle.to_string();
    bounded(label, secs, "数进程", move || count_now(&n)).await
}

/// 两条 PS 命令**共用的那把筛子**：匹配 `needle`，且**排除发起查询的那个进程自己**。
///
/// # 🔴 它是 09-10 云端一个真读数逼出来的〔第六拍〕
///
/// run `34473562660` 实测：查询进程自己的 `CommandLine` 里就带着 needle
/// （`-like '*<needle>*'` 那个字面量本身），于是 `Get-CimInstance Win32_Process`
/// **把发起查询的那个 `powershell` 也数了进去** ——
/// 格④ 实得 2，逐条倒出来看，其中一条逐字是
/// `944 ppid=2768 "powershell" -NoProfile -Command "Get-CimInstance …'*ccbus-orphan-2768*'…"`；
/// 格② 用 exe stem 当针同样虚高 1。⇒ **那两格的读数都被抬高了 1。**
/// 这正是本仓最高频的那一族：**量具的作用域对不上事实**。
///
/// ⚠ **按 pid 排自己，不按名字排 `powershell`**：一刀切排掉所有 powershell，
/// 会把「泄漏的那个恰好是 powershell」一起吞掉 —— 那是拿一个更大的空真去补一个小的。
/// `$PID` 是 PowerShell 的自动变量、恒指当前这个进程 ⇒ 它**只**排掉一个进程，
/// 而那个进程是我们**自己为了这次查询起的、查完就退**，构造上不可能是被测的泄漏。
///
/// ⚠ **POSIX 那半不需要同样的处理，而理由不是「大概没事」**：`pgrep` 自己
/// 从不把自己算作匹配（man 页逐字：the running pgrep or pkill process will never
/// report itself as a match）。本机现打验过两格：针**只**出现在 `pgrep` 自己的 argv 上时
/// 回 `0`（不是 1）；真有一个带针的进程时回 `1`（不是 2）。
/// 且我们是 `Command::new("pgrep")` 直起、**不经过 shell** ⇒ 中间没有第二个带着针的进程。
fn ps_same_needle_filter(needle: &str) -> String {
    format!("Where-Object {{ $_.ProcessId -ne $PID -and $_.CommandLine -like '*{needle}*' }}")
}

/// [`count_live_processes`] 的同步半 —— 真正去问这台机器的那一下。
fn count_now(needle: &str) -> usize {
    let ps = format!(
        "@(Get-CimInstance Win32_Process | {}).Count",
        ps_same_needle_filter(needle)
    );
    let (prog, argv): (&str, Vec<&str>) = if cfg!(windows) {
        ("powershell", vec!["-NoProfile", "-Command", ps.as_str()])
    } else {
        ("pgrep", vec!["-fc", needle])
    };
    let out = std::process::Command::new(prog)
        .args(&argv)
        .output()
        .expect("数进程那条命令起不来 —— 数不出来就不许当成绿");
    let raw = String::from_utf8_lossy(&out.stdout);
    let raw = raw.trim();
    let parsed = raw.parse::<usize>();
    assert!(
        parsed.is_ok(),
        "`{prog}` 没回出一个数（实得 {raw:?}）—— 数不出来就不许当成绿。\n\
             ⚠ 原写法 `.unwrap_or(0)` 会把「尺子坏了」读成「一个孤儿都没有」。"
    );
    parsed.expect("上面那条断言已经保证它是 Ok")
}

/// **只在失败那条路上用**：把匹配到的进程原样倒出来（pid / ppid / cmdline）。
///
/// # 它为什么值这几行〔ccbus-win 09-10 第四拍〕
///
/// 「超时之后还留着 1 个」有**两种**读法（真漏 / 夹具的锅，见格④那条断言），
/// 而分辨它们只要一样东西：**留下来那个到底是谁**。没有它，下一趟的红仍然是
/// 「留了 1 个，自己去查」——而「自己去查」在云端等于再烧一趟 CI。
///
/// ⚠ **它是诊断，不是尺子**：这里的失败一律降级成一句话塞进消息里，
/// **绝不 panic、绝不参与判定**。别把这份宽容读成 [`count_now`] 也可以宽容 ——
/// 那一个数不出来必须红，口径一个字没变。
async fn describe_live_processes(needle: &str, secs: u64, label: &str) -> String {
    let n = needle.to_string();
    bounded(label, secs, "倒出进程", move || {
        // 与 `count_now` **共用同一把筛子** —— 两处各写一份，下一次就只有一处记得排自己。
        let ps = format!(
            "Get-CimInstance Win32_Process | {} | ForEach-Object {{ \
                 ($_.ProcessId).ToString() + ' ppid=' + ($_.ParentProcessId).ToString() \
                 + ' ' + $_.CommandLine }}",
            ps_same_needle_filter(&n)
        );
        let (prog, argv): (&str, Vec<&str>) = if cfg!(windows) {
            ("powershell", vec!["-NoProfile", "-Command", ps.as_str()])
        } else {
            ("pgrep", vec!["-af", n.as_str()])
        };
        match std::process::Command::new(prog).args(&argv).output() {
            Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            Err(e) => format!("（倒不出来：`{prog}` 起不来：{e}）"),
        }
    })
    .await
}

/// 在**裸线程**上跑一件会阻塞的活，并给它一个上限；超了带着格号 panic。
///
/// 🔴 上限收在这里，不靠调用方自觉：`Command::output()` **没有超时形态**
/// （`ccm_probe::probe_with` 头注早就逐字记着），而它是这条测试路径上
/// **原先唯一无界的一格** —— 09-10 云端那趟整条测试 25 分钟没回来。
///
/// ⚠ 为什么是**裸线程**而不是 `spawn_blocking` + `timeout`：tokio 的运行时
/// **在 drop 时会等正在跑的 blocking 任务跑完** ⇒ 真卡住的话，我们 panic 完
/// 照样卡在运行时析构里，又变回一条挂死的测试。裸线程漏掉就漏掉，进程退出时一起走。
///
/// ⚠ 上限炸出来的是「**答不上**」，调用方**不许**把它读成一个具体的答案
/// （数进程那处：超时 ≠「0 个孤儿」，两者在 `usize` 上同形，而后者正是判据要买的那一面）。
async fn bounded<T: Send + 'static>(
    label: &str,
    secs: u64,
    what: &str,
    job: impl FnOnce() -> T + Send + 'static,
) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(job());
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    loop {
        match rx.try_recv() {
            Ok(v) => return v,
            // 发送端没了 = 那条线程里 panic 了（`count_now` 自己会 panic）。
            Err(std::sync::mpsc::TryRecvError::Disconnected) => panic!(
                "【{label}】{what}那条线程没把结果送回来（多半是它自己 panic 了，\
                     真因在它那条 panic 上）—— 答不上就不许当成绿。"
            ),
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
        if std::time::Instant::now() >= deadline {
            // 上限自己也留一条 —— 否则「它到底炸没炸」只能从 panic 文案倒推。
            breadcrumb(&format!("{label}：{what}的 {secs}s 上限到了，即将 panic"));
        }
        assert!(
            std::time::Instant::now() < deadline,
            "【{label}】{what}超过 {secs}s 没回来。\n\
                 🔴 **这是「答不上」，不是一个答案** —— 数进程那处尤其要紧：\n\
                 「尺子答不上」与「一个孤儿都没有」在 `usize` 上同形，\n\
                 而后者正是本判据要买的那一面，绝不许拿一次超时冒充它。\n\
                 Windows 那半跑的是 `powershell -NoProfile -Command …Get-CimInstance…`，\n\
                 而 `Command::output()` 从来没有超时形态 —— 本上限就是为它加的\n\
                 （09-10 云端那趟整条测试 25 分钟没回来，这一格是当时唯一无界的一格）。"
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

/// ★ 在线灯**必须先问身份空间**，不能只有那条按名字的老探法。
///
/// ⚠ 变异实测：把「先问 daemon」那一步整个拿掉，上面那条纯函数判据**照样绿** ——
/// 它钉的是"答案怎么算"，不是"有没有去问"。⇒ 这条钉接线本身（位置：问在前，探在后）。
///
/// ⚠⚠ **本条的射程如实写**：它按**字面量位置**判，所以
/// · 真删掉那一步 ⇒ **红**（实测）；
/// · 把调用留在原地却不用它的结果（`if let Some(x) = None { … online_via_daemon(…) }`）
///   ⇒ **绿**（我自己第一次的变异恰好是这个形状，它溜过去了）。
/// 后者要靠行为判据（真起 daemon 跑一遍）才逮得住，那归 daemon 侧那套 e2e。
/// **写下来**是因为：不写的话，下一个人会以为这条比它实际能做的更强。
#[test]
fn the_online_lamp_asks_the_identity_space_and_has_nothing_else_to_ask() {
    let code = non_test_code();
    // ⚠〔`K-R112` 09-13〕原文钉的是**顺序**（先问身份空间、再退回按名字探）。
    //   本件把老探法整条删了 ⇒ 「顺序」这件事不存在了，**位置比较的参照物没了**。
    //   照字面留着它只能放宽成「提到过 online_via_daemon」——那是空真。
    //   ⇒ 翻成更强的一格：这条路上**只有一条路**。
    for name in ["check_cc_bus_agent_online", "online_via_daemon"] {
        let body = fn_body(&code, name);
        assert!(
            body.chars().count() > 40,
            "`{name}` 的体只切出 {} 字 —— 抽取器坏了，本条在空转",
            body.chars().count()
        );
        let hits = shell_line_markers(&body);
        assert!(
            hits.is_empty(),
            "`{name}` 这条路上又出现了命令串的痕迹 {hits:?} —— 老探法回潮了。\n\
                 它按**名字**探（`tmux has-session`），而名字会被重用：那盏灯会为别人的会话亮。"
        );
    }
    assert!(
        fn_body(&code, "check_cc_bus_agent_online").contains("online_via_daemon(&origin, &id)"),
        "在线灯没有把 origin 原样交给那条唯一的路"
    );
    assert!(
        fn_body(&code, "online_via_daemon").contains(".call(\n            BUS_LIST"),
        "`online_via_daemon` 没在调 `bus-list` —— 那盏灯又不问身份空间了"
    );
}

/// ★★ **给 `agents.tsv` 加一列，不许打破读它的人**〔08-13〕。
///
/// 今天为身份核对加了第 4 列（登记时的 pane 根进程 pid）。这张表有**四个读者**：
/// 本解析器 · `cc-list` · `cc-broadcast` · `cc-kill`/`cc-agents`。
/// 三个 shell 读者用 `read -r a b c`（多出的落进最后一个变量、且它们都不用它）；
/// 本解析器用 `row_fields(line, 3)`，判的是 `len < want` ⇒ **多列照过**。
///
/// ⇒ 兼容是**设计成立的**，不是碰巧 —— 但没有判据的话，下一个把它改成
/// `f.len() != want` 的人不会知道自己拆掉了什么。这条钉住两个方向。
#[test]
fn adding_a_column_to_agents_tsv_does_not_break_the_reader() {
    // 老格式（3 列）——用户机器上那 86 行今天还是这个形状
    let (old, bad_old) = parse_agents_tsv(
        "a_cc	a_cc:0.0	2026-07-18T07:26:31-07:00
",
    );
    assert_eq!(old.len(), 1, "老三列行读不出来了");
    assert_eq!(bad_old, 0);
    assert_eq!(old[0].pane, "a_cc:0.0");
    // 新格式（4 列，第 4 列是 pane 根进程 pid）
    let (new, bad_new) = parse_agents_tsv(
        "b_cc	b_cc:0.0	2026-08-13T00:00:00-07:00	12345
",
    );
    assert_eq!(new.len(), 1, "四列行被当成坏行了 —— 加一列就把驾驶舱清空了");
    assert_eq!(bad_new, 0);
    assert_eq!(new[0].id, "b_cc");
    assert_eq!(
        new[0].registered_at, "2026-08-13T00:00:00-07:00",
        "多出的那列串进了时间戳"
    );
    // 两种混在一起也要都认（迁移期的真实形状）
    let (both, bad_both) = parse_agents_tsv(
        "a_cc	a_cc:0.0	ts
b_cc	b_cc:0.0	ts	12345
",
    );
    assert_eq!(both.len(), 2, "新老混排时丢了行");
    assert_eq!(bad_both, 0);
    // 字段**不够**仍要算坏行（别把"宽容多列"做成"什么都收"）
    let (short, bad_short) = parse_agents_tsv(
        "c_cc	c_cc:0.0
",
    );
    assert!(short.is_empty());
    assert_eq!(bad_short, 1, "两列行该算坏行");
}

/// ★ 在线灯：**「问不到」不许渲染成「不在线」**。
///
/// 老探法是 `tmux has-session -t '=<id>:'`（纯按名字）——名字被别人占着时它照样说在线。
/// 换成问 `bus-list` 之后，`None` 有两种来源（不在名单 / `live` 是 null）。
///
/// ⚠〔`K-R112` 09-13〕原文第三句写「**两种都要回落到老探法**」—— 那半句今天假了：
/// 老探法删了。**保住的是这一条本来要买的东西**（灭灯是一个确定的答案，而我们并不确定），
/// 只是出口从「回落」换成 [`unknown_liveness`] 那句诚实的「问不到」。
/// 出口那一半由 `an_unknown_liveness_is_never_rendered_as_dark` 钉。
#[test]
fn the_online_lamp_never_guesses_dark() {
    use serde_json::json;
    let agents = vec![
        json!({"id": "a_cc", "live": true}),
        json!({"id": "b_cc", "live": false}),
        json!({"id": "c_cc", "live": null}),
    ];
    assert_eq!(live_of(&agents, "a_cc"), Some(true));
    assert_eq!(
        live_of(&agents, "b_cc"),
        Some(false),
        "确定不在线要答得出来"
    );
    assert_eq!(
        live_of(&agents, "c_cc"),
        None,
        "live=null 是「问不到」，不是「不在」"
    );
    assert_eq!(
        live_of(&agents, "nobody_cc"),
        None,
        "不在名单里也是「答不上」"
    );
}

/// ★★ **广播不许再打进幽灵收件箱**〔P4f 08-13，用户机器上实测出来的〕。
///
/// 老路（`cc-broadcast` 脚本）发给 `agents.tsv` 的**每一行**。用户机器实测：
/// **86 行登记、只有 8 个会话还活着** ⇒ 一次广播打进 **78 个没人读的收件箱**，
/// 而它报「已向 86 个 agent 发出广播」—— 那个数把 78 个幽灵也算了进去。
#[test]
fn broadcast_only_goes_to_the_ones_that_are_actually_there() {
    use serde_json::json;
    let agents = vec![
        json!({"id": "a_cc", "live": true}),
        json!({"id": "b_cc", "live": false}),
        json!({"id": "c_cc", "live": true}),
        json!({"id": MONITOR_BUS_ID, "live": true}),
    ];
    let plan = pick_broadcast_targets(&agents, MONITOR_BUS_ID);
    assert_eq!(plan.targets, vec!["a_cc", "c_cc"], "只该发给活着的");
    assert_eq!(plan.skipped_offline, 1, "不在线的要计数，不是悄悄丢掉");
    assert!(!plan.liveness_unknown);
    assert!(
        !plan.targets.iter().any(|t| t == MONITOR_BUS_ID),
        "不发给自己"
    );
}

/// ★ **「问不到」不等于「都不在」**。
///
/// 身份空间答不上时（没装 tmux 等，`live` 全是 `null`），退回「发给所有登记的」——
/// 若问不到就谁都不发，用户会看到一次「已广播给 0 个」，那是**把不知道渲染成了确定**。
#[test]
fn unknown_liveness_does_not_silently_become_nobody() {
    use serde_json::json;
    let agents = vec![
        json!({"id": "a_cc", "live": null}),
        json!({"id": "b_cc", "live": null}),
    ];
    let plan = pick_broadcast_targets(&agents, MONITOR_BUS_ID);
    assert_eq!(plan.targets.len(), 2, "问不到时不许把人全滤掉");
    assert!(
        plan.liveness_unknown,
        "而且要**标出来**是问不到，不是装作知道"
    );
    assert_eq!(plan.skipped_offline, 0);
    let said = describe_broadcast(&plan, 2, &[]);
    assert!(said.contains("问不到谁在线"), "话没说清：{said}");
}

/// ★ 三个数**分开说**：发到几个 / 跳过几个 / 失败几个。
#[test]
fn the_broadcast_wording_keeps_the_three_counts_apart() {
    let plan = BroadcastPlan {
        targets: vec!["a_cc".into(), "b_cc".into()],
        skipped_offline: 78,
        liveness_unknown: false,
    };
    let said = describe_broadcast(&plan, 1, &["b_cc（超时）".to_string()]);
    assert!(said.contains("1 个**在线**"), "{said}");
    assert!(said.contains("跳过 78 个"), "跳过的没说：{said}");
    assert!(said.contains("1 个失败"), "失败的没说：{said}");
    // 老路那句话的形状（把所有人算成一个 N）不许回来
    assert!(!said.contains("已向 79"), "又把跳过的算进总数了：{said}");
}

/// ★★ **三态在线不许在讲人话这一层被抹平**〔P4f 08-13，变异 M2 逼出来的〕。
///
/// daemon 的 `bus-send` 回 `registered` + 三态 `live`，而 UI 拿到的是一句话。
/// 变异实测：把 `describe_send_reply` 改成恒说「已投递给 X」——**60 条测试全绿**。
/// 也就是说那三态一路传到最后一米，然后被一句话吃掉，没有任何东西看着。
///
/// 「发出去了」和「发出去了但没人会读」对用户是两件事：后者要么名字打错了，
/// 要么对方不在线（消息躺在收件箱里等它下次起来）。
#[test]
fn the_delivery_wording_keeps_the_three_states_apart() {
    use serde_json::json;
    let say = |v: serde_json::Value| describe_send_reply("proj_cc", Some(&v));
    let ok = say(json!({"registered": true, "live": true}));
    let offline = say(json!({"registered": true, "live": false}));
    let ghost = say(json!({"registered": false, "live": null}));
    let unknown = say(json!({"registered": true, "live": null}));
    for (a, b, why) in [
        (&ok, &offline, "「在线」与「不在线」"),
        (&ok, &ghost, "「在线」与「名字没登记过」"),
        (&offline, &ghost, "「不在线」与「名字没登记过」"),
        (&ok, &unknown, "「在线」与「问不到在不在线」"),
    ] {
        assert_ne!(a, b, "{why} 说的是同一句话 —— 三态被抹平了");
    }
    // 各自要点到实处（不是只要求"不一样"就行）
    assert!(offline.contains("不在线"), "{offline}");
    assert!(ghost.contains("没在总线上登记过"), "{ghost}");
    assert!(unknown.contains("问不到"), "{unknown}");
    // 四种都得说「已投递」—— 投递是照做的，三态只是附加说明
    for m in [&ok, &offline, &ghost, &unknown] {
        assert!(m.contains("已投递"), "投递本身没说清：{m}");
    }
}

/// ★ P4a-Y2：**写面两条必须在 `cfg_of` 之前就把本机挡掉。**
///
/// ⚠ 本条是**变异逼出来的**：M5（拿掉 `cc_bus_send` 的本机拒绝）第一次跑
/// **照样绿** —— 因为 `local_origin_registry` 只扫**直接**调
/// `load_remote_config_by_label(` 的地方，而这两条走的是 `cfg_of` 这个**包装**。
/// ⇒ 那条护栏对「隔了一层包装」是瞎的（已在它的头注里登记）。
///
/// 钉**位置**而不是「有没有这句话」：本机分支必须在 `cfg_of` 之前，
/// 否则用户拿到的是 `cfg_of` 那句通用话，而不是这条路真实的说法。
///
/// ⚠⚠ **08-13 P4f 改过一次口径**：本条原来钉的是 `refuse_local_write(` 这个**写法**。
/// 而 `cc_bus_send` 的本机路当时**不再是拒绝** —— 它走 daemon 的 `bus-send` 原语
/// （拒绝理由逐字写着「等命令组件做出来」，那些组件做出来了）。
/// ⇒ 钉的东西从「有没有那句拒绝」改成**「本机分支在不在 `cfg_of` 前面」**：
/// 前者是实现，后者才是这条判据真正要保的性质。
/// **两种形态都算数**：`refuse_local_write(` 或 `== LOCAL_ORIGIN` 的早返回。
///
/// ⚠⚠⚠ **09-13 `K-R98` 又改一次人群 —— 而这一次是把 `cc_bus_send` 挪出去，
/// 不是把它豁免掉。** 它今天**整条路只有一份**（远端那半也改走 `bus-send`），
/// 函数体里**根本没有 `cfg_of`** ⇒ 「本机会不会掉进 `cfg_of` 拿到一句通用话」
/// 这个问题在它身上**结构上不成立**（不是「今天恰好不会」）。
/// 🔴 而「不成立」必须**判出来**：下面那一段单独钉它 ——
/// 从表里删掉了事就是静默失去覆盖，那正是本仓栽过的「把判据的分母改小」。
#[test]
fn the_write_face_branches_on_local_before_it_asks_for_a_remote_config() {
    let code = non_test_code();
    // 窗口按**函数边界**截 —— 两处都用它（第一版各写一遍，其中一处越进了邻居）。
    let body_of = |name: &str| -> String {
        let at = code
            .find(name)
            .unwrap_or_else(|| panic!("生产段找不到 {name} —— 判据在空转"));
        let rest = &code[at..];
        let end = rest[1..]
            .find("\npub async fn ")
            .or_else(|| rest[1..].find("\npub fn "))
            .map(|k| k + 1)
            .unwrap_or_else(|| rest.len().min(1400));
        rest[..end].to_string()
    };

    // ── ① `cc_bus_send`：**不问远端配置**，因为它没有「远端专属」那一支了 ──────
    let send = body_of("pub async fn cc_bus_send(");
    assert!(
        send.chars().count() > 40,
        "`cc_bus_send` 的体只切出 {} 字 —— 窗口坏了，下面三条都在空转",
        send.chars().count()
    );
    for needle in ["cfg_of(", "exec_read(", "local_shell_read("] {
        assert!(
            !send.contains(needle),
            "`cc_bus_send` 里又出现了 `{needle}` —— 那意味着它重新长出了一支\n\
                 「远端专属」的路。本条此前钉的是「本机分支要排在 `cfg_of` 前面」，\n\
                 `K-R98` 之后钉的是**根本没有那个问题**：两侧同一条路，origin 是入参。"
        );
    }
    assert!(
        send.contains("send_via_daemon(&origin, &id, &text)"),
        "`cc_bus_send` 没有把 origin 原样交给那条唯一的路 —— \n\
             它一旦自己判 origin，两条路就又有两份实现了。"
    );

    // ── ①b 〔`K-R112` 09-13〕**另外三条也进了这一段** ──────────────────────
    //     收掉 / 广播 / 查在线本件都改走 daemon 原语 ⇒ 它们与发消息同形：
    //     没有「远端专属」那一支，也就没有「本机分支排在哪」这个问题。
    // ⚠ 这里用 `fn_body`（**按顶格行配边界**）而不是上面那个 `body_of`：
    //   `check_cc_bus_agent_online` 与下一个 `pub async fn` 之间隔着一大段非 pub 代码
    //   （构造器 / `exec_read` / `cfg_of` / `local_shell_read` 的**定义**），
    //   `body_of` 的窗口会把它们整段读进来 ⇒ 下面那三个 needle 恒命中 = 一次假红。
    for name in [
        "cc_bus_kill",
        "cc_bus_broadcast",
        "check_cc_bus_agent_online",
    ] {
        let b = fn_body(&code, name);
        assert!(
            b.chars().count() > 30,
            "{name} 的体只切出 {} 字 —— 窗口坏了",
            b.chars().count()
        );
        for needle in ["cfg_of(", "exec_read(", "local_shell_read("] {
            assert!(
                !b.contains(needle),
                "{name} 里又出现了 `{needle}` —— 那意味着它重新长出了一支「远端专属」的路"
            );
        }
    }

    // ── ② 仍然「本机分支必须排在 `cfg_of` 前面」的那些 ────────────────────────
    let mut checked = 0usize;
    for (name, what) in [
        // ⚠ 这里的"说清在做什么"必须是**代码里**的词（`non_test_code` 剥注释）：
        //   `cc_bus_spawn` 仍是拒绝，说清的是拒绝文案里那句。
        ("pub async fn cc_bus_spawn(", "spawn 一个 agent"),
    ] {
        // ⚠⚠ **窗口要按函数边界截**〔08-13 当场撞到〕：原来是「从函数名起取 1400 字」，
        //   而 `cc_bus_send` 比 1400 字短 ⇒ 窗口**越进了下一个函数**
        //  （`cc_bus_broadcast`），把邻居的 `refuse_local_write(` 当成了自己的，
        //   于是位置比较拿到的是**别人的**那处，判据当场误红。
        //   ★ 这正是本判据头注自己警告过的「块粒度」病 —— 而它发生在判据脚下。
        let body: String = body_of(name);
        // 本机分支有**两种形态**（早返回走 daemon / 拒绝），取**先出现**的那个位置。
        let refuse = [
            body.find("refuse_local_write(&origin, \""),
            body.find("origin == crate::inbound_client::LOCAL_ORIGIN"),
        ]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or_else(|| {
            panic!("{name} 没有本机分支 —— `<local>` 会掉进 `cfg_of` 拿到一句通用话")
        });
        let cfg = body
            .find("cfg_of(&origin)")
            .unwrap_or_else(|| panic!("{name} 里找不到 `cfg_of(&origin)` —— 判据的参照物没了"));
        assert!(
            refuse < cfg,
            "{name} 的本机拒绝排在 `cfg_of` **后面** —— 那就永远走不到，\n\
                 用户看到的仍是「远端 `<local>` 未配置或未启用」。"
        );
        assert!(
            body[refuse..].contains(what),
            "{name} 的本机分支没说清它在做什么（应含 {what:?}）——\n\
                 一句不说清是哪件事的错误，与那句「未找到远端配置」是同一族。"
        );
        checked += 1;
    }
    assert_eq!(
        checked, 1,
        "只核到 {checked} 条「要排在 `cfg_of` 前面」的写面命令 —— 本断言在空转。\n\
             ⚠ 09-13 起这个数是 **1**（`cc_bus_send` 挪进上面那一段，它没有 `cfg_of` 可排）。"
    );
}

// ════════════════════════════════════════════════════════════════════════
// `K-R98`（09-13）：发消息**远端那半**也改走 daemon 的 `bus-send` 原语。
// 下面三条分别是 `KR98D1` / `KR98D2` / `KR98D3` 的机检。
// ════════════════════════════════════════════════════════════════════════

/// 「这段代码在**拼 / 跑一条 shell 串**吗」—— 认形态，不认某一个符号。
///
/// 🔴 `KR98D1` 逐字记下的失效方向：判「代码里还有没有 `exec_read`」是**判写法**，
/// 它挡不住换个写法再拼一遍（内联 `format!` ＋ `connect_and_exec_cmd`）。
/// ⇒ 本谓词认的是「命令串」这件事的**几种形态**，而它对每一种真的会响这件事，
/// 由 `the_shell_line_detector_really_sees_each_shape` 用活体语料证明。
fn shell_line_markers(body: &str) -> Vec<&'static str> {
    // 逐条：经命令构造器 · 送进远端 shell · 送进本机 shell · 自己连上去跑 ·
    // 为了拼进 shell 才需要的引用 · 问「这台远端怎么连」· 重定向（命令串的指纹） ·
    // 那条老命令自己。
    [
        "_cmd(",
        "exec_read(",
        "local_shell_read(",
        "connect_and_exec_cmd(",
        "shell_quote(",
        "cfg_of(",
        "2>&1",
        "cc-send ",
    ]
    .into_iter()
    .filter(|m| body.contains(m))
    .collect()
}

/// ★ 上面那个谓词的**活体夹具**：它对每一种形态都得真的响。
///
/// 没有这一条的话，`shell_line_markers` 哪天被改瘸（比如有人为了让某条判据变绿
/// 把 `_cmd(` 从表里拿掉），真判据会**零命中地绿** —— 那正是本仓最贵的一类假绿。
#[test]
fn the_shell_line_detector_really_sees_each_shape() {
    // 形态一律**现拼**，免得夹具自己被真树上的扫描收进人群。
    let old_send = format!(
        "let cmd = build{u}send{u}cmd(&id, &text)?;\n\
             let cfg = cfg{u}of(&origin)?;\n\
             let out = exec{u}read(&cfg, &cmd, CAP, 30, \"x\", OnOverflow::Truncate).await?;",
        u = "_"
    );
    assert!(
        shell_line_markers(&old_send).len() >= 3,
        "老那条路（构造器 + cfg_of + exec_read）没被认出来：{:?}",
        shell_line_markers(&old_send)
    );
    let inlined = format!("let c = format!(\"cc{d}send {{id}} {{q}} 2>&1\");", d = "-");
    assert!(
        !shell_line_markers(&inlined).is_empty(),
        "**换个写法内联拼一份**没被认出来 —— 那正是「判写法」买不到的那一格"
    );
    // 反向：一段真的只调原语的代码不许被误判。
    let clean = "let args = json!({ \"to\": id });\nclient.call(BUS_SEND, args, d).await";
    assert!(
        shell_line_markers(clean).is_empty(),
        "只调原语的代码被误判成拼串：{:?}",
        shell_line_markers(clean)
    );
}

/// ★★ `KR98D1`：**发一条给远端时，走的是 daemon 原语，不是拼出来的 shell 串。**
///
/// 判的是**这条路**（`cc_bus_send` → `send_via_daemon`）上有没有命令串，
/// 不是「文件里还有没有 `exec_read`」—— 文件里当然还有（inbox / 广播 / 收掉 / spawn
/// 四条仍旧走它，`KR98D3` 正是钉着它们别被顺手放行）。
///
/// 第 ③ 刀单列在末尾：**本机与远端两条路的可观测行为等价**。
#[test]
fn the_send_path_asks_the_backend_instead_of_composing_a_shell_line() {
    let code = non_test_code();
    // ── ① 这条路上没有命令串 ────────────────────────────────────────────
    let mut checked = 0usize;
    for name in ["cc_bus_send", "send_via_daemon"] {
        let body = fn_body(&code, name);
        assert!(
            body.chars().count() > 60,
            "`{name}` 的体只切出 {} 字 —— 抽取器坏了，本条在空转",
            body.chars().count()
        );
        let hits = shell_line_markers(&body);
        assert!(
            hits.is_empty(),
            "`{name}` 这条路上又出现了命令串的痕迹 {hits:?}。\n\
                 发消息归 daemon 的 `{BUS_SEND}` 原语（`P4f` 逐字「cc-bus 的基础命令」）——\n\
                 拼一份 shell 串走 SSH 就是同一件事的第二份实现（`K33`「所有命令只许有一处」）。"
        );
        checked += 1;
    }
    assert_eq!(checked, 2, "只核到 {checked} 段 —— 本断言在空转");

    // ── ② 它真的调了那条原语，而且校验没在搬家的路上丢掉 ────────────────
    let send = fn_body(&code, "send_via_daemon");
    for needle in [
        ".call(BUS_SEND",
        "is_valid_bus_id(id)",
        "text.trim().is_empty()",
    ] {
        assert!(
            send.contains(needle),
            "`send_via_daemon` 里找不到 `{needle}` —— 要么它没在调原语，\n\
                 要么那两道随构造器一起被删的校验（id 白名单 / 空消息）没跟着搬过来。"
        );
    }

    // ── ③ 🔴 两条路的可观测行为等价：**origin 是入参，不是分支** ──────────
    for name in ["cc_bus_send", "send_via_daemon"] {
        let body = fn_body(&code, name);
        for forbidden in ["LOCAL_ORIGIN", "origin =="] {
            assert!(
                !body.contains(forbidden),
                "`{name}` 里出现了 `{forbidden}` —— 它又开始按 origin 分岔了。\n\
                     两条路一分岔就有了两份实现，而用户看得见的那一面（措辞 / 校验 / 三态在线）\n\
                     会各自漂 —— 这一条判的正是「同一输入 ⇒ 同一结果形状」。"
            );
        }
    }
    // 现跑一趟：同一输入喂给本机与一台远端，**两句话逐字相同**。
    // （这两格在 `client_for` 之前就返回 ⇒ 与进程内那张登记表无关，不会飘。）
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("建运行时");
    let local = crate::inbound_client::LOCAL_ORIGIN;
    for (id, text) in [("proj_cc", "   "), ("--help", "hi")] {
        let a = rt.block_on(cc_bus_send(local.into(), id.into(), text.into()));
        let b = rt.block_on(cc_bus_send(
            "kr98-no-such-host".into(),
            id.into(),
            text.into(),
        ));
        assert_eq!(
            a, b,
            "同一输入（id={id:?} text={text:?}）在本机与远端上给了两种结果 —— 不等价。"
        );
        assert!(a.is_err(), "这一格本来就该拒：id={id:?} text={text:?}");
    }
    // 要说到「这台机器」的那几句，也只准差一个称呼。
    let pairs = [
        (
            describe_no_channel(local, "p"),
            describe_no_channel("h1", "p"),
        ),
        (
            describe_daemon_too_old(local, "p"),
            describe_daemon_too_old("h1", "p"),
        ),
    ];
    for (l, r) in pairs {
        assert_ne!(l, r, "两句话逐字相同 —— 那说明称呼根本没进去，本格在空转");
        assert_eq!(
            l.replacen(&machine_label(local), "<M>", 1),
            r.replacen(&machine_label("h1"), "<M>", 1),
            "本机与远端不只差一个称呼 —— 那就是同一件事的第二种说法"
        );
    }
}

/// ★★ `KR98D2`：**老 daemon 那一档说得出话，不与超时 / 网络错同形。**
///
/// 「这台机器的后端没有这条命令」是**问得出答案**的（`hello` 里那张命令表），
/// 「超时」「连接断了」是**问不出答案**的。压成同一句「发消息失败」就是
/// 本工作区最贵的那一形 —— 一个值装了两件事。
#[test]
fn an_old_daemon_on_the_send_path_is_told_apart_from_a_timeout() {
    use crate::inbound_client::CallError;
    let origin = "h1";
    let id = "proj_cc";
    let too_old = describe_daemon_too_old(origin, id);
    let timeout = describe_send_error(
        id,
        &CallError::Timeout {
            after: std::time::Duration::from_secs(30),
        },
    );
    let dropped = describe_send_error(id, &CallError::Disconnected);
    let no_chan = describe_no_channel(origin, id);

    // ① 四句话两两不同 —— 一句都不许被另一句吸收掉。
    let all = [&too_old, &timeout, &dropped, &no_chan];
    for (i, a) in all.iter().enumerate() {
        for b in all.iter().skip(i + 1) {
            assert_ne!(a, b, "两档被压成了同一句话");
        }
    }
    // ② 「太旧」那一档必须自己说出「太旧」，而且要说清消息没发出去。
    assert!(too_old.contains("太旧"), "{too_old}");
    assert!(too_old.contains("没有发出去"), "{too_old}");
    // ③ 🔴 分得开的那一刀：超时 / 断连**不许**长成「太旧」的样子，
    //    而且它们自己带着「无法确认是否已经执行过」那句（不能证明没发出去）。
    for other in [&timeout, &dropped] {
        assert!(
            !other.contains("太旧"),
            "超时 / 断连被说成了「daemon 太旧」：{other}"
        );
        assert!(
            other.contains("无法确认"),
            "超时 / 断连丢掉了「不能证明没发出去」那一格：{other}"
        );
    }
    // ④ 反过来：「太旧」不许借用那句「无法确认」—— 它是**确认过**的（能力协商）。
    assert!(
        !too_old.contains("无法确认"),
        "「太旧」被说成了不确定：{too_old}"
    );

    // ⑤ 🔴 **它得真的走得到**：纯函数再分得开，没人调也是空转。
    let code = non_test_code();
    let body = fn_body(&code, "send_via_daemon");
    let ask = body
        .find("accepts(BUS_SEND)")
        .expect("`send_via_daemon` 没有先问一句能力 —— 「太旧」那句话永远说不出口");
    let call = body
        .find(".call(BUS_SEND")
        .expect("`send_via_daemon` 没在调那条原语 —— 判据的参照物没了");
    assert!(
        ask < call,
        "能力协商排在真发之后 —— 那就永远走不到，用户拿到的仍是一句含糊的失败"
    );
    assert!(
        body.contains("describe_daemon_too_old(origin, id)"),
        "问了能力却没把「太旧」讲出来"
    );
    // ⑥ 分流本身仍然只有一份（`daemon_route` 的登记表逐字要求每个发送端表态）。
    // ⚠〔`K-R112` 09-13〕住址换了：`describe_send_error` 今天是 4 行薄壳，
    //   三档分流搬进 `describe_bus_error`（收掉 / 查在线也用它）。
    //   钉的仍是同一件事，而且**更强一格**：全模块只有这一处 `route_call_error`。
    assert!(
        fn_body(&code, "describe_bus_error").contains("route_call_error"),
        "失败分流没走共用的那一份 —— 第二份分流规则会把「被门拒绝」洗成「换条路重做」"
    );
    // ⚠ **刻意不在这里数 `route_call_error` 的处数**：`broadcast_via_daemon` 自己
    //   也调它（那是对的 —— 它是**用**分流器，不是第二份分流规则）。
    //   「不许有第二份分流规则」那条由 `daemon_route` 的登记表守
    //   （它判的是「本文件生产段里有没有自己 match 那个错误枚举」），不在这里重复一份。
}

/// ★★ `KR112D1`（**纪律 ⑱**，形状抄 `KR98D3`）：本件放行**三条**，`spawn` 仍旧拒。
///
/// 「被放行」在这里有确切的意思：那条命令的路**离开了 shell 串**，
/// 而且它对 `<local>` 不再有一句「本机做不了」。两样都发生才算放行。
///
/// 🔴 **它的分母是从源码派生的，不是这里手写的一张名单**：人群 = `#[tauri::command]`
/// 里所有 `cc_bus_*` / `check_cc_bus_*` / `read_cc_bus_*` 命令。
/// 手写名单看不见新长出来的第五条 —— 那正是本仓在三个模块上栽过的同一个坑。
#[test]
fn letting_kill_broadcast_and_online_through_did_not_let_spawn_through() {
    let code = non_test_code();
    // ── 人群派生：本模块所有 tauri 命令 ──────────────────────────────
    let mut cmds: Vec<String> = Vec::new();
    for (i, _) in code.match_indices("pub async fn ") {
        let name: String = code[i + "pub async fn ".len()..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if name.contains("cc_bus") {
            cmds.push(name);
        }
    }
    cmds.sort();
    cmds.dedup();
    assert_eq!(
        cmds.len(),
        7,
        "本模块的 cc-bus 命令现打 {} 条（`K-R112` 09-13 实测 **7**：\
             state · online · inbox · send · broadcast · kill · spawn）\
             —— 抽取器坏了或有人加了新命令：{cmds:?}\n\
             ⚠ `deploy_local_cc_bus` / `cc_bus_install_state` 住 `cc_bus_deploy.rs`，不在本文件的分母里。",
        cmds.len()
    );
    // ── ① 走原语那几条：路上没有命令串，也没有对 `<local>` 的那句拒绝 ────
    let freed = [
        "cc_bus_send",
        "cc_bus_broadcast",
        "cc_bus_kill",
        "check_cc_bus_agent_online",
    ];
    for name in freed {
        let body = fn_body(&code, name);
        assert!(
            body.chars().count() > 30,
            "`{name}` 的体只切出 {} 字 —— 抽取器坏了",
            body.chars().count()
        );
        let hits = shell_line_markers(&body);
        assert!(
            hits.is_empty(),
            "`{name}` 这条路上还有命令串的痕迹 {hits:?} —— 它没有真的改走原语"
        );
        assert!(
            !body.contains("refuse_local_write("),
            "`{name}` 还在对 `<local>` 说「本机做不了」—— 而它今天两侧同一条路"
        );
    }
    // ── ② `spawn` **仍旧拒**，而且拒得有自己的话 ────────────────────────
    let spawn = fn_body(&code, "cc_bus_spawn");
    assert!(
        spawn.contains("build_spawn_cmd(") && spawn.contains("exec_read("),
        "`cc_bus_spawn` 的远端路不再是「构造器 + exec_read」了 —— \n\
             daemon 帧面 10 条里**没有 spawn**（`K-R111 §C3` 现打），本件不许把它一起放行。\n\
             真要放行就单独开一件，把 daemon 那条原语先做出来。"
    );
    let head = format!("refuse_local_write(&origin, {}", "\"");
    let at = spawn
        .find(&head)
        .expect("`cc_bus_spawn` 不再对 `<local>` 说话了 —— 那是把它一起放行了");
    let reason: String = spawn[at + head.len()..]
        .chars()
        .take_while(|c| *c != '"')
        .collect();
    assert!(
        reason.chars().count() >= 3,
        "`cc_bus_spawn` 对本机说的那句话只抠出 {reason:?} —— 抽取器坏了，本条在空转"
    );
    // ── ③ 那句公共文案必须**把各自那件事填进去** ────────────────────────
    assert!(
        fn_body(&code, "refuse_local_write").contains("{what}"),
        "`refuse_local_write` 不再把「在做哪件事」填进那句话 —— 逐条就名存实亡了"
    );
    // ── ④ 反向自检：`read_cc_bus_inbox` 那条**确实**还在老路上 ──────────
    //     （它是「不是接线」那一档：daemon 侧没有 inbox 读口，`K-R111 §C3` 现打）
    let inbox = fn_body(&code, "read_cc_bus_inbox");
    assert!(
        !shell_line_markers(&inbox).is_empty(),
        "`read_cc_bus_inbox` 也走掉了 —— 本条此刻比较的是一个不存在的对照。\n\
             若真做了，把它从这条反向自检里挪出去，并去 `K-R111` 那把尺子上把刻度拧下来。"
    );
}

/// ★★ `KR112D1`：收掉那道 id 校验**没有丢，换了住址** —— 而且要走**生产入口本体**。
///
/// # 它替掉了什么，为什么不是放宽
///
/// 原文（`P4c-Y1`）打的是 `build_broadcast_cmd` / `build_kill_cmd` 这两个**构造器**〔散文墓碑〕，
/// 理由逐字：「只测 happy path 不够 —— 断言要落在**命令构造真的调了它**上」。
/// 本件把两条都改走 daemon 原语 ⇒ **那两个构造器整块删了**，原判据的参照物不存在。
///
/// ⇒ 这一条不扫源码、不看构造器：**真调一遍那两个 tauri 命令**，看它拒不拒。
/// 比原文强一格 —— 原文证的是「构造器会拒」，这一条证的是「**这条命令会拒**」。
///
/// ⚠ 射程：两格都在 `client_for` **之前**返回 ⇒ 与进程内那张登记表无关，不会飘。
#[test]
fn the_kill_entry_still_refuses_bad_ids_before_it_asks_anyone() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("建运行时");
    let local = crate::inbound_client::LOCAL_ORIGIN;
    // 收掉：非法 id 拒绝 —— **不能靠对端校验**，这一条的后果是杀掉一棵进程树。
    for bad in ["", "a b", "--help", "x;y", "../etc", "a'b"] {
        let r = rt.block_on(cc_bus_kill(local.into(), bad.into()));
        let e = r.expect_err(&format!("非法 id {bad:?} 竟然被放去 `bus-kill` 了"));
        assert!(
            e.contains("非法 agent id"),
            "非法 id {bad:?} 被拒了，但拒的理由不是「非法 id」：{e}\n\
                 —— 那说明它是掉进别的分支（没通道 / 太旧）才失败的，校验其实没跑到。"
        );
    }
    // 合法 id 要走过校验那一关（这一格证明上面那几条不是「什么都拒」）。
    let ok = rt.block_on(cc_bus_kill(local.into(), "proj_cc".into()));
    let e = ok.expect_err("测试机上没有 daemon 通道，这一格本来就该失败");
    assert!(
        !e.contains("非法 agent id"),
        "合法 id `proj_cc` 也被白名单拒了 —— 那道校验拒过头了：{e}"
    );
    // ⚠ **刻意不在这里断广播的空消息**（诚实边界）：测试进程里没有入方向通道，
    //   空消息与非空消息**都**会失败在「没通道」那一档 ⇒ 一句 `is_err()` 分不出
    //   它到底是被空消息校验拒的，还是被通道拒的。那是一次**假举证**。
    //   空消息那道校验今天住 `send_via_daemon`（广播是「列成员 + 逐个发」的组合），
    //   由 `the_send_path_asks_the_backend_instead_of_composing_a_shell_line`
    //   的现跑那一格钉着（`("proj_cc", "   ")` 两侧同拒）。
}

/// ★ P4c：对 `<local>` 诚实拒绝的那几条，**拒绝必须排在 `cfg_of` 之前**（排后面永远走不到）。
///
/// ⚠〔`K-R112` 09-13〕这条判据的**人群从两条缩到一条**，而缩的理由要写准：
/// `cc_bus_broadcast` 与 `cc_bus_kill` 不是「拒绝没了」，是**它们不再问远端配置** ——
/// 两侧同一条路，`cfg_of` 这个参照物在那两个函数体里不存在了。
/// 人群因此**从源码派生**（生产段里所有还调 `cfg_of(&origin)` 的 tauri 命令），
/// 不是手写的一张名单：手写名单看不见下一个新长出来的调用方。
#[test]
fn whoever_still_asks_for_a_remote_config_branches_on_local_first() {
    let code = non_test_code();
    let body_of = |name: &str| -> String {
        let at = code
            .find(name)
            .unwrap_or_else(|| panic!("生产段找不到 {name} —— 判据在空转"));
        let rest = &code[at..];
        let end = rest[1..]
            .find("\npub async fn ")
            .or_else(|| rest[1..].find("\npub fn "))
            .map(|k| k + 1)
            .unwrap_or_else(|| rest.len().min(1200));
        rest[..end].to_string()
    };
    // 人群派生：谁还在问远端配置。
    let mut askers: Vec<String> = Vec::new();
    for (i, _) in code.match_indices("pub async fn ") {
        let name: String = code[i + "pub async fn ".len()..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if fn_body(&code, &name).contains("cfg_of(&origin)") {
            askers.push(name);
        }
    }
    askers.sort();
    assert_eq!(
        askers,
        vec!["cc_bus_spawn".to_string(), "read_cc_bus_inbox".to_string()],
        "还在问远端配置的 cc-bus 命令变了。\n\
             **少了** ⇒ 有人把它改走了原语（好事）：把它从这条判据的期望里挪掉，\n\
             并去 `tests/evidence/K-R111-ruler.py` 把那条的刻度一起拧下来。\n\
             **多了** ⇒ 新长出一条远端专属的路，它必须先分本机。"
    );
    // 逐条：本机分支要排在 `cfg_of` 之前，而且要说清在做哪件事。
    for (name, what) in [
        ("cc_bus_spawn", "spawn 一个 agent"),
        ("read_cc_bus_inbox", "LOCAL_ORIGIN"),
    ] {
        let body = fn_body(&code, name);
        let refuse = [
            body.find("refuse_local_write(&origin, \""),
            body.find("origin == crate::inbound_client::LOCAL_ORIGIN"),
        ]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or_else(|| panic!("{name} 没有本机分支 —— `<local>` 会掉进 `cfg_of`"));
        let cfg = body
            .find("cfg_of(&origin)")
            .unwrap_or_else(|| panic!("{name} 里找不到 `cfg_of(&origin)`"));
        assert!(
            refuse < cfg,
            "{name} 的本机分支排在 `cfg_of` 后面 —— 永远走不到"
        );
        assert!(
            body[..refuse].contains(what) || body[refuse..].contains(what),
            "{name} 的本机分支没说清它在做什么（应含 {what:?}）"
        );
    }
}

// ════════════════════════════════════════════════════════════════════════
// `K-R112`（09-13）：收掉 / 广播 / 查在线三条改走 daemon 原语。
// 下面四条是 `KR112D1` 的机检。
// ════════════════════════════════════════════════════════════════════════

/// ★★ `KR112D1` 刀①：**收掉一个 agent 走的是 `bus-kill` 帧，不是拼出来的 shell 串。**
///
/// 判的是**这条路**（`cc_bus_kill` → `kill_via_daemon`）上有没有命令串 ——
/// 🔴 **不是**「文件里还有没有 `Command::new`」：`K-R111` 现打过，`cc_bus.rs` 那 9 处里
/// **6 处在文档注释里**、2 处在测试段，数它会同时假红与假绿。**判的是那一跳走哪条路。**
#[test]
fn the_kill_path_asks_the_backend_instead_of_composing_a_shell_line() {
    let code = non_test_code();
    // ── ① 这条路上没有命令串 ────────────────────────────────────────────
    let mut checked = 0usize;
    for name in ["cc_bus_kill", "kill_via_daemon"] {
        let body = fn_body(&code, name);
        assert!(
            body.chars().count() > 60,
            "`{name}` 的体只切出 {} 字 —— 抽取器坏了，本条在空转",
            body.chars().count()
        );
        let hits = shell_line_markers(&body);
        assert!(
            hits.is_empty(),
            "`{name}` 这条路上又出现了命令串的痕迹 {hits:?}。\n\
                 收掉 agent 归 daemon 的 `{BUS_KILL}` 原语 —— 拼一份 `cc-kill` 串走 SSH\n\
                 就是同一件事的第二份实现（`K33`「所有命令只许有一处」）。"
        );
        checked += 1;
    }
    assert_eq!(checked, 2, "只核到 {checked} 段 —— 本断言在空转");

    // ── ② 它真的调了那条原语，能力协商排在动手之前，校验没在搬家路上丢掉 ──
    let kill = fn_body(&code, "kill_via_daemon");
    let ask = kill
        .find("accepts(BUS_KILL)")
        .expect("`kill_via_daemon` 没有先问一句能力 —— 「这台的后端太旧」永远说不出口");
    let call = kill
        .find("BUS_KILL,")
        .expect("`kill_via_daemon` 没在调那条原语 —— 判据的参照物没了");
    assert!(ask < call, "能力协商排在真杀之后 —— 那就永远走不到");
    assert!(
        kill.contains("is_valid_bus_id(id)"),
        "那道随构造器一起被删的 id 白名单没跟着搬过来 —— \n\
             它的后果是**杀掉一棵进程树**，不能靠对端校验。"
    );

    // ── ③ 🔴 两条路的可观测行为等价：**origin 是入参，不是分支** ──────────
    for name in ["cc_bus_kill", "kill_via_daemon"] {
        let body = fn_body(&code, name);
        for forbidden in ["LOCAL_ORIGIN", "origin =="] {
            assert!(
                !body.contains(forbidden),
                "`{name}` 里出现了 `{forbidden}` —— 它又开始按 origin 分岔了"
            );
        }
    }
    // 现跑一趟：同一输入喂给本机与一台远端，**两句话逐字相同**。
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("建运行时");
    let local = crate::inbound_client::LOCAL_ORIGIN;
    for id in ["--help", "a b"] {
        let a = rt.block_on(cc_bus_kill(local.into(), id.into()));
        let b = rt.block_on(cc_bus_kill("kr112-no-such-host".into(), id.into()));
        assert_eq!(
            a, b,
            "同一输入（id={id:?}）在本机与远端上给了两种结果 —— 不等价"
        );
        assert!(a.is_err(), "这一格本来就该拒：id={id:?}");
    }
    // 要说到「这台机器」的那几句，也只准差一个称呼。
    let pairs = [
        (
            describe_no_channel_for(local, "收掉 agent", "x"),
            describe_no_channel_for("h1", "收掉 agent", "x"),
        ),
        (
            describe_daemon_too_old_for(local, BUS_KILL, "x"),
            describe_daemon_too_old_for("h1", BUS_KILL, "x"),
        ),
    ];
    for (l, r) in pairs {
        assert_ne!(l, r, "两句话逐字相同 —— 那说明称呼根本没进去，本格在空转");
        assert_eq!(
            l.replacen(&machine_label(local), "<M>", 1),
            r.replacen(&machine_label("h1"), "<M>", 1),
            "本机与远端不只差一个称呼 —— 那就是同一件事的第二种说法"
        );
    }
}

/// ★★ `KR112D1` 刀②的落点：**`bus-kill` 的三态不许被压成一句「已收掉」。**
///
/// daemon 那条原语刻意回三个字段（`control/cc_bus.rs` 逐字：「回值要说清到底动了什么」）。
/// 老那条 SSH 路把 `cc-kill` 的 stdout `trim` 一下就交给用户 —— 「杀了会话」与
/// 「身份对不上、只摘了陈旧登记」在那一坨里**分不开**。这一条钉住它们分得开。
#[test]
fn the_kill_reply_keeps_the_three_states_apart() {
    use serde_json::json;
    let killed =
        describe_kill_reply("a_cc", Some(&json!({"killed": true, "stale_only": false})));
    let stale =
        describe_kill_reply("a_cc", Some(&json!({"killed": false, "stale_only": true})));
    let nothing =
        describe_kill_reply("a_cc", Some(&json!({"killed": false, "stale_only": false})));
    let broken = describe_kill_reply("a_cc", Some(&json!({"nope": 1})));
    let none = describe_kill_reply("a_cc", None);
    let all = [&killed, &stale, &nothing];
    for (i, a) in all.iter().enumerate() {
        for b in all.iter().skip(i + 1) {
            assert_ne!(a, b, "两档被压成了同一句话");
        }
    }
    assert!(killed.contains("已收掉"), "{killed}");
    assert!(stale.contains("会话没动"), "{stale}");
    assert!(
        !stale.contains("已收掉"),
        "只摘了登记被说成了「已收掉」：{stale}"
    );
    assert!(
        !nothing.contains("已收掉"),
        "什么都没动被说成了「已收掉」：{nothing}"
    );
    // 🔴 形状不认识那一档：**不知道它动没动**，不许说成「没杀成」。
    assert_eq!(broken, none, "缺字段与整个 body 缺应当同档");
    assert!(broken.contains("不知道它到底动没动"), "{broken}");
    assert!(!broken.contains("已收掉"), "{broken}");
}

/// ★★ `KR112D1`：**「问不到」不许被渲染成「不在线」** —— 走生产入口本体验。
///
/// 这是删回落之后唯一要守的那件事。老探法（按名字 `tmux has-session`）删掉之后，
/// 「答不上」的出口只有 [`unknown_liveness`] 一处 ⇒ 「不在线」这个答案在这条路上
/// **造不出来**。本条从两侧钉：纯函数那一份说得对 ＋ 生产入口真的走它。
#[test]
fn an_unknown_liveness_is_never_rendered_as_dark() {
    let msg = unknown_liveness("h1", "a_cc", "这台的 daemon 通道没起来");
    assert!(msg.contains("问不到"), "{msg}");
    assert!(msg.contains("不是**不在线**"), "{msg}");
    assert!(msg.contains("h1"), "没说是哪台机器：{msg}");
    // 生产入口：没有通道时它给的是 `Err`（一句「问不到」），**不是** `Ok(false)`。
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("建运行时");
    let got = rt.block_on(check_cc_bus_agent_online(
        "kr112-no-such-host".into(),
        "a_cc".into(),
    ));
    match got {
        Ok(v) => panic!("问不到的时候它答了一个确定的 {v} —— 那正是这一条要拦的"),
        Err(e) => {
            assert!(e.contains("问不到"), "答不上时没说「问不到」：{e}");
            assert!(
                e.contains("不是**不在线**"),
                "答不上那句话没经 `unknown_liveness` —— 出口不止一处了：{e}"
            );
        }
    }
    // 非法 id 那一格**先于**通道判：它是调用方自己的责任，不许被说成「问不到」。
    let bad = rt.block_on(check_cc_bus_agent_online(
        "kr112-no-such-host".into(),
        "--help".into(),
    ));
    assert!(
        bad.unwrap_err().contains("非法 agent id"),
        "非法 id 没在问通道之前被拒 —— 那道白名单没跟着搬过来"
    );
}

/// ★★ `KR112D1`：**广播没有第二条路了**，而三个数分开说那份诚实还在。
#[test]
fn the_broadcast_has_no_ssh_fallback_left() {
    let code = non_test_code();
    let body = fn_body(&code, "cc_bus_broadcast");
    let hits = shell_line_markers(&body);
    assert!(
        hits.is_empty(),
        "`cc_bus_broadcast` 这条路上又出现了命令串的痕迹 {hits:?} —— \n\
             老 `cc-broadcast` 回潮了。那条脚本发给 `agents.tsv` 的**每一行**\n\
             （用户机器实测 86 行登记 / 8 个活着），正是 `P4f` 要治的那次事故。"
    );
    // 两侧同一句话：本机与远端不再各说各的。
    for forbidden in ["LOCAL_ORIGIN", "origin =="] {
        assert!(
            !body.contains(forbidden),
            "`cc_bus_broadcast` 里出现了 `{forbidden}` —— 它又开始按 origin 分岔了"
        );
    }
    // 三态分流仍在，而且三条都 return（同 `K-R72` 给 kill / send-keys 记的那条）。
    for arm in ["BroadcastRoute::NoChannel", "BroadcastRoute::Failed"] {
        assert!(
            body.contains(arm),
            "`cc_bus_broadcast` 少了 `{arm}` 那一臂 —— 两个读数被合并了：\n\
                 「一条都没发出去」与「已经发了一部分」压成一句，用户就不知道该不该重试。"
        );
    }
}

/// ★★ **解析 `bash` 的地方恰好一处，而且那一处不是「按裸名让操作系统猜」**〔ccbus-win 09-10〕。
///
/// # 它买的是什么
///
/// 09-10 云端那条红的根因是 `Command::new("bash")`：Windows 的进程创建把
/// `C:\Windows\System32` 排在 `PATH` 之前，而那里有一个 WSL 存根
/// （`actions/runner-images` #12646）。⇒ **裸名这一形本身就是缺陷**，
/// 不是「今天恰好没配好」。本条把它从生产段里彻底赶出去。
///
/// 第二半治的是「一段逻辑三种表示」：解析点一多，就会有人在第二处写个略有不同的候选表，
/// 而两份候选表会各自漂 —— 与 `local_shell_read` 头注里那条「一段逻辑、两种表示」同族。
///
/// # 🔴 铁律 12：修之前它一定红（静态推演，跑不了测试所以逐条推）
///
/// 本件之前，`local_shell_read` 的函数体逐字含
/// `let mut child = tokio::process::Command::new("bash")`，而生产段里
/// **没有任何** `fn resolve_bash` ⇒
/// · 第一条（`Command::new("bash")` 计数为 0）实得 1 ⇒ **红**；
/// · 第二条（`fn resolve_bash(` 恰好 1 处）实得 0 ⇒ **红**；
/// · 第四条（`local_shell_read` 窗口里有 `resolve_bash()?`）实得没有 ⇒ **红**。
///
/// # ⚠ 它的射程（写下来，别读成比它强）
///
/// 本条只看**本文件**。全仓另外两处 `bash` 起进程（`ccm_probe::probe_with`、
/// `launch.rs` 那条 `-lic` 的下游）**在 Windows 上根本不编译**，裸名在 POSIX 上是对的
/// ⇒ 本轮刻意不动它们。但「全仓不许有 Windows 够得到的裸名 bash」这条**仓级**判据
/// 今天**没有人立** —— 它的正确落点是 `write_site_registry::spawn_sites`
/// （那张表已经按 `Command::new(` 派生人群），不在本件写区。**已上报，别当它有。**
#[test]
fn the_bash_cc_bus_runs_is_resolved_in_exactly_one_place() {
    let code = non_test_code();
    assert_eq!(
        code.matches(concat!("Command::", "new(\"bash\")")).count(),
        0,
        "生产段又出现了按裸名起 bash —— Windows 上那会拿到 System32 里的 WSL 存根\n\
             （进程创建把系统目录排在 PATH 之前，PATH 怎么排都没用）。走 `resolve_bash()`。"
    );
    assert_eq!(
        code.matches("fn resolve_bash(").count(),
        1,
        "解析 `bash` 的生产取值口不是恰好一处 —— 两份候选表会各自漂"
    );
    assert_eq!(
        code.matches("fn resolve_bash_with(").count(),
        1,
        "纯函数半不是恰好一处"
    );
    // 平台那一格必须**复用**既有的唯一真相源，不许在本文件里再写一份 `cfg!(windows)`
    // —— `history::platform_is_windows` 的头注逐字写着「只有这一处说得出这句话」，
    // 而写在调用点上的 `cfg!(windows)` 是常量表达式、判据翻不动它（那次的刀实测全绿）。
    assert!(
        code.contains("crate::history::platform_is_windows()"),
        "平台那一格没走 `history::platform_is_windows` —— 本文件自己写 `cfg!(windows)` \n\
             会让判据翻不动它，而且那句话就有了第二个家。"
    );
    assert_eq!(
        code.matches(concat!("cfg!(", "windows)")).count(),
        0,
        "本文件自己写了 `cfg!(windows)` —— 那句话只准有一个家"
    );
    let at = code
        .find("async fn local_shell_read(")
        .expect("生产段找不到本机执行口 —— 判据在空转");
    let body: String = code[at..].chars().take(1200).collect();
    assert!(
        body.contains("resolve_bash()?"),
        "本机执行口没有先解析 `bash` —— 判据的参照物没了。实得窗口：{body}"
    );
}

/// ★★ **找不到 `bash` 要响亮地失败，不许退化成「读到空」**〔ccbus-win 09-10〕。
///
/// 那正是本件上半场那个缺陷换个地方重演：一次读不到，被渲染成「一个 agent 都没有」。
///
/// # 为什么这条在 Linux 上也跑得到（这是刻意设计的）
///
/// `resolve_bash_with` 把**平台 / 环境 / 盘上有没有**三样全收成入参。若写成
/// `#[cfg(windows)]`，本条在 Linux 上就一格都量不到，而本仓 `launch.rs` 头注逐字记着
/// 那次教训：`cfg!(windows)` 是常量表达式，刀「`cfg!(windows)` → `false`」实测**全绿**。
///
/// # 🔴 铁律 12：修之前它一定红
///
/// 本件之前 `resolve_bash_with` **根本不存在** ⇒ 编译不过 ⇒ 红。
/// 而更要紧的是**行为**：那时 `local_shell_read` 拿到的是裸 `"bash"`，
/// 「这台机器上没有可用的 bash」这一形**根本没有任何代码路径会回 `Err`**
/// —— 它会成功起一个存根，然后回 `Ok("")`。⇒ 第一格要的那个 `Err` 当时造不出来。
///
/// # 反向：不许恒错
///
/// 第二格（候选表里有一条真在盘上）与第四格（POSIX）都要求 `Ok` ——
/// 一个「一律报错」的糊涂修法在那两格当场红。
#[test]
fn a_bash_that_cannot_be_found_is_a_loud_error_not_a_silent_empty_read() {
    use std::ffi::OsString;
    use std::path::{Path, PathBuf};
    let no_env = |_: &str| -> Option<OsString> { None };
    let nothing_exists = |_: &Path| false;

    // ① Windows + 一条候选都不在盘上 ⇒ **响亮失败**，且说得出「不是没有 agent」。
    let e = resolve_bash_with(true, &no_env, &nothing_exists)
        .expect_err("找不到 bash 必须是错，不是一个能跑的裸名");
    assert!(e.contains("找不到可用的 `bash`"), "{e}");
    assert!(
        e.contains("不是"),
        "错误没写明它不是「一个 agent 都没有」—— 那就是上半场那个缺陷换个地方重演：{e}"
    );
    assert!(
        e.contains(BASH_OVERRIDE_VAR),
        "响亮失败没给逃生口 —— 那就从「诚实」变成了「装了也用不了」：{e}"
    );

    // ② **非空对照**：候选表里那条有读数的路径真在盘上 ⇒ 挑中它（证明本条不恒错）。
    let git_bash = r"C:\Program Files\Git\bin\bash.exe";
    let only_git = |p: &Path| p.to_string_lossy() == git_bash;
    let picked = resolve_bash_with(true, &no_env, &only_git).expect("盘上有就该挑中");
    assert_eq!(picked, OsString::from(git_bash));

    // ③ 逃生口指到系统目录里那个存根 ⇒ **拒绝**（那正是根因本身）。
    let stub = r"C:\Windows\System32\bash.exe";
    let env_stub = |k: &str| (k == BASH_OVERRIDE_VAR).then(|| OsString::from(stub));
    let all_exist = |_: &Path| true;
    let e3 = resolve_bash_with(true, &env_stub, &all_exist).expect_err("存根必须被拒");
    assert!(e3.contains("WSL"), "拒了但没说清拒的是什么：{e3}");
    // 候选表里若混进同一个门牌，也一样挡住（不只逃生口那一条路）。
    assert!(is_system_dir_bash(&PathBuf::from(stub)));
    // 大小写与正斜杠都要认（`SysWOW64` / `Sysnative` 是同一个目录的另外两个门牌）。
    let syswow = PathBuf::from(r"C:/WINDOWS/SysWOW64/bash.exe");
    assert!(is_system_dir_bash(&syswow));
    assert!(!is_system_dir_bash(&PathBuf::from(git_bash)));

    // ④ **非空对照**：POSIX 上裸名是对的，不许被这条改坏
    //    （`execvp` 只查 PATH；写死 `/bin/bash` 会在 NixOS/Homebrew 上当场坏掉）。
    let posix = resolve_bash_with(false, &no_env, &nothing_exists).expect("POSIX 不该失败");
    assert_eq!(posix, OsString::from("bash"));

    // ⑤ 逃生口指了一个不存在的路径 ⇒ 报错，**不许悄悄回落到候选表**
    //    （回落会让用户以为自己指的那条生效了）。
    let missing = r"D:\nope\bash.exe";
    let env_missing = |k: &str| (k == BASH_OVERRIDE_VAR).then(|| OsString::from(missing));
    let e5 = resolve_bash_with(true, &env_missing, &only_git).expect_err("指错了要说");
    assert!(e5.contains(missing), "{e5}");

    // ⑥ 候选表**每一条都是绝对路径**——一个裸名都不许有。
    //    ⚠ 不能用 `Path::is_absolute()`：它在 Linux 上按 POSIX 判，
    //    会把 `C:\…` 判成相对路径，于是这一格在 Linux 上恒真、等于没测。
    let env_win = |k: &str| match k {
        "ProgramFiles" => Some(OsString::from(r"C:\Program Files")),
        "LOCALAPPDATA" => Some(OsString::from(r"C:\Users\u\AppData\Local")),
        _ => None,
    };
    let cands = windows_bash_candidates(&env_win);
    let n = cands.len();
    assert!(n >= 3, "候选表只剩 {n} 条 —— 抽取器坏了");
    // 去重真的发生了：`%ProgramFiles%` 展开出来的与写死那条**就是**同一个，
    // 留着重复会让「找过了哪些」那份清单当着用户的面说两遍同一句话。
    let mut uniq = cands.clone();
    uniq.sort();
    uniq.dedup();
    assert_eq!(uniq.len(), n, "候选表里有重复项");
    for c in &cands {
        let s = c.to_string_lossy().into_owned();
        assert!(
            s.contains(":\\") || s.starts_with("\\\\"),
            "候选 {s:?} 不是绝对路径 —— 裸名会被系统目录抢走，那正是本件的病根"
        );
        assert!(s.ends_with("bash.exe"), "候选 {s:?} 指的不是 bash.exe");
    }
    assert!(
        cands.iter().any(|c| c.to_string_lossy() == git_bash),
        "候选表里没有 `{git_bash}` —— 那是唯一一条有读数的路径\n\
             （09-10 云端 run 的日志里 runner 自己给 bash 步骤的 shell 逐字就是它）"
    );
}

/// ★ P4a-Y3：**本机那条执行口，远端有的守卫一件都不许少。**
///
/// 本机看着「自家文件、能出什么事」—— 但 `agents.tsv` / `inbox` 都是只增文件，
/// 而 monitor 与它跑在**同一台机器**上，撑爆的是用户正在用的那个进程。
///
/// ⇒ 逐件对着远端那条核：上限（多读一字节才分得清「刚好满」与「其实还有」）·
/// 超时 · 溢出两档各自有处置。
#[test]
fn the_local_cc_bus_read_keeps_every_guard_the_remote_one_has() {
    let code = non_test_code();
    let body = |name: &str| -> String {
        let at = code
            .find(name)
            .unwrap_or_else(|| panic!("生产段找不到 {name} —— 判据在空转"));
        // 取到下一个顶层 `}` 之后一点，够覆盖函数体即可。
        code[at..].chars().take(2600).collect()
    };
    let local = body("async fn local_shell_read(");
    let remote = body("async fn exec_read(");
    for (what, needle) in [
        ("上限（多读一字节）", "take(cap + 1)"),
        ("超时", "tokio::time::timeout"),
        ("溢出·拒收", "OnOverflow::Reject"),
        ("溢出·截断", "OnOverflow::Truncate"),
    ] {
        assert!(
            remote.contains(needle),
            "远端那条 `exec_read` 里找不到{what}（`{needle}`）—— 本判据的参照物没了，它此刻在空转"
        );
        assert!(
            local.contains(needle),
            "本机那条 `local_shell_read` 缺{what}（`{needle}`）。\n\
                 远端有而本机没有 = 「本地 = 不走 ssh 的远端」这句话在这一格是假的。"
        );
    }
}

/// **重要-6 的守卫**：「定值命令零插值」此前断言打在常量上，
/// 没有任何东西守「`fetch_remote_cc_bus` 原样把它交出去」。往里塞一个 `format!` 就穿了。
#[test]
fn cat_command_reaches_ssh_unmodified() {
    // **断言打在调用点**：光断言 `CC_BUS_CAT_CMD` 这个常量长得干净不够
    // （B03 审计重要-6），得守住"它原样到达 SSH"——往中间塞一层 format! 就穿了。
    // **不能**断言"函数体内没有 format!"：那过宽，错误消息用 format! 是正当的
    // （我第一版就是这么写的，当场假红）。
    let code = non_test_code();
    assert!(
        code.contains("connect_and_exec_cmd(cfg, CC_BUS_CAT_CMD)"),
        "定值命令必须原样交给 SSH（不得包 format!/push_str）"
    );
    // ★ P4a（08-12）：**从「那一处长这样」升成「每一处都是原样传参」**。
    //
    // 本条原来钉的是一个**硬编码的调用形状**（`connect_and_exec_cmd(cfg, CC_BUS_CAT_CMD)`）
    // 加一个计数 2。P4a 给本机加了第二条传输路（同一条串，只是不包进 ssh）之后，
    // 计数当场红 —— **那是对的**，它逐字问的正是「是否多了第二条构造路径」。
    // 但答案是「多了第二条**传输**路、串没变」，⇒ 光把 2 改成 3 会让本条退回
    // 「只盯着远端那一处」：本机那处塞个 `format!` 它照样绿。
    //
    // 现在逐处核**用法**：每一次出现要么是定义处，要么是**裸着当实参传**
    // （前面是 `(`/`,`/空白，后面是 `,`/`)`）。拼接、`format!`、`push_str` 都会破坏这个形状。
    let mut sites = 0usize;
    let mut from = 0usize;
    while let Some(rel) = code[from..].find("CC_BUS_CAT_CMD") {
        let at = from + rel;
        let end = at + "CC_BUS_CAT_CMD".len();
        from = end;
        let before = code[..at].chars().next_back().unwrap_or(' ');
        let after = code[end..].chars().next().unwrap_or(' ');
        // 定义处：`const CC_BUS_CAT_CMD: &str = …`
        if after == ':' {
            continue;
        }
        sites += 1;
        assert!(
            matches!(before, '(' | ',' | ' ' | '\n' | '\t'),
            "`CC_BUS_CAT_CMD` 第 {sites} 处用法前面是 {before:?} —— 它被拼进了别的东西，\n\
                 而本条的全部意义是「定值命令原样到达执行口」。"
        );
        assert!(
            matches!(after, ',' | ')'),
            "`CC_BUS_CAT_CMD` 第 {sites} 处用法后面是 {after:?} —— 同上，它没有裸着当实参传。"
        );
    }
    // 计数仍然守着「有没有人新开一条路」——只是现在它不再是唯一的防线。
    assert_eq!(
        sites, 2,
        "传输路条数变了（今天两条：远端 ssh + 本机 bash）。\n\
             新增一条要回来改这个数，并确认它也是**原样传参**。"
    );
}

#[test]
fn inbox_missing_fields_degrade_not_panic() {
    let (m, sk) = parse_inbox_jsonl(r#"{"text":"orphan"}"#);
    assert_eq!(sk, 0);
    assert_eq!(m[0].from, "");
    assert_eq!(m[0].text, "orphan");
}

/// `P4a2`：「为什么这条读面还没走 daemon」的**读数**必须留在代码里。
///
/// # 为什么这段散文值得一条判据
///
/// 它挡的是**重复摸底**：这个问题（「省一次握手不好吗」）已经被问过两轮，
/// 而两轮的结论都靠一个**具体数**（180ms）与一个**具体代价**（把正要变的文件格式
/// 焊进 daemon）。删掉那个数，下一个人只能靠感觉重答一遍。
///
/// ★ 更要紧的是钉住**解锁条件的实质**：`P4a2` 原文写的是「`P4b` 落地之后」，
/// 而 `P4b` 08-12 已签收 —— 照字面读就该开工了。但它只删掉了 cc-spawn 的复用判定，
/// **`agents.tsv` 的格式契约一字未动**。⇒ 判据钉「解锁条件不是『P4b 落地』」这句话在。
#[test]
fn why_the_read_face_is_not_on_the_daemon_yet_stays_measured() {
    let prod = guard_core::production_source(include_str!("../../src/bridge/src/cc_bus.rs"));
    for needle in ["180ms", "格式契约稳下来", "解锁条件不是"] {
        assert!(
            prod.contains(needle),
            "读面头注里少了「{needle}」—— 那段是 `P4a2` 摸底的全部产出，\
                 删了它下一个人会拿感觉重答一遍「省一次握手不好吗」"
        );
    }
}
