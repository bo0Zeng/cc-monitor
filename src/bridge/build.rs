use std::path::{Path, PathBuf};

fn main() {
    emit_backend_build_id();
    emit_backend_capabilities();
    check_vendor_freshness();
    check_acct_iso_vendor_freshness();
    embed_backends();
    embed_panoramas();
    embed_native_backend();
    embed_native_panorama();
    tauri_build::build()
}

/// backend 源码里**装身份与声明的那一份**的住址 —— 本文件里五处要它
/// （build_id · 身份戳界标 · capabilities · mtime · 内嵌校验）。
/// 抽出来的理由与 `local_ccm_entry_name` 同族：五份手抄的路径迟早有一份被漏改。
///
/// 🔴 〔步 9 · 09-19〕**从 `main.rs` 改成 `lib.rs`。** `BUILD_ID` / `BUILD_STAMP_*` /
/// `CAPABILITIES` 这一族已按 `设计/00 §1.5.4` 前置 2 搬进后端的库面 —— 理由是
/// in-process 那条路**没有那个 `main.rs`**，身份会跟着它一起消失。
/// ⚠ 本函数**没有回退到 `main.rs` 的分支**，这是刻意的：读一份旧住址会悄悄给出一个
/// 过期的身份，而**抠不到必须是一条响亮的失败**（住址见 [`backend_source_build_id`]）。
/// 〔墓碑，本行原话逐字：「抠不到时上面那几个消费者各自落到「`unknown` / 空界标」那一支
///  并把失败逐字印出来」。**那半句在 `19b` 之前是真的，而它正是病灶** ——「落到 `unknown`」
///  只在**恰好铺了字节**的构建里才有人 panic，平常每一次 `cargo build` 都静静地把
///  `BACKEND_BUILD_ID="unknown"` 烤进 exe（射程外的账住 `设计/96 §7.2.5` 那张表）。〕
fn backend_lib_rs() -> PathBuf {
    // build.rs 的 cwd 是本包根（`src/bridge/`）⇒ `..` 是 `src/`。
    // 2026-09-18：仓库重组把后端树从 `remote-daemon-proto/src/` 搬到 `src/backend/`，
    // 这一处漏改了（原为 `../src/backend/src/main.rs`，解出 `src/src/backend/src/main.rs`）。
    // 它没有当场现形，唯一的原因是**本包在本机从未编过** —— 装齐 Tauri 栈后头一次 check 就炸了。
    Path::new("..").join("backend").join("lib.rs")
}

/// 分派那一半的住址。只有 mtime 那条安全网要它（改了分派，内嵌的二进制一样陈了）。
fn backend_main_rs() -> PathBuf {
    Path::new("..").join("backend").join("main.rs")
}

/// 🔴 **`BUILD_ID` bump 的同拍步骤只有这一个住址**〔步 `19c` · 09-19〕。
///
/// # 它治的是一件真发生过的事，而且是一件本文件自己造成的事
///
/// 协议面一变就要 bump `lib.rs` 的 `const BUILD_ID`，而 bump 的那一刻
/// `embedded-backends/` 里那两份 musl 字节立刻变旧 ⇒ 下面那条**半 bump 守卫**当场
/// `panic!`，**整棵树编不过**。2026-09-18 实地踩过一次：删用量 ⇒ bump `p2j`→`p2k`
/// ⇒ **四路 agent 同时编不过**。`设计/99 §4` 步 `19c` 的裁定逐字：
/// 「**把 re-embed 写成 bump 的同拍步骤**，别让它变成一次事故。」
///
/// # 🔴 为什么是一条命令，而不是像以前那样在 panic 文案里手抄一段配方
///
/// 〔墓碑 —— 本文件下面那条半 bump panic 原来写着整整八行出路，逐字是
///  `cargo build --release --target {arch}-unknown-linux-musl --config
///  'target.{arch}-unknown-linux-musl.linker="rust-lld"'` ＋ 一段 `cp`，
///  并自陈「这样编出来的形态与发版 CI 的 zigbuild 产物**不同**」。
///  **那是第二条产字节的路，也就是第二个住址** —— 而 09-18 那次真正救活那棵树的
///  是 `cargo zigbuild --release --locked`（＝发版那趟那条），**不是它**。
///  ⇒ 手抄的那段撤掉，出路收成本常量指的那一条命令；那条命令与 `release.yml` 的
///  `Cross-compile backend for both musl targets` **逐字同源**，
///  由 `tests/evidence/K-R124-ruler.py` ⑬b 每趟两向对拍。
///  08-25 那条实测读数（`ring` 的 C 要 `zig cc`，`rust-lld` 替不了，aarch64 不给就 rc=101）
///  **一个字没丢**，搬进了 `tests/scripts/re-embed.sh` 的头注 —— 它现在是那条命令
///  必须用 zigbuild 的理由，写在跑那条命令的地方。〕
///
/// ⚠ **它不撤任何一条既有的守卫。** mtime 那张安全网、三处 `panic!` 全部留着；
/// 本常量只是把「出事之后怎么办」收成一个住址。安全网与机制的分工逐字写在
/// `tests/scripts/re-embed.sh` 的头注里。
const REEMBED_CMD: &str = "bash tests/scripts/re-embed.sh";

/// 远端那两份 musl 字节的落点目录名。与 [`NATIVE_BACKEND_DIR`] 对称，两个都是
/// **机检过的落点**：`tests/evidence/K-R124-ruler.py` ⑬d 把这两个常量与
/// `src/bridge/.gitignore` 里带 `⇐ 内嵌落点` 锚的那几行**两向集合相等**。
///
/// 🔴 立这个常量的直接起因：步 8 全仓改名（`daemon` → `backend`）之后，
/// `.gitignore` 里还写着 `/embedded-daemons/` 与 `/native-daemon/` ——
/// **两个落点从那天起就没被挡住**（09-19 现打 `git check-ignore` 两条都不命中），
/// 而 `设计/96 §7.1.2` 与 `release.yml` 文件头都还把「三个落点全部 gitignore」
/// 当成硬事实在用。目录名从字面量变成常量，那条对拍才有东西可读。
const EMBEDDED_BACKENDS_DIR: &str = "embedded-backends";

/// backend 源码里那个 `const BUILD_ID`。**这是本机与远端两条内嵌路共用的期望值。**
///
/// # 🔴 `19b`（09-19）：**抠不到 ＝ 构建当场失败，没有兜底值**（`设计/96 §7.2.5`）
///
/// 规格逐字：「事故的形状不是『读失败』，是**读失败被换成了一个会参与比较的字符串**——
/// `"unknown"` 必然不等于任何真 `BUILD_ID` ⇒ 必然判 StaleBuild ⇒ 必然重装 ⇒ 装完还是不等。」
/// 事故本体住 `设计/16 §5.4a`：抠不到就 `unwrap_or_else(|| "unknown")`，
/// 装出去**每台远端无限重装**。
///
/// **为什么住在这里（一个住址），而不是三个消费者各写一次**：本函数在 `19b` 之前返回
/// `Option`，三个消费者里只有 `embed_backends` / `embed_native_backend` 会 panic，
/// 而那两条 panic **都在 `src.exists()` 的里面** —— 平常每一次 `cargo build`（没铺字节）
/// 走的是上面那条 `cargo:warning` 分支，**panic 一次都不触发**，`BACKEND_BUILD_ID`
/// 就这么带着 `"unknown"` 被烤进 exe。⇒ 把「响」挪到**取值这一跳**，
/// 它在**所有**构建形态下都响（`设计/16 §5.4a` 的元教训逐字：
/// 「『有一条能跑的检查』和『那条检查的人群是全的』是两件事」）。
///
/// ⚠ **`Option` 没有消失，消失的是兜底字符串**：抽取那一层仍是
/// [`extract_build_id`] `-> Option<String>`，`None` 不会被换成任何「看起来像身份」的值，
/// 它只通向下面这条 panic。
fn backend_source_build_id() -> String {
    let p = backend_lib_rs();
    std::fs::read_to_string(&p)
        .ok()
        .and_then(|s| extract_build_id(&s))
        .unwrap_or_else(|| {
            panic!(
                "抠不到后端源码的 `const BUILD_ID` —— 读的是 `src/bridge/{}`\
                 （路径失效 / crate 改名 / 身份又搬家了 / `const` 写法变了）。\n\
                 🔴 **这一步刻意没有兜底值**：给它一个 `\"unknown\"` 之类的字符串，\
                 它会照样参与 `reported_build_id != EXPECTED_BACKEND_BUILD_ID` 那个比较，\
                 于是装出去的每一台远端都被判 StaleBuild 并**无限重装**\
                 （真事故，账住 `设计/16 §5.4a`）。\n\
                 出路：把身份搬回 `src/backend/lib.rs` 的 `pub const BUILD_ID: &str = \"…\";`，\
                 或同拍改本文件的 `backend_lib_rs()`（那是全仓唯一的住址）。",
                p.display()
            )
        })
}

