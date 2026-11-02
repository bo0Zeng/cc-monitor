//! 通信层边界判据的本体 —— 模块头注（成员怎么认 · 这几条管什么 · 买到什么买不到什么）住
//! `tests/common/guard-core/rules/comm_boundary_registry.rs`，不在这里抄第二份。
//!
//! 谁守哪一条：读盘 · 读环境 · 起进程 · 绑端口 · 期限字面量（`Duration::from_*`）· `try_send` 由两包的
//! `clippy.toml`（`src/comms/clippy.toml` · `src/comms/outward/clippy.toml`）挡；`match` 列全变体由两包
//! `Cargo.toml` 的 `wildcard_enum_match_arm = "deny"` 挡（通配臂只盖一个变体的那一形它不管，归 `X1`）；
//! 期限只收紧、调用点必给期限由 `Budget` 字段私有（只经 `Budget::new` 造、只经 `narrow` 取小）与
//! `call` 的签名挡。本文件只留编译器挡不住的那几条。
//!
//! 买不到（从此没人守）：`chan.ts` 那一侧的期限字面量与「重新 `now() + …`」（TS 没有字段私有那一道）；
//! 名字带 `SECS` 之类的裸数常量（它变成 `Duration` 时才被名单咬，`Duration::new` 不在名单里）。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // 住址唯一源：`guard_core::addr`。
    guard_core::addr::repo_root()
}

// ════════════════════════════════════════════════════════════════════════════
//  一、成员 ＝ 通信层那两个 crate（`cargo metadata` 现取）
// ════════════════════════════════════════════════════════════════════════════

/// 通信层那两个 crate 的包名（面 A `comms-inward` · 面 B `comms-outward`）。
const COMMS_CRATES: &[&str] = &["comms-inward", "comms-outward"];

/// 每个通信层 crate 的普通依赖**只许**这些（传递进来的第一方 crate 也必须在表里）。
/// 第一方业务 crate（后端 · 壳 · 文件窗口 · `acct-core` · `creds-core` · `host-core` · `filewin-contract` …）出现即红。
/// `comms-inward` 的 `ts-rs` 是可选依赖，只在测试档（`ts` 那一格）进来、不进产物。
const ALLOWED_DEPS: &[(&str, &[&str])] = &[
    (
        "comms-inward",
        &[
            "copy-core",
            // 契约 crate：机器名（`Origin` ＝ `cmd_wire::Machine`）· typed 命令的形状指纹表；不是业务。
            "cmd-wire",
            // 契约 crate、零依赖：经 `cmd-wire` 的 `Sid` 进来（会话 id 解码按它那一份 `session_id_ok` 判）。
            "shell-quote-core",
            // 契约 crate、零依赖：读上限的类型 `Cap`（`line` 那一份带上限读收它）。
            "relay-route-core",
            "serde",
            "serde_json",
            "tokio",
            "futures",
            "tracing",
            "uuid",
            "ts-rs",
        ],
    ),
    (
        "comms-outward",
        &[
            "copy-core",
            // 契约 crate：经 `copy-core` 进来（失败投成应答时补的兜底码 `cmd_wire::failed::FALLBACK_CODE`）；不是业务。
            "cmd-wire",
            // 契约 crate、零依赖：经 `cmd-wire` 的 `Sid` 进来。
            "shell-quote-core",
            "relay-route-core",
            "upstream-url-core",
            "rustls",
            "webpki-roots",
            "tracing",
        ],
    ),
];

/// `cargo metadata` 里的一个工作区成员：包名 · 包目录 · crate 根 · 普通依赖 `(名, 第一方的话它的包目录)`。
struct Package {
    name: String,
    dir: PathBuf,
    roots: Vec<PathBuf>,
    normal_deps: Vec<(String, Option<PathBuf>)>,
}

