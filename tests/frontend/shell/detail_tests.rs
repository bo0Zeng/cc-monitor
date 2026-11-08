//! 壳这一端写的复制详情：远端拒绝补「本机」、本机拒绝不补；通道断了壳写全份；壳命令失败那一形。

use super::*;
use crate::origin::Origin;

fn label(k: &str) -> String {
    copy_text(k, &[])
}

#[test]
fn a_remote_refusal_gets_a_local_line_and_a_local_one_does_not() {
    let remote = format!("{}：kill_failed", label("detail.label.code"));
    let got = relayed(&Origin::named("devbox"), &remote);
    assert!(got.starts_with(&remote), "{got}");
    assert!(
        got.contains(&format!("\n{}：cc-monitor ", label("detail.label.local"))),
        "{got}"
    );
    assert_eq!(relayed(&Origin::local(), &remote), remote);
}

#[test]
fn the_refusal_body_detail_is_read_and_missing_reads_empty() {
    assert_eq!(
        of_refusal_body(br#"{"code":"x","said":"m","detail":"d"}"#),
        "d"
    );
    assert_eq!(of_refusal_body(br#"{"code":"x","said":"m"}"#), "");
    assert_eq!(of_refusal_body(b"not json"), "");
}

#[test]
fn a_channel_failure_detail_names_the_machine_command_hop_and_code() {
    let e = w::CallError::Hop {
        at: w::HopId {
            idx: 1,
            tag: "open",
        },
        reach: w::Reach::NotSent,
        why: w::HopFault::Unreachable,
    };
    let d = of_channel(&Origin::named("devbox"), "accounts-list", &e);
    for want in [
        format!(
            "{}：devbox（{}）",
            label("detail.label.machine"),
            label("detail.value.notConnected")
        ),
        format!("{}：accounts-list", label("detail.label.command")),
        format!("{}：1:open NotSent", label("detail.label.hop")),
        format!("{}：Unreachable", label("detail.label.code")),
        format!("{}：cc-monitor ", label("detail.label.local")),
    ] {
        assert!(d.contains(&want), "缺「{want}」：\n{d}");
    }
    let local = of_channel(&Origin::local(), "kill", &w::OursFault::Broken.into());
    assert!(
        !local.contains(&format!("{}：cc-monitor", label("detail.label.local"))),
        "{local}"
    );
    assert!(
        local.contains(&format!("{}：Broken", label("detail.label.code"))),
        "{local}"
    );
}

#[test]
fn a_shell_command_failure_is_one_shape_with_a_nonempty_detail() {
    let s = Said::with_raw("读取配置失败".into(), "os error 2");
    // 投出去那一形（`named`）：码 · 那一句 · 复制详情三格，没有按码定形的 `data` 就不出那一格。
    let f = s.named("load_config");
    assert_eq!(f.said, "读取配置失败");
    assert_eq!(f.code, cmd_wire::failed::FALLBACK_CODE);
    let lines: Vec<&str> = f.detail.lines().collect();
    assert_eq!(lines.len(), 4, "{}", f.detail);
    assert!(
        lines[0].starts_with(&format!("{}：", label("detail.label.at"))),
        "{}",
        f.detail
    );
    assert_eq!(
        lines[1],
        format!("{}：{}", label("detail.label.local"), local_line())
    );
    assert_eq!(
        lines[2],
        format!("{}：load_config", label("detail.label.command"))
    );
    assert_eq!(
        lines[3],
        format!("{}：os error 2", label("detail.label.raw"))
    );
    let v = serde_json::to_value(&f).unwrap();
    let keys: std::collections::BTreeSet<&str> =
        v.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, ["code", "detail", "said"].into(), "{v}");
    // 带码的那一形：码进 `code` 与详情「码」那一项（命令之后、原话之前）。
    let coded = Said::with_raw("x".into(), "boom")
        .coded("2")
        .named("open_log_dir");
    assert_eq!(coded.code, "2");
    let at = |k: &str| coded.detail.find(&format!("{}：", label(k))).unwrap();
    assert!(
        at("detail.label.command") < at("detail.label.code"),
        "{}",
        coded.detail
    );
    assert!(
        at("detail.label.code") < at("detail.label.raw"),
        "{}",
        coded.detail
    );
}

/// 问后端一条命令没成：后端答了失败 ⇒ 它交来的那一份原样（码 · 详情 · `data`，壳不拆、不补）；
/// 没走到那台 ⇒ 壳自己的错：时刻 · 本机 · 命令（问的那一条）· 码。
#[test]
fn a_backend_call_failure_is_handed_on_as_the_backend_wrote_it() {
    use crate::inbound_client::CallError;
    let wrote = format!(
        "{}：refused\n{}：uname: not found",
        label("detail.label.code"),
        label("detail.label.raw")
    );
    let theirs = cmd_wire::failed::Failed {
        code: "refused".into(),
        said: "box 系统未知".into(),
        detail: wrote.clone(),
        data: Some(serde_json::json!({"k": 1})),
    };
    let f = of_call(
        "box 系统未知".into(),
        "deploy-plan",
        &CallError::Remote(theirs.clone()),
    );
    assert_eq!(f, theirs, "后端那一份原样");
    let f = of_call("y".into(), "deploy-plan", &CallError::Disconnected);
    assert_eq!(f.said, "y");
    assert_eq!(f.code, "disconnected");
    assert!(
        f.detail
            .contains(&format!("{}：deploy-plan", label("detail.label.command"))),
        "{}",
        f.detail
    );
    assert!(
        f.detail
            .ends_with(&format!("{}：disconnected", label("detail.label.code"))),
        "{}",
        f.detail
    );
}

/// 换一句给人看的话：码 · 原话照旧。
#[test]
fn restating_keeps_the_code_and_the_raw_words() {
    let s = Said::restate(
        "外层".into(),
        Said::with_raw("里层".into(), "boom").coded("c"),
    );
    assert_eq!(s.said, "外层");
    assert_eq!(s.raw(), "boom");
    assert_eq!(s.code.as_deref(), Some("c"));
    assert_eq!(Said::from("x".to_string()).raw(), "");
}

/// 壳命令签名上的错型（交出去那一形）。
const FAILED: &str = "cmd_wire::failed::Failed";

/// 错型是不是交出去那一形：写全路径，或者那一份 `use cmd_wire::failed::Failed;` 进口了它、写短名。
fn is_failed(e: &str, prod: &str) -> bool {
    e == FAILED || (e == "Failed" && prod.contains("use cmd_wire::failed::Failed;"))
}

/// 一份源码里全部 `#[tauri::command]`：(名字, 签名到 `{` 为止, 整块)。
fn shell_commands(prod: &str) -> Vec<(String, String, String)> {
    let starts: Vec<usize> = prod
        .match_indices("#[tauri::command]")
        .map(|(i, _)| i)
        .collect();
    let mut out = Vec::new();
    for (k, at) in starts.iter().enumerate() {
        let end = starts.get(k + 1).copied().unwrap_or(prod.len());
        let chunk = &prod[*at..end];
        let Some(f) = chunk.find("fn ") else { continue };
        let name: String = chunk[f + 3..]
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let sig_end = chunk[f..].find('{').map_or(chunk.len(), |b| f + b);
        out.push((name, chunk[..sig_end].to_string(), chunk.to_string()));
    }
    out
}

/// 签名里返回的 `Result<…, E>` 的 `E`（没有 `Result` ⇒ `None`）。
fn error_type(sig: &str) -> Option<String> {
    let ret = &sig[sig.find("->")?..];
    let open = ret.find("Result<")? + "Result<".len();
    let (mut depth, mut last_comma) = (1, open);
    for (i, c) in ret[open..].char_indices() {
        match c {
            '<' | '(' => depth += 1,
            '>' | ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(ret[last_comma..open + i].trim().to_string());
                }
            }
            ',' if depth == 1 => last_comma = open + i + 1,
            _ => {}
        }
    }
    None
}

