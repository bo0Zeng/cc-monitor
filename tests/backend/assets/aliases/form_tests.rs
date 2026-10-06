// 别名表单 ⇄ 一条别名：两向只住后端、按 ccm 自己的解析器走。
//
// 要求：「任何一条后端认得的 argv，转成表单再转回来逐字相等（含空格、含引号、含表单不认的旗标、含 `--` 两侧都有的）」——
// 体检那两例（带空格的透传被拆成两段 · 表单不认的 ccm 旗标被挪到 `--` 左边）是这里的头两条。
// 异源在哪：往返那一条拿的是原样的 argv（任意词序列，不经表单生成）；规范写法那一条拿表单拼出来的参数交 `argv::parse`（ccm 运行时那一份）判。

use super::*;
use crate::assets::aliases::{answer_from_form, answer_to_form, Alias};
use serde_json::{json, Value};

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn alias(args: &[&str]) -> Alias {
    Alias::new("mine", s(args))
}

/// 往返：表单不动 ⇒ 逐字相等；只改名字 ⇒ 参数逐字不变。
fn assert_round_trip(a: &Alias) {
    let f = to_form(a);
    assert_eq!(
        from_form(&f, Some(a)).as_ref(),
        Ok(a),
        "往返改了字：{a:?}\n表单：{f:?}"
    );
    let mut g = f.clone();
    g.name = "renamed".into();
    let b = from_form(&g, Some(a)).expect("只改名字");
    assert_eq!(
        (b.args, b.rest_to),
        (a.args.clone(), a.rest_to),
        "只改名字参数却变了：{a:?}"
    );
}

/// 体检那两例：带空格的透传不拆开；表单不认的 ccm 旗标留在 `--` 右边原位；改另一格也不挪它。
#[test]
fn the_two_audit_cases_round_trip_byte_for_byte() {
    let a = alias(&["--append-system-prompt", "be brief", "--", "--account", "z"]);
    assert_round_trip(&a);
    assert_eq!(to_form(&a).passthru, "--append-system-prompt 'be brief'");
    let b = alias(&["--", "--account-dir", "/srv/acc", "--cwd", "/w"]);
    assert_round_trip(&b);
    let mut f = to_form(&b);
    assert_eq!((f.cwd.as_str(), f.passthru.as_str()), ("/w", ""));
    f.cwd = "/v".into();
    assert_eq!(
        from_form(&f, Some(&b)).unwrap().args,
        s(&["--", "--account-dir", "/srv/acc", "--cwd", "/v"])
    );
}

/// 极小的确定性伪随机（不引依赖）。
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn pick<'a, T>(&mut self, v: &'a [T]) -> &'a T {
        &v[(self.next() % v.len() as u64) as usize]
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// 两边都可能出现的词（含空格 · 两种引号 · 反斜杠 · 空串 · 汉字 · agent 自己的 `--` · `--model` 的几种写法）。
const LEFT_POOL: &[&str] = &[
    "-p",
    "--model",
    "opus",
    "--model=haiku",
    "--append-system-prompt",
    "be brief",
    "it's",
    "say \"hi\"",
    "C:\\work",
    "a\\ b",
    "",
    "--",
    "x",
    "文 档",
    "--add-dir",
    "/w w",
    "\t",
    "new",
    "--resume",
    "abc",
    "'",
    "\\",
];

/// `--` 右边的词组（认得的 · 写法不一 · 重复 · 表单没格子的 · 认不得的 · 缺值的）。
const RIGHT_POOL: &[&[&str]] = &[
    &["new"],
    &["--cwd", "/w"],
    &["--cwd=/x y"],
    &["--cwd-if", "~", "~/a b"],
    &["--cwd-if=/srv", "/srv/w"],
    &["--account", "z"],
    &["--account=b"],
    &["--base"],
    &["--account-dir", "/srv/acc"],
    &["--ccm-tmux"],
    &["--ccm-tmux=w"],
    &["--ccm-tmux="],
    &["--tmux-base", "b"],
    &["--ccm-agent", "codex"],
    &["--ccm-agent", ""],
    &["--launcher", "ccr code"],
    &["--tmux-size", "80x24"],
    &["--detach"],
    &["--detach=x"],
    &["--bus-register"],
    &["--bus-note", "备 注"],
    &["--ccm-sid", "abc"],
    &["--ccm-print"],
    &["--ccm-help"],
    &["--weird"],
    &["--attach", "s"],
    &["--attach"],
    &["--cwd"],
    &["it's"],
    &[""],
];