/// 壳那个 workspace 的成员（通信层两个 crate 都在里面），`cargo metadata --no-deps` 现取。
fn workspace_packages() -> Vec<Package> {
    let manifest = repo_root().join("src/frontend/shell/Cargo.toml");
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let out = std::process::Command::new(cargo)
        .args([
            "metadata",
            "--no-deps",
            "--offline",
            "--format-version",
            "1",
        ])
        .arg("--manifest-path")
        .arg(&manifest)
        .output()
        .expect("跑不动 `cargo metadata`");
    assert!(
        out.status.success(),
        "`cargo metadata` 退出码 {:?}：{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("`cargo metadata` 吐的不是 JSON");
    let path_of = |x: &serde_json::Value| PathBuf::from(x.as_str().unwrap_or_default());
    v["packages"]
        .as_array()
        .expect("`cargo metadata` 里没有 packages")
        .iter()
        .map(|p| Package {
            name: p["name"].as_str().unwrap_or_default().to_string(),
            dir: path_of(&p["manifest_path"])
                .parent()
                .expect("清单有父目录")
                .to_path_buf(),
            roots: p["targets"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|t| path_of(&t["src_path"]))
                .collect(),
            normal_deps: p["dependencies"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|d| d["kind"].is_null())
                .map(|d| {
                    (
                        d["name"].as_str().unwrap_or_default().to_string(),
                        d.get("path").and_then(|x| x.as_str()).map(PathBuf::from),
                    )
                })
                .collect(),
        })
        .collect()
}

/// 通信层那两个 crate：恰好 [`COMMS_CRATES`] 那两个，都住 `src/comms/` 下。
fn comms_packages(all: &[Package]) -> Vec<&Package> {
    let comms_root = repo_root().join("src/comms");
    let found: Vec<&Package> = all
        .iter()
        .filter(|p| COMMS_CRATES.contains(&p.name.as_str()))
        .collect();
    let names: BTreeSet<&str> = found.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(
        names,
        COMMS_CRATES.iter().copied().collect::<BTreeSet<_>>(),
        "壳那个 workspace 里找不全通信层那两个 crate —— 改名了 / 出列了"
    );
    for p in &found {
        assert!(
            p.dir.starts_with(&comms_root),
            "`{}` 不住 `src/comms/` 下（{}）",
            p.name,
            p.dir.display()
        );
    }
    found
}

// ════════════════════════════════════════════════════════════════════════════
//  二、人群：两个 crate 的源文件（含同住目录的 `chan.ts`）—— 每条判据的第一句
// ════════════════════════════════════════════════════════════════════════════

/// 一份成员：`(仓根相对路径, 生产段)`。
struct Member {
    rel: String,
    prod: String,
}

/// 剥生产段 —— 按后缀分派，**一份文件一种剥法**。
///
/// ⚠ 为什么判据看的是生产段而不是整份文件：注释里写「本层与 tmux 无关」是**散文**，
/// 不是代码。拿整份文件判的话，成员文件连解释自己为什么干净都做不到，
/// 而那种摩擦的终局是有人回来把判据削掉。
/// ⚠ 代价如实记：**注释里长出来的业务词本族判据看不见**。
///
/// 🔴 **三档全部调共享原语，一行剥法都不自己写。**
/// 第一版的 `.ts` 那档内联了一个 `starts_with("//")` 过滤，
/// `structural_scan::every_comment_stripping_transformer_is_registered` 当场逮住它
/// 并逐字问「共享原语 `guard_core::strip_comment_lines` 为什么不够」—— 答案是**够**。
/// ⇒ 那不是登记一条豁免的理由，是改成调它的理由（`E3`：一个事实一个权威源）。
fn production_of(rel: &str, raw: &str) -> String {
    if rel.ends_with(".rs") {
        // 连 `#[cfg(test)]` 整块一起剥（判据要看的是生产段）。
        return guard_core::production_code(raw);
    }
    if rel.ends_with(".toml") {
        return guard_core::strip_hash_comment_lines(raw);
    }
    // `.ts`：没有 `#[cfg(test)]` 这回事，剥注释就够 —— `//` 那套形态与 Rust 同形。
    guard_core::strip_comment_lines(raw)
}

/// 人群：两个通信层 crate 目录下的 `.rs` / `.ts`（`chan.ts` 住 `comms-inward` 的目录、同归边界判据管）。
///
/// 反空真：每个 crate 的 crate 根（`cargo metadata` 的 `targets[].src_path`）必须在人群里（两侧异源：清单 vs 走目录）；
/// 人群里至少有一份 `.ts`（`C1` / `C3` 的 TS 那一面）。
fn boundary() -> Vec<Member> {
    let root = repo_root();
    let all = workspace_packages();
    let mut out: Vec<Member> = Vec::new();
    for pkg in comms_packages(&all) {
        let files = guard_core::scan_tree_excluding(&pkg.dir, &["rs", "ts"], &[]);
        let seen: BTreeSet<PathBuf> = files.iter().map(|(p, _)| p.clone()).collect();
        for r in &pkg.roots {
            assert!(
                seen.contains(r),
                "`{}` 的 crate 根 {} 不在走目录收到的人群里 —— 取法坏了",
                pkg.name,
                r.display()
            );
        }
        for (p, raw) in files {
            let rel = p
                .strip_prefix(&root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            let prod = production_of(&rel, &raw);
            out.push(Member { rel, prod });
        }
    }
    assert!(
        out.iter().any(|m| m.rel.ends_with(".ts")),
        "人群里一份 `.ts` 都没有（`chan.ts` 那一面）—— 取法坏了"
    );
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    out
}

// ════════════════════════════════════════════════════════════════════════════
//  三、判据清单（元判据用它对拍「这些条真的在跑」）
// ════════════════════════════════════════════════════════════════════════════

/// `(编号, 判据函数名, 它钉什么)`。**闭集**，与本文件里真实的 `#[test]` 两向相等。
const CRITERIA: &[(&str, &str, &str)] = &[
    (
        "成员",
        "members_are_the_two_comms_crates_and_nothing_is_path_mounted_into_them",
        "生产代码里 `#[path]` 指进 `src/comms/` 的零处（成员只经 crate 依赖进来）",
    ),
    (
        "C1",
        "c1_no_business_concept_is_named_on_the_public_surface",
        "公开面上不许命名业务概念（豁免必须为零）",
    ),
    (
        "C2",
        "c2_the_comms_crates_depend_only_on_the_allowed_crates",
        "两个 crate 的普通依赖（含传递的第一方 crate）⊆ 只许的那几个，业务 crate 一个都不许出现",
    ),
    (
        "C3",
        "c3_the_word_transport_never_crosses_the_boundary",
        "前端发出的请求里不许含 `transport`（`transport` 是本层的内部选择）",
    ),
    (
        "X1",
        "x1_every_match_on_the_three_wire_types_is_exhaustive",
        "对 `CallError` / `Item` / `Reach` 的 `match` 穷尽、零 `_ =>`（只盖一个变体的那一形开关挡不住）",
    ),
    (
        "X4",
        "x4_the_only_way_to_drop_is_to_say_gap",
        "丢弃只能经 `Item::Gap` 表达 —— 零 `let _ = …send(…)`（`try_send` 由名单挡）",
    ),
    (
        "元",
        "every_criterion_is_on_the_execution_chain",
        "上面这张表与本文件里真实的 `#[test]` 两向相等",
    ),
];

// ════════════════════════════════════════════════════════════════════════════
//  四、成员只经 crate 依赖进来（`#[path]` 指进 `src/comms/` 零处）
// ════════════════════════════════════════════════════════════════════════════

/// 一份生产段里 `#[path = "…"]` 指到的、落在 `comms` 目录下的那几个目标（`file` 是这份源码的绝对住址）。
fn path_mounts_into(comms: &Path, file: &Path, prod: &str) -> Vec<String> {
    let mut out = Vec::new();
    for rest in prod.split("#[path = \"").skip(1) {
        let rel = rest.split('"').next().unwrap_or_default();
        let mut target = PathBuf::new();
        for c in file
            .parent()
            .unwrap_or(Path::new(""))
            .join(rel)
            .components()
        {
            match c {
                std::path::Component::ParentDir => {
                    target.pop();
                }
                std::path::Component::CurDir => {}
                other => target.push(other),
            }
        }
        if target.starts_with(comms) {
            out.push(rel.to_string());
        }
    }
    out
}

/// ★ **成员 ＝ crate**：生产代码里 `#[path]` 指进 `src/comms/` 的零处 —— 通信层的文件只经 crate 依赖进别的包，
/// 不再被谁的模块树挂进去（挂进去的那一刻它就不受那两个 crate 的依赖边界管了）。
///
/// 人群：`src/` 下全部 `.rs` 的生产段（测试段里挂测试文件不算，那是 `tests/` 那一侧）。
/// 正控：往一份合成的后端源码里塞一行 `#[path = "../comms/outward/x.rs"]` 必须数得出来。
#[test]
fn members_are_the_two_comms_crates_and_nothing_is_path_mounted_into_them() {
    let root = repo_root();
    let comms = root.join("src/comms");
    let files = guard_core::scan_tree_excluding(&root.join("src"), &["rs"], &[]);
    assert!(
        files
            .iter()
            .any(|(p, _)| *p == root.join("src/frontend/shell/src/lib.rs")),
        "`src/` 下的遍历没走到 `frontend/shell/src/lib.rs`（走到 {} 份）—— 取法坏了，下面的零命中在空转",
        files.len()
    );
    let mut hits: Vec<String> = Vec::new();
    for (p, raw) in &files {
        for rel in path_mounts_into(&comms, p, &guard_core::production_code(raw)) {
            hits.push(format!("  {}：`#[path = \"{rel}\"]`", p.display()));
        }
    }
    assert!(
        hits.is_empty(),
        "生产代码用 `#[path]` 把通信层的文件挂进了自己的模块树：\n{}\n\n\
         ⇒ 通信层的文件只许经 crate 依赖用（`comms_inward::…` / `comms_outward::…`）；\
         挂进去的那份就不再受那两个 crate 的依赖边界管。",
        hits.join("\n")
    );
    // 正控：识别器认得出那一形（相对路径按那份源码的住址解开）。
    let planted = format!("#[{} = \"../comms/outward/x.rs\"]\nmod x;\n", "path");
    assert_eq!(
        path_mounts_into(&comms, &root.join("src/backend/lib.rs"), &planted),
        vec!["../comms/outward/x.rs".to_string()],
        "识别器认不出一行指进 `src/comms/` 的 `#[path]` —— 上面那条零命中在空转"
    );
    // 两个 crate 都在、都住 `src/comms/` 下（人群的前提）。
    let _ = comms_packages(&workspace_packages());
}

// ════════════════════════════════════════════════════════════════════════════
//  六、C1–C3：通信层的铁律（扫文字的那三条）
// ════════════════════════════════════════════════════════════════════════════

/// 业务词表 —— `C1` 的九个概念，**每个带它的复数形**
/// 〔「看怎么加」〕。
///
/// # 为什么是 `(单数, 复数)` 的**对**，不是一张摊平的十八词表
///
/// 现打了一个洞：词表全是单数，而匹配单位是标识符子词
/// ⇒ `tmux_sessions` 切出 `[tmux, sessions]`，而 `sessions ≠ session`。
/// 那一篇数出**十处真业务 `C1` 完全看不见**，含 `"/sessions/"`（那道 Claude 数据围栏
/// 自己的路径串）与 `reaper_tracked` 的形参 `announced_sids`（七行纯业务，一个词都不咬）。
///
/// 写成**对**买的是一件事：**加一个概念时不可能只加单数**。摊平的话
/// 「漏了复数」与「刻意只要单数」在盘上一模一样 —— 而那正是 `103` 逮到的那个洞的成因。
///
/// ⚠ `tmuxes` / `claudes` / `mcps` 这三个复数形**现打全仓零命中**（`src/` 233 份 `.rs`
/// 的生产段）。留着是对的，理由同 `C2` 那条依赖名单的反面：**本表是禁入名单，
/// 不是现存清单**。🔴 它**不是**本仓明禁的「留着用不上的豁免」—— 豁免是给违例**放行**的口子，
/// 这里是**禁**的那一侧，多一条只会更严，不会更松。
const BUSINESS_WORDS: &[(&str, &str)] = &[
    ("session", "sessions"),
    ("sid", "sids"),
    ("account", "accounts"),
    ("skill", "skills"),
    ("mcp", "mcps"),
    ("tmux", "tmuxes"),
    ("claude", "claudes"),
    ("jsonl", "jsonls"),
    ("agent", "agents"),
];

/// 词表摊平成「要拿去比子词的那些形」 —— 单数与复数一视同仁。
fn business_word_forms() -> Vec<&'static str> {
    BUSINESS_WORDS.iter().flat_map(|(s, p)| [*s, *p]).collect()
}

/// ★★ **加复数买到了什么** —— `(标识符, 靠哪个复数形咬住, 它是什么)`〔现打〕。
///
/// 🔴 **这张表是「加复数前后」那个读数唯一活着的住址，而它刻意不记处数。**
/// 处数（改时：整个传输面 365 ⇒ 375 处，新增 10 处）会随任何一次编辑腐掉，
/// 而**标识符不会** —— 表里每一行都被两条断言夹着：
/// ① 今天的词表靠那个复数形真的咬住它 · ② **只拿单数去比抓不到它**
/// （⇐ 这一条就是「加复数之前它完全看不见」，做成了可机检的形状）。
///
/// ⚠ `tmux_sessions` / `TmuxSessions` **刻意不在表里**：它们本来就被 `tmux` 咬住，
/// 复数只是多给了一个判词，不是新咬住。那十处里，
/// 这一族占 5 处 —— 把它们混进来就是把「多一个判词」读成「补了一个洞」。
///
/// ⚠ **假红那一侧量过**：`src/` 233 份 `.rs` 的生产段上，加复数**新增的命中一处假红都没有**
/// —— 全是 `accounts` / `sessions` / `agents` / `skills` / `sids` / `jsonls` 那几族真业务。
/// 最像假红的候选是 OpenSSH 的 `MaxSessions`（传输概念，不是 Claude 会话），
/// 而它**全部住在文档注释里** ⇒ 本条只看生产段，一处都不碰。
const PLURALS_NEWLY_CAUGHT: &[(&str, &str, &str)] = &[
    (
        "sessions",
        "sessions",
        "那条 `/sessions/` 路径串 —— **那道 Claude 数据围栏自己的一半**",
    ),
    ("sids", "sids", "tmux 账本里按 origin 存的那批会话 id"),
    (
        "announced_sids",
        "sids",
        "`reaper_tracked` 的形参 —— 收割器要对账「哪些 sid 该在 tmux 后端里」。\
         那个函数**七行纯业务**而 `C1` 一个词都不咬，是最值钱的一格",
    ),
];

/// 一个标识符 ⇒ 它的**子词**（按 `_` / `-` 与驼峰拆，逐块小写），塞进 `out`。
///
/// 驼峰带一条缩写规则：`HTTPServer` ⇒ `http` ＋ `server`。没有它的话，
/// 一串大写会被整块吞成一个子词，`SSHSession` 这种写法就又躲过去了。
fn push_subwords(ident: &str, out: &mut BTreeSet<String>) {
    for part in ident.split(['_', '-']) {
        let chars: Vec<char> = part.chars().collect();
        if chars.is_empty() {
            continue;
        }
        let mut start = 0usize;
        for i in 1..chars.len() {
            let (prev, cur) = (chars[i - 1], chars[i]);
            // ① 驼峰的峰：非大写后面跟大写 —— `sessionId` ⇒ `session` | `Id`。
            let hump = !prev.is_uppercase() && cur.is_uppercase();
            // ② 缩写收尾：大写 + 大写 + 小写 —— `HTTPServer` ⇒ `HTTP` | `Server`。
            let acronym_end = prev.is_uppercase()
                && cur.is_uppercase()
                && chars.get(i + 1).is_some_and(|n| n.is_lowercase());
            if hump || acronym_end {
                out.insert(chars[start..i].iter().collect::<String>().to_lowercase());
                start = i;
            }
        }
        out.insert(chars[start..].iter().collect::<String>().to_lowercase());
    }
}

/// `C1` 的**匹配单位**：把一段文本切成标识符子词（小写、去重）。
///
/// 两刀，顺序固定：
/// 1. 切**标识符** —— 连续的标识符字符（字母 / 数字 / `_` / `-`）算一个，其余一律是分隔。
///    ⇒ `path.ends_with(".jsonl")` 里那个 `jsonl` 也进得来：**字符串字面量里的业务词算数**，
///    它照样是这一层认识了业务（铁律说的是「不知道」，不是「不直接命名」）。
/// 2. 每个标识符走 [`push_subwords`]。
///
/// # 为什么它不住 `guard_core`
///
/// 今天它只有一个用户（`C1`）。共享原语的住址纪律（`E3`）治的是「同一个事实两份实现」，
/// 而这里还没有第二份 —— 真出现第二个用户时再提升，别先把一个单用户的取舍摊给全仓。
/// ⚠ 它**不是** [`guard_core::contains_word`] 的替代品：那个答「有没有这个词」，
/// 本函数答「这段文本由哪些子词构成」，两者射程不同，别互相顶替。
fn identifier_subwords(text: &str) -> BTreeSet<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    let mut ident = String::new();
    for c in text.chars() {
        if c.is_alphanumeric() || c == '_' || c == '-' {
            ident.push(c);
        } else if !ident.is_empty() {
            push_subwords(&ident, &mut out);
            ident.clear();
        }
    }
    if !ident.is_empty() {
        push_subwords(&ident, &mut out);
    }
    out
}