/// 回通道那一形 `Fail` 的壳命令（失败照 `chan.ts::decodeFail` 解回三层）：`chan_call` · 聊天附件逐块送进那台暂存区那一条（每一问就是 `call_via`）。
const FAILING_AS_CHANNEL: [&str; 2] = ["chan_call", "attach_stage"];

/// 壳命令的失败只一形：签名回 `Result` 的，错型只许交出去那一形 `cmd_wire::failed::Failed`（[`FAILING_AS_CHANNEL`] 那两条回通道那一形 `Fail`）。不合的那几条。
fn commands_failing_otherwise(prod: &str) -> Vec<String> {
    shell_commands(prod)
        .into_iter()
        .filter_map(|(name, sig, _)| {
            let e = error_type(&sig)?;
            let ok = if FAILING_AS_CHANNEL.contains(&name.as_str()) {
                e == "Fail"
            } else {
                is_failed(&e, prod)
            };
            (!ok).then(|| format!("{name} → {e}"))
        })
        .collect()
}

/// 每条回失败的壳命令，最外层都经 `.named("<它自己的名字>")` 一次（复制详情里有「命令」那一项）；
/// 回 `Result` 的壳命令错型只许 `cmd_wire::failed::Failed`（[`FAILING_AS_CHANNEL`] 许 `Fail`）。人群：生产源码里全部 `#[tauri::command]`。
#[test]
fn every_shell_command_fails_as_said_and_names_itself() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let (mut seen, mut bad, mut other) = (0usize, Vec::new(), Vec::new());
    for (p, text) in guard_core::scan_tree_excluding(&root, &["rs"], &[]) {
        let prod = guard_core::production_code(&text);
        let file = p.file_name().unwrap().to_string_lossy().to_string();
        other.extend(
            commands_failing_otherwise(&prod)
                .into_iter()
                .map(|c| format!("{file}::{c}")),
        );
        for (name, sig, chunk) in shell_commands(&prod) {
            if !error_type(&sig).is_some_and(|e| is_failed(&e, &prod)) {
                continue;
            }
            seen += 1;
            if !chunk.contains(&format!(".named(\"{name}\")")) {
                bad.push(format!("{file}::{name}"));
            }
        }
    }
    assert!(seen >= 29, "人群只扫到 {seen} 条 —— 取法坏了");
    assert!(
        bad.is_empty(),
        "这几条壳命令失败时复制详情里没有命令名：{bad:?}"
    );
    assert!(
        other.is_empty(),
        "这几条壳命令的错不是交出去那一形（码 · 句子 · 复制详情）：{other:?}"
    );
    // 正控：一条回 `Result<(), String>` 的命令要被认出来；`chan_call` 回 `Fail` 不算。
    let fixture = "#[tauri::command]\nfn bare() -> Result<(), String> {\n}\n\
                   #[tauri::command]\npub async fn chan_call(a: u8) -> Result<tauri::ipc::Response, Fail> {\n}\n\
                   #[tauri::command]\nfn fine() -> Result<Vec<(u8, String)>, cmd_wire::failed::Failed> {\n}\n";
    assert_eq!(commands_failing_otherwise(fixture), vec!["bare → String"]);
}