/// F5：vendored cc-acct-iso 过期软检查（SS-10「过期看得见」）。从 `VENDOR.md` 抠上游仓路径
/// （`~` 展开为 $HOME），若上游存在则比对三个脚本与 vendored 副本，不一致则 `cargo:warning`。
/// 上游缺席 → no-op（同 `check_vendor_freshness`：开发期上游领先副本是常态，软警告非硬失败）。
fn check_acct_iso_vendor_freshness() {
    let vendor_dir = Path::new("../shared/cc-acct-iso"); // 〔MIG-3a · 09-28〕从 `vendor/` 挪去 `src/shared/`（字节随后端二进制走，两棵树都不属于）
    let vendor_md = vendor_dir.join("VENDOR.md");
    println!("cargo:rerun-if-changed={}", vendor_md.display());
    println!(
        "cargo:rerun-if-changed={}",
        vendor_dir.join(".vendor_id").display()
    );
    // D 审计 S1/S5：指纹须覆盖**全部被部署文件**，故过期检查也逐个比这 6 个（不只 3 脚本）。
    // 顺序须与 VENDOR.md 菜谱 / `.vendor_id` 计算一致（自洽校验按同一顺序拼接）。
    const DEPLOYED: [&str; 6] = [
        "scripts/cc-acct-iso",
        "scripts/lib.sh",
        "scripts/cc-acct-iso-install.sh",
        "scripts/test/run-tests.sh",
        "SKILL.md",
        "examples/config",
    ];
    for f in DEPLOYED {
        println!("cargo:rerun-if-changed={}", vendor_dir.join(f).display());
    }

    // (a) 自洽校验：vendored 6 文件的 sha256 前 16 位是否等于 `.vendor_id`（防「改了 vendored
    //     脚本却忘了重算指纹」→ 远端 Skip 不更新而 build 期无声）。用 sha256sum shell-out（同
    //     VENDOR.md 菜谱），缺 sha256sum 则跳过该项。
    if let Ok(recorded) = std::fs::read_to_string(vendor_dir.join(".vendor_id")) {
        let recorded = recorded.trim();
        let cat_cmd = format!(
            "cat {} | sha256sum | cut -c1-16",
            DEPLOYED
                .iter()
                .map(|f| format!("'{}'", vendor_dir.join(f).display()))
                .collect::<Vec<_>>()
                .join(" ")
        );
        if let Ok(out) = std::process::Command::new("sh")
            .arg("-c")
            .arg(&cat_cmd)
            .output()
        {
            let computed = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !computed.is_empty() && computed != recorded {
                println!(
                    "cargo:warning=vendor cc-acct-iso 指纹不自洽:.vendor_id={recorded} 但脚本实际 sha={computed}。改了 vendored 文件后请按 VENDOR.md 菜谱重算 .vendor_id。"
                );
            }
        }
    }

    // (b) 与上游比对（上游缺席 → no-op）。
    let Ok(text) = std::fs::read_to_string(&vendor_md) else {
        return;
    };
    let Some(up_raw) = extract_backtick_after(&text, "上游仓:") else {
        return;
    };
    let up = if let Some(rest) = up_raw.strip_prefix("~/") {
        match std::env::var_os("HOME") {
            Some(home) => Path::new(&home).join(rest),
            None => return,
        }
    } else {
        Path::new(&up_raw).to_path_buf()
    };
    if !up.exists() {
        return; // 上游缺席 → no-op
    }
    // 上游布局：脚本在 scripts/、test 在 scripts/test/、SKILL.md 在根、config 在 examples/。
    let mut stale = 0usize;
    for f in DEPLOYED {
        let vb = std::fs::read(vendor_dir.join(f)).ok();
        let ub = std::fs::read(up.join(f)).ok();
        if let (Some(vb), Some(ub)) = (vb, ub) {
            if vb != ub {
                stale += 1;
            }
        }
    }
    if stale > 0 {
        println!(
            "cargo:warning=vendor cc-acct-iso 过期:上游有 {stale} 个文件与 vendored 副本不一致。见 src/shared/cc-acct-iso/VENDOR.md 的 re-vendor 菜谱。"
        );
    }
}

/// F68：vendor 副本过期检查（SS-10「过期看得见」）。从 `VENDOR.md` **单源**抠 pin + 上游
/// 仓路径，若上游 sibling 仓存在则比对 `pin..HEAD` 有没有未 re-vendor 的 core 改动，非空
/// 发**可见的 `cargo:warning`**。**上游仓缺席（CI/Windows）→ 静默 no-op，绝不拖垮构建**
/// （同 `embed_backends` 二进制缺席 no-op）。软警告非硬失败——开发期上游领先副本是常态。
fn check_vendor_freshness() {
    // 〔RE〕vendor 随唯一消费者搬进 `src/panorama-engine/vendor/`（`99 §2.1 ⑰`）；本检查原地留在这里、只改住址。
    let vendor_md = Path::new("../panorama-engine/vendor/code-picture-core/VENDOR.md");
    println!("cargo:rerun-if-changed={}", vendor_md.display());
    let Ok(text) = std::fs::read_to_string(vendor_md) else {
        return;
    };
    let (Some(pin), Some(up)) = (
        extract_backtick_after(&text, "vendored commit:"),
        extract_backtick_after(&text, "上游仓:"),
    ) else {
        return;
    };
    let up = Path::new(&up);
    if !up.join(".git").exists() {
        return; // 上游仓缺席 → no-op
    }
    let Ok(out) = std::process::Command::new("git")
        .arg("-C")
        .arg(up)
        .args([
            "log",
            "--oneline",
            &format!("{pin}..HEAD"),
            "--",
            // 只比对**真被 vendor 的内容**（src + Cargo.toml）；tests/ 不 vendor，
            // 上游只改 tests 的提交不该触发"过期"（审计建议收窄）。
            "crates/code-picture-core/src",
            "crates/code-picture-core/Cargo.toml",
        ])
        .output()
    else {
        return;
    };
    let n = out
        .stdout
        .split(|&b| b == b'\n')
        .filter(|l| !l.is_empty())
        .count();
    if n > 0 {
        println!(
            "cargo:warning=vendor code-picture-core 过期:上游 core 有 {n} 个未 re-vendor 的提交(pin={pin})。见 src/panorama-engine/vendor/code-picture-core/VENDOR.md 的 re-vendor 菜谱。"
        );
    }
}