fn business_words_in(text: &str) -> Vec<&'static str> {
    let subwords = identifier_subwords(text);
    business_word_forms()
        .into_iter()
        .filter(|w| subwords.contains(*w))
        .collect()
}

// ── `C1` 的射程：**公开面上的声明名字** ──────

/// 一段文本切成标识符记号 `(起, 止, 文本)`（下标按 `char`，不按字节）。
fn ident_tokens(line: &str) -> Vec<(usize, usize, String)> {
    let chars: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i].is_alphanumeric() || chars[i] == '_' {
            let s = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push((s, i, chars[s..i].iter().collect()));
        } else {
            i += 1;
        }
    }
    out
}

/// 声明关键字 —— 它们后面紧跟的那个标识符是**我们在这里起的名字**。
const DECL_KEYWORDS: &[&str] = &[
    "fn", "struct", "enum", "trait", "type", "mod", "const", "static", "union",
];

/// 带花括号体的 item —— 体内的可见性规矩各不相同，见 [`public_surface_names`]。
const BRACED_ITEMS: &[&str] = &["struct", "enum", "trait", "union"];

/// 一段声明文本上处在**绑定位置**的标识符 —— 即「我们在这里起的名字」。
///
/// 🔴 **这一步是新射程的全部机关，而它是纯语法的**：它只问「这个标识符出现在
/// 起名字的位置，还是指向别人的位置」，**从不问这个名字是谁的**。
///
/// | 位置 | 例 | 算不算我们起的名字 |
/// |---|---|---|
/// | 声明关键字之后 | `pub type SshSession = …` | ✅ 算（`SshSession`） |
/// | 形参 / 字段的**左侧** | `pub fn f(sid: &str)` · `pub agent_kind: String` | ✅ 算（`sid` · `agent_kind`） |
/// | 类型引用（`:` 右侧 · `->` 之后） | `-> Result<SftpSession, E>` | ❌ 不算 |
/// | 带 `::` 的路径段 | `russh_sftp::client::SftpSession` | ❌ 不算 |
///
/// ⇒ 那条反驳（「要放过第三方名字就得解析类型」）在这个射程下
/// **不需要回答**：第三方的名字之所以过得去，不是因为判据认出它是第三方的，
/// 而是因为它出现在**引用位**。同一份文件里我们自己起的 `SshSession` 照样咬住
/// （`103 §P5.4` 订正 4 逐字：它是我们自己的 `type` 别名，**改得了**）。
///
/// ⚠ `use` 那一档单独走：`pub use a::b::C;` 把 `C` 绑进我们的公开命名空间
/// ⇒ 末段算我们起的名字，前面的路径段不算。
fn declared_names(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let toks = ident_tokens(text);
    let mut out: Vec<String> = Vec::new();
    let char_at = |k: usize| -> Option<char> { chars.get(k).copied() };
    let path_before = |start: usize| -> bool {
        let mut k = start;
        while k > 0 && chars[k - 1].is_whitespace() {
            k -= 1;
        }
        k >= 2 && chars[k - 1] == ':' && chars[k - 2] == ':'
    };
    // 记号之后第一个非空白字符的下标。
    let next_at = |end: usize| -> usize {
        let mut k = end;
        while k < chars.len() && chars[k].is_whitespace() {
            k += 1;
        }
        k
    };

    if toks.iter().any(|(_, _, t)| t == "use") {
        for (s, e, t) in &toks {
            if matches!(
                t.as_str(),
                "use" | "as" | "crate" | "self" | "super" | "pub"
            ) {
                continue;
            }
            let k = next_at(*e);
            let followed_by_path = char_at(k) == Some(':') && char_at(k + 1) == Some(':');
            if !followed_by_path {
                let _ = path_before(*s); // 末段无论前面有没有路径都算（`a::b::C` 的 `C`）
                out.push(t.clone());
            }
        }
        return out;
    }

    for (i, (s, e, t)) in toks.iter().enumerate() {
        if DECL_KEYWORDS.contains(&t.as_str()) {
            if let Some((_, _, n)) = toks.get(i + 1) {
                out.push(n.clone());
            }
            continue;
        }
        let k = next_at(*e);
        let single_colon = char_at(k) == Some(':') && char_at(k + 1) != Some(':');
        if single_colon && !path_before(*s) {
            out.push(t.clone());
        }
    }
    out
}

/// 一段 `enum` 体内的文本 ⇒ 变体名 ＋ 它的具名字段名。
///
/// 变体随它的 `enum` 公开（Rust 不给变体独立的可见性）⇒ `pub enum` 的体整块是公开面。
/// 这与**结构体**相反：结构体字段默认私有，要自己带 `pub`。
fn variant_names(seg: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Some((_, _, first)) = ident_tokens(seg).first() {
        if first.chars().next().is_some_and(|c| c.is_uppercase()) {
            out.push(first.clone());
        }
    }
    out.extend(declared_names(seg));
    out
}

