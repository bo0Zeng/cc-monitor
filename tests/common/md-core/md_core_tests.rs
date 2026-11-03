//! md-core 的判据：金样（造的短句 → 期望的树）＋ 核心一处定的那几条规矩各一条。夹具全是造的，不放真正文。

use super::*;

fn s(x: &str) -> Inl {
    Inl::S(x.to_string())
}
fn n(x: InlNode) -> Inl {
    Inl::N(x)
}
fn only_p(src: &str) -> Vec<Inl> {
    let doc = parse(src);
    match doc.blocks.as_slice() {
        [MdBlock::P { k }] => k.clone(),
        other => panic!("{src:?} 应当只出一段段落，出的是 {other:?}"),
    }
}
/// 上限够大、判据里不会淘汰的缓存。
const BIG: usize = 1 << 24;

/// 全文各段拉一遍，照先后接起来。
fn hl_all(doc: &Doc) -> Vec<Vec<u32>> {
    let mut h = Highlighter::new(BIG);
    (0..=doc.segs.len()).flat_map(|i| h.seg(doc, i)).collect()
}
fn only_code(src: &str) -> (Option<String>, String, Vec<u32>) {
    let doc = parse(src);
    let hl = hl_all(&doc);
    match (doc.blocks.as_slice(), hl.as_slice()) {
        (
            [MdBlock::Code {
                lang,
                x,
                cont: false,
            }],
            [hl],
        ) => (lang.clone(), x.clone(), hl.clone()),
        other => panic!("{src:?} 应当只出一个代码块，出的是 {other:?}"),
    }
}
/// 一份里所有代码块（先序，连嵌着的）。
fn codes_of(blocks: &[MdBlock], out: &mut Vec<(Option<String>, String)>) {
    for b in blocks {
        match b {
            MdBlock::Code { lang, x, .. } => out.push((lang.clone(), x.clone())),
            MdBlock::List { items, .. } => items.iter().for_each(|i| codes_of(&i.blocks, out)),
            MdBlock::Quote { blocks } | MdBlock::Alert { blocks, .. } => codes_of(blocks, out),
            MdBlock::Notes { items } => items.iter().for_each(|i| codes_of(&i.blocks, out)),
            _ => {}
        }
    }
}
/// `hl` 三元组 ⇒ 绝对的 (起点, 长, 类)，偏移是 UTF-16 码元。
fn spans(hl: &[u32]) -> Vec<(u32, u32, u32)> {
    assert_eq!(hl.len() % 3, 0, "hl 不是三元组：{hl:?}");
    let mut at = 0;
    hl.chunks(3)
        .map(|c| {
            let start = at + c[0];
            at = start + c[1];
            (start, c[1], c[2])
        })
        .collect()
}
/// 按 UTF-16 码元取一段代码原文。
fn slice16(x: &str, start: u32, len: u32) -> String {
    let u: Vec<u16> = x.encode_utf16().collect();
    String::from_utf16(&u[start as usize..(start + len) as usize]).unwrap()
}

#[test]
fn goldens() {
    let raw = include_str!("../../__fixtures__/md-core.golden.json");
    let cases: Vec<serde_json::Value> = serde_json::from_str(raw).unwrap();
    assert!(cases.len() >= 24, "金样少于 24 份：{}", cases.len());
    let mut bad = Vec::new();
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let md = c["md"].as_str().unwrap();
        let doc = parse(md);
        let got = serde_json::to_value(&doc).unwrap();
        if got != c["doc"] {
            bad.push(format!("{name}\n  期望 {}\n  得到 {}", c["doc"], got));
        }
        let want_hl = c.get("hl").cloned().unwrap_or(serde_json::json!([]));
        let got_hl = serde_json::to_value(hl_all(&doc)).unwrap();
        if got_hl != want_hl {
            bad.push(format!("{name} 的高亮\n  期望 {want_hl}\n  得到 {got_hl}"));
        }
        // 线上那一份读回来还是同一棵树（untagged 的 Inl 双向对得上）。
        let back: Doc = serde_json::from_value(got.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(back).unwrap(),
            got,
            "{name} 读回来不一样"
        );
    }
    assert!(
        bad.is_empty(),
        "金样对不上 {} 份：\n{}",
        bad.len(),
        bad.join("\n")
    );
}