/// 从含 `label` 的行里抠出**第一个反引号包裹**的内容（pin / 上游路径）。
fn extract_backtick_after(text: &str, label: &str) -> Option<String> {
    let line = text.lines().find(|l| l.contains(label))?;
    let after = &line[line.find(label)? + label.len()..];
    let start = after.find('`')? + 1;
    let rel_end = after[start..].find('`')?;
    Some(after[start..start + rel_end].to_string())
}

/// 从后端源码（[`backend_lib_rs`]，现打 `src/backend/lib.rs`）提取 `const BUILD_ID`，
/// emit 成编译期 env `BACKEND_BUILD_ID`，让 monitor 的 `EXPECTED_BACKEND_BUILD_ID`
/// 与内嵌二进制的 build_id **单一事实源**（SS-B：消除 F06 时的手工同步）。
///
/// 🔴 `19b`（09-19）：抠不到**不再退化成 `"unknown"`**，理由全文住
/// [`backend_source_build_id`]（一句话：那条退化只在恰好铺了字节的构建里才有人拦）。
fn emit_backend_build_id() {
    println!("cargo:rerun-if-changed={}", backend_lib_rs().display());
    // 🔴 〔步 9 · 09-19〕分派那一半也要登记 —— 否则改 `main.rs` 不触发重跑本 build 脚本，
    //    上面那条 mtime 安全网拿到的是**缓存过的旧值**，它就不响了。
    println!("cargo:rerun-if-changed={}", backend_main_rs().display());
    let build_id = backend_source_build_id();
    println!("cargo:rustc-env=BACKEND_BUILD_ID={build_id}");
    // 🔴 `K-R70`：身份戳的两个界标，**闭集的唯一住址在后端源码里** —— 这里只是把它
    // 搬过来（同上面那条 `BUILD_ID` 的既有机制），monitor 生产段一律 `env!` 取，不许再抄字面量。
    let (open, close) = backend_stamp_marks();
    println!("cargo:rustc-env=BACKEND_STAMP_OPEN={open}");
    println!("cargo:rustc-env=BACKEND_STAMP_CLOSE={close}");
    // F05a：本机后端的文件名是 `<stem>-<target-triple>`（Tauri `externalBin` 的规矩），
    // 而 std 里没有「当前 target triple」这个常量 —— 只有 build script 拿得到 `TARGET`。
    println!(
        "cargo:rustc-env=CCM_TARGET_TRIPLE={}",
        std::env::var("TARGET").unwrap_or_else(|_| "unknown-target".into())
    );
}

/// 从源码里抠出 `const BUILD_ID: &str = "<x>";` 的 `<x>`。
fn extract_build_id(src: &str) -> Option<String> {
    extract_str_const(src, "BUILD_ID")
}

/// 从源码里抠出 `const <名>: &str = "<x>";` 的 `<x>`（`extract_build_id` 的推广）。
fn extract_str_const(src: &str, name: &str) -> Option<String> {
    let needle = format!("const {name}");
    let line = src.lines().find(|l| l.contains(&needle))?;
    let start = line.find('"')? + 1;
    let rel_end = line[start..].find('"')?;
    Some(line[start..start + rel_end].to_string())
}

/// 🔴 `K-R70`：身份戳的两个界标 —— 从后端源码抠出来。
///
/// # 为什么不在这里写死这两个字面量
///
/// 闭集只许有一个住址（`brief` 13b）。两处手抄的界标，哪天后端那侧改了一个字符，
/// 这边**扫不到戳**⇒ 下面那两条内嵌路会以「这份二进制没有身份」panic ——
/// 那是一次响亮的假报警，而真病是量具与被测对象脱钩。抠源码则结构上不会脱钩。
///
/// # 🔴 `19b`（09-19）：抠不到 ＝ **构建当场失败**，与 [`backend_source_build_id`] 同一条理由
///
/// 〔墓碑，本段原话逐字：「抠不到时给一对**空串**：`bytes_build_id` 见空串直接答 `None`，
///  于是内嵌路走「扫不出身份」那一支并把这里的失败逐字印进 panic 文案（比静默用一个错界标
///  去扫好）」。**它对『恰好铺了字节』那种构建是真的，对别的构建是假的** —— 平常每一次
///  `cargo build` 既不铺字节也不 panic，两个界标就这么带着**空串**被 emit 成
///  `BACKEND_STAMP_OPEN` / `BACKEND_STAMP_CLOSE`，而运行期那一侧
///  （`sftp::bytes_carry_build_stamp`）拿空界标去扫，**对任何字节都答不出身份**。
///  ⇒ 与 `"unknown"` 同族：一个不会参与比较、却让每一次判定都落空的兜底值。〕
///
/// ⚠ 这里 panic 的代价说清楚：后端源码读不到 / 界标改名，**整个 monitor 编不过**。
/// 那是刻意的方向 —— 编不过是一条响亮的失败，而「编过了但发出去的东西问不出身份」
/// 是 `v2.19–v2.22` 那批安装包的形状。
fn backend_stamp_marks() -> (String, String) {
    let p = backend_lib_rs();
    // ⚠ 读不到也是**响亮的失败**，不给空串 —— 空串会让下面两条抽取各自 panic，
    //   诊断却指向「界标不见了」，而真病是「这份源码根本读不到」。归因错了比失败本身贵。
    let src = std::fs::read_to_string(&p).unwrap_or_else(|e| {
        panic!(
            "读不到后端源码 `src/bridge/{}`（{e}）—— 身份戳界标与 `BUILD_ID` 都没有来源了。\
             住址是本文件的 `backend_lib_rs()`（全仓唯一一处）。",
            p.display()
        )
    });
    let mark = |name: &str| {
        extract_str_const(&src, name).unwrap_or_else(|| {
            panic!(
                "抠不到后端源码的 `const {name}` —— 读的是 `src/bridge/{}`。\n\
                 🔴 **这一步刻意没有兜底值**：给它一个空串，`bytes_build_id` 会对**任何**\
                 字节都答「问不出身份」，而那条失败只在恰好铺了字节的构建里才有人拦 ——\
                 平常的 `cargo build` 会把一对空界标烤进 exe，运行期的身份判定从此恒假。\n\
                 出路：界标的唯一住址是 `src/backend/lib.rs` 的 `BUILD_STAMP_OPEN` /\
                 `BUILD_STAMP_CLOSE`，改了名就同拍改本函数。",
                p.display()
            )
        })
    };
    (mark("BUILD_STAMP_OPEN"), mark("BUILD_STAMP_CLOSE"))
}

/// 🔴 `K-R70`：**从一份二进制的字节里问出它是谁** —— 不看它旁边任何文件。
///
/// 返回去重后的全部取值：**恰好一个**才是可用的身份；0 个 = 这份字节没有戳
/// （不是我们编的 / 太旧 / 被改过），多个 = 身份不唯一，两种都不许当成答案。
///
/// # 它取代了什么（这一段是本函数存在的全部理由）
///
/// 在它之前，三个载体的身份来自**旁边那个 `.build_id` 文本文件**。而那个文件是
/// `release.yml` 用 `Select-String … 'const BUILD_ID'` 从**源码**抠出来写的
/// ⇒ 三个载体的清单**恒等**，而恒等的东西一格证据都不提供（`K-R68` 摸底 ·
/// `DECISIONS.md#R26` 裁定零：「PM 那句『有 build_id 就不用靠推，可以直接比』是错的，
/// 它把标签当成了指纹」）。
///
/// 旧代码的头注逐字记着为什么当初选了旁挂清单：「编译器可把 BUILD_ID 优化成立即数指令
/// （字符串在字节里**不连续**），运行时 `bytes_contain` 启发式会误拒正品二进制」。
/// 那句话对**当时那个被测对象**是真的。今天不成立的原因是后端那侧换了东西：
/// `CC_MONITOR_BUILD_STAMP` 是一个 `#[used]` 的 `static [u8; N]` ——
/// **有地址、进 `.rodata`、字节按定义连续**，拆不成立即数。
fn bytes_build_id(bytes: &[u8], open: &str, close: &str) -> Option<String> {
    if open.is_empty() || close.is_empty() {
        return None;
    }
    let (o, c) = (open.as_bytes(), close.as_bytes());
    let mut found: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i + o.len() <= bytes.len() {
        if &bytes[i..i + o.len()] == o {
            let rest = &bytes[i + o.len()..];
            let win = &rest[..rest.len().min(96)];
            if let Some(e) = win.windows(c.len()).position(|w| w == c) {
                if let Ok(s) = std::str::from_utf8(&win[..e]) {
                    // 空串 / 怪字符一律不收：两个界标挨着放在 `.rodata` 里就是空串那一形，
                    // 收它会把「有几个身份」这个读数变假（backend 侧同一条纪律）。
                    if !s.is_empty()
                        && s.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_'))
                    {
                        found.push(s.to_string());
                    }
                }
            }
            i += o.len();
        } else {
            i += 1;
        }
    }
    found.sort();
    found.dedup();
    match found.len() {
        1 => Some(found.remove(0)),
        _ => None,
    }
}