/// 一行的圆括号净增量。
fn paren_delta(line: &str) -> i32 {
    line.matches('(').count() as i32 - line.matches(')').count() as i32
}

/// 折行的签名最多往后拼几行（rustfmt 下一个签名不会比这更长；拼不完就原样退回）。
const SIG_JOIN_MAX: usize = 40;

fn is_pub(trimmed: &str) -> bool {
    guard_core::strip_visibility(trimmed) != trimmed
}

/// ★★ **公开面** —— 一份生产段上「我们在公开声明里起的那些名字」，`(名字, 它出自的那行)`。
///
/// 三档进公开面，逐档的可见性规矩不同（这是 Rust 的规矩，不是我们定的）：
///
/// | 档 | 什么算公开面 | 依据 |
/// |---|---|---|
/// | 带 `pub` 的 item 行 | 它起的名字 ＋ 形参名 ＋ 带 `pub` 的字段名 | 有 `pub` 就是对外的脸 |
/// | `pub enum` 的体 | 变体名 ＋ 变体的具名字段名 | 变体没有独立可见性，随 `enum` 公开 |
/// | `pub trait` 的体 | 方法签名 | trait 成员随 trait 公开 |
///
/// 🔴 **折行的签名要先拼回来**：rustfmt 把长签名折成多行，而续行**不带 `pub`**
/// ⇒ 只看物理行的话 `pub(crate) async fn connect_session(` 后面那几行形参会**整批漏掉**。
/// 本函数按圆括号配平往后拼（[`paren_delta`]），只从**声明头**那一行开始拼
/// —— 从任意一行开始拼的话，一条括号不配平的函数体会把后面那个 `pub fn` 吞进去，
/// 而那是**假绿**方向的错（漏掉一个公开声明）。
///
/// # ⚠ 它认不出的那几格（如实登记，已知的洞）
///
/// 1. **私有类型上的 `pub fn`** —— `impl` 的接收者是不是公开的，本函数不看
///    ⇒ 私有类型的 `pub fn` 也当公开面判。方向是**收紧**（不会漏判），如实记着。
/// 2. **`pub(crate)` 与 `pub` 一视同仁** —— 两者对「模块外看得见」都成立，而
///    `C1` 守的是层与层之间那道边界，不是 crate 的导出面。同样是收紧方向。
/// 3. **宏生成的公开面** —— `macro_rules!` 展开出来的 `pub` 项本函数看不见。
///    挡这一形要展开宏，那不是文本判据干的事。
fn public_surface_names(prod: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = prod.lines().collect();
    let mut out: Vec<(String, String)> = Vec::new();
    let mut depth: i32 = 0;
    // 公开 `enum` / `trait` 的体：`(进去之前的 depth, 哪一档)`
    let mut open_pub: Vec<(i32, &'static str)> = Vec::new();
    let mut i = 0usize;
    while i < lines.len() {
        let first = lines[i].trim();
        let pubbed = is_pub(first);
        let inside = open_pub.last().and_then(|(d, k)| match *k {
            "enum" if depth > *d => Some("enum"),
            "trait" if depth == *d + 1 => Some("trait"),
            _ => None,
        });

        // 折行的声明头往后拼到圆括号配平。
        let mut consumed = 1usize;
        let mut text = first.to_string();
        if pubbed || inside.is_some() {
            let mut d = paren_delta(first);
            while d > 0 && i + consumed < lines.len() && consumed < SIG_JOIN_MAX {
                let nxt = lines[i + consumed].trim();
                text.push(' ');
                text.push_str(nxt);
                d += paren_delta(nxt);
                consumed += 1;
            }
        }

        let after = guard_core::strip_visibility(text.trim());
        let kind = after
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_end_matches(|c: char| !c.is_alphanumeric());

        let mut names: Vec<String> = Vec::new();
        if pubbed {
            if BRACED_ITEMS.contains(&kind) && after.contains('{') {
                // 同一行就开了体（内联形）：头部走声明名，体内按档分派。
                let (head, tail) = after.split_once('{').unwrap_or((after, ""));
                names.extend(declared_names(head));
                let tail = tail.trim_end().trim_end_matches('}');
                for seg in tail.split(',') {
                    let s = seg.trim();
                    if kind == "enum" {
                        names.extend(variant_names(s));
                    } else if kind == "trait" || is_pub(s) {
                        names.extend(declared_names(guard_core::strip_visibility(s)));
                    }
                }
            } else {
                names.extend(declared_names(after));
            }
        } else if inside == Some("enum") {
            names.extend(variant_names(&text));
        } else if inside == Some("trait")
            && (text.trim_start().starts_with("fn ") || text.trim_start().starts_with("async fn "))
        {
            names.extend(declared_names(&text));
        }

        let mut seen: BTreeSet<String> = BTreeSet::new();
        for n in names {
            if !n.is_empty() && seen.insert(n.clone()) {
                out.push((n, first.to_string()));
            }
        }

        let opens = text.matches('{').count() as i32 - text.matches('}').count() as i32;
        if pubbed && (kind == "enum" || kind == "trait") && opens > 0 {
            open_pub.push((depth, if kind == "enum" { "enum" } else { "trait" }));
        }
        depth += opens;
        while open_pub.last().is_some_and(|(d, _)| depth <= *d) {
            open_pub.pop();
        }
        i += consumed;
    }
    out
}

/// 一份成员的**公开面**上被咬住的那些处 —— `(名字, 判词, 出处那行)`。
fn public_surface_offences(prod: &str) -> Vec<(String, Vec<&'static str>, String)> {
    offences_among(public_surface_names(prod))
}

fn offences_among(names: Vec<(String, String)>) -> Vec<(String, Vec<&'static str>, String)> {
    names
        .into_iter()
        .filter_map(|(name, line)| {
            let hits = business_words_in(&name);
            (!hits.is_empty()).then_some((name, hits, line))
        })
        .collect()
}

/// 按成员的语言取公开面：`.ts` 成员（`src/comms/inward/chan.ts`）走 [`ts_public_surface_names`]，
/// 其余走 Rust 的 [`public_surface_names`]。**判词表与匹配单位同一份**（[`business_words_in`]）——
/// 只有「什么算公开面」随语言的可见性规矩换。
fn surface_names_of(rel: &str, prod: &str) -> Vec<(String, String)> {
    if rel.ends_with(".ts") {
        ts_public_surface_names(prod)
    } else {
        public_surface_names(prod)
    }
}

fn surface_offences_of(rel: &str, prod: &str) -> Vec<(String, Vec<&'static str>, String)> {
    offences_among(surface_names_of(rel, prod))
}

/// TS 那一门的声明关键字（它后面紧跟的标识符是我们起的名字）。
const TS_DECL_KEYWORDS: &[&str] = &[
    "function",
    "const",
    "let",
    "class",
    "interface",
    "type",
    "enum",
];

/// **TS 成员的公开面** —— TS 的可见性规矩是 `export`：
///
/// | 档 | 什么算公开面 |
/// |---|---|
/// | `export` 起头的那一行 | 它起的名字（`function` / `const` / `class` / `interface` / `type` / `enum` 之后那个）＋ 同一行上处在绑定位的名字（形参 · 字段） |
/// | 导出声明的**体**（接口 / 类 / 对象字面量，直接成员那一层） | 成员名（`name:` 形）＋ 方法名（`name(` 形）＋ 方法的形参名 |
/// | `export type X =` 之后以 `\|` 起头的续行（联合类型） | 那几行上处在绑定位的名字 |
///
/// ⚠ 买不到：再往里嵌一层的字面量类型（`at: { idx; tag }` 里的 `idx`/`tag` 在同一行时照收，折行时不收）；
/// 未导出的东西一律不算（与 Rust 那一门「私有的不算」同一条规矩）。
fn ts_public_surface_names(prod: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut depth: i32 = 0;
    let mut body: Option<i32> = None;
    let mut union = false;
    for line in prod.lines() {
        let t = line.trim();
        let mut names: Vec<String> = Vec::new();
        if let Some(rest) = t.strip_prefix("export ") {
            let rest = rest.strip_prefix("default ").unwrap_or(rest);
            let rest = rest.strip_prefix("async ").unwrap_or(rest);
            let toks = ident_tokens(rest);
            let kw = toks.first().map(|(_, _, k)| k.as_str()).unwrap_or("");
            if TS_DECL_KEYWORDS.contains(&kw) {
                if let Some((_, _, n)) = toks.get(1) {
                    names.push(n.clone());
                }
            }
            names.extend(declared_names(rest));
            if rest.matches('{').count() > rest.matches('}').count() {
                body = Some(depth);
            }
            union = kw == "type" && !rest.contains('{') && rest.trim_end().ends_with('=');
        } else if body.is_some_and(|d| depth == d + 1) {
            let toks = ident_tokens(t);
            if let Some((_, e, first)) = toks.first() {
                if t[..].chars().nth(*e) == Some('(') {
                    names.push(first.clone());
                }
            }
            names.extend(declared_names(t));
        } else if union && t.starts_with('|') {
            names.extend(declared_names(t));
        } else {
            union = false;
        }
        for n in names {
            if !n.is_empty() && seen.insert(n.clone()) {
                out.push((n, t.to_string()));
            }
        }
        depth += t.matches('{').count() as i32 - t.matches('}').count() as i32;
        if body.is_some_and(|d| depth <= d) {
            body = None;
        }
    }
    out
}

/// ★ `C1` —— **公开面上不许命名业务概念**。
///
/// 「⚠ **`C1` 的豁免必须为零。** 一旦开始豁免，它就变成第三个业务的家」
/// ⇒ 本条**没有白名单，也不给一个**。要留口子，先去改设计。
///
/// # ★★ 用户的裁决：「零业务判断」，不是「零业务语义」
///
/// 「**理论上可以懂业务, 不然他怎么把流量分流成我们想要的样子**」。
/// 现打的支撑：中转手里是一个 `RouteKey{ seg1, seg2 }` ＋ 一个 `stream`，
/// `route.rs` 的注释逐字「**它是 sid，但中转不需要知道**」，而「谁是 agent、谁是账号」
/// 只在上游选择（`accounts/`）才有名字 ⇒ 分流是「中转只切、上游选择才决定」。
/// **但中转的形状本身**（前两段当键、第三段当流名）**就是一条业务事实** ——
/// 它「知道」一个请求长成账号/agent/会话那个样子，只是管它们叫 `seg1`/`seg2`。
/// ⇒ 它真正做到的是**零业务判断**，不是零业务语义。
///
/// # ★★ 于是射程从「出现」换成「在公开面上命名」
///
/// | | 上一版 | 本版 |
/// |---|---|---|
/// | 禁的是 | 生产段里**出现**业务词 | 在**公开面**（[`public_surface_names`] 那三档）上**命名**业务概念 |
/// | 放过的 | 无 | **内部提到** —— 类型引用（`-> SftpSession`）· 私有字段/局部名 · 报错文案串 |
/// | 守住的还是 | 「不许有第三个业务家」 | **同一条** —— 业务**判断**靠的是公开面上的类型，不是内部提到谁 |
///
/// 🔴 **豁免仍为零，变的是射程，不是例外。** 这与 2026-09-18 那次「人群不含注释」
/// 是同一个动作，那一次的措辞逐字就是这一句。
///
/// # 🔴 它为什么绕得开那三条结构性反驳
///
/// `103` 否掉的是**另一条**候选（那个「第三方 crate 的导入名不算」），
/// 三条理由逐条对照本射程：
///
/// | `103` 的反驳 | 本射程怎么绕开 | 现打的证据 |
/// |---|---|---|
/// | ① A 类要**类型解析**才认得出（第三方固有方法 `channel_open_session` 全仓零定义） | 本射程**不问名字是谁的**，只问它在**声明位**还是**引用位** —— 纯语法 | 传输面上 `channel_open_session` 4 处 · `SftpSession` 17 处 · `RawSftpSession` 3 处，**全部落在引用位 ⇒ 一处不咬**；而我们自己的 `pub(crate) type SshSession` 在声明位 ⇒ **照咬** |
/// | ② A 与 C 在语法上**同形**（报错文案里的 `ssh-agent` 是传输、`tmux` 是业务） | 本射程**不需要分开它们**：串里没有任何声明位 ⇒ 两者一起出射程 | `103` 那 14 处「凑绿」全在报错文案里 ⇒ 新射程 **0 处** |
/// | ③ B 改完 C 搬完之后仍有 14 处咬着 | 同 ②，那 14 处出射程 | 同上 |
///
/// ⚠ **代价要认下来，它不小**：② 那条是靠「把 A 与 C 一起放过」绕开的
/// ⇒ **写在串里的业务名从此本条看不见**。现打的活样本是 `relay/tee.rs`：
/// 它那 4 处 `agent`/`account` 是 tee 输出的 **JSON 字段名**，写成格式串
/// ⇒ 上一版咬（`C1` ＋ `X4`），本版只剩 `X4`。那不是报错文案，是一条线上契约，
/// 而**本射程分不出这两者** —— `103 §P5.3 ②` 那个形状换了个位置又出现了一次。
/// 🔴 这一格**今天判不了**，缺的证据是一份「线上契约的字段名住哪」的独立读数。
/// 挡它的不是本条，是线上类型列全变体（两包的 `wildcard_enum_match_arm` 开关 ＋ `X1`）与金标准逐字节对拍。
///
/// # 匹配单位仍是**标识符子词**，词表加了复数〔`§8.1.5`〕
///
/// 走 [`identifier_subwords`]：先切标识符，再按 `_`/`-` 与驼峰拆。
/// · `sid` 当形参名 ⇒ 咬 · `SshSession` ⇒ `[ssh, session]` 咬 · `tmux_sessions` ⇒ `[tmux, sessions]` 咬
/// · `considered` ⇒ `[considered]` ≠ `sid` **不假红**
///
/// 🔴 **裸 `contains` 那条路是被否掉的，别退回去**。
/// 下面那条阴性对照钉着它，退回去**当场红**。
///
/// **买到**：登记成员的公开面上，那九个概念（单数与复数两形）一个都不许出现 ——
/// `pub` 类型名 · `pub fn` 签名（含折行的续行）· 带 `pub` 的字段名 ·
/// `pub enum` 的变体名 · `pub trait` 的方法名，全部算数。
///
/// **买不到**：
/// ① 注释里的业务词（本条只看生产段，理由见 [`production_of`]）；
/// ② **换了名字的业务** —— 把 `sid` 改叫 `handle` 照样过。改名不是剥；
///    **剥是把业务语义搬出这一层**。本射程让这条诱惑更便宜了：只要别写在公开面上。
/// ③ 🔴 **实现里的业务判断** —— 那个 `reaper_tracked`
///    （七行纯业务：收割器要对账的 sid 集合）是个**私有** `fn`
///    ⇒ 上一版看不见它（复数），本版**照样看不见**（不在公开面）。
///    加复数买到的是它那两个形参在**别处**被数出来，不是这一条咬住了它。
/// ④ 🔴 **既没有分隔符、也没有驼峰的连写** —— `sessionid` / `session2` / `sidfoo`
///    拆不出子词 ⇒ 看不见。裁定逐字只给了「`_` 与驼峰」两刀，
///    **数字边界不在裁定里，这里没有擅自加**。
/// ⑤ 宏展开出来的公开面、私有类型上的 `pub fn` —— 逐格记在 [`public_surface_names`] 里。
///
/// # 🔴 **换射程这件事，今天的人群一个字都证明不了**（这一格必须写出来）
///
/// 现打：登记在册那几份成员在**新旧两个射程下都是 0 处**
/// ⇒ 那条相等断言在换射程前后**一模一样地绿**，它分不出这两个射程。
/// ⇒ **唯一在为新射程作证的是下面那两组对照**（公开面七形必须红 · 实现六形必须不红），
/// 以及模块头注里记的 `D1`–`D4`／`D9` 那几刀。
/// 🔴 谁哪天觉得那两组对照「啰嗦」把它删了，这一改就当场退化成
/// **「把规矩放宽了」而不是「换了射程」**，而那是用户拍这一板时**没有**授权的东西。
/// **不许删。**
#[test]
fn c1_no_business_concept_is_named_on_the_public_surface() {
    let pop = boundary();
    let offenders: Vec<String> = pop
        .iter()
        .flat_map(|m| {
            surface_offences_of(&m.rel, &m.prod)
                .into_iter()
                .map(move |(name, hits, line)| {
                    format!("  {} —— `{name}` {hits:?}   ← {line}", m.rel)
                })
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "通信层成员的**公开面**上命名了业务概念：\n{}\n\n\
铁律：**通信层不知道什么是会话、账号、skill、agent。**\n\
         它只知道地址（`origin` / 路由键）· 操作名 · 载荷 · 流的订阅与分发。\n\
         ⚠ **豁免必须为零** —— 本条没有白名单，别来加。\n\
         ⚠ 处置不是「把名字挪进实现」那种凑绿：给的是\n\
         **用位置称呼它搬的东西**（`RouteKey{{ seg1, seg2 }}`），业务名只出现在后端那一半。",
        offenders.join("\n")
    );

    // ── 阳性对照：公开面那三档，每个词的单数与复数各喂一遍 ──────────────────
    // ★ 人群为空 / 人群全绿的日子里，这才是本条真正跑过的东西。
    for (singular, plural) in BUSINESS_WORDS {
        for w in [singular, plural] {
            let camel = {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            };
            for (shape, synthetic) in [
                ("pub 函数名", format!("pub fn route_{w}(id: &str) -> u8 {{ 0 }}\n")),
                (
                    "pub 签名里的形参名",
                    format!("pub fn route(origin: &str, {w}: &str) -> u8 {{ 0 }}\n"),
                ),
                (
                    "折行的 pub 签名",
                    format!("pub(crate) async fn route(\n    origin: &str,\n    {w}: &str,\n) -> u8 {{\n    0\n}}\n"),
                ),
                ("pub 类型名", format!("pub struct Wrap{camel}Handle;\n")),
                (
                    "pub 字段名",
                    format!("pub struct Wire {{\n    pub {w}_of: String,\n}}\n"),
                ),
                (
                    "pub enum 的变体名",
                    format!("pub enum Frame {{\n    {camel}Added {{ raw: String }},\n}}\n"),
                ),
                (
                    "pub use 的末段",
                    format!("pub use crate::wire::{camel}Handle;\n"),
                ),
            ] {
                let got: Vec<String> = public_surface_offences(&synthetic)
                    .into_iter()
                    .flat_map(|(_, h, _)| h.into_iter().map(|s| s.to_string()))
                    .collect();
                assert!(
                    got.iter().any(|g| g == w),
                    "词表里写着 `{w}`，识别器在**{shape}**这一形上认不出它 —— \
                     射程比事实小。喂的是：{synthetic:?}，抓到：{got:?}"
                );
            }
        }
    }

    // ── 🔴 阴性对照一：**同一个词注在实现里必须不红**。两个方向都要，否则这一改
    //    就退化成「把规矩放宽了」而不是「换了射程」。
    for (singular, plural) in BUSINESS_WORDS {
        for w in [singular, plural] {
            let camel = {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                    None => String::new(),
                }
            };
            for (shape, synthetic) in [
                (
                    "局部名",
                    format!("pub fn route(id: &str) -> u8 {{\n    let {w} = id;\n    0\n}}\n"),
                ),
                (
                    "私有字段",
                    format!("pub struct Wire {{\n    {w}_of: String,\n}}\n"),
                ),
                (
                    "私有函数名与它的形参",
                    format!("fn helper_{w}({w}_of: &str) -> u8 {{ 0 }}\n"),
                ),
                (
                    "报错文案串",
                    format!("pub fn route() -> u8 {{\n    panic!(\"打开 {w} 失败\");\n}}\n"),
                ),
                (
                    "第三方类型路径（引用位）",
                    format!("pub fn route() -> russh_x::client::{camel}Thing {{\n    todo!()\n}}\n"),
                ),
                (
                    "返回类型里的第三方名",
                    format!("pub(crate) fn dial(&self) -> Result<{camel}Session, Error> {{\n    todo!()\n}}\n"),
                ),
            ] {
                let got = public_surface_offences(&synthetic);
                assert!(
                    got.is_empty(),
                    "**{shape}**里的 `{w}` 被判成违例 —— 那是**内部提到**，\
                     那条裁决把它放过了（射程只覆盖公开面）。\n\
                     没有这一条，这一改就退化成「把规矩放宽了」而不是「换了射程」。\n\
                     喂的是：{synthetic:?}，抓到：{got:?}"
                );
            }
        }
    }

    // ── 🔴 阴性对照二：**被撑大的那一族不许命中**（匹配单位没有放宽成裸 `contains`）。
    // 否掉了那条路。谁把 [`identifier_subwords`] 换回
    //    `text.contains(w)`，这里当场红。
    let stretched = "pub fn route(considered: u8, residual: u8, sessionize: u8, accounting: u8, \
         skillet: u8, claudette: u8, mcpx: u8, agentic: u8, sideline: u8) -> u8 { 0 }\n";
    assert!(
        public_surface_offences(stretched).is_empty(),
        "被撑大的标识符（`considered` / `sessionize` / `accounting` / `agentic` / `sideline` …）\
         被判成业务词 —— 匹配单位放宽过头了（裸 `contains` 那条路否掉过）。\
         假红比不查更坏：它会训练人绕过判据。实际命中：{:?}",
        public_surface_offences(stretched)
    );
    assert!(
        public_surface_offences("pub fn route(origin: &str, op: &str, payload: &[u8]) {}\n")
            .is_empty(),
        "一段**只用位置词**的干净公开面被判成有业务词 —— 假红比不查更坏"
    );

    // ── 🔴TS 那一门的牙：导出面上的业务名必须咬（函数名 · 形参 · 导出接口的成员）；
    //    只用位置词的导出面不许咬；**未导出**的不算（与 Rust「私有的不算」同一条规矩）。
    let ts_bad = "export function sessionFor(account: string): void {}\n\
                  export interface Face {\n  tmuxName: string;\n}\n";
    let got: BTreeSet<String> = offences_among(ts_public_surface_names(ts_bad))
        .into_iter()
        .map(|(n, _, _)| n)
        .collect();
    assert_eq!(
        got,
        ["account", "sessionFor", "tmuxName"]
            .into_iter()
            .map(str::to_string)
            .collect::<BTreeSet<_>>(),
        "TS 导出面上的业务名没咬全 —— TS 那一门的公开面提取器在空转"
    );
    let ts_clean = "export const chan = {\n  call(origin: Origin, op: string, payload: Uint8Array, budget: Budget) {},\n};\n\
                    function sessionPrivate(account: string) {}\n";
    assert!(
        offences_among(ts_public_surface_names(ts_clean)).is_empty(),
        "只用位置词的 TS 导出面被判成有业务词、或未导出的名字被算进了公开面：{:?}",
        offences_among(ts_public_surface_names(ts_clean))
    );

    // ── 🔴 提取器自检：**每一份成员**的公开面都非空（不是合计非空）。
    //    射程收窄之后最阴的失效形是「一个名字都抠不出来」—— 那时上面那条相等断言
    //    会拿两个空集比出绿。合计式的地板挡不住「其中一份掉到 0」，而本仓正是栽在地板上。
    let mute: Vec<String> = pop
        .iter()
        .filter(|m| surface_names_of(&m.rel, &m.prod).is_empty())
        .map(|m| format!("  {}", m.rel))
        .collect();
    assert!(
        mute.is_empty(),
        "这几份成员的公开面上**一个名字都抠不出来**：\n{}\n\n\
         ⇒ 提取器坏了 / 剥法把整段剥空了 / 那份文件真的一个 `pub` 都没有。\n\
         前两种情形下，上面那条断言是在两个空集之间比对（恒绿）。\n\
         第三种情形也得有人看一眼：一份**对外零公开面**的文件，`C1` 在它身上买到的是零。",
        mute.join("\n")
    );

    // ── 🔴 复数那一改补上的那个洞，逐个标识符钉住。
    //    这一段是「加复数前后」那个读数**唯一活着的住址**：处数会腐，标识符不会。
    for (ident, plural, what) in PLURALS_NEWLY_CAUGHT {
        let now = business_words_in(ident);
        assert!(
            now.contains(plural),
            "登记表说 `{ident}`（{what}）靠复数形 `{plural}` 才咬得住，\
             而识别器在它身上抓到的是 {now:?} —— 词表与登记脱钩了"
        );
        let singular_only: Vec<&str> = BUSINESS_WORDS
            .iter()
            .map(|(s, _)| *s)
            .filter(|s| identifier_subwords(ident).contains(*s))
            .collect();
        assert!(
            singular_only.is_empty(),
            "登记表把 `{ident}`（{what}）记成「加复数之前完全看不见」，\
             而只拿单数去比也抓到了 {singular_only:?} —— 那这一行买到的不是复数这一改，\
             登记错了（那十处里，`tmux_sessions` 一族正是靠 `tmux` 咬住的，\
             它们**不该**进这张表）"
        );
    }
    // 反向控制：单数那一侧还活着（否则上面那条「只拿单数抓不到」恒真）。
    assert!(
        !business_words_in("session_id").is_empty(),
        "连 `session_id` 都抓不到了 —— 单数那一侧整批瞎了，上面那组断言会恒真"
    );
}

/// 一份 `ALLOWED_DEPS` 里那个 crate 的「只许」名单。
fn allowed_for(name: &str) -> BTreeSet<&'static str> {
    ALLOWED_DEPS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, deps)| deps.iter().copied().collect())
        .unwrap_or_default()
}