/// 壳生产段不把下层的错拼成那一句（`map_err(|e| format!(…))`）：句子取文案表的原因词，原话进详情（[`Said::with_raw`]）。
fn format_into_said(prod: &str) -> Vec<usize> {
    prod.lines()
        .enumerate()
        .filter(|(_, l)| guard_core::contains_word(l, "map_err(|e| format!("))
        .map(|(i, _)| i + 1)
        .collect()
}

#[test]
fn no_shell_error_is_a_formatted_lower_error() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = guard_core::scan_tree_excluding(&root, &["rs"], &[]);
    assert!(files.len() > 50, "扫描面塌了：{}", files.len());
    let bad: Vec<String> = files
        .iter()
        .flat_map(|(p, text)| {
            format_into_said(&guard_core::production_code(text))
                .into_iter()
                .map(move |n| format!("{}:{n}", p.display()))
        })
        .collect();
    assert!(
        bad.is_empty(),
        "这几处把下层原话拼进了那一句（改 `Said::with_raw(<原因词那一句>, e)`）：\n{}",
        bad.join("\n")
    );
    assert_eq!(
        format_into_said("ok\n    .map_err(|e| format!(\"x failed: {e}\"))?;\n"),
        vec![2],
        "正控没认出来 —— 量法坏了"
    );
}

/// 打开日志那一下起不来：句子是「打开失败 · 原因词」，系统原话只进详情。
#[test]
fn a_failed_open_says_the_reason_and_keeps_the_raw_words_in_the_detail() {
    let bin = "/nonexistent-ccm-opener/xdg-open";
    let e = crate::open_with(bin, &[]).expect_err("不存在的程序起不来");
    assert_eq!(
        e.said,
        copy_text(
            "rsShellCmd.open.failed",
            &[(
                "why",
                &copy_core::spawn_reason(std::io::ErrorKind::NotFound)
            )]
        )
    );
    assert!(!e.said.contains(bin), "{}", e.said);
    let detail = e.named("open_log_dir").detail;
    assert!(
        detail.contains(&format!("{}：{bin}", label("detail.label.raw"))),
        "{detail}"
    );
}

/// 补上的「命令」排在原话前面（项名次序照 `Label::ALL`）。
#[test]
fn the_named_command_goes_before_the_raw_words() {
    let s = Said::with_raw("x".into(), "boom").named("open_log_dir");
    let lines: Vec<&str> = s.detail.lines().collect();
    let at = |k: &str| {
        lines
            .iter()
            .position(|l| l.starts_with(&format!("{}：", label(k))))
            .unwrap()
    };
    assert!(
        at("detail.label.command") < at("detail.label.raw"),
        "{}",
        s.detail
    );
}