#[test]
fn single_tilde_is_text() {
    assert_eq!(
        only_p("~/a~/b 与 ~~删~~"),
        vec![s("~/a~/b 与 "), n(InlNode::Del { k: vec![s("删")] })]
    );
    assert_eq!(only_p("约 ~3 到 ~5 秒"), vec![s("约 ~3 到 ~5 秒")]);
}

#[test]
fn raw_html_is_text_except_br() {
    assert_eq!(
        only_p("把 <Composer> 和 Vec<String> 换掉"),
        vec![s("把 <Composer> 和 Vec<String> 换掉")]
    );
    assert_eq!(only_p("上<br>下"), vec![s("上"), n(InlNode::Br), s("下")]);
    assert_eq!(
        only_p("按 <kbd>Esc</kbd> 退出"),
        vec![s("按 <kbd>Esc</kbd> 退出")]
    );
}

#[test]
fn money_is_not_math() {
    assert_eq!(only_p("花了 $5 到 $10"), vec![s("花了 $5 到 $10")]);
    assert_eq!(
        only_p("质能 $E=mc^2$ 式"),
        vec![
            s("质能 "),
            n(InlNode::M {
                tex: "E=mc^2".into(),
                m: math::lay("E=mc^2", false),
            }),
            s(" 式")
        ]
    );
}

#[test]
fn latex_brackets() {
    assert_eq!(
        parse("\\[ \\sum_i x_i \\]").blocks,
        vec![MdBlock::Math {
            tex: "\\sum_i x_i".into(),
            m: math::lay("\\sum_i x_i", true),
        }]
    );
    assert_eq!(
        only_p("和是 \\(a+b\\) 吧"),
        vec![
            s("和是 "),
            n(InlNode::M {
                tex: "a+b".into(),
                m: math::lay("a+b", false),
            }),
            s(" 吧")
        ]
    );
}

#[test]
fn task_list() {
    let doc = parse("- [ ] 待办\n- [x] 已做\n- 普通");
    let MdBlock::List {
        start: None, items, ..
    } = &doc.blocks[0]
    else {
        panic!("{doc:?}")
    };
    let tasks: Vec<Option<bool>> = items.iter().map(|i| i.task).collect();
    assert_eq!(tasks, vec![Some(false), Some(true), None]);
}

#[test]
fn footnotes_collect_at_end() {
    let doc = parse("前文[^a] 又一处[^b]\n\n[^b]: 二号\n[^a]: 一号\n\n尾段");
    assert_eq!(
        doc.blocks[0],
        MdBlock::P {
            k: vec![
                s("前文"),
                n(InlNode::Fn { n: 1 }),
                s(" 又一处"),
                n(InlNode::Fn { n: 2 })
            ]
        }
    );
    assert_eq!(
        doc.blocks[1],
        MdBlock::P {
            k: vec![s("尾段")]
        }
    );
    assert_eq!(
        doc.blocks.last().unwrap(),
        &MdBlock::Notes {
            items: vec![
                MdNote {
                    n: 1,
                    blocks: vec![MdBlock::P {
                        k: vec![s("一号")]
                    }]
                },
                MdNote {
                    n: 2,
                    blocks: vec![MdBlock::P {
                        k: vec![s("二号")]
                    }]
                },
            ]
        }
    );
}

#[test]
fn cjk_strong() {
    assert_eq!(
        only_p("这是**“强调”**文字"),
        vec![
            s("这是"),
            n(InlNode::B {
                k: vec![s("“强调”")]
            }),
            s("文字")
        ]
    );
}

#[test]
fn link_scheme_allowlist() {
    let a = |href: &str, t: &str| {
        n(InlNode::A {
            href: href.into(),
            k: vec![s(t)],
        })
    };
    assert_eq!(
        only_p("[站](https://example.com/a)"),
        vec![a("https://example.com/a", "站")]
    );
    assert_eq!(
        only_p("[信](mailto:x@example.com)"),
        vec![a("mailto:x@example.com", "信")]
    );
    assert_eq!(only_p("[本地](file:///etc/hosts)"), vec![s("本地")]);
    assert_eq!(only_p("[脚本](javascript:alert(1))"), vec![s("脚本")]);
    assert_eq!(only_p("[相对](./docs/a.md)"), vec![s("相对")]);
    assert_eq!(only_p("[跳](intent://x)"), vec![s("跳")]);
    // 自动链接也过同一道门。
    assert_eq!(
        only_p("看 www.example.com 吧"),
        vec![
            s("看 "),
            a("http://www.example.com", "www.example.com"),
            s(" 吧")
        ]
    );
    // 图只放行 http · https：别的交成它的替代字。
    assert_eq!(only_p("![图](./a.png)"), vec![s("图")]);
    assert_eq!(
        only_p("![图](https://example.com/a.png)"),
        vec![n(InlNode::Img {
            src: "https://example.com/a.png".into(),
            alt: "图".into()
        })]
    );
}