/// 一个通信层 crate 的依赖越界：直接的普通依赖不在名单里 ＋ 顺着第一方 path 依赖传递进来、不在名单里的第一方 crate。
fn dependency_offences(pkg: &Package, all: &[Package]) -> Vec<String> {
    let allowed = allowed_for(&pkg.name);
    let mut bad: Vec<String> = pkg
        .normal_deps
        .iter()
        .filter(|(n, _)| !allowed.contains(n.as_str()))
        .map(|(n, _)| format!("直接依赖 `{n}`"))
        .collect();
    let by_dir: BTreeMap<&Path, &Package> = all.iter().map(|p| (p.dir.as_path(), p)).collect();
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    let mut todo: Vec<(String, PathBuf)> = pkg
        .normal_deps
        .iter()
        .filter_map(|(n, d)| d.clone().map(|d| (n.clone(), d)))
        .collect();
    while let Some((name, dir)) = todo.pop() {
        if !seen.insert(dir.clone()) {
            continue;
        }
        if !allowed.contains(name.as_str()) {
            bad.push(format!("第一方 crate `{name}`（{}）", dir.display()));
        }
        if let Some(dep) = by_dir.get(dir.as_path()) {
            todo.extend(
                dep.normal_deps
                    .iter()
                    .filter_map(|(n, d)| d.clone().map(|d| (n.clone(), d))),
            );
        }
    }
    bad.sort();
    bad.dedup();
    bad
}