/// 任意一条别名（结构化地拼，再加一批完全随机的词序列）都逐字往返；只改名字参数逐字不变。
#[test]
fn every_alias_round_trips_byte_for_byte() {
    let mut cases: Vec<Alias> = vec![
        alias(&[]),
        alias(&["--"]),
        alias(&["-p", "--"]),
        alias(&["-p", "--", "-x", "--"]),
        alias(&["--", "new", "--ccm-tmux"]),
        alias(&["--", "--account", "z", "--cwd", "/w"]),
        alias(&["--", "--cwd", "a", "--cwd", "b"]),
        alias(&["--", "--ccm-tmux", "--tmux-base", "b"]),
        alias(&[
            "--model",
            "opus",
            "--model",
            "haiku",
            "--",
            "--base",
            "--account",
            "z",
        ]),
        Alias {
            name: "cca".into(),
            args: s(&["--", "--attach"]),
            rest_to: RestTo::Ccm,
        },
        Alias {
            name: "odd".into(),
            args: s(&["--", "--account", "z", "--attach"]),
            rest_to: RestTo::Ccm,
        },
        Alias {
            name: " spaced ".into(),
            args: s(&["--", "--attach"]),
            rest_to: RestTo::Agent,
        },
    ];
    let mut r = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..3000 {
        let mut args: Vec<String> = Vec::new();
        for _ in 0..r.below(5) {
            args.push(r.pick(LEFT_POOL).to_string());
        }
        if r.below(4) != 0 {
            args.push("--".into());
            for _ in 0..r.below(7) {
                args.extend(r.pick(RIGHT_POOL).iter().map(|w| w.to_string()));
            }
        }
        let rest_to = if r.below(10) == 0 {
            RestTo::Ccm
        } else {
            RestTo::Agent
        };
        cases.push(Alias {
            name: "g".into(),
            args,
            rest_to,
        });
    }
    // 完全随机：两个池的词混在一起、`--` 随处可能出现。
    let flat: Vec<&str> = LEFT_POOL
        .iter()
        .copied()
        .chain(RIGHT_POOL.iter().flat_map(|g| g.iter().copied()))
        .collect();
    for _ in 0..2000 {
        let args = (0..r.below(9)).map(|_| r.pick(&flat).to_string()).collect();
        cases.push(Alias::new("h", args));
    }
    for a in &cases {
        assert_round_trip(a);
    }
}

/// 认不得的词不挪边：左边的进「交给 agent 的其余参数」、右边的进「其它 ccm 参数」，切回来与原词逐个相等。
#[test]
fn words_the_form_has_no_cell_for_stay_on_their_own_side() {
    let a = alias(&[
        "--weird",
        "a b",
        "--",
        "new",
        "--ccm-sid",
        "abc",
        "--cwd",
        "/w",
        "--odd",
    ]);
    let f = to_form(&a);
    assert_eq!(split_box(&f.passthru), Ok(s(&["--weird", "a b"])));
    assert_eq!(
        split_box(&f.ccm_other),
        Ok(s(&["--ccm-sid", "abc", "--odd"])),
        "`new` 不进「其它」"
    );
    assert_eq!(f.cwd, "/w");
}