#[test]
fn soft_break_between_cjk_is_empty() {
    assert_eq!(only_p("中文\n续行"), vec![s("中文续行")]);
    assert_eq!(only_p("abc\ndef"), vec![s("abc def")]);
    assert_eq!(only_p("中文\nabc"), vec![s("中文 abc")]);
    assert_eq!(
        only_p("中文\n**加粗**"),
        vec![
            s("中文"),
            n(InlNode::B {
                k: vec![s("加粗")]
            })
        ]
    );
}

#[test]
fn lang_alias() {
    for (info, want) in [
        ("sh", "bash"),
        ("shell", "bash"),
        ("zsh", "bash"),
        ("Bash", "bash"),
        ("ts", "typescript"),
        ("yml", "yaml"),
        ("rs", "rust"),
        ("py", "python"),
        ("kt", "kotlin"),
        ("rust ignore", "rust"),
    ] {
        let (lang, _, hl) = only_code(&format!("```{info}\necho 1\n```"));
        assert_eq!(lang.as_deref(), Some(want), "{info}");
        let _ = hl;
    }
    // 认不出的不猜：语言名照写（小写），不高亮。
    let (lang, _, hl) = only_code("```Foolang\nlet x = 1\n```");
    assert_eq!(lang.as_deref(), Some("foolang"));
    assert!(hl.is_empty(), "{hl:?}");
    let (lang, _, hl) = only_code("```\nlet x = 1\n```");
    assert_eq!(lang, None);
    assert!(hl.is_empty());
}

#[test]
fn highlight_classes_utf16() {
    let (_, x, hl) = only_code("```rust\nlet s = \"汉字😀\"; // 注\nfn f() {}\n```");
    assert_eq!(x, "let s = \"汉字😀\"; // 注\nfn f() {}");
    let sp = spans(&hl);
    let class_of = |text: &str| {
        sp.iter()
            .find(|(st, len, _)| slice16(&x, *st, *len) == text)
            .map(|t| t.2)
            .unwrap_or_else(|| {
                panic!(
                    "没有正好是 {text:?} 的一段：{:?}",
                    sp.iter()
                        .map(|(a, b, c)| (slice16(&x, *a, *b), *c))
                        .collect::<Vec<_>>()
                )
            })
    };
    assert_eq!(class_of("let"), HlClass::Keyword as u32);
    assert_eq!(class_of("\"汉字😀\""), HlClass::String as u32);
    // 😀 占两个 UTF-16 码元：注释段的起点照码元算才切得准。
    assert_eq!(class_of("// 注"), HlClass::Comment as u32);
    assert_eq!(class_of("fn"), HlClass::Keyword as u32);
    assert_eq!(class_of("f"), HlClass::Function as u32);
    // 两端常用而 syntect 自带表里没有的几种也认得。
    for lang in ["typescript", "toml", "kotlin", "bash"] {
        let (_, _, hl) = only_code(&format!("```{lang}\nx = 1 # c\n```"));
        assert!(!hl.is_empty(), "{lang} 没出高亮");
    }
    let (_, x, hl) = only_code("```diff\n+加\n-删\n```");
    let sp = spans(&hl);
    assert!(sp.iter().any(|(a, b, c)| slice16(&x, *a, *b).starts_with('+') && *c == HlClass::Inserted as u32), "{sp:?}");
    assert!(
        sp.iter()
            .any(|(a, b, c)| slice16(&x, *a, *b).starts_with('-') && *c == HlClass::Deleted as u32),
        "{sp:?}"
    );
}

#[test]
fn inl_wire_shape() {
    let v = serde_json::to_value(vec![
        s("字"),
        n(InlNode::Br),
        n(InlNode::C { x: "a".into() }),
    ])
    .unwrap();
    assert_eq!(
        v,
        serde_json::json!(["字", {"t": "br"}, {"t": "c", "x": "a"}])
    );
}

fn weight(b: &MdBlock) -> usize {
    serde_json::to_string(b).unwrap().len()
}

#[test]
fn short_doc_has_one_segment() {
    assert!(parse("一段\n\n两段").segs.is_empty());
}

