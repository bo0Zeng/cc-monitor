//! 通信层边界判据的本体 —— 模块头注（成员怎么认 · 十一条管什么 · 买到什么买不到什么）住
//! `src/frontend/shell/src/comm_boundary_registry.rs`，不在这里抄第二份。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
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

/// 通信层**对前端的入口符号** —— `(符号名, 说明)`。`C3` 与 `X6` 的人群从这儿派生。
///
/// 前端只有两个动作（`call` / `subscribe`）。
///
/// 🔴 〔2026-09-24，通道那一拍〕两个动作**在盘上有了**：
/// `src/comms/inward/chan/client.rs` 的 `Client` 实现了 `Comms`。
/// ⚠ **但本表仍然刻意为空**，理由是人群的形状对不上，不是入口不存在：
/// `X6` 的人群是「本表 × **TS** 前端语料」，而第一个用上它的外部前端是 **Rust**（egui 文件窗口，
/// 下一波 F2 才接）。把 `call` 填进来的话，`X6` 会去 TS 语料里数与本通道无关的 `call(`
/// （`fn.call(this, …)` 那一族），那是假红；而 Rust 那一侧今天**零个**真调用点。
/// ⇒ 这一格是一笔欠账：F2 落第一个真调用点时，`X6` 的人群要扩到 Rust 前端那一侧。
/// 住址在末尾「面 A 的第一个外部客户端：通道」的欠账那一节，不在这里抄第二份。
///
/// 🔴**那一天到了**：文件窗口（今住独立包 `src/frontend/filewin/`）是第一个真调用点
/// （`source::ask`）。表从两列扩成三列 —— 第二列是**这个入口的前端语料住在哪一种语言里**：
/// `call` / `subscribe` 这两个裸词在 TS 语料里另有与本通道无关的同名调用
/// （`launcher-diagnostics.ts` 的本地 `call(true)`、`session-accounts-poll.ts` 的 `subscribe(() => …)`），
/// 按语言分人群才不假红。Rust 那一侧的人群是 [`RUST_FRONTENDS`]。
const ENTRIES: &[(&str, &str, &str)] = &[
    (
        "chan.call",
        "ts",
        "**主界面**（webview）说 `call` 的入口（`src/comms/inward/chan.ts` 的 `chan.call`）。\
         入口名带着 `chan.` 前缀，是因为 TS 语料里另有与本通道无关的裸 `call(`（`fn.call(this, …)` 一族）——\
         按语言分人群之外再按全名收窄，才不假红。期限由调用方给（`Budget.within(…)`）。",
    ),
    (
        "call",
        "rs",
        "一次性请求（`Comms::call`）—— 期限由调用方给（`Budget`，绝对时刻）",
    ),
    (
        "chan.subscribe",
        "ts",
        "**主界面**（webview）说 `subscribe` 的入口（`src/comms/inward/chan.ts` 的 `chan.subscribe`，会话内容流）。\
         同 `chan.call` 那一行按全名收窄（TS 语料里另有与本通道无关的裸 `subscribe(`）。它**没有期限参数**\
         （订阅是长期意向）⇒ 只进调用点条数恒等，不进「显式给 `Budget`」那条。",
    ),
    (
        "subscribe",
        "rs",
        "订阅（`Comms::subscribe`）—— 窗口恰好一处（`filewin/source.rs::watch`，传输进度流\
         `transfer/<id>`，生产上第一条流）。⚠ 它**没有期限参数**（签名逐字：订阅是长期意向，\
         不被一次调用的期限拴住）⇒ 本表这一格不进「显式给 `Budget`」那条，只进调用点条数恒等",
    ),
];

/// `X6` 的 **Rust 前端语料**：`(目录前缀, 摘掉的文件, 为什么摘)`。
///
/// ⚠ 从前摘掉的那一份是 **monitor 那一侧**的入口（`entry.rs`）：它调的是通道宿主注入给路由器的
/// 那个句柄（`Backends::call`，入参是「这一跳还剩多少」的 `Duration`），不是前端的 `Comms::call`。
/// 窗口独立成包 `src/frontend/filewin/`：那一侧整个就是这一个前端；monitor 那一侧 `src/frontend/shell/src/filewin/`
/// （开窗入口 · 起进程 · `[[bin]]` 入口）留在 monitor 进程，同 `entry.rs` 那条理由不进前端语料。
const RUST_FRONTENDS: &[(&str, &[&str], &str)] = &[(
    "src/frontend/filewin/src/",
    &[],
    "文件窗口进程（又一个前端）：它经 `comms_inward` 的 `Client` 说 `Comms::call` / `subscribe`",
)];