/// ★ `C2` —— 由**依赖图**判：两个通信层 crate 的普通依赖 ⊆ [`ALLOWED_DEPS`]，
/// 顺着第一方 path 依赖传递进来的第一方 crate 也必须在名单里。业务 crate 一个都不许出现。
///
/// 同 crate 内的越界由编译器挡（crate 里写 `crate::ssh_source::…` 根本编不过）；跨 crate 的只有依赖这一条路，本条就钉它。
/// **买不到**：第三方 crate 的传递依赖（那一层不是我们的业务）。
#[test]
fn c2_the_comms_crates_depend_only_on_the_allowed_crates() {
    let all = workspace_packages();
    let comms = comms_packages(&all);
    let mut offenders: Vec<String> = Vec::new();
    for pkg in &comms {
        assert!(
            !pkg.normal_deps.is_empty(),
            "`{}` 一条普通依赖都没读到 —— 取法坏了，下面那条在空转",
            pkg.name
        );
        for b in dependency_offences(pkg, &all) {
            offenders.push(format!("  {} → {b}", pkg.name));
        }
    }
    assert!(
        offenders.is_empty(),
        "通信层 crate 依赖了名单之外的东西：\n{}\n\n\
`C2`：通信层是纯基础设施，业务 crate 一个都不许依赖。\n\
         ⇒ 真需要那份数据，让**宿主交给它**（不读盘 · 不读环境是同一句话的另一面，由两包的名单挡）。",
        offenders.join("\n")
    );
    // 正控：给面 B 那个 crate 合成一条业务依赖（`creds-core`，它的目录就是盘上那一份）必须咬出来。
    let creds = all
        .iter()
        .find(|p| p.name == "creds-core")
        .expect("workspace 里有 `creds-core`");
    let outward = comms
        .iter()
        .find(|p| p.name == "comms-outward")
        .expect("有 `comms-outward`");
    let mut normal_deps = outward.normal_deps.clone();
    normal_deps.push(("creds-core".to_string(), Some(creds.dir.clone())));
    let planted = Package {
        name: outward.name.clone(),
        dir: outward.dir.clone(),
        roots: outward.roots.clone(),
        normal_deps,
    };
    assert!(
        !dependency_offences(&planted, &all).is_empty(),
        "合成一条 `creds-core` 依赖也咬不出来 —— 名单与识别器脱钩了"
    );
}