#[test]
fn long_doc_segments_on_top_level_blocks() {
    let para = "这是一句造的话。".repeat(40); // 约 960 字节
    let src = vec![para.as_str(); 60].join("\n\n");
    let doc = parse(&src);
    assert_eq!(doc.blocks.len(), 60, "顶层段落不该被切");
    assert!(doc.segs.len() >= 2, "{:?}", doc.segs);
    let mut bounds = vec![0u32];
    bounds.extend(&doc.segs);
    bounds.push(doc.blocks.len() as u32);
    for w in bounds.windows(2) {
        assert!(w[0] < w[1], "段界不递增：{:?}", doc.segs);
        let bytes: usize = doc.blocks[w[0] as usize..w[1] as usize]
            .iter()
            .map(weight)
            .sum();
        assert!(bytes <= SEG_BYTES + 2_000, "一段 {bytes} 字节");
    }
}

#[test]
fn oversize_code_splits_by_line() {
    let line = "let value = \"一行造的代码\"; // 注释";
    let body = vec![line; 1500].join("\n");
    let doc = parse(&format!("```rust\n{body}\n```"));
    let hls = hl_all(&doc);
    assert_eq!(hls.len(), doc.blocks.len());
    let codes: Vec<_> = doc
        .blocks
        .iter()
        .zip(hls)
        .map(|(b, hl)| match b {
            MdBlock::Code { lang, x, cont } => (lang.clone(), x.clone(), hl, *cont),
            other => panic!("{other:?}"),
        })
        .collect();
    assert!(codes.len() >= 3, "{} 块", codes.len());
    assert!(!codes[0].3 && codes[1..].iter().all(|c| c.3), "cont 标错");
    assert!(codes.iter().all(|c| c.0.as_deref() == Some("rust")));
    let joined: Vec<&str> = codes.iter().map(|c| c.1.as_str()).collect();
    assert_eq!(joined.join("\n"), body, "切开再接回不是原文");
    for (_, x, hl, _) in &codes {
        let n16 = x.encode_utf16().count() as u32;
        let sp = spans(hl);
        assert!(!sp.is_empty());
        assert!(sp.iter().all(|(a, b, _)| a + b <= n16), "高亮段越出这一块");
        assert!(sp
            .iter()
            .any(|(a, b, c)| slice16(x, *a, *b) == "let" && *c == HlClass::Keyword as u32));
    }
    assert_eq!(
        doc.segs.len(),
        codes.len() - 1,
        "每块代码各起一段：{:?}",
        doc.segs
    );
}

#[test]
fn oversize_ordered_list_splits_by_item() {
    let item = "一项造的说明文字。".repeat(20);
    let src: Vec<String> = (0..120).map(|_| format!("1. {item}")).collect();
    let doc = parse(&format!("3. 起头\n{}", src.join("\n")));
    let lists: Vec<_> = doc
        .blocks
        .iter()
        .map(|b| match b {
            MdBlock::List { start, items, .. } => (*start, items.len()),
            other => panic!("{other:?}"),
        })
        .collect();
    assert!(lists.len() >= 2, "{lists:?}");
    let mut want = 3;
    for (start, len) in &lists {
        assert_eq!(*start, Some(want), "{lists:?}");
        want += *len as u32;
    }
    assert_eq!(want, 3 + 121);
}

#[test]
fn oversize_paragraph_splits_at_sentence_end() {
    let src = "一句造的长话，没有别的意思。".repeat(2_000);
    let doc = parse(&src);
    assert!(doc.blocks.len() >= 2);
    let mut all = String::new();
    for b in &doc.blocks {
        let MdBlock::P { k } = b else { panic!("{b:?}") };
        let [Inl::S(t)] = k.as_slice() else {
            panic!("{k:?}")
        };
        assert!(t.ends_with('。'), "没切在句末");
        all.push_str(t);
    }
    assert_eq!(all, src);
}

// ───────────── 高亮：看见才算（按段算 ＋ 按内容缓存） ─────────────

/// 一页造的长正文：每段都有代码，段与段之间的代码各不一样。
fn multi_seg_doc() -> Doc {
    let para = "这是一句造的话。".repeat(40);
    let mut parts = Vec::new();
    for i in 0..60u32 {
        parts.push(para.clone());
        if i.is_multiple_of(6) {
            parts.push(format!("```rust\nfn f{i}() {{ let x = {i}; }}\n```"));
        }
    }
    let doc = parse(&parts.join("\n\n"));
    assert!(doc.segs.len() >= 2, "{:?}", doc.segs);
    doc
}