/// 读一份二进制并问它是谁。读不到 / 扫不出都给 `None`（调用方各自决定怎么说）。
fn file_build_id(p: &Path, open: &str, close: &str) -> Option<String> {
    let bytes = std::fs::read(p).ok()?;
    bytes_build_id(&bytes, open, close)
}

/// F66（#58③）：从后端源码提取 `const CAPABILITIES`，emit 成编译期 env
/// `BACKEND_CAPABILITIES`（逗号分隔），让 monitor 的 `embedded_backend_capabilities()` 与
/// backend 声明的能力**单一事实源**——同 build_id 的 SS-B，杜绝手工同步债（审计 B1/S1：
/// 否则两份手抄常量漂移时，乐观路径可能声明当前后端不剥离的 flag → §26 死循环窄窗）。
fn emit_backend_capabilities() {
    let main_rs = backend_lib_rs();
    // rerun-if-changed 已由 emit_backend_build_id 对同一文件登记，无需重复。
    let caps = std::fs::read_to_string(&main_rs)
        .ok()
        .and_then(|s| extract_capabilities(&s))
        .unwrap_or_default();
    println!("cargo:rustc-env=BACKEND_CAPABILITIES={caps}");
}

/// 从源码里抠出 `const CAPABILITIES: &[&str] = &["a", "b"];` 的所有字符串，逗号拼接
/// （`a,b`）。**取 `=` 右侧再抠数组**——否则 `line.find('[')` 会命中类型标注 `&[&str]`
/// 的 `[`（里面 `&str` 无引号 → 抠成空，此坑由 `embedded_capabilities_single_source_wired`
/// 测试抓出）。
///
/// 🔴 **射程写清楚，别让下一个人读宽**〔`K-R61` 09-11 现打〕：本函数只喂
/// [`backend_lib_rs`] 那一个文件，抠的是 backend **流模式**那个 `CAPABILITIES`
/// （现打逐字 `const CAPABILITIES: &[&str] = &["bg", "tail-only"];`，`lib.rs` 里
/// 含这个串的行**恰好 1 行**）。
/// 〔订正 `19b` 09-19：这两处原写 `main.rs` —— 步 9 把这一族搬进库面时漏改了这段头注，
///  而它点名的是**本函数真读的那份文件**，指错就等于把读者引到一份抠不出东西的住址。〕
///
/// ⚠ 仓里**还有两个同名常量**，本函数一个都盖不到：
/// `src/backend/control/ccm/mod.rs`（`ccm` 的能力 token）与
/// `src/backend/agents/fake/mod.rs`（假 agent 的）。
/// `K-R61` 往前者加了一个 token，实测 `BACKEND_CAPABILITIES` **逐字不变**（仍是 `bg,tail-only`）
/// —— 这是量出来的，不是推的。谁在数 `ccm` 那一份，清单住它自己的头注。
fn extract_capabilities(src: &str) -> Option<String> {
    let line = src.lines().find(|l| l.contains("const CAPABILITIES"))?;
    let rhs = &line[line.find('=')? + 1..]; // 跳过 `: &[&str]` 类型标注里的 `[`
    let inner_start = rhs.find('[')? + 1;
    let inner_end = rhs[inner_start..].find(']')? + inner_start;
    let inner = &rhs[inner_start..inner_end];
    let mut tokens = Vec::new();
    let mut rest = inner;
    while let Some(q1) = rest.find('"') {
        let after = &rest[q1 + 1..];
        let q2 = after.find('"')?;
        tokens.push(&after[..q2]);
        rest = &after[q2 + 1..];
    }
    Some(tokens.join(","))
}