/// 原话里恰好有一行以项名打头（多行 stderr）：补「命令」照项名次序插在原话前面，不插进原话中间（不拆渲染好的字）。
#[test]
fn naming_does_not_split_raw_words_that_look_like_items() {
    let raw = format!(
        "line one\n{}：not a code\n{}：not a command\n{}：not a path",
        label("detail.label.code"),
        label("detail.label.command"),
        label("detail.label.path")
    );
    let s = Said::with_raw("x".into(), &raw).named("open_log_dir");
    let cmd = format!("{}：open_log_dir", label("detail.label.command"));
    let raw_line = format!("{}：line one", label("detail.label.raw"));
    let at = |w: &str| {
        s.detail
            .find(w)
            .unwrap_or_else(|| panic!("缺「{w}」：{}", s.detail))
    };
    assert!(at(&cmd) < at(&raw_line), "{}", s.detail);
    assert!(s.detail.ends_with(&raw), "原话被拆开了：{}", s.detail);
}

/// 通道宿主把对端的拒绝交出去之前（主界面与文件窗口同一口）：远端 ⇒ 体里那份详情补「本机」一行；本机 ⇒ 原样；别的层原样。
#[test]
fn a_relayed_refusal_gets_the_local_line_once_at_the_host() {
    let wrote = format!("{}：io_failed", label("detail.label.code"));
    let body =
        serde_json::to_vec(&serde_json::json!({"code": "io_failed", "said": "m", "detail": wrote}))
            .unwrap();
    let refused = || w::err_from_wire(w::WireErr::Refused, body.clone());
    let detail_of = |e: &w::CallError| match e {
        w::CallError::Peer {
            why: w::PeerFault::Refused { body },
        } => of_refusal_body(&body.0),
        other => panic!("不是拒绝：{other:?}"),
    };
    let far = relay_refusal(&Origin::named("devbox"), refused());
    let d = detail_of(&far);
    assert!(d.starts_with(&wrote), "{d}");
    assert_eq!(
        d.matches(&format!("{}：cc-monitor ", label("detail.label.local")))
            .count(),
        1,
        "{d}"
    );
    assert_eq!(
        detail_of(&relay_refusal(&Origin::local(), refused())),
        wrote
    );
    let hop: w::CallError = w::OursFault::Broken.into();
    assert_eq!(
        format!("{:?}", relay_refusal(&Origin::named("devbox"), hop.clone())),
        format!("{hop:?}")
    );
}

/// 壳命令里 `spawn_blocking` 那根线程没回来（崩了 / 被取消）：那一句一律是 [`Said::crashed`]（「程序出错」那一形），
/// join 的原话只进复制详情 —— 不许 `e.to_string()` / `format!` 把英文原话直接当那一句。
/// 人群：壳生产段全部 `.rs`，`spawn_blocking(` 之后第一个 `.await` 紧跟的 `.map_err(…)`。
#[test]
fn a_blocking_thread_that_never_came_back_is_said_as_crashed() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let files = guard_core::scan_tree_excluding(&root, &["rs"], &[]);
    assert!(files.len() > 50, "扫描面塌了：{}", files.len());
    let mut bad = Vec::new();
    let mut seen = 0;
    for (p, text) in &files {
        let lines: Vec<&str> = text.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            if !guard_core::contains_word(l, "spawn_blocking(") || l.trim_start().starts_with("//")
            {
                continue;
            }
            // 那根线程的结局：本行起第一个 `.await` 之后紧跟的那一个 `.map_err(…)`（闭包体可以有好几行）。
            let Some(k) = (i..lines.len().min(i + 90))
                .find(|&k| guard_core::contains_word(lines[k], ".await"))
            else {
                continue;
            };
            for (j, m) in lines.iter().enumerate().skip(k).take(2) {
                if !guard_core::contains_word(m, ".map_err(") {
                    continue;
                }
                seen += 1;
                if guard_core::contains_word(m, "|e| e.to_string()")
                    || guard_core::contains_word(m, "join error")
                {
                    bad.push(format!("{}:{}  {}", p.display(), j + 1, m.trim()));
                }
                break;
            }
        }
    }
    assert!(
        seen >= 10,
        "一处 `spawn_blocking … .map_err` 都没量到（{seen}）—— 扫法坏了"
    );
    assert!(
        bad.is_empty(),
        "这几处把 join 的原话当那一句（改 `.map_err(Said::crashed)`）：\n{}",
        bad.join("\n")
    );
}
