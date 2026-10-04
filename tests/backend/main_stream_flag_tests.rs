use super::split_stream_flags;

fn v(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

/// F66（#58③）★ §26 死循环护栏的**代码强制**：`CAPABILITIES` 里每个能力 token 的
/// CLI flag 都必须被 `split_stream_flags` 剥离——否则声明它 = 埋 monitor 侧死循环
/// （monitor 发该 flag → 本后端不剥 → 当一次性查询退出 → 无 hello → 重连死循环）。
/// 加新能力 token 时，若忘了在此登记它的 flag、或忘了给 `split_stream_flags` 加剥离
/// 分支，本测试红。把审计指出的「约定强制」拉回「代码强制」。
#[test]
fn every_capability_token_is_strippable() {
    // token → 它对应的 CLI flag（加新能力时同步扩这张表）
    fn flag_of(token: &str) -> &'static str {
        match token {
            "bg" => "--with-bg",
            "tail-only" => "--tail-only",
            other => panic!(
                "CAPABILITIES 声明了 token `{other}` 但此处无 flag 映射——加新能力必须在此登记它的 flag 并确认 split_stream_flags 剥离它（否则埋 §26 死循环）"
            ),
        }
    }
    for &token in super::CAPABILITIES {
        let flag = flag_of(token);
        let (rest, _) = split_stream_flags(v(&[flag]));
        assert!(
            rest.is_empty(),
            "能力 token `{token}` 的 flag `{flag}` 未被 split_stream_flags 剥离 → §26 死循环"
        );
    }
}

/// F25 DoD ③：流模式 flag 剥离后不残留（不会误入查询模式判定），且每条只置自己那一位。
/// 两半都要断：只断“置位了”会漏掉 §26 那条（不剥 ⇒ 当查询退出 ⇒ 无 hello），
/// 只断“剥干净了”会漏掉“剥掉了但忘了抬位”（客户端永远收不到要的那一格、而没任何信号）。
#[test]
fn flags_are_stripped_and_detected() {
    use super::StreamWants;
    let none = StreamWants::default();
    for (args, want) in [
        (
            v(&["--with-bg", "--tail-only"]),
            StreamWants {
                with_bg: true,
                tail_only: true,
                ..none
            },
        ),
        (
            v(&["--tail-only"]),
            StreamWants {
                tail_only: true,
                ..none
            },
        ),
        (v(&[]), none),
        (
            v(&["--with-pid"]),
            StreamWants {
                with_pid: true,
                ..none
            },
        ),
        (
            v(&["--with-raw"]),
            StreamWants {
                with_raw: true,
                ..none
            },
        ),
        (
            v(&["--stream", "--with-raw", "--tail-only"]),
            StreamWants {
                tail_only: true,
                with_raw: true,
                ..none
            },
        ),
    ] {
        let (rest, got) = split_stream_flags(args.clone());
        assert!(rest.is_empty(), "{args:?} 没被剥干净 → §26 死循环");
        assert_eq!(got, want, "{args:?}");
    }
}

/// 查询参数与流 flag 互不干扰：查询参数原样保留（顺带守住"flag 混进查询
/// 命令行也不会破坏查询"的边角）。
#[test]
fn query_args_pass_through() {
    let (rest, wants) = split_stream_flags(v(&["--read-session", "/p/s.jsonl", "--with-bg"]));
    assert_eq!(rest, v(&["--read-session", "/p/s.jsonl"]));
    assert!(wants.with_bg);
    assert!(!wants.tail_only && !wants.with_pid && !wants.with_raw);
}