/// 把交叉编译好的 musl backend 二进制
/// （`src/bridge/embedded-backends/cc-monitor-backend-<arch>`）复制进 OUT_DIR 并置
/// `embedded_backends` cfg；任一缺失则不置 cfg（〔DP1〕`byte_table::pick` 对 Linux 两格返回 None → 部署那一步说「这一版没带」）。
///
/// 🔴 〔步 `19c` · 09-19〕**这两份字节从哪来，只有两个地方**：发版那趟是 `release.yml` 的
/// `Cross-compile backend for both musl targets`，本机那趟是 [`REEMBED_CMD`]，
/// 两者**逐字同一条配方**（`K-R124` ⑬b 两向对拍）。
/// 〔墓碑，本段原话逐字：「二进制由 `cargo zigbuild --target *-unknown-linux-musl` 产出后
///  放进 `embedded-backends/`（见 doc/REMOTE-PHASE0-DEPLOY 的 F08b 段）」——
///  那是**半条配方**（漏了 `--release --locked`），照它敲出来的字节不是发版那份。
///  ⚠ 那个文档**还在**，只是路径陈了：今天它住 `src/doc/REMOTE-PHASE0-DEPLOY.md`
///  （`doc/` 这个前缀是仓库重组之前的写法）。那份文档里的旧路径**本拍没核**
///  —— `tests/evidence/K-W4-D1-rename-surface.md` 逐字登记着它有 8 处旧路径待查，
///  归那一件，不归本拍。〕
fn embed_backends() {
    // 允许自定义 cfg（Rust 1.80+ unexpected_cfgs 检查）。
    println!("cargo:rustc-check-cfg=cfg(embedded_backends)");
    let out = std::env::var("OUT_DIR").expect("OUT_DIR");
    let dir = Path::new(EMBEDDED_BACKENDS_DIR);
    // staleness 安全网（审计 SUGGESTION-1）：backend 源码 mtime，用于提示「bump BUILD_ID 后
    // 忘了 re-zigbuild」——否则内嵌旧二进制 build_id 与源码不符 → 永不收敛的重复部署。
    // 🔴 〔步 9 · 09-19〕**两份都看，取较新的那个。** 身份搬去了 `lib.rs`，而分派仍在
    //    `main.rs` —— 只看一份，改另一份时这张安全网当场变瞎。
    // 🔴 〔步 19c · 09-19〕**它是安全网，不是机制** —— 原话里「唯一拦截点」那半句已经不成立了：
    //    ① 它只在**已经出事之后**说话；② 它说「旧了」却不说怎么办；③ 它是一条 `cargo:warning`，
    //    在几百行输出里滚过去。机制那一半住 [`REEMBED_CMD`]：一条真跑得起来的命令，
    //    外加 `--check` —— 「盘上的字节与源码对不对得上」当场用相等断言回答，
    //    不必等谁去编整棵树。⚠ 安全网**一条没撤**（判据 `K-R124` ⑬f 钉着这两份源码都还在被看）。
    let src_mtime = [backend_lib_rs(), backend_main_rs()]
        .iter()
        .filter_map(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok())
        .max();
    // U-1：mtime 之外再取 **build_id 字符串本身**，用于下面那条硬校验（mtime 漏掉过一次真事故）。
    let source_build_id = backend_source_build_id();
    let (stamp_open, stamp_close) = backend_stamp_marks();
    let mut all = true;
    for arch in ["x86_64", "aarch64"] {
        let src = dir.join(format!("cc-monitor-backend-{arch}"));
        println!("cargo:rerun-if-changed={}", src.display());
        // 🔴 **`K-R70`（09-12）：身份改从这份字节自己里读，旁边那个 `.build_id` 不再有话语权。**
        //
        // 〔墓碑 —— 本处原话逐字：「Batch9 E2E 发现：编译器可把 BUILD_ID 优化成立即数指令
        //  （字符串在字节里**不连续**），运行时 bytes_contain 启发式会误拒正品二进制。
        //  根治 = 旁挂 `.build_id` 清单文件（构建放置二进制时一并写入，= 字节的真实身份）」。
        //  **那段话对当时那个被测对象是真的**，它错在最后半句：旁挂清单**不是**字节的真实身份，
        //  它是 `release.yml` 从**源码常量**抠出来写的一张标签 ⇒ 三个载体的清单恒等，
        //  而恒等的东西一格证据都不提供（`K-R68` 摸底 · `DECISIONS.md#R26` 裁定零）。〕
        //
        // 今天「扫字节」不再是启发式：backend 侧的 `CC_MONITOR_BUILD_STAMP` 是一个 `#[used]`
        // 的 `static [u8; N]`，有地址、进 `.rodata`、字节按定义连续，拆不成立即数。
        // ⇒ 这里问的是**这份二进制**，不是它旁边的谁。
        // ⚠ 顺带没了一条 `rerun-if-changed`：清单不再是输入 ⇒ 动它不会重建，也不会改身份。
        //   **那正是 `KR70D1` 的死值验**（改旁文件而二进制不动 ⇒ 产品给的身份不许跟着变）。
        let embedded_id = file_build_id(&src, &stamp_open, &stamp_close).unwrap_or_default();
        println!(
            "cargo:rustc-env=BACKEND_EMBEDDED_ID_{}={embedded_id}",
            arch.to_uppercase()
        );
        if src.exists() {
            if let (Ok(bin_mtime), Some(sm)) = (
                std::fs::metadata(&src).and_then(|m| m.modified()),
                src_mtime,
            ) {
                if bin_mtime < sm {
                    println!(
                        "cargo:warning=内嵌 backend {arch} 比后端源码旧——若刚 bump 了 BUILD_ID，同拍跑 `{REEMBED_CMD}`（它重编两个 musl arch 并铺回落点；`{REEMBED_CMD} --check` 只问对不对得上，不产字节）"
                    );
                }
            }
            // ★ U-1（2026-08-01）：**mtime 不够，要比 build_id 字符串本身。**
            //
            // 上面那条 mtime 警告漏掉了真实发生过的一次：源码 bump 到 `p1v-attachable`，
            // 而 `embedded-backends/*.build_id` 还是 `p1u-fork-session`（内嵌二进制**更新**
            // 但内容是旧版）。这就是发版纪律里说的「半 bump 比不 bump 更糟」。
            //
            // 为什么必须 **panic** 而不是 warning：monitor 判「远端该不该换后端」的唯一
            // 判据是 `reported_build_id != EXPECTED_BACKEND_BUILD_ID`。内嵌的是 p1u、期望的是
            // p1v ⇒ 装上去之后**永远判 stale** ⇒ **无限重装循环**。这不是「慢一点」，是坏的。
            // 出路只有两条：重跑 zigbuild 生成对得上的二进制，或删掉 embedded-backends/
            // （那样自动部署诚实地关掉，不会装一个注定被判过期的东西）。
            // ★ Phase D 审计 I3：**问不出身份不能只是 warning** —— 那正是本轮硬校验的静默旁路，
            // 而且是最危险的场景（有人手工塞了个陈旧二进制）恰好绕开校验。
            // 〔`K-R70` 订正这一段的后半句：原话是「`sftp.rs` 运行时对 x86_64 的身份识别
            //  **只能靠清单**」——今天不是了，运行期那一侧同样扫字节（`sftp::bytes_carry_build_stamp`）。〕
            // 🔴 `19b`（09-19）：这里原本还有一条 `unwrap_or_else(panic!)` ——
            // 〔墓碑，原话逐字：「抠不到源码 build_id ⇒ 单源链条已断，`BACKEND_BUILD_ID`
            //  此刻是 "unknown"，装出去每台远端都会被判 StaleBuild。比 mismatch 更该拦。」〕
            // 它说的是对的，**但它站错了地方**：它在 `src.exists()` 的里面，
            // 而「抠不到」与「铺没铺字节」毫无关系 ⇒ 没铺字节的构建它一次都不响。
            // 今天那条 panic 搬进了 `backend_source_build_id()`（取值那一跳，所有构建形态都过它）
            // ⇒ 走到这里时 `expected` 在构造上已经是一个真身份，本处不再重复一遍。
            let expected = source_build_id.as_str();
            if embedded_id.is_empty() {
                panic!(
                    "内嵌 backend {arch}（src/bridge/{}）**问不出身份** —— \
                     在它的字节里找不到恰好一个 `{stamp_open}…{stamp_close}` 身份戳。\n\
                     可能是：① 它不是这套源码编出来的（太旧 —— `p2f-build-stamp` 之前的\
                     backend 根本没有戳）；② 它被改过 / 截断了。\n\
                     ⚠ **不会是「界标抠错了」**：`19b` 起 `backend_stamp_marks()` 抠不到就当场 panic\
                     ⇒ 走到这里时界标必然非空（现打 open=`{stamp_open}` close=`{stamp_close}`）。\n\
                     🔴 **别去写一个 `.build_id` 旁文件来糊它** —— `K-R70` 之后没有任何东西\
                     读那个文件了，写一份只是把标签换个地方抄。\n\
                     出路二选一，**两条都是同一条命令**（步 `19c`）：\n\
                     ① `{REEMBED_CMD}` —— 重编两个 musl arch 并铺回落点；\n\
                     ② `{REEMBED_CMD} --clean` —— 删掉落点，自动部署诚实关闭，编译立刻恢复。",
                    src.display()
                );
            }
            // 🔴 〔步 `19c` · 09-19〕**这里原来手抄着第二条产字节的配方，已经撤掉。**
            //
            // 〔墓碑，原话的骨架逐字：「① 重编。两步都在 `src/backend/` 目录下跑：
            //  `cargo build --release --target {arch}-unknown-linux-musl --config
            //  'target.{arch}-unknown-linux-musl.linker="rust-lld"'` ＋ `cp target/…`」，
            //  后面跟着一段按 arch 分岔的 `c_cross_note`〔散文墓碑〕（08-25 实测：aarch64 不给
            //  `zig cc` 就 rc=101，死在 `cc-rs: failed to find tool
            //  "aarch64-linux-musl-gcc"`），以及它自己的一句自陈：「这样编出来的形态与
            //  发版 CI 的 zigbuild 产物**不同** —— static-pie / 未 strip / 不同 rustc」，
            //  最后一句「发版 CI 走的是 `cargo zigbuild`……**那条路多半仍通，但我没量过**」。〕
            //
            // **它为什么必须撤**：那是「本机怎么重编这两份字节」的**第二个住址**，
            // 而且与发版那趟**不是同一条路**。09-18 那次四路 agent 编不过，真正救活那棵树的
            // 是 `cargo zigbuild --release --locked`（＝发版那趟那条），**不是这段文案**。
            // ⇒ 出路收成 [`REEMBED_CMD`] 一条；那条命令与 `release.yml` 的
            //   `Cross-compile backend for both musl targets` 逐字同源（`K-R124` ⑬b 两向对拍）。
            // ⚠ **那段 08-25 的实测读数一个字没丢**，它搬进了 `tests/scripts/re-embed.sh`
            //   的头注 —— 它现在是「那条命令为什么必须用 zigbuild、不能换回 rust-lld」的理由，
            //   写在**跑那条命令的地方**，而不是写在一段没人会照着敲的 panic 文案里。
            if embedded_id != expected {
                panic!(
                    "内嵌 backend {arch} 的 build_id 是 `{embedded_id}`，而后端源码是 `{expected}` —— \
                     **半 bump**。装上去会被 monitor 永远判 StaleBuild 并无限重装。\n\
                     🔴 这是 `BUILD_ID` bump 的**同拍债**（步 `19c`）：bump 了源码，\
                     而盘上这两份字节还是上一版编的。\n\
                     出路二选一，**两条都是同一条命令**：\n\
                     ① `{REEMBED_CMD}`\n\
                        —— 两个 musl arch 各一趟 `cargo zigbuild --release --locked`\
                        （与发版那趟逐字同一条配方），编完铺回落点并当场核一遍身份。\n\
                     ② `{REEMBED_CMD} --clean`\n\
                        —— 删掉落点：自动部署诚实关闭，编译立刻恢复（落点已被 gitignore，删除零代价）。\n\
                     ⚠ **诚实边界**：①买到的是「开发期自洽 ＋ 裸 exe 恢复部署能力」，\
                     **不等于**发版那一拍办完了（本机的 zigbuild 形态与发版 CI 不同，\
                     逐条写在那条命令自己的头注里）。"
                );
            }
            let dst = Path::new(&out).join(format!("backend-{arch}"));
            std::fs::copy(&src, &dst).expect("copy embedded backend binary");
        } else {
            // U-1：原来这里**连 warn 都没有** —— 缺二进制就静默不置 cfg，
            // 取字节那一口返回 None、远端自动部署整个消失而无人知晓。
            // 那正是 v2.19–v2.22 那批安装包的事故形状（见 release.yml 的账）。
            println!(
                "cargo:warning=缺少内嵌 backend {arch}（{}）——远端自动部署将关闭（embedded_backends cfg 不置）。要它就跑 `{REEMBED_CMD}`",
                src.display()
            );
            all = false;
        }
    }
    if all {
        println!("cargo:rustc-cfg=embedded_backends");
    }
}