#[test]
fn parsed_tree_has_no_hl() {
    let doc = parse("```rust\nfn f() {}\n```\n\n- 项\n\n  ```bash\nls\n  ```");
    let json = serde_json::to_string(&doc).unwrap();
    assert!(!json.contains("\"hl\""), "树里带了高亮：{json}");
}

#[test]
fn same_segment_twice_computes_once() {
    let doc = multi_seg_doc();
    let mut h = Highlighter::new(BIG);
    let a = h.seg(&doc, 0);
    let n = h.computed();
    assert!(
        n >= 1 && a.iter().any(|x| !x.is_empty()),
        "第 0 段没算：{a:?}"
    );
    let b = h.seg(&doc, 0);
    assert_eq!(a, b);
    assert_eq!(h.computed(), n, "同一段第二次又算了");
}

#[test]
fn same_content_computes_once() {
    let code = "```rust\nfn same() {}\n```";
    let doc = parse(&format!(
        "{code}\n\n中间一句\n\n{code}\n\n- 列表里\n\n  ```rust\n  fn same() {{}}\n  ```"
    ));
    let mut h = Highlighter::new(BIG);
    let hl = h.seg(&doc, 0);
    assert_eq!(hl.len(), 3, "{hl:?}");
    assert!(!hl[0].is_empty() && hl[0] == hl[1] && hl[1] == hl[2]);
    assert_eq!(h.computed(), 1, "同内容的三块算了 {} 次", h.computed());
    // 语言不同 ⇒ 不同的键。
    let other = parse("```c\nfn same() {}\n```");
    h.seg(&other, 0);
    assert_eq!(h.computed(), 2);
}

#[test]
fn no_lang_is_not_computed() {
    let doc = parse("```\nplain\n```\n\n```foolang\nx\n```");
    let mut h = Highlighter::new(BIG);
    assert_eq!(h.seg(&doc, 0), vec![Vec::<u32>::new(), Vec::new()]);
    assert_eq!(h.computed(), 0);
    assert_eq!(h.bytes(), 0);
}

#[test]
fn over_cap_evicts_least_recently_used() {
    let block = |name: &str| parse(&format!("```rust\nfn {name}() {{ let v = 1; }}\n```"));
    let (a, b, c) = (block("aaaa"), block("bbbb"), block("cccc"));
    // 先量一条占多少，上限只够放两条。
    let mut probe = Highlighter::new(BIG);
    probe.seg(&a, 0);
    let one = probe.bytes();
    assert!(one > 0);
    let mut h = Highlighter::new(one * 2 + one / 2);
    h.seg(&a, 0);
    h.seg(&b, 0);
    h.seg(&a, 0); // a 新近用过 ⇒ 淘汰该轮到 b
    assert_eq!(h.computed(), 2);
    h.seg(&c, 0);
    assert_eq!(h.computed(), 3);
    assert!(h.bytes() <= one * 2 + one / 2, "超了上限：{}", h.bytes());
    h.seg(&a, 0);
    assert_eq!(h.computed(), 3, "a 不该被淘汰");
    h.seg(&b, 0);
    assert_eq!(h.computed(), 4, "b 该被淘汰了");
    // 单条就超过上限的：算完照给，不存。
    let mut tiny = Highlighter::new(1);
    let hl = tiny.seg(&a, 0);
    assert!(!hl[0].is_empty());
    assert_eq!(tiny.bytes(), 0);
    tiny.seg(&a, 0);
    assert_eq!(tiny.computed(), 2);
}

fn at(text: &str, piece: &str, label: &str) -> RefAt {
    let start = text.find(piece).unwrap();
    let cp = |b: usize| text[..b].encode_utf16().count();
    RefAt {
        start: cp(start),
        end: cp(start + piece.len()),
        ids: vec![piece.to_string()],
        label: label.to_string(),
    }
}
fn r(x: &str, label: &str) -> Inl {
    n(InlNode::Ref {
        x: x.into(),
        ids: vec![x.into()],
        label: label.into(),
    })
}

#[test]
fn ref_text_keeps_words_only_and_gaps_the_rest() {
    let doc = parse("先看 **a1-1**，`A2` 在代码里\n\n```\nA3\n```\n\n- 末 $x$ 项");
    assert_eq!(
        ref_text(&doc),
        "先看 a1-1，\u{FFFC} 在代码里\n\n\u{FFFC}\n\n末 \u{FFFC} 项"
    );
}