/// 一行里既有公开面的记号、又有 `transport` 这个词。
fn public_line_leaks_transport(line: &str) -> bool {
    let public = line.contains("pub ") || line.contains("export ");
    public && guard_core::contains_word(line, "transport")
}

/// ★ `C3` —— 前端发出的请求里不许含 `transport`。
///
/// 「`transport` 是通信层的**内部**选择，前端不知道」
/// ⇒ 这个词在层**内部**是合法的，只有跨出边界的那一面不许有它。
///
/// 本条按后缀分两档判：
/// - **`.rs` 成员**：只判**公开面那几行**（含 `pub ` 的行）。层内部的
///   `let transport = pick(origin);` 是对的，不该红。
/// - **`.ts` 成员**：整段生产段零 `transport` —— TS 那一侧**就是**前端的脸，
///   `§3.2` 逐字「前端不知道」。
///
/// **买不到**：① `.rs` 里 `pub struct` 的字段若不带 `pub`（同模块可见），本条看不见；
/// ② 换个名字（`via` / `channel_kind`）传同一件事，本条一个字都不说。
/// 这两格要的是类型层的真检查（`ts-rs` 导出面对拍），今天**判不了** ——
/// 缺的证据是通信层的第一版类型定义。
#[test]
fn c3_the_word_transport_never_crosses_the_boundary() {
    let pop = boundary();
    let mut offenders: Vec<String> = Vec::new();
    for m in &pop {
        if m.rel.ends_with(".ts") {
            if guard_core::contains_word(&m.prod, "transport") {
                offenders.push(format!("  {} —— TS 那一侧整段不许出现这个词", m.rel));
            }
            continue;
        }
        for (i, line) in m.prod.lines().enumerate() {
            if public_line_leaks_transport(line) {
                offenders.push(format!("  {}:{} —— `{}`", m.rel, i + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "`transport` 漏到了通信层的公开面上：\n{}\n\n\
**`transport` 是通信层的内部选择，前端不知道。**\n\
         前端只给 `origin`（`§3.1`：本机也带值，不是 `null`），选哪条路是本层的事。",
        offenders.join("\n")
    );
    assert!(
        public_line_leaks_transport("    pub transport: Transport,"),
        "公开面上的 `transport` 认不出来 —— 上面那条在空转"
    );
    assert!(
        public_line_leaks_transport("export type Req = { transport: string };"),
        "TS 导出面上的 `transport` 认不出来 —— 上面那条在空转"
    );
    assert!(
        !public_line_leaks_transport("    let transport = pick_transport(&origin);"),
        "层**内部**选传输被判成违例 —— `§3.2` 逐字说那是它的本职，假红比不查更坏"
    );
}

// ════════════════════════════════════════════════════════════════════════════
//  七、X1 · X4：开关与名单挡不住的那两形
// ════════════════════════════════════════════════════════════════════════════

/// 从 `at` 起第一对配平花括号里的内容。
///
/// ⚠ **粗尺子，如实登记**：它不解析字符串与字符字面量 ——
/// 一个写在字符串里的 `}` 会让它提前收尾。本仓的判据语料里满是合成源码串，
/// 精确解析一次失步就整段跟着错，而错的方向是**静默的绿**；
/// 宁可用一把粗而稳的尺子（`scanning_guard_registry::guard_fn_item` 的头注同此论证）。
fn braced_block(text: &str, at: usize) -> Option<&str> {
    let open = at + text[at..].find('{')?;
    let mut depth = 0usize;
    for (i, c) in text[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[open + 1..open + i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// 那三个线上类型 —— `X1` 的人群按它们认。
const WIRE_TYPES: &[&str] = &["CallError", "Item", "Reach"];

/// 生产段里「对那三个类型的 `match`」中带 `_ =>` 的那些（返回每处的片段）。
fn inexhaustive_wire_matches(prod: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mark = format!("{} ", "match");
    for (at, _) in prod.match_indices(mark.as_str()) {
        let Some(block) = braced_block(prod, at) else {
            continue;
        };
        if !WIRE_TYPES
            .iter()
            .any(|t| guard_core::contains_word(block, t))
        {
            continue;
        }
        let wildcard = format!("_ {}", "=>");
        if block.contains(wildcard.as_str()) {
            out.push(block.trim().chars().take(120).collect::<String>());
        }
    }
    out
}

/// ★ `X1` —— 对 `CallError` / `Item` / `Reach` 的 `match` 必须穷尽，零 `_ =>`。
///
/// 两包的 `wildcard_enum_match_arm = "deny"` 挡住了大半：通配臂盖住两个以上变体就编不过。
/// 它挡不住的那一形是**通配臂只盖住剩下的一个变体**（那归 clippy 的 match_wildcard_for_single_variants，两包没开）——
/// 本条只为这一形留着。什么时候删：两包 `Cargo.toml` 开了那一条 lint 那天。
///
/// **买不到**：① 类型对不对（本条按**名字**认，不做类型检查）；
/// ② `_ =>` 之外的兜底写法（`other =>`、`e if true =>`；盖两个以上变体的由开关挡）；
/// ③ 写在字符串里的花括号会让块提前收尾（见 [`braced_block`]）。
#[test]
fn x1_every_match_on_the_three_wire_types_is_exhaustive() {
    let pop = boundary();
    let offenders: Vec<String> = pop
        .iter()
        .flat_map(|m| {
            inexhaustive_wire_matches(&m.prod)
                .into_iter()
                .map(|blk| format!("  {} —— `{blk}`", m.rel))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "这几处对线上类型的 `match` 用了通配臂：\n{}\n\n\
`X1`：三个类型的每一处 `match` 必须**穷尽、零 `_ =>`**。\n\
         ⇒ 通配臂的代价是：错误面加一个变体时，**没有任何地方会红** —— \n\
         新错误被悄悄归进旧分支，而那正是这一层最不能出的事。",
        offenders.join("\n")
    );
    for t in WIRE_TYPES {
        let bad = format!("    match e {{ {t}::A => 1, _ => 0 }}\n");
        assert_eq!(
            inexhaustive_wire_matches(&bad).len(),
            1,
            "识别器认不出 `{t}` 上的通配臂 —— `X1` 此刻在空转"
        );
        let good = format!("    match e {{ {t}::A => 1, {t}::B => 0 }}\n");
        assert!(
            inexhaustive_wire_matches(&good).is_empty(),
            "穷尽的 `{t}` match 被判成违例 —— 假红比不查更坏"
        );
    }
    assert!(
        inexhaustive_wire_matches("    match kind { Foo::A => 1, _ => 0 }\n").is_empty(),
        "**别的**类型上的通配臂被算进了 `X1` 的人群 —— 人群要等于它真正证明的那件事"
    );
}

/// 静默丢弃里名单挡不住的那一形：`let _ = …send(…)`（`try_send` 在两包的名单里）。
fn silent_drop_sites(prod: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in prod.lines() {
        let t = line.trim();
        if t.starts_with("let _ =") && guard_core::contains_word(t, "send") {
            out.push(t.to_string());
        }
    }
    out
}

/// ★ `X4` —— 丢弃只能经 `Item::Gap` 表达。
///
/// **回推优先 · 推不动才丢 · 丢必须说**。
/// `let _ = …send(…)` 是「推不动就当没发生」—— 订阅方**看不出**中间少了东西，
/// 而流的语义整个塌在这一点上。`try_send` 那一形由两包的 `clippy.toml` 挡（发 `Gap` 那一处就地放行）；
/// 这一形名单挡不住：`send` 返回 `Result` 时 `let _ =` 不触发 rustc 的 `#[must_use]` 告警。
/// 什么时候删：两包 `Cargo.toml` 开了 clippy 的 let_underscore_must_use 那天。
///
/// **买不到**：① 别的丢弃写法（`if ch.capacity() == 0 { return; }` · `.ok()`）；
/// ② **`§9` 逐字**：SSH 那条路上回推到底推不推得回去**判不了** ——
///    缺的证据是一次真机的慢消费者实验。本条只买「丢的时候有没有说」。
#[test]
fn x4_the_only_way_to_drop_is_to_say_gap() {
    let pop = boundary();
    let offenders: Vec<String> = pop
        .iter()
        .flat_map(|m| {
            silent_drop_sites(&m.prod)
                .into_iter()
                .map(|s| format!("  {} —— `{s}`", m.rel))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "这几处在**静默地**丢东西：\n{}\n\n\
**回推优先 · 推不动才丢 · 丢必须说**。\n\
         ⇒ 丢了就发一个 `Item::Gap`，让订阅方知道中间缺了东西。",
        offenders.join("\n")
    );
    assert_eq!(
        silent_drop_sites("    let _ = ch.send(item);\n").len(),
        1,
        "识别器认不出被丢掉的发送结果 —— `X4` 此刻在空转"
    );
    assert!(
        silent_drop_sites("    ch.send(item).await?;\n    ch.send(Item::Gap(n)).await?;\n")
            .is_empty(),
        "一段**回推 ＋ 报 Gap** 的干净代码被判成静默丢弃 —— 假红比不查更坏"
    );
}

// ════════════════════════════════════════════════════════════════════════════
//  八、元判据：这几条真的在跑
// ════════════════════════════════════════════════════════════════════════════

/// 一份 Rust 源码里所有 `#[test] fn <名字>` 的名字。
fn test_fn_names(src: &str) -> BTreeSet<String> {
    let lines: Vec<&str> = src.lines().collect();
    let attr = format!("#[{}]", "test");
    let mut out = BTreeSet::new();
    for (i, l) in lines.iter().enumerate() {
        if l.trim() != attr {
            continue;
        }
        for next in lines.iter().skip(i + 1) {
            let t = next.trim();
            if t.is_empty() || t.starts_with("//") || t.starts_with('#') {
                continue;
            }
            if let Some(rest) = guard_core::strip_visibility(t).strip_prefix("fn ") {
                if let Some(name) = rest.split('(').next() {
                    out.insert(name.trim().to_string());
                }
            }
            break;
        }
    }
    out
}

/// ★★ **「判据不在执行链上就等于不存在」的机检形态**。
///
/// [`CRITERIA`] 那张表与本文件里**真实的** `#[test]` 两向集合相等，
/// 并且编号那一列恰好覆盖 `C1`–`C3` 与 `X1` · `X4`。
///
/// # 它治什么
///
/// 删掉一条判据而把表留着（读的人以为那条性质有人守着）· 加一条判据而不登记
/// （编号与盘上的东西对不上）· 把 `X4` 悄悄改名（闭集按名字认）。
///
/// # ⚠ 它**接不住**的那一格（如实登记，这是已知的洞）
///
/// **整个模块被从 `lib.rs` 摘掉**。那时本文件一条都不跑，本条也不跑 ——
/// 「摘掉了」与「全绿」在终端上一模一样。挡这一形要在 `tests/scripts/gate.sh` 上
/// 给这一族开一个**自己的格**（形状照 `f3-copy`：pin · declared · ran 三方相等）。
/// ⚠ **那一格今天已经在了** —— `tests/scripts/gate.sh` 的 `comm-boundary`，
/// 它的 `pin` 与本表条数是一条**恒等**腿（不是上限）⇒ 真加/删一条判据，`pin` 必须**同拍**抬。
#[test]
fn every_criterion_is_on_the_execution_chain() {
    let me = include_str!("comm_boundary_registry_tests.rs");
    let actual = test_fn_names(me);
    // 抽取器自检：本条自己必须被抠到（否则两个空集相等，恒绿）。
    assert!(
        actual.contains("every_criterion_is_on_the_execution_chain"),
        "没抠出本条自己的 `#[test]` —— 抽取器坏了，下面那条会拿两个空集比出绿：{actual:?}"
    );
    let declared: BTreeSet<String> = CRITERIA.iter().map(|(_, f, _)| (*f).to_string()).collect();
    let undeclared: Vec<&String> = actual.difference(&declared).collect();
    let phantom: Vec<&String> = declared.difference(&actual).collect();
    assert!(
        undeclared.is_empty(),
        "本文件里有 `#[test]` 没登记进 `CRITERIA`：{undeclared:?}\n\
         ⇒ 它守的是哪一条判据？写进表里，否则下一个人读表会以为它不存在。"
    );
    assert!(
        phantom.is_empty(),
        "`CRITERIA` 里登记着这几条，而本文件里**没有**对应的 `#[test]`：{phantom:?}\n\
         ⇒ 判据被删了 / 改名了 / 被 `#[ignore]` 挡住了。\n\
         🔴 表还在而判据没了 —— 读表的人会以为那条性质有人守着。那正是本仓治的那一族。"
    );
    assert_eq!(
        actual.len(),
        CRITERIA.len(),
        "两向都查过还对不上 —— 表里有重名（{} 条登记 vs {} 个去重后的名字）",
        CRITERIA.len(),
        declared.len()
    );
    let ids: BTreeSet<&str> = CRITERIA.iter().map(|(id, _, _)| *id).collect();
    let want_c: BTreeSet<&str> = ["C1", "C2", "C3"].into_iter().collect();
    let want_x: BTreeSet<&str> = ["X1", "X4"].into_iter().collect();
    let got_c: BTreeSet<&str> = ids.iter().filter(|i| i.starts_with('C')).copied().collect();
    let got_x: BTreeSet<&str> = ids.iter().filter(|i| i.starts_with('X')).copied().collect();
    assert_eq!(
        got_c, want_c,
        "扫文字的铁律恰好三条（C1–C3；读盘 · 起进程那两条由名单挡），盘上是 {got_c:?}"
    );
    assert_eq!(
        got_x, want_x,
        "扫文字的签名判据恰好两条（X1 · X4；别的由名单 · 开关 · `Budget` 私有挡），盘上是 {got_x:?}"
    );
}