/// 〔RM1c · 第四波〕把交叉编译好的**全景小程序**（`src/panorama-engine`，只装代码全景引擎的
/// 独立二进制，用户 09-24 V108 选 B）两个 musl arch 复制进 OUT_DIR，置 `embedded_panoramas` cfg；
/// 任一缺失 ⇒ 不置 cfg ＋ **可见的** warning（`byte_table::choose` 那一格答「这一版没带」〔TL1：原先点的是 `panorama_bytes` 里一个按两个词取字节的函数，删了〕，
/// 远端全景那一台就只能报「这台机器上还没装」）。
///
/// # 与 [`embed_backends`] 同一个落点、同一条配方，**不同的一件事**
///
/// - 落点同是 [`EMBEDDED_BACKENDS_DIR`]（远端部署产物那一类；已被 gitignore 挡着），
///   文件名 `cc-monitor-panorama-<arch>`。产它的同样只有两条路：发版那趟 `release.yml` 的
///   `Cross-compile panorama for both musl targets`，本机那趟 [`REEMBED_CMD`]（配方逐字同源）。
/// - **没有身份戳、没有半 bump 守卫**：它不随 `BUILD_ID` 走（后端的协议面与它无关），
///   「这份字节与源码是不是同一代」由 [`REEMBED_CMD`] 的 `--check` 真起一趟 `--probe`、
///   比它报的能力表与源码那张 op 表来答（能跑的那个 arch）。
/// - ⚠ **缺席不 panic**：它是「只传给开过远端全景的机器」的可选件，缺了只是远端全景那一格关着，
///   不是「装出去就无限重装」那种坏。
fn embed_panoramas() {
    println!("cargo:rustc-check-cfg=cfg(embedded_panoramas)");
    let out = std::env::var("OUT_DIR").expect("OUT_DIR");
    let dir = Path::new(EMBEDDED_BACKENDS_DIR);
    let mut all = true;
    for arch in ["x86_64", "aarch64"] {
        let src = dir.join(format!("cc-monitor-panorama-{arch}"));
        println!("cargo:rerun-if-changed={}", src.display());
        if src.exists() {
            let dst = Path::new(&out).join(format!("panorama-{arch}"));
            std::fs::copy(&src, &dst).expect("copy embedded panorama binary");
        } else {
            println!(
                "cargo:warning=缺少内嵌全景小程序 {arch}（{}）——远端代码全景那一格将没有字节可推（embedded_panoramas cfg 不置）。要它就跑 `{REEMBED_CMD}`",
                src.display()
            );
            all = false;
        }
    }
    if all {
        println!("cargo:rustc-cfg=embedded_panoramas");
    }
}

/// 目标平台的可执行后缀。
///
/// 🔴 **由 `TARGET` 算，不许用 `std::env::consts::EXE_SUFFIX`** —— build script 跑在
/// **构建机**上（交叉编译 Windows 产物时那台是 Linux）⇒ 那个常量给的是**宿主**的后缀，
/// 而这里要的是**目标**的。同一个坑 `emit_backend_build_id` 里那条 `CCM_TARGET_TRIPLE`
/// 已经踩过一次（「std 里没有『当前 target triple』这个常量，只有 build script 拿得到 `TARGET`」）。
fn target_exe_suffix(target: &str) -> &'static str {
    if target.contains("windows") {
        ".exe"
    } else {
        ""
    }
}

/// 本机内嵌后端的落点 —— `release.yml` 那一步按同一条路径铺，
/// `byte_table.rs`（〔DP1〕全仓唯一的取字节口）用**同一条路径的字面量** `include_bytes!` 它。
///
/// # 🔴 为什么是**定死的名字**，而不是像 `embedded-backends/` 那样把 triple 编进文件名
///
/// 因为消费侧那个 `include_bytes!` 的参数**必须是字面量**：
/// `cross_half_edge_registry::every_non_literal_include_is_registered_with_a_reason` 默认拒绝
/// 非字面量的 `include_*!`，而它的登记表**不在本件写区**。
/// ⇒ 名字定死、**用一个旁挂清单把 triple 记下来再核**（`.target`，下面那条硬校验）。
/// 这一换其实更强：文件名是**没人核的约定**，清单是**每次构建都核的断言**。
///
/// # 🔴 为什么不铺进已有的 `embedded-backends/`
///
/// 那个目录名是 `local_backend_host_tests.rs::every_test_that_starts_the_real_backend_demands_a_private_tmux`
/// 认「谁会起真后端」的**来历串之一**。字面量路径写进 `local_backend.rs` 的生产段，
/// 会把整段生产代码拖进那条判据的人群（实测：人群里多出一条 `local_backend.rs::default`，
/// 而那条连测试都不是；同时 `the_local_backend_host_really_registers_an_inbound_client`
/// 因为切块规则一起掉出人群，那条判据的地板当场红）。
/// ⇒ 换一个目录，两条判据都不被误伤，而且它本来也不是同一类东西
/// （那边是**远端部署产物**，这边是**本机原生**）。
/// ✅ **那笔欠账 09-10 当天就还了**：`src/bridge/.gitignore` 里已经有 `/native-backend/`。
/// 〔墓碑 —— 本行原话逐字：「⚠ **一条如实记着的欠账**：`src/bridge/.gitignore` 里还没有这一行，
///  而它**不在本件写区** —— 见 `K-R42` 件文件的上报口。」它写下时是真的，
///  补上那一行的那一拍（`track/alarm`）当场把它变成了假话，**而那一拍也点名了它**。〕
/// ★ 记这一笔的形状：**造了债的那一拍自己报出来，并分清它是新债还是存量** ——
/// 这条正是**它自己造的新债**，不是从别处继承来的。
const NATIVE_BACKEND_DIR: &str = "native-backend";
const NATIVE_BACKEND_FILE: &str = "cc-monitor-native";