#[test]
fn with_refs_turns_the_number_into_a_ref_node() {
    let doc = parse("😀 先看 **a1-1**，再看 a1-2 的写法。");
    let text = ref_text(&doc);
    let got = with_refs(&doc, &[at(&text, "a1-1", "甲"), at(&text, "a1-2", "乙")]);
    assert_eq!(
        got.blocks,
        vec![MdBlock::P {
            k: vec![
                s("😀 先看 "),
                n(InlNode::B {
                    k: vec![r("a1-1", "甲")]
                }),
                s("，再看 "),
                r("a1-2", "乙"),
                s(" 的写法。"),
            ]
        }]
    );
    // 没有一处 ⇒ 原样；跨了加粗边界的那一处 ⇒ 不插。
    assert_eq!(with_refs(&doc, &[]), doc);
    let cross = parse("**a1**-2 后");
    let t = ref_text(&cross);
    assert_eq!(with_refs(&cross, &[at(&t, "a1-2", "丙")]), cross);
}

#[test]
fn parse_marked_reads_refs_at_utf16_spans() {
    let src = "😀 见 a1-2 与 `A3`\n\n## 标题 A4";
    let u16 = |piece: &str| {
        let b = src.find(piece).unwrap();
        let s = src[..b].encode_utf16().count();
        RefAt {
            start: s,
            end: s + piece.encode_utf16().count(),
            ids: vec![piece.to_string()],
            label: String::new(),
        }
    };
    let got = parse_marked(src, &[u16("a1-2"), u16("A3"), u16("A4")]);
    assert_eq!(
        got.blocks,
        vec![
            MdBlock::P {
                k: vec![
                    s("😀 见 "),
                    r("a1-2", ""),
                    s(" 与 "),
                    n(InlNode::C { x: "A3".into() })
                ]
            },
            MdBlock::H {
                level: 2,
                k: vec![s("标题 "), r("A4", "")]
            },
        ]
    );
    // 没有一处 ⇒ 与直接解析同一棵树。
    assert_eq!(parse_marked(src, &[]), parse(src));
}

#[test]
fn tool_result_strips_line_number_prefixes_first() {
    let src = "1\t# 标题\n2\t\n3\t- 一\n4\t- 二";
    assert_eq!(tool_result(src), parse("# 标题\n\n- 一\n- 二"));
    // Grep 的「路径:行号:」前缀（路径不带扩展名：夹具里写成「文件.rs:数」会被行号地址扫描当成源码地址）。
    let grep = "src/a:10:# x\nsrc/a:11:y\nlib\\b:3:z";
    assert_eq!(tool_result(grep), parse("# x\ny\nz"));
    let colon = "  7:**粗**\n  8:平\n  9:平";
    assert_eq!(tool_result(colon), parse("**粗**\n平\n平"));
}

#[test]
fn tool_result_keeps_text_without_enough_prefixes() {
    // 带前缀的不到八成 ⇒ 照原文。
    let mixed = "1\t# x\n正文\n更多\n再多\n5\ty";
    assert_eq!(tool_result(mixed), parse(mixed));
    // 不到三行非空 ⇒ 不判。
    let short = "1\t# x\n2\ty";
    assert_eq!(tool_result(short), parse(short));
}

#[test]
fn segment_computes_only_its_own_blocks() {
    let doc = multi_seg_doc();
    let last = doc.segs.len();
    let mut mine = Vec::new();
    codes_of(seg_blocks(&doc, last), &mut mine);
    assert!(!mine.is_empty(), "最后一段里没有代码");
    let mut h = Highlighter::new(BIG);
    let hl = h.seg(&doc, last);
    assert_eq!(hl.len(), mine.len());
    assert_eq!(h.computed() as usize, mine.len(), "算了别的段的代码");
    // 每块的高亮就是那一块自己的。
    for ((lang, x), got) in mine.iter().zip(&hl) {
        let one = parse(&format!("```{}\n{x}\n```", lang.as_deref().unwrap()));
        assert_eq!(&Highlighter::new(BIG).seg(&one, 0)[0], got);
    }
    // 段号越界 ⇒ 空、不算。
    assert!(h.seg(&doc, last + 1).is_empty());
    assert!(seg_blocks(&doc, last + 1).is_empty());
    // 各段的块接起来就是整份。
    let total: usize = (0..=last).map(|i| seg_blocks(&doc, i).len()).sum();
    assert_eq!(total, doc.blocks.len());
}