/// 改一格只动它自己那一组：其余的写法、位置原样；动了的换成规范写法、放在原来那一处；原来没有的追加。
#[test]
fn editing_one_cell_touches_only_its_own_words() {
    let edit = |args: &[&str], f: &dyn Fn(&mut AliasForm)| {
        let a = alias(args);
        let mut g = to_form(&a);
        f(&mut g);
        from_form(&g, Some(&a)).unwrap().args
    };
    assert_eq!(
        edit(&["-p", "--", "--cwd=/x", "--ccm-tmux", "--detach"], &|g| {
            g.tmux = TmuxMode::None
        }),
        s(&["-p", "--", "--cwd=/x"]),
        "不进 tmux ⇒ 容器那几格一起走"
    );
    assert_eq!(
        edit(&["--", "new", "--account", "z", "--cwd", "/w"], &|g| g
            .account =
            "b".into()),
        s(&["--", "new", "--account", "b", "--cwd", "/w"])
    );
    assert_eq!(
        edit(&["--model", "opus", "-p", "--", "--account", "z"], &|g| g
            .model =
            String::new()),
        s(&["-p", "--", "--account", "z"])
    );
    assert_eq!(
        edit(&["--model", "opus", "-p", "--", "--account", "z"], &|g| g
            .passthru =
            "-p --verbose".into()),
        s(&["--model", "opus", "-p", "--verbose", "--", "--account", "z"])
    );
    assert_eq!(
        edit(&["-p"], &|g| g.model = "opus".into()),
        s(&["--model", "opus", "-p"]),
        "新加的模型放最前（agent 自己的 `--` 之后就成了位置参数）"
    );
    assert_eq!(
        edit(&["--", "--account-dir", "/d"], &|g| g.tmux = TmuxMode::Auto),
        s(&["--", "--account-dir", "/d", "--ccm-tmux"])
    );
    assert_eq!(
        edit(&["--", "--cwd", "a", "--cwd", "b"], &|g| g.cwd =
            String::new()),
        Vec::<String>::new(),
        "清空一格 ⇒ 那一格的每一处都走（前面那处不会复活）"
    );
    assert_eq!(
        edit(&["--", "--attach"], &|g| g.cwd = "/w".into()),
        s(&["--", "--attach", "--cwd", "/w"]),
        "`--attach` 缺值留在「其它」原位"
    );
}

/// 新增（没有 `orig`）出规范写法，顺序与从前那张表单拼的一样。
#[test]
fn a_new_form_renders_in_the_canonical_order() {
    let f = AliasForm {
        name: " convz ".into(),
        cwd_if: vec![
            CwdCase {
                at: "~".into(),
                to: "~/文档/c c".into(),
            },
            CwdCase {
                at: "/srv".into(),
                to: "/srv/w".into(),
            },
            CwdCase {
                at: "~".into(),
                to: " ".into(),
            },
        ],
        cwd: "/home/u/x".into(),
        account: "z".into(),
        base: false,
        tmux: TmuxMode::Named,
        tmux_name: "w".into(),
        agent: "codex".into(),
        model: "opus".into(),
        launcher: "/usr/bin/claude".into(),
        tmux_size: "200x50".into(),
        detach: true,
        bus_register: true,
        bus_note: "备注".into(),
        passthru: "--verbose --x".into(),
        ccm_other: String::new(),
    };
    let a = from_form(&f, None).unwrap();
    assert_eq!(a.name, "convz");
    assert_eq!(
        a.args,
        s(&[
            "--model",
            "opus",
            "--verbose",
            "--x",
            "--",
            "--cwd-if",
            "~",
            "~/文档/c c",
            "--cwd-if",
            "/srv",
            "/srv/w",
            "--cwd",
            "/home/u/x",
            "--account",
            "z",
            "--ccm-tmux=w",
            "--ccm-agent",
            "codex",
            "--launcher",
            "/usr/bin/claude",
            "--tmux-size",
            "200x50",
            "--detach",
            "--bus-register",
            "--bus-note",
            "备注",
        ])
    );
    let blank = to_form(&Alias::new("", vec![]));
    let only = |g: &dyn Fn(&mut AliasForm)| {
        let mut x = blank.clone();
        g(&mut x);
        from_form(&x, None).unwrap()
    };
    assert_eq!(only(&|x| x.base = true).args, s(&["--", "--base"]));
    assert_eq!(
        only(&|x| x.passthru = "-p -- -x".into()).args,
        s(&["-p", "--", "-x", "--"])
    );
    assert_eq!(
        only(&|x| {
            x.tmux_size = "80x24".into();
            x.detach = true;
            x.bus_register = true;
        })
        .args,
        Vec::<String>::new(),
        "不进 tmux ⇒ 容器那几格一个都不出现"
    );
    let attach = only(&|x| {
        x.tmux = TmuxMode::Attach;
        x.account = "z".into();
        x.passthru = "-p".into();
    });
    assert_eq!(
        (attach.args, attach.rest_to),
        (s(&["--", "--attach"]), RestTo::Ccm)
    );
}