/// `K-R42`：**把「本机后端」也内嵌进 exe**，让裸 `monitor.exe` 自己带得上一份。
///
/// # 它与 `embed_backends` 是两件事，别合并
///
/// | | `embed_backends`（远端那条） | 本函数（本机这条） |
/// |---|---|---|
/// | 内嵌什么 | `cc-monitor-backend-{x86_64,aarch64}`，**musl Linux**，按 **arch** 分派 | `cc-monitor-native-<target triple>`，**当前 TARGET 的原生二进制** |
/// | 给谁用 | SFTP 推到远端主机（远端就是 Linux ⇒ musl 是对的） | 本机自释放（`local_backend::start_or_extract`） |
/// | 认不认 OS | **不认**（只看 arch） | **由 TARGET 定死**，编译期就选好了 |
///
/// 🔴 **「不认 OS」正是 09-10 那个真机读数的根因**：`local_backend_host.rs` 里那道
/// `if cfg!(target_os = "linux")` 的闸（D 阶段补审 08-11 加的）挡的就是
/// 「往 Windows 上释放一个 Linux ELF、然后报告『已起』」——
/// 那道闸**是对的**，它挡住的是**没有 Windows 版可嵌**这件事，不是「自释放这条路不该走」。
/// ⇒ 本函数补的就是那个缺口：**给 Windows 一份能跑的字节**。
/// 缺口本身一年前就登记在 `devbench/ROADMAP.md` 的 `5f⁗` 上（逐字：「要 release 流程产
/// Windows backend 并内嵌」）—— 今天才第一次有人在真机上撞到它。
///
/// # 为什么这里 panic 而不是 warning（与 `embed_backends` 同一条理由，射程不同）
///
/// 远端那条怕的是「装上去永远判 stale ⇒ 无限重装」。本机这条**不会**无限重装
/// （〔E2〕换版照 HX2 D-b「盘上的比我旧才换」，见 `local_backend::extract_embedded_to`），但它会
/// **把一份贴错标签的二进制留在用户机器上**：清单说它是 X，字节其实是 Y ⇒
/// `BACKEND_CAPABILITIES` 那套乐观路径按 X 谈能力、跑起来的是 Y。
/// ⇒ 半 bump 一样比不 bump 更糟，一样当场拦下。
///
/// # 缺席 = 不置 cfg + **可见的** warning
///
/// 沿用 `embed_backends` 的 U-1 那条账：**静默不置 cfg 正是 v2.19–v2.22 那批安装包的事故形状**。
/// 缺席时 `byte_table::pick` 在这一格给不出本机原生那份（〔DP1〕Linux 构建上还有 musl 那份可给），自释放那条路说「这一版没带」。
fn embed_native_backend() {
    // 允许自定义 cfg（Rust 1.80+ unexpected_cfgs 检查）。
    println!("cargo:rustc-check-cfg=cfg(embedded_native_backend)");
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown-target".into());
    // ⚠ **无条件 emit**：`local_backend::local_ccm_entry_name` 是**无条件**的生产代码，
    // 它 `env!` 这个名字 —— 只在某些分支 emit 会让别的分支编不过。
    println!(
        "cargo:rustc-env=CCM_TARGET_EXE_SUFFIX={}",
        target_exe_suffix(&target)
    );

    let dir = Path::new(NATIVE_BACKEND_DIR);
    let src = dir.join(NATIVE_BACKEND_FILE);
    let target_manifest = dir.join(format!("{NATIVE_BACKEND_FILE}.target"));
    for f in [&src, &target_manifest] {
        println!("cargo:rerun-if-changed={}", f.display());
    }
    // 🔴 **`K-R70`：身份从这份字节自己里读**（同 `embed_backends` 那条，理由全文住
    // `bytes_build_id` 的头注）。原来读的是旁边那个 `cc-monitor-native.build_id` 文本文件 ——
    // 那是标签不是指纹。⚠ 旁边那个 `.target` **留着**：它答的是「给哪个平台编的」，
    // 不是「你是谁」，本件不碰（如实登记为同族剩余，见件文件 `§8`）。
    let (stamp_open, stamp_close) = backend_stamp_marks();
    let embedded_id = file_build_id(&src, &stamp_open, &stamp_close).unwrap_or_default();
    // ⚠ **无条件 emit**（同上那条理由）。
    println!("cargo:rustc-env=BACKEND_NATIVE_ID={embedded_id}");

    // 🔴〔2026-09-24 · B1〕**缺席不再被说成「开发构建里的正常情况」。**
    //
    // 〔墓碑，本条 warning 原话逐字：「没有本机内嵌后端 ——**裸可执行文件起不了本机后端**，
    //  只有安装包那份带本机后端的能起。开发构建里这是正常的；……」。**后半句违反 `D11`**
    //  （用户逐字「不要退路 / 所有东西都不要假设后端没起来」）：它把「开发构建上窗口
    //  『走后端』那条主路一直在走退路」写成了常态，于是本机上验证任何后端功能都先天带洞。〕
    //
    // ⇒ 修在**构建这一侧**，运行期一个分支都没加：开发构建的路径是明写的、而且与发版
    //   **同一条** —— 那条命令的 `--native` 编出本机原生后端铺进本目录，本函数照常内嵌，
    //   运行期按原路自释放再起。「起不起得来」由它的 `--check-dev` **真起一趟**来判
    //   （读那份字节 stdout 的第一行，必须是一帧自报源码 `BUILD_ID` 的 hello；缺席即红）。
    // ⚠ 这里**仍然是 warning 不是 panic**，理由是硬的、不是手软：门禁与 CI 上每一格
    //   `cargo check/test/clippy` 都在**没铺字节**的树上跑（字节不进仓），panic 会把它们全拖红，
    //   而那几处不在本件写区。**判词住那条命令上，不住这句 warning 上。**
    if !src.exists() {
        println!(
            "cargo:warning=没有本机内嵌后端（src/bridge/{}）⇒ **这一趟编出来的可执行文件起不了本机后端**\
             （D11：这不是开发构建的正常情况）。开发构建要它：`{REEMBED_CMD} --native`；\
             判它起不起得来：`{REEMBED_CMD} --check-dev`。\
             发版构建里出现这一行 = 那一版的裸 exe 又回到 09-10 那个读数（0 个本机后端进程）。",
            src.display()
        );
        return;
    }
    // ── ① 它是**给这个 target 编的**吗 ─────────────────────────────────────
    //
    // 名字定死（理由见 `NATIVE_BACKEND_DIR` 头注）⇒ 「这份字节属于哪个平台」**只能靠这个清单**。
    // 缺清单 / 对不上都当场拦：放它过去等于把一个别的平台的二进制内嵌进来，
    // 而那正是 08-11 补审逮到的那个阻塞级缺陷（往 Windows 上释放 Linux ELF 再报「已起」）。
    let staged_target = read_trimmed(&target_manifest);
    if staged_target != target {
        panic!(
            "本机内嵌后端（src/bridge/{}）是给 `{}` 编的，而这一趟的 TARGET 是 `{target}`。\n\
             （清单读作 `{staged_target}`；空串 = 根本没有 `{}.target` 这个文件。）\n\
             内嵌一个别的平台的二进制 = 释放到用户盘上再起，起不来 —— \
             而 `Resolved::Found` 会先撒一次谎（它只证明文件落地了）。\n\
             出路二选一，**两条都是同一条命令**（步 `19c`）：\
             ① `{REEMBED_CMD} --native` —— 为这一趟的 TARGET 重编并重铺那两个文件；\
             ② `{REEMBED_CMD} --clean` —— 删掉落点，自释放诚实关闭，编译立刻恢复。",
            src.display(),
            staged_target,
            NATIVE_BACKEND_FILE,
        );
    }
    // ── ② 它的 build_id 与后端源码对得上吗 ─────────────────────────────
    // 🔴 `19b`：同 `embed_backends` 那一处 —— 「抠不到」那条 panic 已经搬进
    // `backend_source_build_id()`（取值那一跳），本处不再抄一份射程更窄的副本。
    let expected = backend_source_build_id();
    if embedded_id.is_empty() {
        panic!(
            "本机内嵌后端（src/bridge/{}）**问不出身份** —— \
             在它的字节里找不到恰好一个 `{stamp_open}…{stamp_close}` 身份戳。\n\
             身份是它落到用户盘上时的文件名来源（`cc-monitor-backend-<build_id>`），\
             问不出就拼不出落点。\n\
             可能是：① 它不是这套源码编出来的（`p2f-build-stamp` 之前的后端没有戳）；\
             ② 被改过 / 截断；③ 界标抠失败（现打 open=`{stamp_open}` close=`{stamp_close}`）。\n\
             🔴 **别去补一个 `.build_id` 旁文件** —— `K-R70` 之后没人读它，那只是把标签换个地方抄。\n\
             出路二选一，**两条都是同一条命令**（步 `19c`）：`{REEMBED_CMD} --native` 重编重铺，\
             或 `{REEMBED_CMD} --clean` 删掉落点（自释放诚实关闭，编译立刻恢复）。",
            src.display(),
        );
    }
    if embedded_id != expected {
        panic!(
            "本机内嵌后端（src/bridge/{}）**的字节自报** `{embedded_id}`，而后端源码是 `{expected}` \
             —— **半 bump**。\n\
             这一份会以 `cc-monitor-backend-{embedded_id}` 之名落到用户盘上，而 monitor 这一侧\
             按 `{expected}` 谈能力：能力协商按源码谈、跑起来的是另一个。\n\
             🔴 这是 `BUILD_ID` bump 的**同拍债**（步 `19c`），与远端那两份同一形。\
             出路二选一，**两条都是同一条命令**：\
             ① `{REEMBED_CMD} --native` —— `cargo build --release --locked` 重编，\
             产物铺成 `src/bridge/{}` 那两个文件（二进制 ＋ `.target`）；\
             ② `{REEMBED_CMD} --clean` —— 删掉落点，自释放诚实关闭，编译立刻恢复。",
            src.display(),
            NATIVE_BACKEND_DIR
        );
    }
    // ⚠ **不往 `OUT_DIR` 拷一份**（`embed_backends` 那条是拷的）——消费侧那个 `include_bytes!`
    // 直接按**字面量相对路径**读得到，而拷贝会让本函数变成一个**写盘落点**，
    // 那要在 `write_site_registry::WRITE_SITES` 里申报（`fs::copy(` 是它的 needle 之一），
    // 而那张表**不在本件写区**。少一次拷贝同时也少一条要申报的写点 —— 两头都更干净。
    println!("cargo:rustc-cfg=embedded_native_backend");
}

