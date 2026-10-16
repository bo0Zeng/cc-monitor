//! 壳这一端写的复制详情：远端拒绝补「本机」、本机拒绝不补；通道断了壳写全份；壳命令失败那一形。

use super::*;
use crate::origin::Origin;

fn label(k: &str) -> String {
    copy_text(k, &[])
}

#[test]
fn a_remote_refusal_gets_a_local_line_and_a_local_one_does_not() {
    let remote = format!("{}：kill_failed", label("detail.label.code"));
    let got = relayed(&Origin("devbox".into()), &remote);
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
        of_refusal_body(br#"{"code":"x","message":"m","detail":"d"}"#),
        "d"
    );
    assert_eq!(of_refusal_body(br#"{"code":"x","message":"m"}"#), "");
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
    let d = of_channel(&Origin("devbox".into()), "accounts-list", &e);
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
    let s = Said::new("读取配置失败", "load_config", Some("os error 2"));
    assert_eq!(s.said, "读取配置失败");
    assert!(s
        .detail
        .contains(&format!("{}：load_config", label("detail.label.command"))));
    assert!(s
        .detail
        .contains(&format!("{}：os error 2", label("detail.label.raw"))));
    let v = serde_json::to_value(&s).unwrap();
    assert_eq!(v.as_object().unwrap().len(), 2, "{v}");
}

/// 问后端一条命令没成：那台写了详情 ⇒ 原样带上（远端补「本机」）；没写（老后端 / 没走到那台）⇒ 壳写时刻 · 本机 · 命令。
#[test]
fn a_backend_call_failure_keeps_the_detail_the_backend_wrote() {
    use crate::inbound_client::CallError;
    let wrote = format!(
        "{}：refused\n{}：uname: not found",
        label("detail.label.code"),
        label("detail.label.raw")
    );
    let refused = CallError::Remote {
        code: "refused".into(),
        message: "box 系统未知".into(),
        detail: wrote.clone(),
        data: None,
    };
    let s = Said::of_call(
        "box 系统未知".into(),
        "deploy-plan",
        &Origin::local(),
        &refused,
    );
    assert_eq!(s.said, "box 系统未知");
    assert_eq!(s.detail, wrote, "本机那台写的那份原样");
    let far = Said::of_call(
        "x".into(),
        "deploy-plan",
        &Origin("devbox".into()),
        &refused,
    );
    assert!(far.detail.starts_with(&wrote), "{}", far.detail);
    assert!(
        far.detail
            .contains(&format!("\n{}：cc-monitor ", label("detail.label.local"))),
        "{}",
        far.detail
    );
    let s = Said::of_call(
        "y".into(),
        "deploy-plan",
        &Origin::local(),
        &CallError::Disconnected,
    );
    assert!(
        s.detail
            .contains(&format!("{}：deploy-plan", label("detail.label.command"))),
        "{}",
        s.detail
    );
}

/// 已有「命令」那一项（自己写的 · 那台后端写的）⇒ 不补第二项；只有时刻 · 本机那两项 ⇒ 补上命令名。
#[test]
fn naming_a_shell_command_failure_adds_the_command_once() {
    let cmd = |d: &str| {
        d.lines()
            .filter(|l| l.starts_with(&format!("{}：", label("detail.label.command"))))
            .count()
    };
    let bare = Said::written("读取失败".to_string()).named("load_config");
    assert_eq!(cmd(&bare.detail), 1, "{}", bare.detail);
    assert!(
        bare.detail
            .ends_with(&format!("{}：load_config", label("detail.label.command"))),
        "{}",
        bare.detail
    );
    let own = Said::new("x", "deploy-plan", Some("raw")).named("deploy_remote_backend");
    assert_eq!(cmd(&own.detail), 1, "{}", own.detail);
    assert!(own.detail.contains("deploy-plan"), "{}", own.detail);
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

/// 壳命令的失败只一形：签名回 `Result` 的，错型只许 [`Said`]（`chan_call` 一条回通道那一形 `Fail`）。不合的那几条。
fn commands_failing_otherwise(prod: &str) -> Vec<String> {
    shell_commands(prod)
        .into_iter()
        .filter_map(|(name, sig, _)| {
            let e = error_type(&sig)?;
            let allowed = if name == "chan_call" { "Fail" } else { "Said" };
            (e != allowed).then(|| format!("{name} → {e}"))
        })
        .collect()
}

/// 每条返回 `Said` 的壳命令，最外层都经 `.named("<它自己的名字>")` 一次（复制详情里有「命令」那一项）；
/// 回 `Result` 的壳命令错型只许 `Said`（`chan_call` 许 `Fail`）。人群：生产源码里全部 `#[tauri::command]`。
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
            if !sig.contains("Said>") {
                continue;
            }
            seen += 1;
            if !chunk.contains(&format!(".named(\"{name}\")")) {
                bad.push(format!("{file}::{name}"));
            }
        }
    }
    assert!(seen >= 30, "人群只扫到 {seen} 条 —— 取法坏了");
    assert!(
        bad.is_empty(),
        "这几条壳命令失败时复制详情里没有命令名：{bad:?}"
    );
    assert!(
        other.is_empty(),
        "这几条壳命令的错不是 `Said`（句子 ＋ 复制详情）：{other:?}"
    );
    // 正控：一条回 `Result<(), String>` 的命令要被认出来；`chan_call` 回 `Fail` 不算。
    let fixture = "#[tauri::command]\nfn bare() -> Result<(), String> {\n}\n\
                   #[tauri::command]\npub async fn chan_call(a: u8) -> Result<tauri::ipc::Response, Fail> {\n}\n\
                   #[tauri::command]\nfn fine() -> Result<Vec<(u8, String)>, Said> {\n}\n";
    assert_eq!(commands_failing_otherwise(fixture), vec!["bare → String"]);
}

/// 壳生产段不把下层的错拼成那一句（`map_err(|e| format!(…))`）：句子取文案表的原因词，原话进详情（[`Said::with_raw`]）。
fn format_into_said(prod: &str) -> Vec<usize> {
    prod.lines()
        .enumerate()
        .filter(|(_, l)| l.contains("map_err(|e| format!("))
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
    assert!(
        e.detail
            .contains(&format!("{}：{bin}", label("detail.label.raw"))),
        "{}",
        e.detail
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
    let body = serde_json::to_vec(
        &serde_json::json!({"code": "io_failed", "message": "m", "detail": wrote}),
    )
    .unwrap();
    let refused = || w::err_from_wire(w::WireErr::Refused, body.clone());
    let detail_of = |e: &w::CallError| match e {
        w::CallError::Peer {
            why: w::PeerFault::Refused { body },
        } => of_refusal_body(&body.0),
        other => panic!("不是拒绝：{other:?}"),
    };
    let far = relay_refusal(&Origin("devbox".into()), refused());
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
        format!("{:?}", relay_refusal(&Origin("devbox".into()), hop.clone())),
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