/// 规范写法那一向：一批前后一致的表单 → 参数 → 表单，逐格相等；参数交 ccm 运行时的解析器，收得下、意思对得上。
#[test]
fn a_form_comes_back_from_its_own_args_and_ccm_accepts_them() {
    let mut r = Rng(0x2545_f491_4f6c_dd1d);
    let words: &[&str] = &[
        "-p",
        "be brief",
        "it's",
        "say \"hi\"",
        "C:\\work",
        "",
        "--",
        "x",
        "文 档",
        "\\",
    ];
    let others: &[&[&str]] = &[
        &["--ccm-sid", "abc"],
        &["--ccm-print"],
        &["--weird"],
        &["--attach", "s"],
    ];
    for n in 0..4000 {
        let mut f = to_form(&Alias::new("", vec![]));
        f.name = r.pick(&["a", "work", "alphacc"]).to_string();
        if n % 50 == 0 {
            f.tmux = TmuxMode::Attach;
        } else {
            for _ in 0..r.below(3) {
                f.cwd_if.push(CwdCase {
                    at: r.pick(&["~", "/srv", "~/文 档"]).to_string(),
                    to: r.pick(&["~/w", "/srv/w x", "/t"]).to_string(),
                });
            }
            f.cwd = r.pick(&["", "/w", "~/a b", "auto"]).to_string();
            match r.below(3) {
                0 => f.account = "z".into(),
                1 => f.base = true,
                _ => {}
            }
            f.tmux = *r.pick(&[
                TmuxMode::None,
                TmuxMode::Auto,
                TmuxMode::Named,
                TmuxMode::Base,
            ]);
            if matches!(f.tmux, TmuxMode::Named | TmuxMode::Base) {
                f.tmux_name = r.pick(&["w", "s1"]).to_string();
            }
            f.agent = r.pick(&["", "codex", "claude"]).to_string();
            // Codex 没有账号这一维（ccm 明说不收 `--account`）⇒ 那一家的表单不选号。
            if f.agent == "codex" {
                f.account.clear();
            }
            f.model = r.pick(&["", "opus", "claude-x[1m]"]).to_string();
            f.launcher = r.pick(&["", "ccr code", "/usr/bin/claude"]).to_string();
            if f.tmux != TmuxMode::None {
                f.tmux_size = r.pick(&["", "80x24"]).to_string();
                f.detach = r.below(2) == 0;
                f.bus_register = f.detach && r.below(2) == 0;
                if f.bus_register {
                    f.bus_note = r.pick(&["", "备 注"]).to_string();
                }
            }
            let pass: Vec<String> = (0..r.below(4)).map(|_| r.pick(words).to_string()).collect();
            f.passthru = join_box(&pass);
            if r.below(3) == 0 {
                f.ccm_other = join_box(
                    &r.pick(others)
                        .iter()
                        .map(|w| w.to_string())
                        .collect::<Vec<_>>(),
                );
            }
        }
        let a = from_form(&f, None).unwrap();
        assert_eq!(to_form(&a), f, "表单拼出 {a:?} 读不回原样");
        if f.tmux == TmuxMode::Attach || !f.ccm_other.is_empty() {
            continue;
        }
        let o = match crate::control::ccm::argv::parse(&a.args) {
            Ok(crate::control::ccm::argv::Parsed::Opts(o)) => o,
            other => panic!("ccm 不收表单拼出来的 {:?}：{other:?}", a.args),
        };
        assert_eq!(o.account, f.account);
        assert_eq!(o.use_base, f.base);
        assert_eq!(o.detach, f.detach);
        assert_eq!(o.bus_register, f.bus_register);
        assert_eq!(o.use_tmux, f.tmux != TmuxMode::None);
        let pass = split_box(&f.passthru).unwrap();
        let model: Vec<String> = if f.model.is_empty() {
            vec![]
        } else {
            s(&["--model", &f.model])
        };
        assert_eq!(
            o.passthru,
            [model, pass].concat(),
            "交给 agent 的那一串不对：{:?}",
            a.args
        );
    }
}