/// 〔RM1f〕本机原生全景小程序的文件名（落点目录同 [`NATIVE_BACKEND_DIR`]，旁挂 `.target` 清单同那一对）。
/// `byte_table.rs`（〔DP1〕全仓唯一的取字节口）用**同一条路径的字面量** `include_bytes!` 它
/// （四处同一个串：本常量 · 那个字面量 · `re-embed.sh --native` · `release.yml` 的 Windows 那一格 —— 判据对拍）。
const NATIVE_PANORAMA_FILE: &str = "cc-monitor-panorama";

/// 〔RM1f · V108 后半句「之后本机也走这条路、monitor 摘内嵌引擎」〕**把本机原生的全景小程序也内嵌进 exe**。
///
/// # 为什么要它
///
/// monitor 摘掉内嵌引擎之后，本机全景 = 「本机后端 → 插件口 → `cc-monitor-panorama`」。本机后端要在
/// `~/.cc-monitor/bin/` 找到一份**这台机器能跑的**小程序 ⇒ 字节得跟着 monitor 走（本机不经推送，
/// 由 monitor 放下来：`local_backend::place_local_panorama`）。Linux 本机用远端那两份 musl 就跑得起来
/// （`panorama_bytes::local_panorama_binary` 的第二个来源）；**Windows / macOS 本机没有**，只能按 `TARGET` 原生编一份。
///
/// # 形状与 [`embed_native_backend`] 同一套（理由逐字住那里，不抄第二份）
///
/// 定死名字（消费侧 `include_bytes!` 要字面量）· 旁挂 `.target` 清单（名字里没有 triple，只能靠它）·
/// 对不上当场 panic（内嵌一个别的平台的程序 = 放到用户盘上起不来）· 缺席 ⇒ 不置 cfg ＋ **可见的** warning。
/// ⚠ **没有身份戳、没有半 bump 守卫**：它不随 `BUILD_ID` 走（同 [`embed_panoramas`] 那一条）；
/// 「与源码是不是同一代」由 [`REEMBED_CMD`] 的 `--check` 真起一趟 `--probe` 比能力表来答。
/// ⚠ **不往 `OUT_DIR` 拷**（同 [`embed_native_backend`] 末尾那条：少一个要申报的写点）。
fn embed_native_panorama() {
    println!("cargo:rustc-check-cfg=cfg(embedded_native_panorama)");
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown-target".into());
    let dir = Path::new(NATIVE_BACKEND_DIR);
    let src = dir.join(NATIVE_PANORAMA_FILE);
    let target_manifest = dir.join(format!("{NATIVE_PANORAMA_FILE}.target"));
    for f in [&src, &target_manifest] {
        println!("cargo:rerun-if-changed={}", f.display());
    }
    if !src.exists() {
        println!(
            "cargo:warning=没有本机内嵌全景小程序（src/bridge/{}）⇒ 这一趟编出来的可执行文件在**非 Linux 本机**上\
             没有代码全景（Linux 本机用远端那两份 musl 字节，若铺了）。要它：`{REEMBED_CMD} --native`。",
            src.display()
        );
        return;
    }
    let staged_target = read_trimmed(&target_manifest);
    if staged_target != target {
        panic!(
            "本机内嵌全景小程序（src/bridge/{}）是给 `{}` 编的，而这一趟的 TARGET 是 `{target}`。\n\
             （清单读作 `{staged_target}`；空串 = 根本没有 `{}.target` 这个文件。）\n\
             内嵌一个别的平台的程序 = 放到用户盘上起不来。\
             出路二选一，**两条都是同一条命令**：① `{REEMBED_CMD} --native` 为这一趟的 TARGET 重编重铺；\
             ② `{REEMBED_CMD} --clean` 删掉落点（本机全景诚实关着，编译立刻恢复）。",
            src.display(),
            staged_target,
            NATIVE_PANORAMA_FILE,
        );
    }
    println!("cargo:rustc-cfg=embedded_native_panorama");
}

/// 读一份单值清单，读不到就给空串（「没有这个文件」与「文件是空的」在这里同义：都不可信）。
fn read_trimmed(p: &Path) -> String {
    std::fs::read_to_string(p)
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}
