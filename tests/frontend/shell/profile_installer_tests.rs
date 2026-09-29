//! 〔MIG-3a〕别名块那一半的判据随 `assets/aliases/block.rs` 进了后端（`tests/backend/assets/aliases/block_tests.rs`）；
//! 这里只剩**用户级 PATH** 那一格（本机后端的引导，`ccm_user_path_*`）与共享片段对后端落点（`relay_route_core::BACKEND_LANDING_REL`）的那一条。
use super::*;

use std::path::PathBuf;

/// panic 也要清 tempdir（`Drop` 不受 panic 影响）。
struct TmpDir(std::path::PathBuf);
impl Drop for TmpDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn tmpdir(tag: &str) -> TmpDir {
    let d = std::env::temp_dir().join(format!(
        "ccm-fence-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|x| x.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&d).expect("建 tempdir");
    TmpDir(d)
}

/// ★★ `KR132D2` 的第二半：**PATH 上写的那个目录，就是我们真放 `ccm` 下去的那个。**
///
/// # 为什么单独一条：上一条只证「这两处一致」，这一条证「它们对得上现实」
///
/// 上一条比的是**我们生成的那段命令**与后端落点那个常量（〔MIG-3b 续〕从前是足迹申报表）—— **两边同时改错**
/// 照样全绿。真落点住在**第三处**：`local_backend_host.rs` 里算 `extract_dir` 的那一行，
/// 它就是 `install_local_ccm_entry` 的 `dir` 实参。⇒ 这一条去读**那一行源码**。
///
/// # 诚实边界
///
/// 它按**源码文本**对拍，不是跑起来量的（跑起来要一台 Windows）。
/// 所以它认得的是「那一行长什么样」；有人改成用一个中间变量拼路径，
/// 它会**红**而不是静默放过 —— 那是有意的：这一格宁可误红逼人来看，
/// 也不要在「PATH 指向一个空目录」这件事上假绿。
///
/// # 死值验（`KR132D2` 刀④）
///
/// 把 `relay_route_core::BACKEND_LANDING_REL` 的目录改成别的 ⇒ 本条红
/// （而上一条**不红** —— 它是自洽的）。
#[test]
fn the_path_line_points_at_the_directory_we_really_install_ccm_into() {
    let dir = ccm_bin_dir_rel().expect("后端落点的目录");
    // `.cc-monitor/bin` ⇒ `.join(".cc-monitor").join("bin")` —— 真落点那一行的形状。
    let want: String = dir
        .split('/')
        .map(|seg| format!(".join(\"{seg}\")"))
        .collect::<Vec<_>>()
        .join("");

    // 真落点：`install_local_ccm_entry` 的 `dir` 实参是 `local_backend_host.rs` 算的 `extract_dir`。
    //
    // ⚠ **比之前先把空白抹掉**：`ccm_probe.rs` 那一处是
    // `h.join(".cc-monitor")\n            .join("bin")`（rustfmt 断的行）——
    // 按原文 `contains` 会**漏掉它**，而漏掉的那一形正是「判据够不着」，
    // 不是「那一处不存在」。这一刀是现打出来的：第一版就栽在这里。
    let squeeze = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    let want = squeeze(&want);
    let hosts: [(&str, &str); 2] = [
        (
            "local_backend_host.rs",
            include_str!("../../../src/frontend/shell/src/local_backend_host.rs"),
        ),
        (
            "ccm_probe.rs",
            include_str!("../../../src/frontend/shell/src/ccm_probe.rs"),
        ),
    ];
    for (name, raw) in hosts {
        let prod = squeeze(&guard_core::production_code(raw));
        assert!(
            prod.contains(&want),
            "`{name}` 的生产段里找不到 `{want}` —— 也就是说\
                 「PATH 上写的那个目录」与「盘上真放 ccm 的那个目录」已经分家了。\n\
                 PATH 那一侧现算自后端落点那个常量的 `{dir}`；\
                 要么那个常量改错了，要么真落点搬了家而这一侧没跟。\n\
                 ⚠ 指错目录与根本没补 PATH，在用户终端上是**同一个结果**（命令找不到）。"
        );
    }
}

/// ★★ `KR132D1` ＋〔`KR135D1` 09-15 **扩到「撤」那一侧**〕：
/// **那两条要在用户机器上改 PATH 的命令，一条都不许踩 Windows 的那两个经典地雷。**
///
/// 上一轮这条判据只数「加」那一条。`R85` 用户逐字「**也能管理删除**」把「撤」那一半
/// 加进来了，而件文件 `§0b` 逐字点名：**做「撤」那一半会再撞一次同一个坑**
/// ⇒ 这里把人群从**一条**扩成**两条**，两条走同一批断言。
///
/// | 地雷 | 后果 | 撤这一侧为什么更重 |
/// |---|---|---|
/// | `setx` | 值截断在 1024 字符，只给一句警告、**退出码照样 0** ⇒ 静默截断用户 PATH | 加那一次截掉的是「多出来的尾巴」，撤是**整条重写** ⇒ 截掉的是用户本来就有的那一截 |
/// | 拿 `$env:PATH` 当新值写回用户级 | 进程里那份是**机器级 ＋ 用户级拼起来的** ⇒ 整条系统 PATH 被复制进用户 PATH，此后系统 PATH 的更新对这个用户永久失效 | 「先读一下现在的 PATH」在撤的语境里读起来**格外顺手** |
///
/// # 撤这一侧独有的第三条：**只摘自己那一段**
///
/// 过滤必须是**整格**（`-ne $d`）。`-replace` / `-like` / `.Replace(` 全是**子串**口径，
/// 用户 PATH 上有 `…\.cc-monitor\bin-old` 时会被一起打掉 —— 同一个病在加那一侧
/// 只是「误判成已经在了」，在撤这一侧是**误删用户的目录**，不可逆。
/// ⚠ 还要数一条**反向**的：不许出现 `Where-Object { $_ }` 那种「顺手把空项也滤掉」
/// —— 空项在 Windows 上语义是「当前目录」，删它是一次我们没被要求做的改动。
///
/// # 死值验（`KR135D1` 刀①/②）
///
/// ① 把 [`render_user_path_setup_command`] 改成 `setx PATH "%PATH%;…"` ⇒ 本条红；
/// ② 把 [`render_user_path_removal_command`] 的 `-ne $d` 改成
///    `-notlike "*$d*"` ⇒ 本条红（撤那一侧的整格断言）。
#[test]
fn the_generated_path_command_edits_only_the_user_scope_and_never_via_setx() {
    let both = [
        (
            "探",
            render_user_path_probe_command().expect("生成的「探」命令"),
        ),
        (
            "加",
            render_user_path_setup_command().expect("生成的「加」命令"),
        ),
        (
            "撤",
            render_user_path_removal_command().expect("生成的「撤」命令"),
        ),
    ];
    for (which, cmd) in &both {
        // ── 地板：它确实在写 PATH，而不是一段无关的文字 ──────────────
        assert!(
            !cmd.trim().is_empty(),
            "「{which}」生成出来是空的 —— 下面全是空真"
        );
        // 「探」那条只读，不写 ⇒ 写那一条只对「加」「撤」成立。
        if *which != "探" {
            assert!(
                cmd.contains("SetEnvironmentVariable('Path', ") && cmd.contains("'User')"),
                "「{which}」没有在写**用户级** PATH：\n{cmd}"
            );
        }
        assert!(
            cmd.contains("GetEnvironmentVariable('Path', 'User')"),
            "「{which}」的新值不是从**用户级**那一档读出来的 —— 拿 `$env:PATH`\
                 （机器级＋用户级拼起来的那一份）当基准，会把整条系统 PATH 复制进用户 PATH。\n{cmd}"
        );
        // ── 两个地雷，两侧同一批 ─────────────────────────────────────
        for landmine in ["setx", "'Machine'", "\"Machine\"", "$env:PATH"] {
            assert!(
                !cmd.contains(landmine),
                "「{which}」那条命令里出现了 `{landmine}` —— 见本判据头注那张表。\n{cmd}"
            );
        }
    }
    // ── 加：幂等（跑两次不许把目录塞两遍）──────────────────────────
    let add = &both[1].1;
    assert!(
        add.contains("-notcontains $d"),
        "「加」跑第二次会重复追加：\n{add}"
    );
    // ── 撤：整格，而且**只**摘我们那一格 ──────────────────────────
    let del = &both[2].1;
    assert!(
        del.contains("-ne $d"),
        "「撤」不是按**整格**比的 —— 子串口径会把 `<我们那段>-old` 之类一起删掉，\
             而那是删用户的东西，不可逆。\n{del}"
    );
    for substr_op in ["-replace", "-like", "-notlike", "-match", ".Replace("] {
        assert!(
            !del.contains(substr_op),
            "「撤」用了子串口径 `{substr_op}` —— 见本判据头注「撤这一侧独有的第三条」。\n{del}"
        );
    }
    assert!(
        !del.contains("Where-Object { $_ }"),
        "「撤」顺手把 PATH 上的空项也滤掉了 —— 空项在 Windows 上语义是「当前目录」，\
             删它是一次**我们没被要求做的改动**。除我们那一格之外要逐字复原。\n{del}"
    );
    // ── 🔴 〔`R88` 09-15〕**这一条翻正了，不是放宽了** ────────────────
    //
    // 上一轮它是 `!prod.contains("Command::new(")`（生产段一个进程都不许起），
    // 理由是「产品自己跑就变成了替用户改他的环境」。`R85` 用户逐字
    // 「**应该让用户手动点击加，也能管理删除**」⇒ **点击即执行是允许的**
    // （`§0c`：`K33` 禁的是产品**替**用户决定，用户点一下就是用户自己决定），
    // 而「现在状态」那一格**根本不可能靠用户去跑** ——
    // `R88` 就是为这件事推翻 `R87`「本件不需要起进程」那句假前提的。
    // ⇒ `testing.md` 规则 12：反向锚点的出路是**重新裁定**，不是删掉。
    //
    // 🔴 **先量人群再翻正**（`R88` 点名要求的那一步）：上一轮那条断言的分母
    // **不是整份文件，是 `production_code()` 剥完之后的生产段**。现打：整份文件
    // 4 处 `Command::new(`，**落在生产段的 0 处** —— 另外那几处（`icacls` 两处、
    // 夹具起 `bash` 一处、本断言自己的字符串一处）全在 `#[cfg(test)]` 里。
    // ⇒ 那条断言当时是**对的、也够得着**，不是一条空转的判据。
    //
    // **翻正之后钉的是三件事**，比原来那条更紧：
    // ① 生产段起进程的地方**恰好一处**（不是「不许起」，是「只许这一处」）；
    // ② 那一处起的是 `powershell.exe`，且带 `-NoProfile` / `-NonInteractive`；
    // ③ **写 PATH 的地方恰好两处**，而它们**就是那两个 `render_*` 函数** ——
    //    这一条守的是 `K33`「实现只许有一处」：谁要在别处再拼一段改 PATH 的
    //    PowerShell（哪怕拼得对），这里当场红。
    let prod = guard_core::production_code(include_str!(
        "../../../src/frontend/shell/src/profile_installer.rs"
    ));
    let spawns = prod.matches("Command::new(").count();
    assert_eq!(
        spawns, 1,
        "生产段里起进程的地方应当**恰好一处**（`run_user_path_powershell`），实得 {spawns} 处。\n\
             0 处 = 那一跳被删了 ⇒ 「现在状态」与两个按钮都成了摆设；\n\
             >1 处 = 有第二个地方在起进程 —— 它也得去 `write_site_registry::SPAWNS` 报到，\
             而那张表是**默认拒绝**的。"
    );
    assert!(
        prod.contains("Command::new(\"powershell.exe\")"),
        "那一处起的不是 `powershell.exe` —— 本模块唯一许可的起进程对象就是它\
             （`R88`：跑的必须是我们生成给用户看的那段字节）"
    );
    for flag in ["-NoProfile", "-NonInteractive"] {
        assert!(
            prod.contains(flag),
            "argv 里少了 `{flag}`。`-NoProfile` 是承重的：不读用户自己的 profile ——\
                 本件刚把我们那一段从 profile 里删掉，再回头去读 profile 是自相矛盾；\
                 `-NonInteractive` 保证它不会弹提示等人回车，把界面挂住。"
        );
    }
    let writers = prod.matches("SetEnvironmentVariable").count();
    assert_eq!(
        writers, 2,
        "生产段里「写环境变量」的地方应当**恰好两处**（`render_user_path_setup_command` \
             与 `render_user_path_removal_command` 各一处），实得 {writers} 处。\n\
             多一处 = 有人在别处又拼了一段改 PATH 的 PowerShell ⇒ **同一件事有了第二份实现**，\
             而那正是 `K33`「所有命令只许有一处」禁的；也正是 `R88` 否掉「Rust 直接写注册表」\
             那条路的第一理由。"
    );
}

/// ★★ 〔`KR135D1` 09-15〕**线上字段名与前端那份手写接口逐字对得上。**
///
/// # 它补的是一处**有意的例外**，让这处例外不比生成物弱
///
/// `src/ipc/commands.ts` 头注那「三桶」规则说：TS 侧真消费字段的命令（桶③）该用
/// **生成物**类型。`ccm_user_path_status` 破了这条 —— 生成物要落进 `src/generated/`，
/// 而那份目录**不在 `K-R135` 的写区里**。⇒ 手写一份，并用这条判据钉住它：
/// 生成物买的是「Rust 改了、TS 自动跟着变」，这一条买的是
/// 「**Rust 改了、TS 没跟 ⇒ 当场红**」。⚠ 它买不到「TS 多写了一个 Rust 没有的字段」
/// （那一形 `tsc` 也不会红，因为多出来的字段只是永远 `undefined`）—— 如实记，不假装。
///
/// # 人群怎么来的：**现算，不是在判据里手抄一份字段名单**
///
/// 把一个样本实例 `serde_json` 序列化一遍再读它的 key —— 那就是**线上**真正的字段名
/// （`rename_all = "camelCase"` 已经生效过了）。在这里手抄一份名单，
/// 就又是同一个病：**一个事实两个住址**。
///
/// # 死值验（`KR135D1` 刀⑨）
///
/// 给 `UserPathStatus` 加一个字段而不改 `commands.ts` ⇒ 本条红。
#[test]
fn the_user_path_status_wire_fields_match_the_hand_written_ts() {
    let sample = UserPathStatus {
        supported: true,
        dir: Some("d".into()),
        on_user_path: true,
        add_command: Some("a".into()),
        remove_command: Some("r".into()),
        error: Some("e".into()),
    };
    let v = serde_json::to_value(&sample).expect("序列化");
    let keys: Vec<String> = v.as_object().expect("是个对象").keys().cloned().collect();
    assert!(
        !keys.is_empty(),
        "一个线上字段都没算出来 —— 判据够不着被测对象了，先修判据"
    );
    let ts = include_str!("../../../src/ipc/commands.ts");
    // 地板：那份接口真的在（否则下面每一条都靠「找不到也不出声」蒙混过去）。
    assert!(
        ts.contains("export interface UserPathStatus {"),
        "`src/ipc/commands.ts` 里那份手写接口不见了 —— 要么它换成了生成物\
             （那就把本判据删掉，并把 `commands.ts` 那条注释一起改），要么有人顺手删了它"
    );
    for k in &keys {
        assert!(
            ts.contains(&format!("\n  {k}")),
            "线上字段 `{k}` 在前端那份**手写**接口里找不到。\n\
                 改了 Rust 侧的 `UserPathStatus` 就要同拍改 `src/ipc/commands.ts` —— \
                 这一处没有生成物替你跟（理由住那条注释）。\n\
                 线上字段现算是这几个：{keys:?}"
        );
    }
}

/// ★★ 〔`KR135D1` 09-15〕**那一格的「现在状态」：`~/.cc-monitor\bin` 在不在
/// 用户级 PATH 上 —— 它与那两条命令必须用**同一种相等**。**
///
/// # 为什么这一条单独存在：不一致的代价是**界面撒谎**
///
/// 加用 `-notcontains $d`、撤用 `-ne $d`，两者在 PowerShell 里都是
/// **整格 · 大小写不敏感**。状态那一格要是比它们**聪明**（做了路径规范化），
/// 就会出现「显示 ✓、点一下又多出一格重复」；要是比它们**笨**，
/// 就会出现「显示 ✗、点了没反应」。**两边同样地笨，比一边聪明一边笨要好。**
///
/// # 地板先证
///
/// 第一条断言是**正向**的（整格命中要认得出）—— 少了它，
/// 一个恒回 `false` 的实现能让下面每一条 `!` 断言全绿。
///
/// # 死值验（`KR135D1` 刀③）
///
/// 把 [`user_path_has_our_bin`] 的 `split(';').any(整格相等)` 换成
/// `user_path_raw.contains(dir_abs)` ⇒ 本条红（`-old` 那两格）。
#[test]
fn the_user_path_status_uses_the_same_equality_as_the_generated_commands() {
    let dir = r"C:\Users\me\.cc-monitor\bin";
    // ── 地板：整格命中真认得出（否则下面全是空真）──────────────────
    assert!(
        user_path_has_our_bin(&format!(r"C:\Windows;{dir};C:\foo"), dir),
        "整格命中都认不出来 —— 判据够不着被测对象了，先修实现"
    );
    assert!(user_path_has_our_bin(dir, dir), "独占一格认不出");
    assert!(
        user_path_has_our_bin(&format!(r"C:\Windows;{dir}"), dir),
        "在末尾认不出"
    );
    // ── 大小写不敏感：PowerShell 的 -contains / -ne 就是这样 ────────
    assert!(
        user_path_has_our_bin(r"C:\Windows;c:\users\me\.cc-monitor\BIN;C:\foo", dir),
        "大小写不一样就认不出了 —— 而那两条命令认得出 ⇒ 状态与按钮会各说各话"
    );
    // ── 比整格，不比子串：`-old` 这一形正是要挡的那个 ──────────────
    assert!(
        !user_path_has_our_bin(&format!("{dir}-old"), dir),
        "子串比法：`<我们那段>-old` 被读成「已经在了」，然后「加」那个按钮什么都不做"
    );
    assert!(!user_path_has_our_bin(
        &format!(r"C:\Windows;{dir}-old"),
        dir
    ));
    assert!(!user_path_has_our_bin(&format!(r"{dir}2;C:\x"), dir));
    // ── 末尾反斜杠：**刻意判 false**（代价写在函数头注里）──────────
    assert!(
        !user_path_has_our_bin(&format!(r"{dir}\"), dir),
        "这一格刻意与那两条命令**同样地笨** —— 它们也不砍末尾反斜杠。\
             要改成规范化，三处（本函数 ＋ 两条命令）必须同拍一起改"
    );
    // ── 拿不到目录时恒 false：不许把「拿不到」读成「已经装好了」──────
    assert!(
        !user_path_has_our_bin(r"C:\a;;C:\b", ""),
        "空目录与 PATH 上的空项相等了"
    );
    assert!(!user_path_has_our_bin("", ""));
    assert!(!user_path_has_our_bin("", dir));
}

/// ★★ 〔`KR135D3` 09-15 · 〔E2〕改裁〕**那份共用的 POSIX 别名 snippet，真的把 `ccm` 落点放上了 PATH，而且只放它。**
///
/// 要求住址：`设计/01 §6.7b`「落点 `~/.cc-monitor/bin/ccm` —— 本机与远端同一个」「清掉旧的 `~/.local/bin/ccm`」· V41（不为旧状态留兼容）。
/// 〔E2〕从前这一行把 `~/.local/bin` 也加进来（「更早的版本放在那儿，不删、照旧能用」）—— 旧入口今天部署时认出来就删（GP1），
/// `ccm` 就是后端本身、只住 `~/.cc-monitor/bin` ⇒ 那一格退役。两边落点同一个常量（共享 crate）；足迹申报表那两条与它相等由后端判据钉住。
///
/// 量真实输出：真 `source` 一趟问 `$PATH`，**加进来的目录集合 == {落点}**（两向）；source 两趟 PATH 不变（幂等）。
/// ⚠ 它跑在 `cfg(unix)` 下（要 `bash`）；这份 snippet 本来就只装进 POSIX rc。证不了「干净 Linux 机上真敲得到 `ccm`」（要一台干净机）。
#[cfg(unix)]
#[test]
fn the_shared_alias_snippet_puts_exactly_the_ccm_landing_on_path() {
    // 〔MIG-3b 续〕本机远端同一个落点（`relay_route_core::BACKEND_LANDING_REL`）；足迹申报表那两条与它相等由后端判据对拍。
    let local = ccm_bin_dir_rel().expect("后端落点的目录");
    let td = tmpdir("aliassnip");
    let home = td.0.join("h");
    std::fs::create_dir_all(&home).expect("造假家目录");
    let snip = td.0.join("snippet.sh");
    std::fs::write(&snip, include_str!("../../../src/shared/ccm-aliases.sh")).expect("写 snippet");
    let run = |times: usize| -> String {
        let dots = ". '".to_string() + &snip.display().to_string() + "'; ";
        let script = dots.repeat(times) + "printf '%s' \"$PATH\"";
        let out = std::process::Command::new("bash")
            .arg("-c")
            .arg(&script)
            .env("HOME", &home)
            .env("PATH", "/usr/bin:/bin")
            .output()
            .expect("跑 bash —— 沙箱里没有 bash 的话这条判据够不着被测对象");
        assert!(
            out.status.success(),
            "source 那份 snippet 直接失败了（这本身就是一条缺陷）：\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).to_string()
    };
    let path = run(1);
    let added: Vec<&str> = path
        .split(':')
        .filter(|s| !["/usr/bin", "/bin"].contains(s))
        .collect();
    let segs: Vec<&str> = path.split(':').collect();
    assert_eq!(
        segs[segs.len().saturating_sub(2)..],
        ["/usr/bin", "/bin"],
        "原来的 PATH 被动了：{path}"
    );
    assert_eq!(
        added,
        vec![format!("{}/{local}", home.display())],
        "别名块加进 PATH 的目录不是恰好 `ccm` 落点那一个（旧的 `~/.local/bin` 退役了）"
    );
    assert_eq!(
        run(2),
        path,
        "source 两趟 PATH 变了 —— 嵌套 shell 会让 PATH 无限变长"
    );
}