/// 一份文件是不是某个入口的前端语料（按 [`ENTRIES`] 第二列的语言分）。
fn is_frontend_for(rel: &str, lang: &str, member_paths: &BTreeSet<&str>) -> bool {
    if member_paths.contains(rel) {
        return false;
    }
    match lang {
        // 只算**生产**前端：`tests/` 那棵树不是调用方，是判据（`chan.vitest.ts` 自己就说了六次
        //   `chan.call(`）。测试文件整棵住 `tests/`（仓库重组：src 与 tests 分离），`src/` 下没有 ——
        //   所以按第一段路径分就够。TS 人群第一次非空，这一格才第一次有牙。
        "ts" => rel.ends_with(".ts") && rel.split('/').next() == Some("src"),
        "rs" => {
            rel.ends_with(".rs")
                && RUST_FRONTENDS.iter().any(|(root, skip, _)| {
                    rel.len() > root.len() && &rel[..root.len()] == *root && !skip.contains(&rel)
                })
        }
        _ => false,
    }
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
/// 人群里至少有一份 `.ts`（`C3` / `X6` 的 TS 那一面）。
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

/// `X6` 的前端语料：`src/` 下的 `.rs` / `.ts`（第三方 `vendor/` 不算）。
fn frontend_corpus() -> Vec<(String, String)> {
    let root = repo_root();
    let mut out: Vec<(String, String)> =
        guard_core::scan_tree_excluding(&root.join("src"), &["rs", "ts"], &[])
            .into_iter()
            .map(|(p, text)| {
                let rel = p
                    .strip_prefix(&root)
                    .unwrap_or(&p)
                    .to_string_lossy()
                    .replace('\\', "/");
                (rel, text)
            })
            .filter(|(rel, _)| !rel.contains("/vendor/"))
            .collect();
    out.sort();
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
        "C4",
        "c4_nothing_inside_the_boundary_reads_disk_or_environment",
        "不许读盘、不许读环境变量 —— 凭据由后端交给它",
    ),
    (
        "C5",
        "c5_nothing_inside_the_boundary_spawns_a_process_or_binds_a_port",
        "不许起进程、不许绑端口 —— 它只用别人交给它的通道",
    ),
    (
        "X1",
        "x1_every_match_on_the_three_wire_types_is_exhaustive",
        "对 `CallError` / `Item` / `Reach` 的 `match` 穷尽、零 `_ =>`",
    ),
    (
        "X2",
        "x2_no_deadline_literal_lives_inside_the_boundary",
        "生产段零期限字面量（期限的值归后端）",
    ),
    (
        "X3",
        "x3_every_hop_construction_names_its_reach",
        "`Hop` 的每一个构造点都显式给 `reach`，无默认值",
    ),
    (
        "X4",
        "x4_the_only_way_to_drop_is_to_say_gap",
        "丢弃只能经 `Item::Gap` 表达 —— 零 `try_send`、零静默 drop",
    ),
    (
        "X5",
        "x5_every_budget_until_derivation_only_tightens",
        "`until` 的每一处派生都是 `min`",
    ),
    (
        "X6",
        "x6_every_frontend_call_site_passes_an_explicit_budget",
        "前端对入口的调用点一律显式给 `Budget`",
    ),
    (
        "面B剩下的",
        "the_relay_files_left_outside_are_blocked_by_exactly_the_criteria_the_prose_names",
        "中转的宿主（`relay/listen.rs`）为什么在外面：逐份**被哪几条咬**与散文两向相等",
    ),
    (
        "传输面三份",
        "the_transport_candidates_left_outside_are_blocked_by_exactly_the_criteria_the_prose_names",
        "面 A（传输面）进不来的那几份，逐份**被哪几条咬**与散文两向相等",
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
        files.len() > 100,
        "`src/` 下只走到 {} 份 `.rs` —— 取法坏了，下面的零命中在空转",
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
//  六、C1–C5：通信层的铁律
// ════════════════════════════════════════════════════════════════════════════

/// 业务词表 —— `C1` 的九个概念，**每个带它的复数形**
/// 〔用户 2026-09-21：「看怎么加」；〕。
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
/// ⚠ `tmuxes` / `claudes` / `mcps` 这三个复数形**本拍现打全仓零命中**（`src/` 233 份 `.rs`
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

/// ★★ **加复数买到了什么** —— `(标识符, 靠哪个复数形咬住, 它是什么)`〔本拍现打，〕。
///
/// 🔴 **这张表是「加复数前后」那个读数唯一活着的住址，而它刻意不记处数。**
/// 处数（本拍：整个传输面 365 ⇒ 375 处，新增 10 处）会随任何一次编辑腐掉，
/// 而**标识符不会** —— 表里每一行都被两条断言夹着：
/// ① 今天的词表靠那个复数形真的咬住它 · ② **只拿单数去比抓不到它**
/// （⇐ 这一条就是「加复数之前它完全看不见」，做成了可机检的形状）。
///
/// ⚠ `tmux_sessions` / `TmuxSessions` **刻意不在表里**：它们本来就被 `tmux` 咬住，
/// 复数只是多给了一个判词，不是新咬住。那十处里，
/// 这一族占 5 处 —— 把它们混进来就是把「多一个判词」读成「补了一个洞」。
///
/// ⚠ **假红那一侧本拍量过**：`src/` 233 份 `.rs` 的生产段上，加复数**新增的命中一处假红都没有**
/// —— 全是 `accounts` / `sessions` / `agents` / `skills` / `sids` / `jsonls` 那几族真业务。
/// 最像假红的候选是 OpenSSH 的 `MaxSessions`（传输概念，不是 Claude 会话），
/// 而它**全部住在文档注释里** ⇒ 本条只看生产段，一处都不碰。
const PLURALS_NEWLY_CAUGHT: &[(&str, &str, &str)] = &[
    (
        "sessions",
        "sessions",
        "那条 `/sessions/` 路径串 —— **那道 Claude 数据围栏自己的一半**",
    ),
    (
        "load_show_bg_sessions",
        "sessions",
        "读「显示后台会话」那个业务设置",
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
/// # 为什么这一刀不住 `guard_core`
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
/// 🔴 **这一刀是新射程的全部机关，而它是纯语法的**：它只问「这个标识符出现在
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
/// # ⚠ 它认不出的那几格（如实登记，本件已知的洞）
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
/// 用户 2026-09-21 逐字：「**理论上可以懂业务, 不然他怎么把流量分流成我们想要的样子**」。
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
/// 挡它的不是本条，是 `X1` 那一族（线上类型必须穷尽）与金标准逐字节对拍。
///
/// # 匹配单位仍是**标识符子词**，词表本拍加了复数〔`§8.1.5`〕
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
///    **数字边界不在裁定里，本件没有擅自加**。
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
                     用户 2026-09-21 的裁决逐字把它放过了（射程只覆盖公开面）。\n\
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
         ⇒ 真需要那份数据，让**宿主交给它**（`C4` 是同一句话的另一面）。",
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
    // `X6` 的人群也从入口表来，这里顺手钉住「入口表与它的读者同源」那一格不成立：
    assert_eq!(
        ENTRIES.len(),
        ENTRIES
            .iter()
            .map(|(n, _, _)| *n)
            .collect::<BTreeSet<_>>()
            .len(),
        "入口表里有重名 —— 人群会被数两遍"
    );
}

/// `C4` 的形状表 —— `(串, 出处)`。
///
/// 前三条是点名的（`read_to_string` / `File::open` / `env::var`）；
/// 后三条是同族的别名，本件补的 —— 补的理由：只挡三种写法等于给第四种写法留门，
/// 而「换个写法就过」在本仓有记录（`readonly_guard` 的前身栽过）。
/// 🔴 **串一律运行时拼**：写成字面量的话，本文件自己就是一处「读盘点」，
/// 而 `local_read_surface_registry` / `write_site_registry` 那几张表会把它数进去。
fn disk_and_env_needles() -> Vec<(&'static str, String, &'static str)> {
    vec![
        ("读文本", format!("read_to_{}(", "string"), "§2 C4 逐字"),
        ("开文件", format!("File::{}(", "open"), "§2 C4 逐字"),
        ("读环境", format!("env::{}(", "var"), "§2 C4 逐字"),
        (
            "读环境OS",
            format!("env::{}_os(", "var"),
            "同族别名（本件补）",
        ),
        (
            "以选项开",
            format!("OpenOptions::{}", "new"),
            "同族别名（本件补）",
        ),
        ("读字节", format!("fs::{}(", "read"), "同族别名（本件补）"),
    ]
}

/// 一份文本里**咬得上的那几个 `C4` 判词**，按**标签**返回。
///
/// 🔴 **为什么登记表用标签而不用那个串本身**：串写成字面量的话，本文件自己就成了一处
/// 「读盘点」，`local_read_surface_registry` / `write_site_registry` 那几张表会把它数进去
/// —— 那正是 [`disk_and_env_needles`] 头注里「串一律运行时拼」在治的事。
/// 标签是中文短名 ⇒ 任何按代码形状扫的判据都不可能命中它。
/// ⚠ 标签拼错 ⇒ 那一行从此**恒不命中**；挡这一形的是 [`assert_left_outside`] 里那条
/// 「表里每个标签都必须是真判词」的自检。
fn disk_and_env_tags_in(prod: &str) -> BTreeSet<&'static str> {
    disk_and_env_needles()
        .into_iter()
        .filter(|(_, n, _)| prod.contains(n.as_str()))
        .map(|(tag, _, _)| tag)
        .collect()
}

/// ★ `C4` —— 不许读盘、不许读环境变量。
///
/// 这是用户那句「**key 什么的这些应该要归后端管，通信只负责流量**」
/// 的操作化 —— 凭据不是"它去拿"，是"后端给它"。
///
/// **买不到**：① `include_str!` 那种**编译期**读盘；② 经由别的 crate 间接读盘；
/// ③ 「它拿到的那张表对不对」。本条只买「这一层自己不伸手」。
///
/// # 🔴 ④〔步 4 剩余那一路现打，2026-09-22〕**从一条流里读也会被咬** —— 一个已登记的假阳类
///
/// `read_to_string(` 是个**形状**，它认不出左边那个接收者是盘还是一条已经拿到手的流。
/// 活样本：`pubkey.rs` 整份文件**只被本条咬住**，而它那个判词下面的两处命中里
/// **只有一处是读盘**（本机 `.pub`）；另一处是 `reader.read_to_string(&mut out)`，
/// 读的是 `connect_and_exec_cmd` 交回来的那条 SSH 流 —— 也就是
/// 「**只使用别人交给它的通道**」正要它干的那件事。
/// ⇒ **「本条咬了几处」不等于「这一层伸手拿了几次东西」**，报数时别把两者读成一个。
///
/// ⚠ **今天刻意不收窄**：那三条判词是**逐字**点名的，收窄成
/// `fs::read_to_string(` 会漏掉 `std::fs` 之外的写法。要动它是改 `§2` 那张表，
/// 不是在这里放宽。下面有一条**钉住这个假阳类**的断言 —— 谁把形状表收窄了那一条当场红，
/// 逼他回来改这一段话（而不是让这段话安静地变成假的）。
///
/// # 🔴 ⑤〔同拍现打〕**「传输载荷」与「凭据/配置」在这张形状表上分不开**
///
/// `C4` 的句子逐字点的是「**凭据、配置、路由表、期限值**」，而 `sftp_pool.rs` 那两个判词
/// 打开的是**用户亲自按下的那一次传输的本地那一头**（上传源 · 下载的 `.part`）——
/// 那是**载荷**，不是句子里那四样。**本条分不出来。**
/// ⚠ 这一格**今天判不了**，缺的证据是一道裁定：「本层该不该连**本地文件句柄**也由调用方交给它」
/// —— `C5` 那句「只使用别人交给它的通道」照字面是**该**，而 `§2` 没写到文件这一头。
/// ⇒ 不许拿本条的处数当那道题的答案。
#[test]
fn c4_nothing_inside_the_boundary_reads_disk_or_environment() {
    let pop = boundary();
    let needles = disk_and_env_needles();
    let offenders: Vec<String> = pop
        .iter()
        .flat_map(|m| {
            needles
                .iter()
                .filter(|(_, n, _)| m.prod.contains(n.as_str()))
                .map(|(tag, n, why)| format!("  {} —— `{n}`〔{tag}〕（{why}）", m.rel))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "通信层成员自己去读盘 / 读环境变量了：\n{}\n\n\
把用户那句话操作化成这一条：\n\
         「key 什么的这些应该要归后端管，**通信只负责流量**」\n\
         ⇒ 凭据、配置、账号映射全部由后端**交给它**（步 4：`creds.rs` ＋ `table.rs` 搬去后端）。",
        offenders.join("\n")
    );
    for (tag, n, why) in &needles {
        let synthetic = format!("pub fn boot() {{ let _ = {n}\"x\"); }}\n");
        assert!(
            disk_and_env_tags_in(&synthetic).contains(tag),
            "形状表里写着 `{n}`〔{tag}〕（{why}），识别器却认不出自己造的那处 —— 表与识别器脱钩了"
        );
    }
    assert!(
        disk_and_env_tags_in("pub fn feed(table: Table) { use_it(table) }\n").is_empty(),
        "一段**由后端喂进来**的干净代码被判成读盘 —— 假红比不查更坏"
    );
    // 标签面的自检：六个判词六个**互不相同**的标签（撞一个 ⇒ 登记表那一侧会少一格而不出声）。
    assert_eq!(
        needles.len(),
        needles
            .iter()
            .map(|(tag, _, _)| *tag)
            .collect::<BTreeSet<_>>()
            .len(),
        "判词的标签有重名 —— 两个判词会在登记表那一侧被数成一个"
    );
    // ★ 钉住头注 ④ 那个**已登记的假阳类**：从一条**已经交到手里**的流里读，形状表照咬。
    //   这一条**不是**在说那样很好 —— 它是在说「这段话与识别器今天是一致的」。
    //   谁把形状表收窄（那样这一条当场红），处置是回去改头注 ④，不是把本条删掉。
    let from_a_stream = "pub async fn drain(io: &mut R) { let mut s = String::new(); \
                         io.read_to_string(&mut s).await.ok(); }\n";
    assert!(
        !disk_and_env_tags_in(from_a_stream).is_empty(),
        "头注 ④ 说「从一条流里读也会被咬」，而形状表今天认不出这一形 —— \
         那句话已经假了（或者有人把判词收窄了而没回来改它）"
    );
}

/// `C5` 的形状表 —— 前两条是点名的，其余是同族别名。
fn spawn_and_bind_needles() -> Vec<(String, &'static str)> {
    vec![
        (format!("Command::{}(", "new"), "§2 C5 逐字"),
        (format!("TcpListener::{}(", "bind"), "§2 C5 逐字"),
        (format!("UnixListener::{}(", "bind"), "同族别名（本件补）"),
        (format!("UdpSocket::{}(", "bind"), "同族别名（本件补）"),
    ]
}

/// ★ `C5` —— 不许起进程、不许绑端口。
///
/// 用户那句「**我不希望一个 app 占用三个端口、三个进程**」的操作化 ——
/// 谁起进程、谁绑端口是**后端生命周期**的事，通信层无权。
/// 它只使用别人交给它的通道（一个 `AsyncRead + AsyncWrite`）。
///
/// **买不到**：① 经由别的 crate 间接起进程 / 绑端口；② **发起连接**（`connect`）——
/// 那是本层的本职，刻意不在名单里；③ 「交给它的那条通道是谁开的」。
#[test]
fn c5_nothing_inside_the_boundary_spawns_a_process_or_binds_a_port() {
    let pop = boundary();
    let needles = spawn_and_bind_needles();
    let offenders: Vec<String> = pop
        .iter()
        .flat_map(|m| {
            needles
                .iter()
                .filter(|(n, _)| m.prod.contains(n.as_str()))
                .map(|(n, why)| format!("  {} —— `{n}`（{why}）", m.rel))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "通信层成员自己起进程 / 绑端口了：\n{}\n\n\
「我不希望一个 app 占用三个端口、三个进程」\n\
         ⇒ 生命周期归后端，本层只**使用**别人交给它的那条通道。",
        offenders.join("\n")
    );
    for (n, why) in &needles {
        let synthetic = format!("pub fn boot() {{ let _ = {n}\"x\"); }}\n");
        assert!(
            needles.iter().any(|(m, _)| synthetic.contains(m.as_str())),
            "形状表里写着 `{n}`（{why}），识别器却认不出自己造的那处 —— 表与识别器脱钩了"
        );
    }
    assert!(
        !needles.iter().any(
            |(n, _)| "pub async fn run(io: impl AsyncRead + AsyncWrite) {}\n".contains(n.as_str())
        ),
        "一段**只用交给它的通道**的干净代码被判成起进程 —— 假红比不查更坏"
    );
}

// ════════════════════════════════════════════════════════════════════════════
//  七、X1–X6：签名判据（与 C1–C5 共用上面那张表）
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
/// **买不到**：① 类型对不对（本条按**名字**认，不做类型检查）；
/// ② `_ =>` 之外的兜底写法（`other =>`、`e if true =>`）；
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

/// 生产段里的期限字面量。
fn deadline_literals(prod: &str) -> Vec<String> {
    let mut out = Vec::new();
    let dur = concat!("Duration", "::from_");
    for (at, _) in prod.match_indices(dur) {
        out.push(prod[at..].chars().take(40).collect::<String>());
    }
    // 裸秒常量：`const 名字里带 SEC/MS/TIMEOUT/DEADLINE 的 = 数字`。
    for line in prod.lines() {
        let t = guard_core::strip_visibility(line.trim_start());
        if !t.starts_with("const ") {
            continue;
        }
        let deadlineish = ["SEC", "MS", "TIMEOUT", "DEADLINE", "INTERVAL"]
            .iter()
            .any(|k| t.contains(k));
        if deadlineish && t.chars().any(|c| c.is_ascii_digit()) {
            out.push(t.trim().to_string());
        }
    }
    out
}

/// ★ `X2` —— 通信层生产段**零期限字面量**。
///
/// **值归后端 · 执行归通信层 · 说法归调用方**。
/// 期限的**值**一个字都不许写在这一层里 —— 它是后端交下来的。
///
/// **买不到**：① 从别处 `use` 进来的常量（本条只看这一层自己的文本）；
/// ② 名字不带那五个关键词的裸秒常量（`const GRACE: u64 = 30;`）；
/// ③ **`§9` 逐字**：具体该给多少秒**判不了** —— 缺的证据是各跳端到端时延的 p50/p99。
///    本条守的是「值不住在这里」，不是「值定对了」。
#[test]
fn x2_no_deadline_literal_lives_inside_the_boundary() {
    let pop = boundary();
    let offenders: Vec<String> = pop
        .iter()
        .flat_map(|m| {
            deadline_literals(&m.prod)
                .into_iter()
                .map(|s| format!("  {} —— `{s}`", m.rel))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "通信层生产段里出现了期限字面量：\n{}\n\n\
**值归后端**。期限从 `Budget` 里进来，不在这一层里写死。",
        offenders.join("\n")
    );
    assert_eq!(
        deadline_literals(&format!(
            "let d = {}(30);\n",
            concat!("Duration", "::from_secs")
        ))
        .len(),
        1,
        "识别器认不出期限字面量 —— `X2` 此刻在空转"
    );
    assert_eq!(
        deadline_literals("const CALL_TIMEOUT_SECS: u64 = 30;\n").len(),
        1,
        "识别器认不出裸秒常量 —— `X2` 此刻在空转"
    );
    assert!(
        deadline_literals("pub fn call(budget: Budget) -> Reach { budget.until }\n").is_empty(),
        "一段**只收 `Budget`** 的干净代码被判成写死期限 —— 假红比不查更坏"
    );
}

/// `Hop` 的构造点里**没给 `reach`** 的那些。
fn hop_sites_without_reach(prod: &str) -> Vec<String> {
    let mut out = Vec::new();
    let hop = format!("{} {{", "Hop");
    for (at, _) in prod.match_indices(hop.as_str()) {
        let Some(block) = braced_block(prod, at) else {
            continue;
        };
        let defaulted = block.contains(concat!("..Default::", "default()"));
        if !guard_core::contains_word(block, "reach") || defaulted {
            out.push(block.trim().chars().take(120).collect::<String>());
        }
    }
    out
}

/// ★ `X3` —— `CallError::Hop` 的每一个构造点都**显式给 `reach`**。
///
/// 错误分三层（传输错 · 对端错 · 我们自己错），
/// 而 `reach`（这一跳到底走到哪儿了）是调用方唯一能据以决定"要不要重试"的东西。
/// 给它默认值 = 把"不知道"伪装成"知道"。
///
/// **买不到**：① 构造点写成 `Hop::new(...)` 那种函数形（本条只认结构体字面量）；
/// ② `reach` 填得**对不对** —— `§3.3.6` 逐字：那一格「只能靠真机实验」。
#[test]
fn x3_every_hop_construction_names_its_reach() {
    let pop = boundary();
    let offenders: Vec<String> = pop
        .iter()
        .flat_map(|m| {
            hop_sites_without_reach(&m.prod)
                .into_iter()
                .map(|s| format!("  {} —— `{s}`", m.rel))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "这几处 `Hop` 构造点没有显式给 `reach`：\n{}\n\n\
`X3`：**无默认值、无 `..Default::default()`**。\n\
         ⇒ `reach` 是调用方判断「能不能重试」的唯一依据；给它默认值 =\n\
         把「不知道走到哪儿了」伪装成「知道」。",
        offenders.join("\n")
    );
    assert_eq!(
        hop_sites_without_reach("let e = Hop { code: 1 };\n").len(),
        1,
        "识别器认不出缺 `reach` 的构造点 —— `X3` 此刻在空转"
    );
    assert_eq!(
        hop_sites_without_reach(&format!(
            "let e = Hop {{ reach: r, {} }};\n",
            concat!("..Default::", "default()")
        ))
        .len(),
        1,
        "带默认填充的构造点没被逮住 —— 那正是 `X3` 点名禁的写法"
    );
    assert!(
        hop_sites_without_reach("let e = Hop { reach: Reach::Sent, code: 1 };\n").is_empty(),
        "显式给了 `reach` 的构造点被判成违例 —— 假红比不查更坏"
    );
}

/// 静默丢弃的两种形状。
fn silent_drop_sites(prod: &str) -> Vec<String> {
    let mut out = Vec::new();
    let try_send = concat!("try_", "send");
    for (at, _) in prod.match_indices(try_send) {
        out.push(prod[at..].chars().take(60).collect::<String>());
    }
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
/// `try_send` 与 `let _ = …send(…)` 都是「推不动就当没发生」——
/// 订阅方**看不出**中间少了东西，而流的语义整个塌在这一点上。
///
/// **买不到**：① 别的丢弃写法（`if ch.capacity() == 0 { return; }`）；
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
        silent_drop_sites(&format!("ch.{}(item);\n", concat!("try_", "send"))).len(),
        1,
        "识别器认不出那种「推不动就算了」的发法 —— `X4` 此刻在空转"
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

/// `until` 的派生点里**不是只收紧**的那些。
fn loose_until_derivations(prod: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in prod.lines() {
        let t = line.trim();
        if !guard_core::contains_word(t, "until") {
            continue;
        }
        let Some(eq) = t.find('=') else { continue };
        if t[eq..].starts_with("==") {
            continue;
        }
        let rhs = &t[eq + 1..];
        let tightening = rhs.contains(".min(") || rhs.contains("min(");
        let loosening = rhs.contains('+')
            || rhs.contains(".max(")
            || rhs.contains("max(")
            || rhs.contains("now(");
        if loosening || !tightening {
            out.push(t.to_string());
        }
    }
    out
}

/// ★ `X5` —— `Budget.until` 的每一处派生都是 `min`。
///
/// 期限沿着调用链**只许越来越紧**。
/// 一处 `+` 或一次重新 `now() + …`，就把上游给的那个期限放宽了 ——
/// 而放宽之后没有任何人会发现：请求只是"慢了一点"。
///
/// **买不到**：① 跨行的派生（本条按行判）；② `min` 之外语义等价的收紧写法
/// （`if a < b { a } else { b }`）会被判成违例 —— 那是**假红**，
/// 而假红在本仓的账上比漏判更危险（它会训练人绕过判据）。
///    ⇒ 真撞上了，**改写代码去用 `min`**，别来放宽本条。
#[test]
fn x5_every_budget_until_derivation_only_tightens() {
    let pop = boundary();
    let offenders: Vec<String> = pop
        .iter()
        .flat_map(|m| {
            loose_until_derivations(&m.prod)
                .into_iter()
                .map(|s| format!("  {} —— `{s}`", m.rel))
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "`until` 的这几处派生不是「只收紧」：\n{}\n\n\
`X5`：**每一处派生都是 `min` —— 零 `+` / `max` / 重新 `now() + …`**。\n\
         ⇒ 放宽上游给的期限之后，症状只是「慢了一点」，没有任何人会发现。",
        offenders.join("\n")
    );
    assert_eq!(
        loose_until_derivations("let until = now() + step;\n").len(),
        1,
        "识别器认不出重新起算的期限 —— `X5` 此刻在空转"
    );
    assert_eq!(
        loose_until_derivations("let until = parent.until.max(mine);\n").len(),
        1,
        "识别器认不出被放宽的期限 —— `X5` 此刻在空转"
    );
    assert!(
        loose_until_derivations("let until = parent.until.min(mine);\n").is_empty(),
        "一处**收紧**的派生被判成违例 —— 假红比不查更坏"
    );
}

/// 前端对某个入口的调用点里，**没显式给 `Budget`** 的那些。
fn call_sites_without_budget(text: &str, entry: &str) -> Vec<String> {
    let mut out = Vec::new();
    let head = format!("{entry}(");
    for (at, _) in text.match_indices(head.as_str()) {
        // 边界：`recall(` 不算 `call(`。
        let before_is_ident = text[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_');
        if before_is_ident {
            continue;
        }
        let tail = &text[at + head.len()..];
        let args: String = tail
            .chars()
            .take_while(|c| *c != ')' && *c != ';')
            .collect();
        if !(args.contains("Budget") || guard_core::contains_word(&args, "budget")) {
            out.push(format!("{head}{args})"));
        }
    }
    out
}

/// ★ `X6` —— 前端侧的调用点**一律显式给 `Budget`**，零处"用库里的默认"。
///
/// **说法归调用方**。一个藏在库里的默认期限意味着
/// 「这条路该等多久」没有任何调用方想过，而 `§9` 逐字记着：
/// 那 8 条无期限路径今天**连实测分布都没有**。
///
/// 人群 = [`ENTRIES`] × 前端语料。两张表今天都空 ⇒ 0 个调用点。
///
/// **买不到**：① 跨行的实参表（本条只取到第一个 `)` 或 `;`）；
/// ② 把 `Budget` 藏进一个变量再传（`call(o, op, p, b)`）——
///    那一格要类型检查，今天**判不了**（缺的证据：通信层的第一版类型定义）。
#[test]
fn x6_every_frontend_call_site_passes_an_explicit_budget() {
    let pop = boundary();
    let member_paths: BTreeSet<&str> = pop.iter().map(|m| m.rel.as_str()).collect();
    let all = frontend_corpus();
    // 抽取器自检：两种语言的前端语料都真的在（人群为空不等于语料为空）。
    assert!(
        all.iter().any(|(rel, _)| rel == "src/frontend/ui/tabs.ts"
            && is_frontend_for(rel, "ts", &member_paths)),
        "前端语料里找不到 `src/frontend/ui/tabs.ts` —— 语料面坏了，本条此刻在空转"
    );
    assert!(
        all.iter()
            .any(|(rel, _)| rel == "src/frontend/filewin/src/source.rs"
                && is_frontend_for(rel, "rs", &member_paths)),
        "Rust 前端语料里找不到 `filewin/source.rs`（窗口进程那一处 `call`）—— 语料面坏了"
    );
    let mut offenders: Vec<String> = Vec::new();
    let mut sites = 0usize;
    // 按入口分开数：`subscribe` 进来了（窗口里第一处），而它**没有期限参数**
    //   （签名逐字）⇒ 「显式给 `Budget`」只对带期限的入口判；条数两个入口各自恒等。
    let mut per_entry: std::collections::BTreeMap<&str, usize> = Default::default();
    // `chan.call`（主界面那一侧）同样带期限 —— 死值验现打：只写 `call` 的话，
    //   把主界面某处调用的期限换成一个不叫 budget 的东西，本条**照绿**（入口按全名分，`chan.call` 不在这张表里就不判）。
    const HAS_DEADLINE: &[&str] = &["call", "chan.call"];
    for (entry, lang, _) in ENTRIES {
        for (rel, text) in all
            .iter()
            .filter(|(rel, _)| is_frontend_for(rel, lang, &member_paths))
        {
            let prod = production_of(rel, text);
            let head = format!("{entry}(");
            let n = prod.matches(head.as_str()).count();
            sites += n;
            *per_entry.entry(entry).or_default() += n;
            if !HAS_DEADLINE.contains(entry) {
                continue;
            }
            for s in call_sites_without_budget(&prod, entry) {
                offenders.push(format!("  {rel} —— `{s}`"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "前端这几处调用通信层入口时没有显式给 `Budget`（本趟共扫到 {sites} 个调用点）：\n{}\n\n\
**说法归调用方**。库里的默认期限 =\n\
         「这条路该等多久」没有任何调用方想过（`§9`：那 8 条无期限路径连实测分布都没有）。",
        offenders.join("\n")
    );
    // 🔴调用点**条数恒等**（不是地板）：`call` 恰好 1 处（`filewin/source.rs::ask`）；
    // `subscribe` 恰好 1 处（`filewin/source.rs::watch`）。
    //    变多 ＝ 窗口里长出了第二处说它的地方（期限 / 撤的住址跟着分家）；
    //    变少 ＝ 那一处没了 —— 上面那条零违例会在零个调用点上**恒绿**。
    // `chan.call`（TS，主界面）恰好 2 处：`account-reads.ts::fetchSessionAccounts`（`accounts-sessions`）·
    //    `views/history-search.ts` 逐台那一问（`history-search`）。**X6 的 TS 人群第一次非空。**
    // 2 → 5：`session-reads.ts` 的三问（`history-index` / `history-user-inputs` / `history-find`，
    //    会话读面那三条从 monitor 的 Tauri 命令改走通道；每处显式给期限）。5 → 6：`settings/plugins-section.ts::fetchSurvey`
    //    （`plugins-marketplaces`）。
    // 6 → 8：`account-reads.ts::fetchAccounts`（`accounts-list`）· `account-reads.ts::checkTrust`（`accounts-trust`）——
    //    账号清单与信任预检从 monitor 的三条 Tauri 命令改走通道；每处显式给期限。
    //    8 → 9：`session-reads.ts::probeSessionRecord`（`history-record`，resume 之前问记录还在不在）。
    //    9 → 11：`settings/backend-section.ts::askExitPolicy` / `putExitPolicy`（「退出行为」问 / 交写）。
    // 11 → 12：`settings/assets-section.ts` 问那台的资产目录（`assets-catalog`，显式给期限）。
    // 12 → 18：历史清单与注解从 monitor 的 Tauri 命令改走通道（`history-reads.ts`），
    //    一律问本机常驻后端（远端那台由它去问）；每处显式给期限。
    // 18 → 20：`settings/assets-section.ts` 问那台记着的「从别处装来的 skill」（`skill-installs`）·
    //    点「卸」之后问那台的卸判定（`skill-uninstall-plan`）；两处都显式给期限。
    // 18 → 19：`tmux-control.ts::capturePane`（`capture-pane`，预览窗抓一屏从 monitor 的 Tauri 命令改走通道；
    //    显式给期限）。
    // 19 → 22：`tmux-control.ts` 的 `killSession`（`kill`）· `sendKeys` · `sendInto`（都是 `launch`）——
    //    杀会话 / 送键 / 就地 resume 三条 Tauri 命令改走通道；每处显式给期限（操作名留在调用点写字面量，见 `settle` 头注）。
    // 22 → 27：`cc-bus-control.ts` 五处（`bus-list` 查在线 · `bus-send` · `bus-kill` · `bus-spawn` · `bus-broadcast`）——
    //    cc-bus 驾驶舱的写面从 monitor 的五条 Tauri 命令改走通道；每处显式给期限。
    // 〔合并 SU1 ＋ C4e〕基数 18 ＋ SU1 增量 2 ＋ C4e 增量 9 = 29（两路各自从 18 起算；上面两段各写各的增量）。
    // 主线 29 ＋ 2：`apikey-reads.ts::readApikeyStatus`（`apikey-read`）· `fetchApikeyRouting`（`apikey-routing`）——
    //    API key 那两问从 monitor 的两条 Tauri 命令改走通道；每处显式给期限。
    // 31 → 32：`account-reads.ts::fetchSessionAccountsOrNull`（`accounts-sessions`，机器页「停」本机后端之前
    //    现问一次走中转的活会话；不走缓存、问不到回 `null`）；显式给期限。
    // 主线 31 ＋ 1：`tasks-panel.ts::fetchSessionTasks`（`tasks-list`）——
    //    任务快照从 monitor 的 Tauri 命令改走通道（C4e 批 4）；显式给期限。
    // 两路各自 31 ＋ 1 ⇒ 31 ＋ 2 = 33。
    // 主线 33 ＋ 1 ⇒ 34：`apikey-reads.ts::writeApikeyKey`（`apikey-key-set`，写 key 从 monitor 那条 Tauri 命令改走通道）；显式给期限。
    // 主线 35 ＋ 1 ⇒ 36：`session-reads.ts::readSessionFacts`（`history-facts`，会话事实出成品 ——
    //    此前是前端 `onLine` 旁路自己攒的，不是替掉一条 Tauri 命令）；显式给期限（`READ_BUDGET_MS`）。
    // `chan.subscribe`（TS，主界面）恰好 1 处：`events.ts::bindEvents` 按 `streams` 订会话内容流
    //    （主窗口每台机器一条、独立窗口一条，都经这一处）。
    // `session-tap` 与会话行走同一处（`plan` 里多一种流），仍是 1。
    // `accounts-changed`（替掉裸事件 `remote-backend-ready`；「前端只有两个动作」）同样经这一处
    //    （合并 TAP 时从单独一处 `watchAccountsChanged` 收回 `bindEvents` 的 `plan`，照 TAP 那一形）⇒ 仍是 1。
    // ＋1（合并主线 3c815828 之后 34 → 35；那一拍两边各自写成 34、git 当同一行合了，现打 35）：`settings/machine-aliases.ts::previewAlias` 一处（别名预览 `ccm-print`，
    //    问本机常驻后端「这条别名实际会执行什么」）；显式给期限（`PREVIEW_BUDGET_MS`）。
    // 36 → 37：`settings/acct-deploy.ts::askAcctIsoCmd` 一处（cc-acct-iso 步骤那一行问那台后端 `acct-iso-cmd`；
    //    新建表单预览 · 启用向导预览 · 弹终端三个用处都经这一处）；显式给期限（`CMD_BUDGET_MS`）。
    // 37 → 39：`cc-bus-control.ts::readState` / `readInbox`（`bus-state` / `bus-inbox`，驾驶舱读面从 monitor 那两条 Tauri 命令改走通道）；显式给期限（`READ_BUDGET_MS`）。
    // 39 → 40：`settings/backend-section.ts::askBackendLog`（`backend-log`，那台后端的诊断文件尾部）；显式给期限。
    // 基数 40 → 增量 +1 ⇒ 41：`resync.ts::resync`（`resync`，机器一行「重新对齐」与关卡 2「对齐后重试」共用这一处）；显式给期限（`RESYNC_BUDGET_MS`）。
    // 基数 41 → 增量 +6 ⇒ 47：`mcp-reads.ts` 三处（`mcp-read` · `mcp-server-put` / `-remove`）＋ `mcp-sync-reads.ts` 三处（`mcp-sync-source` / `-preview` / `-apply`），
    //    MCP 读写与推拉从 monitor 那八条 Tauri 命令改走通道；显式给期限（`MCP_BUDGET_MS` / `SYNC_BUDGET_MS`）。
    // 基数 47 → 增量 +5 ⇒ 52：`skill-install-reads.ts` 四处（`skill-read` · `skill-install-plan` · `-apply` · `skill-uninstall-apply`）
    //    ＋ `assets-sync-reads.ts` 一处（`assets-sync`）；显式给期限（`SKILL_BUDGET_MS` / `SYNC_BUDGET_MS`）。
    // 基数 52 ＋ MIG-3a +11 ＋ MIG-2 +4 ⇒ 67。
    // 基数 61 → 增量 +2 ⇒ 63：`cc-bus-install-reads.ts` 两处（`cc-bus-install` / `-state`）。
    // 基数 63 → 增量 −2 ⇒ 61：MCP 推拉 3 → 2、skill 装 3 → 2（经前端中继那一形改成只问本机枢纽一次）。
    // 基数 57 → 增量 +6 ⇒ 63：`alias-reads.ts` 六处（`aliases-*`）；显式给期限（`ALIAS_BUDGET_MS`）。
    // 基数 54 → 增量 +3 ⇒ 57：`skill-inbox-reads.ts` 三处（`skill-host-list` / `-read` / `-write`）；显式给期限（`INBOX_BUDGET_MS`）。
    // 基数 52 → 增量 +2 ⇒ 54：`acct-iso-reads.ts` 两处（`acct-iso-status` · `acct-iso-shellinit`）；显式给期限（`ACCT_ISO_BUDGET_MS`）。
    // 〔09-28 裁 2〕基数 67 → 增量 +1 ⇒ 68：`acct-iso-reads.ts` 一处（`acct-iso-install`）；显式给期限（`ACCT_ISO_BUDGET_MS`）。
    // 基数 41 → 增量 +3 ⇒ 44：`ssh-config-reads.ts` 三处（`ssh-config-aliases` · `-resolve` · `-import`，`~/.ssh/config` 导入从 monitor 三条 Tauri 命令改问本机常驻后端）；各自显式给期限。
    // 基数 41 ＋ MIG-3a 11 ＋ MIG-1 3 ⇒ 55。
    // 基数 52 → 增量 +4 ⇒ 56：`launch-render.ts` 四处（`launch-render-cli` · `launch-render-payload` · `launch-endpoint` · `launch-local`），
    //    起会话的渲染 / 中转地址 / 本机计划从 monitor 那几条 Tauri 命令改走通道；显式给期限（`budgetWithin(...)`）。
    // 主线 56 ＋ MIG-1 本路 6（ssh 配置三问 ＋ 端口转发三问）⇒ 62。
    assert_eq!(
        per_entry,
        [
            ("call", 1usize),
            ("chan.call", 101usize), // +1：`account-ops.ts::accountsMcpSync`（`accounts-mcp-sync`，共用 MCP 停 / 开同步）；显式给期限（`CHANGE_BUDGET_MS`） // ±0：`account-reads.ts::fetchSessionAccountsOrNull` 删了、`settings/interrupts.ts::askOne`（`machine-interrupts`，停 / 重启 / 更新 / 卸载之前问那台与本机会打断什么）一处，显式给期限（`INTERRUPTS_BUDGET_MS`） // +1：`terminal-reads.ts::sendToTerminal`（`terminal-input`，底部抽屉终端页送字送键）；显式给期限（`INPUT_BUDGET_MS`） // 历史页照稿重做 −2：`history-reads.ts` 那三处（项目清单 · 远端项目清单 · 会话清单）随旧页删了，`history-list-reads.ts::fetchList` 一处（`history-list`，平铺清单问本机后端，显式给期限 `LIST_BUDGET_MS`） // +1：`session-reads.ts::readTurns`（`history-turns`，一轮的摘要）；显式给期限（`READ_BUDGET_MS`） // +5：`quota-reads.ts` 五处（`quota-read` · `rotation-read` · `rotation-session-read` · `rotation-session-set` · `rotation-switch`，额度与轮换那几问）；显式给期限（`READ_BUDGET_MS` / 重启切换那一趟的总期限） // +1：`interrupt-reads.ts::askSessionInterrupts`（`session-interrupts`，动会话之前问那台会打断什么）；显式给期限（`INTERRUPTS_WITHIN_MS`） // +1：`account-ops.ts::accountsSetDefault`（`accounts-set-default`，设默认号改写那台的账号库清单，monitor 那份默认号删了）；显式给期限（`CHANGE_BUDGET_MS`） // +1：`settings/claude-dir-check.ts::claudeDirProblem`（`files-stat`，Claude 数据目录存之前问本机那个路径在不在）；显式给期限（`STAT_BUDGET_MS`） // +2：`alias-reads.ts::aliasToForm` / `aliasFromForm`（`aliases-to-form` / `aliases-from-form`，别名表单与参数互转交那台后端）；显式给期限（`ALIAS_BUDGET_MS`） // 合并：86 基线，这边 −1 ＋2（`terminal-open.ts` 本机那一支不再问 · `remote-terminal-front.ts::planRemoteFront` 两处），主线 ＋3 ⇒ 90 // +1：`sessions-where.ts::standingsOf`（`sessions-where`，这个会话在哪个 tmux 会话里问那台）；显式给期限（`STANDING_BUDGET_MS`） // +2：`tab-batch-run.ts` 的 `sessions-stop` · `sessions-start`（tab 栏批量停 / 起，每台一次）；显式给期限（按个数放宽） // 88 − 2：`launch-render.ts` 里 `launch-render-payload` · `launch-endpoint` 两处随起会话只交一行 `ccm …` 删了 // +1：`relay-optin-reads.ts::fetchRelayOptin`（`relay-optin`，「终端」栏直接敲的也走中转）；显式给期限（`RELAY_OPTIN_BUDGET_MS`） // 两边增量相加：扩展页这边 −1、账号间共用 MCP ＋ 摘全景那边 ±0 ⇒ 88 − 1 = 87 // 扩展页这边 −1：`cc-bus-install-reads.ts` 两处（装 cc-bus 改走枢纽）· 钩子那一块一处删了，`cc-bus-hooks-reads.ts` 一处 · `ext-reads.ts` 多一处（`ext-note-set`）⇒ −3 ＋2 // 〔合并扩展页 × 账号库〕两边的增量相加：95 ＋ 账号库 +3 ＋ 扩展页 −10 ⇒ 88 // 扩展页原注：删 15 处（`mcp-reads.ts` 3 · `mcp-sync-reads.ts` 2 · `skill-install-reads.ts` 3 · `skill-inbox-reads.ts` 3 · `plugins-section.ts` 1 · `assets-section.ts` 3）、加 5 处（`ext-reads.ts`：`ext-list` · `ext-hub-preview` / `-apply` · `ext-uninstall-preview` / `-apply`，各自显式给期限） // 账号库原注：账号库那一族：−4（`acct-iso-reads.ts` 三处 · `acct-deploy.ts` 一处随旧工具删）＋ 7（`account-ops.ts` 七条命令各一处，显式给期限 `CHANGE_BUDGET_MS` / `READ_BUDGET_MS`）// +1：`terminal-open.ts::openTerminal` 本机那一支问本机后端 `terminal-local`（令牌握手前奏由后端接）；显式给期限（`RENDER_BUDGET_MS`） // +1：`alias-reads.ts::allowLocalScripts`（`powershell-policy-set`，用户确认后改执行策略）；显式给期限（`ALIAS_BUDGET_MS`） // +1：`views/history-search.ts::searchAllMachines`（`history-search-merge`，各台结果合一份问本机后端）；显式给期限（`MERGE_BUDGET_MS`） // +1：`terminal-open.ts::openTerminal` 远端那一支（`terminal-ssh`，开终端那一行问本机后端渲）；显式给期限（`RENDER_BUDGET_MS`） // +1：`terminal-name-mint.ts::askMint`（`terminal-name-mint`，起会话要的 tmux 名问那台后端铸）；显式给期限（`MINT_BUDGET_MS`） // +1：`settings/panorama-section.ts::uninstallPanorama`（`panorama-uninstall`，全景小程序卸口）；显式给期限（`UNINSTALL_BUDGET_MS`） // +5：`record-reads.ts` 五处（`history-page` 两处 · `history-lines` · `history-subagent` · `drift-report`；会话正文四条从 monitor 那几条 Tauri 命令改走通道，漂移账记录那两面问那台后端）// +1：`settings/footprint-reads.ts` 的 `ask`（`footprint-report`，足迹从 monitor 那条 Tauri 命令改走通道）// +2：`panorama/api.ts` 的 `remote`（`panorama`）· `edit`（`panorama-edit`），全景从 monitor 那三条 Tauri 命令改走通道 // +1：`pubkey-push.ts::pushPublicKey` 问本机 `pubkey-push` // 基数 79 → 增量 +1：`settings/profile-backups.ts` 问本机 `files-ls`（`$PROFILE` 备份那一格） // 基数 67 ＋ 主线 +11（78）＋ MIG-3a +1（`acct-iso-install`）⇒ 79
            ("chan.subscribe", 2usize), // 3 → 2：订建索引进度流那一处随代码全景删了
            ("subscribe", 2usize) // 1 → 2：`filewin/source.rs::watch_link`（那台连没连着，`link` 流；订阅没有期限参数）
        ]
        .into_iter()
        .collect(),
        "前端对通信层两个入口的调用点不再各恰好一处（共 {sites}）"
    );
    assert_eq!(
        call_sites_without_budget("await call(origin, op, payload);\n", "call").len(),
        1,
        "识别器认不出缺 `Budget` 的调用点 —— `X6` 此刻在空转"
    );
    assert!(
        call_sites_without_budget("await call(origin, op, payload, budget);\n", "call").is_empty(),
        "显式给了 `budget` 的调用点被判成违例 —— 假红比不查更坏"
    );
    assert!(
        call_sites_without_budget("await recall(origin);\n", "call").is_empty(),
        "`recall(` 被当成了 `call(` —— 匹配单位比事实小（`needle_anchor_registry` 那一族）"
    );
}

// ════════════════════════════════════════════════════════════════════════════
//  七b、进不来的那几份：逐份被哪几条咬（散文 ⇒ 机检）—— 面 B ＋ 面 A 各一张表
// ════════════════════════════════════════════════════════════════════════════

/// 中转的宿主（后端 `relay/`）里**不进通信层**的那几份 ——
/// `(仓根相对路径, 咬它的判据编号, 它的 `C4` 判词标签, 为什么它在外面)`。
///
/// ⚠ 第三列〔`C4` 判词标签〕的理由住 [`assert_left_outside`] ——
/// 只比判据编号的话，「一份文件少了一处读盘」在盘上看不出来。标签的住址是
/// [`disk_and_env_needles`]（本表**不写那个串本身**，理由见 [`disk_and_env_tags_in`]）。
///
/// 本条判的是：那段解释「为什么它在外面」的散文有没有腐 —— 盘上现扫的咬人判据集合 == 本表这一行（两向）。
/// ⇒ 一份文件的阻塞清空了（表里那一行变成空集）⇒ 本条当场红，该问的是「它现在该不该搬进通信层 crate」，
/// 不是回来把这一行删掉了事。⚠ 「十一条全绿」不等于「该进」，归属判断永远是人做的。
const RELAY_LEFT_OUTSIDE: &[(&str, &[&str], &[&str], &str)] = &[
    (
        "src/backend/relay/listen.rs",
        &["C5", "X2"],
        &[],
        "自己 `TcpListener::bind` 端口（`C5`）。先前还自己 `std::env::var` 读环境（`C4`，`--relay` 入口那一处）—— 那一形删了，\
         进程内那一形的取值器是宿主递进来的。\
         按 `C5` 括号里那条，端口与端口号本来就归后端 ⇒ \
         这一份**语义上就该在外面**，不是「等它变干净」。\
         🔴 **`X2` 是 `P16`（2026-09-22）新加的一条，而它是「变干净」的反面**：\
         中转那两个期限**常量**搬到了这一份里（把「期限值」\
         算进「全部由后端交给它」）⇒ 这一份**更**该在外面了，而中转少了两处 `X2`。\
         这一格是那张表少见的「多一条反而是对的」—— 别顺手把它改回去。",
    ),
];

/// **面 A 的传输面候选**里进不来的那几份 ——
/// `(仓根相对路径, 咬它的判据编号, 它的 `C4` 判词标签, 为什么它今天圈不进来)`。
///
/// 🔴 **这张表治的病与 [`RELAY_LEFT_OUTSIDE`] 逐字相同，只是换了一个面。**
/// 那一拍（2026-09-21）给**面 B** 把「为什么进不来」从散文换成了机检，而**面 A 一直没有** ——
/// 它那几份的判词只住，也就是一份**会腐的读数**。
///
/// # 🔴 它不是「多一张表」，它补的是一个现打出来的洞
///
/// 〔步 4 剩余那一路，2026-09-22〕死值验：把 `stream_source/` 那处读环境变量**摘掉**
/// ⇒ 这一族当时那十六条**一条都没红**（那几份不是成员、也不在任何一张表里）。
/// ⇒ 「面 A 还剩几处读盘」这件事**此前完全不在执行链上**：清掉一处、或者再长出一处，
/// 都没有任何东西会说话。本条就是那条缺掉的腿。
/// （同一刀真红的是**另一族** `stream_source::dial_move_judge`，而它是**正着**钉住那处
/// 环境变量必须在 ⇒ 清它是改设计，不是做清理。这一格的判词写在下面 `stream_source/` 那一行。）
///
/// # 人群从哪来（**不是**「扫哪个目录」）
///
/// 那张「传输面在新射程下还咬」的表点了四份，**减去** `sftp_move_ledger.rs`
/// （它今天一条都不咬），**加上** `pubkey.rs`（逐份试圈的第 5 行，
/// 也是完成判据里点名的三份之一）。
///
/// ⚠ **`sftp_move_ledger.rs` 为什么不列进来**：它是那一形的第三例
/// —— 十一条一条不咬，**而它仍然不该圈**。列进来的话这张表第一天就红，
/// 而那条红指向的处置（「阻塞清空了 ⇒ 把它圈进来」）**恰好是错的**。
/// 归属判断永远是人做的 ⇒ 照 [`RELAY_LEFT_OUTSIDE`] 对 `accounts/policy.rs` 的处置办：
/// **不列，理由写在这里而不是等人来问。**
///
/// # 🔴 那个「**5 处读盘**」，机检住址就在这张表的第三列
///
/// 那个数**不写在任何一句散文里** —— 它是这张表第三列的**处数合计**，
/// 由下面那条判据与 `expected_c4_sites` 做**相等**断言（不是地板）。
/// ⇒ 清掉一处、或者再多长一处，两个方向都当场红。
/// 〔本拍死值验：摘掉 `stream_source/` 那处读环境变量 ⇒ 逐字
///  「不见了的（表写了而不咬）：["读环境OS"]」；往 `sftp.rs` 注一处 ⇒ 逐字
///  「多出来的（表没写）：["读字节"]」。〕
///
/// # 往里加/减一行要同拍做两件事
///
/// 1. 改这张表（判据编号与判词两列**都是机检的**，写错当场红，两列还互相自检）；
/// 2. 改下面那条判据里的**份数**与**判词处数**（两个都是相等断言，不是地板）。
/// ⚠ 这张表**不是**豁免清单，也**不是**待办清单：它只保证「为什么进不来」这段理由**不是假的**。
const TRANSPORT_LEFT_OUTSIDE: &[(&str, &[&str], &[&str], &str)] = &[
    (
        "src/frontend/shell/src/stream_source/",
        &["C1", "X2"],
        &[],
        "**传输那一段已经搬出去了**：SSH 的全部活进了后端的拨号代理，\
         界面侧读应答的那一段是新的通信层成员 `ssh_link.rs`，起代理的是宿主 `dial_host.rs`。\
         今天咬它的两条**全是业务该做的事**，不是传输面没洗干净：`C1` 公开面上是会话/tmux/agent 那一族\
         （远端数据源本来就是业务）· `X2` 重连退避与快照重试的期限值。\
         ⇒ **这一份不是「还差一点就进来」，是「本来就不该进来」**：登记它等于把业务家圈进通信层。\
         〔`C4` 原来还有一个判词「读环境OS」—— 拨号代理二进制的解析搬去了宿主，那一处随之离开。\
`C4`「读文本」（读 `~/.ssh/config`）与 `C5`（起 `ssh -G`）随「从 ssh config 导入」搬进后端 `dial/ssh_config.rs` 一起离开。〕",
    ),
    (
        "src/frontend/shell/src/sftp.rs",
        &["X2"],
        &[],
        "**今天咬 `X2` 一条**：部署判定进了本机常驻后端（`deploy-plan`），本文件问它要计划那一问         定了一个期限值（`PLAN_BUDGET`）—— 期限值归宿主，而它就是宿主那一侧的调用方（形状同下一行 `sftp_pool.rs`），         照实登记、不圈。 **原来只差 `C1` 一条，后来一条都不咬了** —— 咬它的那个词随 F11 那条 SFTP 直删\
         （连同它的结构守卫〔散文墓碑〕）改经远端后端删一起走了（说的「要清掉那个词得连它一起搬」，\
         搬的是 RW1）。🔴 **而它仍然不圈**，这是一次归属判断、不是判据没跑：\
         本文件今天剩下的是 F08 的**部署**（后端二进制 · 入口 shim · 卸载）（远端 rc 别名块的**规划**\
         `merge_profile_block` / `strip_profile_block` / `CCM_WRAPPER_SNIPPET` 搬去了 `profile_installer.rs`）—— 都是业务，不是传输；\
         SFTP 整体进常驻后端是 4B 的 `SR1b`，这一份的去留归它裁。十一条全绿不等于该圈。",
    ),
    (
        "src/frontend/shell/src/sftp_pool.rs",
        &["X2"],
        &[],
        "**传输本体整段搬进了本机常驻后端**（SFTP 客户端 `dial/sftp.rs`、传输台 `control/transfer.rs`），\
         这份只剩中继：开单 / 起跑 / 撤原样转给本机后端，`transfer` 帧翻成窗口那几格。\
         〔墓碑 —— 从前咬它的是 `C1`（公开面上的传输业务词）与 `C4` 两个判词「开文件」「以选项开」（用户那次传输的本地那一头）；\
         两样都跟着传输本体走了。〕今天只剩 `X2`：它给本机后端那几条**就地记账**命令的应答定了一个期限值（`CALL_BUDGET`）—— \
         期限值归宿主，而它**就是**宿主那一侧的中继，不是传输面候选 ⇒ 不圈，照实登记。",
    ),
    // `pubkey.rs` 那一行摘了：它「正解是搬去后端」—— 公钥推送整件进了本机后端（`pubkey-push`，`src/backend/assets/pubkey.rs`：
    //   读本机 `.pub` · 组请求 · 那台后端在就经它写 / 不在就一次 exec），monitor 那份文件删了。
];

/// 拿十一条判据的识别器扫一份文本，返回**咬它的那些编号**。
///
/// 🔴 **一行识别器都不自己写** —— 全部调 `C1`–`C5` / `X1`–`X6` 各自那一个
/// （`E3`：一个事实一个权威源）。自己近似重写一份的话，本条会与那十一条各自漂。
fn criteria_biting(rel: &str, prod: &str) -> BTreeSet<&'static str> {
    let mut out: BTreeSet<&'static str> = BTreeSet::new();
    if !surface_offences_of(rel, prod).is_empty() {
        out.insert("C1");
    }
    let c3 = if rel.ends_with(".ts") {
        guard_core::contains_word(prod, "transport")
    } else {
        prod.lines().any(public_line_leaks_transport)
    };
    if c3 {
        out.insert("C3");
    }
    if !disk_and_env_tags_in(prod).is_empty() {
        out.insert("C4");
    }
    if spawn_and_bind_needles()
        .iter()
        .any(|(n, _)| prod.contains(n.as_str()))
    {
        out.insert("C5");
    }
    if !inexhaustive_wire_matches(prod).is_empty() {
        out.insert("X1");
    }
    if !deadline_literals(prod).is_empty() {
        out.insert("X2");
    }
    if !hop_sites_without_reach(prod).is_empty() {
        out.insert("X3");
    }
    if !silent_drop_sites(prod).is_empty() {
        out.insert("X4");
    }
    if !loose_until_derivations(prod).is_empty() {
        out.insert("X5");
    }
    if ENTRIES
        .iter()
        // 人群与 `X6` 同一个口径（按入口的语言分前端语料），不另写一份。
        .any(|(e, lang, _)| {
            is_frontend_for(rel, lang, &BTreeSet::new())
                && !call_sites_without_budget(prod, e).is_empty()
        })
    {
        out.insert("X6");
    }
    out
}

/// ★★ 两张「进不来的逐份理由」表**共用的那一条断言** —— `D1`：一个判定只有一个家。
///
/// [`RELAY_LEFT_OUTSIDE`]（面 B）与 [`TRANSPORT_LEFT_OUTSIDE`]（面 A）形状逐字相同，
/// 而它们**只许有一份实现**：各写一份的话两份会各自漂，而「漂开了」在终端上一个字都看不出来
/// —— 那正是 `D1` 在治的那一族（同 [`criteria_biting`] 一行识别器都不自己写的理由）。
///
/// # 反空真：三样各自钉着
///
/// 1. **两向集合相等**（逐份）—— 不是「表里那几条确实咬」（那是**地板**，在「多咬了一条」
///    方向瞎），是**恰好这几条**。
/// 2. **份数相等 ＋ 每份都真读到了** —— 份数用**相等**不用地板；路径漂了当场 panic，
///    不许退化成「那就少判一份」（人群缩水与「全都合规」在终端上一模一样）。
/// 3. **识别器不是恒红** —— 一段干净的合成文本喂进去必须零命中。没有这一条，全咬也是绿。
///
/// # 🔴 为什么还要**判词那一层**（2026-09-22 被死值验逼出来的一格）
///
/// 第一版只比**判据编号**的集合。死值验第一刀当场证否：`stream_source/` 有两个 `C4` 判词，
/// 摘掉其中一个之后编号那个集合**一个字都不变** ⇒ 十七条**全绿**。
/// ⇒ 「集合粒度」在「少了一处」那个方向与**地板**一样瞎，而本仓正是反复栽在那上面。
/// 本条因此多两层：逐份的 `C4` **判词**两向集合相等 ＋ 全表判词**处数**相等，
/// 另有一条两列互相的自检（写了 `C4` 就必须逐个判词点出来，反之也不许凭空多一列）。
///
/// # 买不到（如实登记）
///
/// - **只有 `C4` 收到了判词那一层。** `C2`（业务 crate 名）· `C5`（起进程/绑端口）·
///   `X1`–`X6` 今天仍**只比编号集合** ⇒ 那几条各自「少了一处」的方向**照样瞎**。
///   这一格是有意的：完成判据点名的是「那 5 处读盘」，
///   把六条判据的判词全立起来会变成一张没人读的大表。**要补是另一件活，不是这一件的漏。**
/// - **不买「表里那几份该不该进来」** —— 归属判断永远是人做的，见两张表各自的头注。
fn assert_left_outside(
    table: &[(&str, &[&str], &[&str], &str)],
    label: &str,
    expected_len: usize,
    expected_c4_sites: usize,
) {
    let root = repo_root();
    // 表里的编号必须都是真判据（拼错一个 ⇒ 那一行从此恒不命中）。
    let known: BTreeSet<&str> = CRITERIA.iter().map(|(id, _, _)| *id).collect();
    let known_tags: BTreeSet<&str> = disk_and_env_needles()
        .iter()
        .map(|(tag, _, _)| *tag)
        .collect();
    for (rel, ids, c4, _) in table {
        for id in *ids {
            assert!(
                known.contains(id),
                "`{rel}` 那一行写着判据 `{id}`，而 `CRITERIA` 里没有这个编号 —— \
                 编号拼错了 / 判据改名了。那一行会从此恒不命中。"
            );
        }
        for tag in *c4 {
            assert!(
                known_tags.contains(tag),
                "`{rel}` 那一行写着 `C4` 判词〔{tag}〕，而形状表里没有这个标签 —— \
                 标签拼错了 / 判词改名了。那一格会从此恒不命中。"
            );
        }
        // 两列之间的自检：写了 `C4` 就必须逐个判词点出来，反之也不许凭空多一列。
        assert_eq!(
            ids.contains(&"C4"),
            !c4.is_empty(),
            "`{rel}`：判据那一列{}写 `C4`，而判词那一列{}空 —— 两列必须同进同退，\
             否则「几处读盘」那个数会从一侧悄悄漂掉",
            if ids.contains(&"C4") { "" } else { "没" },
            if c4.is_empty() { "是" } else { "不是" }
        );
    }

    let mut all: BTreeSet<&'static str> = BTreeSet::new();
    let mut c4_sites = 0usize;
    let mut diverged: Vec<String> = Vec::new();
    for (rel, ids, c4, why) in table {
        // 以 `/` 结尾的那一格是一个目录（会话流来源拆成了一个目录）：整个目录的生产段当一份判。
        let prod = if rel.ends_with('/') {
            assert_eq!(
                *rel, "src/frontend/shell/src/stream_source/",
                "目录那一格只认会话流来源那个目录（理由：{why}）"
            );
            crate::guard_support::stream_source_production()
        } else {
            let p = root.join(rel);
            let raw = std::fs::read_to_string(&p).unwrap_or_else(|e| {
                panic!(
                    "表里写着 `{rel}`（理由：{why}），而它读不出来：{e}\n\
                     ⇒ 路径漂了 / 文件搬走了。**不许当成「那就少判一份」** ——\n\
                     人群缩水与「全都合规」在终端上一模一样。"
                )
            });
            production_of(rel, &raw)
        };
        assert!(
            !prod.trim().is_empty(),
            "`{rel}` 的生产段剥完是空的 —— 剥法坏了，下面那条会在两个空集之间比对（恒绿）"
        );
        let got = criteria_biting(rel, &prod);
        all.extend(got.iter().copied());
        let want: BTreeSet<&str> = ids.iter().copied().collect();
        let got_str: BTreeSet<&str> = got.iter().copied().collect();
        if got_str != want {
            let extra: Vec<&&str> = got_str.difference(&want).collect();
            let gone: Vec<&&str> = want.difference(&got_str).collect();
            diverged.push(format!(
                "  {rel}\n    表里写着：{want:?}\n    盘上现扫：{got_str:?}\n    \
                 多出来的（表没写）：{extra:?}\n    不见了的（表写了而不咬）：{gone:?}"
            ));
        }
        // 🔴 **判词那一层也要两向相等** —— 只比判据编号的话，一份文件有两个 `C4` 判词时
        //    摘掉其中一个，编号那个集合**一个字都不变** ⇒ 「少了一处读盘」在盘上看不出来。
        //    〔本拍死值验第一刀逮到的正是这一形：摘掉 `stream_source` 那处读环境变量，
        //     只比编号时十七条全绿。地板在「变少」方向是瞎的，集合粒度在这里也是。〕
        let want_c4: BTreeSet<&str> = c4.iter().copied().collect();
        let got_c4 = disk_and_env_tags_in(&prod);
        c4_sites += got_c4.len();
        if got_c4 != want_c4 {
            let extra: Vec<&&str> = got_c4.difference(&want_c4).collect();
            let gone: Vec<&&str> = want_c4.difference(&got_c4).collect();
            diverged.push(format!(
                "  {rel}〔`C4` 判词那一层〕\n    表里写着：{want_c4:?}\n    \
                 盘上现扫：{got_c4:?}\n    多出来的（表没写）：{extra:?}\n    \
                 不见了的（表写了而不咬）：{gone:?}"
            ));
        }
    }
    assert!(
        diverged.is_empty(),
        "{label}「被哪几条咬」与登记的对不上：\n{}\n\n\
         两个方向各有一种处置，别混：\n\
         ① **多出来一条** ⇒ 有人往那份文件里加了新的违例。补进表里那一行，并写清它是什么。\n\
         ② **少了一条**（表写了而不咬）⇒ 那条阻塞被清掉了。\n\
            🔴 处置**不是**把这一行改小了事 —— 该问的是「它现在圈得进来了吗」：\n\
            阻塞清空 ⇒ 它该不该搬进通信层 crate。\n\
            ⚠ 而「十一条全绿」不等于「该圈」，归属判断仍是人做的。",
        diverged.join("\n")
    );

    // 反空真①：份数**相等**，且至少真咬到过东西（否则「识别器全瞎」与「全都干净」同形）。
    assert_eq!(
        table.len(),
        expected_len,
        "{label}：判据里写的份数是 {expected_len}，而盘上这张表是 {} 行 —— \
         真加/减了一份就回来同拍改这个数（它是散文那一侧，且是相等不是地板）",
        table.len()
    );
    assert!(
        !all.is_empty(),
        "{label}：一条判据都没咬住 —— 识别器整批瞎了，\
         而那时上面那条相等断言是在两个空集之间比对（恒绿）"
    );
    // 🔴 **「还剩几处读盘」那个数的机检住址** —— 相等，不是地板。
    assert_eq!(
        c4_sites, expected_c4_sites,
        "{label}：判据里写的 `C4` 判词处数是 {expected_c4_sites}，而盘上现扫是 {c4_sites} —— \
         真清掉/真多长一处就回来同拍改这个数。\n\
         🔴 变**少**那个方向尤其要停一下：该问的不是「把这个数改小」，\
         是「那一份现在该不该搬进通信层 crate」。"
    );

    // 反空真②：识别器不是恒红 —— 一段干净的合成文本喂进去必须零命中。
    let clean = "pub fn relay(origin: &str, op: &str, payload: &[u8]) -> u8 {\n    \
         let _ = (origin, op, payload);\n    0\n}\n";
    let on_clean = criteria_biting("synthetic.rs", clean);
    assert!(
        on_clean.is_empty(),
        "一段只用位置词、不读盘、不起进程、无期限字面量的干净代码被判成有 {on_clean:?} —— \
         那么上面每一格的绿都不携带任何信息（识别器恒红）"
    );
}

/// ★ **中转的宿主为什么在外面** —— 后端 `relay/` 里不进通信层的那几份，逐份**被哪几条咬**与 [`RELAY_LEFT_OUTSIDE`] 两向相等。
///
/// 反空真那三样与面 A 那条**共用同一份实现**（[`assert_left_outside`]，`D1`）。
/// **不买**「表里那几份该不该进来」—— 见 [`RELAY_LEFT_OUTSIDE`] 头注。
#[test]
fn the_relay_files_left_outside_are_blocked_by_exactly_the_criteria_the_prose_names() {
    assert_left_outside(
        RELAY_LEFT_OUTSIDE,
        "中转的宿主（后端 `relay/`）不进通信层的那几份",
        1,
        0,
    );
}

/// ★ **传输面那四份** —— 面 A 的候选逐份**被哪几条咬**与 [`TRANSPORT_LEFT_OUTSIDE`] 两向相等。
///
/// 〔「步 4 的剩余」，2026-09-22 立〕面 B 那张表 2026-09-21 就有了，
/// **面 A 一直没有** ⇒ 「面 A 还剩几处读盘」这件事此前**完全不在执行链上**
/// （死值验：摘掉一处读环境变量，这一族当时那十六条一条都没红）。人群与逐份理由住
/// [`TRANSPORT_LEFT_OUTSIDE`]，反空真那三样与面 B 那条共用 [`assert_left_outside`]。
///
/// # 买不到（别读大了）
///
/// - **不买「这四份该不该进来」** —— 见 [`TRANSPORT_LEFT_OUTSIDE`] 头注；
///   其中 `pubkey.rs` 与 `sftp_move_ledger.rs` 两格的正解都**不是**「圈进来」。
/// - **不买「传输面就是这四份」** —— 人群是那张表 ＋给的，
///   本条**不去数目录**。哪天传输面多一份文件，本条一个字都不说。
/// - **不买「这几处读盘清得掉」** —— 它只买「那段解释为什么清不掉的理由不是假的」。
///   逐处的写区外前置（另一族判据正着钉住那处环境变量 · 6 条互锁登记表 · 一道未拍的设计题）
///   写在表里各自那一行。
#[test]
fn the_transport_candidates_left_outside_are_blocked_by_exactly_the_criteria_the_prose_names() {
    // `C4` 判词处数 5 → **4**：少的是 `stream_source/` 的「读环境OS」——
    //   拨号代理二进制的解析（`CCM_DIAL_PROXY`）随拨号搬去了宿主 `dial_host.rs`（不是成员，那一处本来就归它）。
    // `C4` 判词处数 4 → **2**：少的是 `sftp_pool.rs` 的「开文件」「以选项开」——
    //   用户那次传输的本地那一头随传输台搬进了本机常驻后端（`control/transfer.rs`）。份数仍是 4（它还是候选，只剩 `X2`）。
    // `C4` 判词处数 2 → **1**：少的是 `stream_source/` 的「读文本」（读 `~/.ssh/config`）——
    //   「从 ssh config 导入」搬进后端 `dial/ssh_config.rs`。份数仍是 4。
    // 4 → 3：`pubkey.rs` 随公钥推送进本机后端删了。
    // 判词处数 1 → 0：那一处读盘就是 `pubkey.rs` 读本机 `.pub`（它搬进了本机后端）。
    assert_left_outside(TRANSPORT_LEFT_OUTSIDE, "面 A 的传输面那三份候选", 3, 0);
}

// ════════════════════════════════════════════════════════════════════════════
//  八、元判据：这十七条真的在跑
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
/// 并且编号那一列恰好覆盖 `C1`–`C5` 与 `X1`–`X6`。
///
/// # 它治什么
///
/// 删掉一条判据而把表留着（读的人以为那条性质有人守着）· 加一条判据而不登记
/// （编号与盘上的东西对不上）· 把 `X4` 悄悄改名（闭集按名字认）。
///
/// # ⚠ 它**接不住**的那一格（如实登记，这是本件已知的洞）
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
    // 抽取器自检：真的抠到了测试名（否则两个空集相等，恒绿）。
    assert!(
        actual.len() >= 10,
        "只抠出 {} 个 `#[test]` —— 抽取器坏了，下面那条会拿两个空集比出绿：{actual:?}",
        actual.len()
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
    let want_c: BTreeSet<&str> = ["C1", "C2", "C3", "C4", "C5"].into_iter().collect();
    let want_x: BTreeSet<&str> = ["X1", "X2", "X3", "X4", "X5", "X6"].into_iter().collect();
    let got_c: BTreeSet<&str> = ids.iter().filter(|i| i.starts_with('C')).copied().collect();
    let got_x: BTreeSet<&str> = ids.iter().filter(|i| i.starts_with('X')).copied().collect();
    assert_eq!(
        got_c, want_c,
        "通信层的铁律恰好五条（C1–C5），盘上是 {got_c:?}"
    );
    assert_eq!(got_x, want_x, "签名判据恰好六条（X1–X6），盘上是 {got_x:?}");
}