/// 一格文本的写法：切回来逐字相等（任意词，含空白 · 两种引号 · 反斜杠 · 空串）；几条手写的样子；引号没配对 ⇒ 拒。
#[test]
fn box_text_splits_back_to_the_same_words() {
    let chars = [
        'a', ' ', '\t', '\n', '\'', '"', '\\', '-', '文', '\u{3000}', '$',
    ];
    let mut r = Rng(0xdead_beef_cafe_f00d);
    for _ in 0..5000 {
        let ws: Vec<String> = (0..r.below(4))
            .map(|_| (0..r.below(6)).map(|_| *r.pick(&chars)).collect())
            .collect();
        assert_eq!(
            split_box(&join_box(&ws)),
            Ok(ws.clone()),
            "{:?}",
            join_box(&ws)
        );
    }
    assert_eq!(
        join_box(&s(&["C:\\work", "a b", "it's", ""])),
        "C:\\work 'a b' \"it's\" ''"
    );
    assert_eq!(
        split_box("a\\ b 'c d'\"e\" \"x\\\"y\""),
        Ok(s(&["a b", "c de", "x\"y"]))
    );
    assert_eq!(
        split_box("C:\\work\\x"),
        Ok(s(&["C:\\work\\x"])),
        "引号外的反斜杠只转义空白、引号与它自己"
    );
    for bad in ["'a", "\"a", "a 'b c", "\"a\\\""] {
        assert!(split_box(bad).is_err(), "{bad:?} 放行了");
    }
}

/// 线上两口：形状不对 ⇒ `bad_args`；`orig` 必给（可为 `null`）；引号没配对 ⇒ `refused`、那句话走表。
#[test]
fn the_two_wire_answers_are_strict_and_speak_from_the_table() {
    let a = json!({ "name": "mine", "args": ["--", "--cwd", "/w"], "restTo": "agent" });
    let form = answer_to_form(&json!({ "alias": a })).unwrap()["form"].clone();
    assert_eq!(form["cwd"], "/w");
    assert_eq!(answer_to_form(&json!({})).unwrap_err().0, "bad_args");
    assert_eq!(
        answer_to_form(&json!({ "alias": { "name": "x", "args": [] } }))
            .unwrap_err()
            .0,
        "bad_args"
    );
    assert_eq!(
        answer_from_form(&json!({ "form": form })).unwrap_err().0,
        "bad_args",
        "`orig` 没给"
    );
    let back = answer_from_form(&json!({ "form": form, "orig": a })).unwrap();
    assert_eq!(back, json!({ "alias": a }));
    let mut extra = form.clone();
    extra["boundTerminals"] = json!(2);
    assert_eq!(
        answer_from_form(&json!({ "form": extra, "orig": Value::Null }))
            .unwrap_err()
            .0,
        "bad_args"
    );
    let mut open = form.clone();
    open["passthru"] = json!("'be brief");
    let (code, said) = answer_from_form(&json!({ "form": open, "orig": Value::Null })).unwrap_err();
    assert_eq!(
        (code, said),
        ("refused", copy_text("beAliasForm.passthru.unbalanced", &[]))
    );
}

/// 跨语言金样那几条：后端现算两向都与金样相等（界面的解码器读同一份）。
#[test]
fn the_form_cases_match_the_cross_language_golden() {
    let g: Value =
        serde_json::from_str(include_str!("../../../__fixtures__/aliases.golden.json")).unwrap();
    let cases = g["formCases"].as_array().expect("金样里没有 formCases");
    assert!(cases.len() >= 3);
    for c in cases {
        let got = answer_to_form(&json!({ "alias": c["alias"] })).unwrap();
        assert_eq!(got["form"], c["form"], "{}", c["alias"]);
        let back = answer_from_form(&json!({ "form": c["form"], "orig": c["alias"] })).unwrap();
        assert_eq!(back["alias"], c["alias"]);
    }
}
