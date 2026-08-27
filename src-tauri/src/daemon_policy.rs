//! P2s（定框 `C8`）：**每台机一份 daemon 策略**。
//!
//! 今天只有一条策略：**monitor 退出时要不要主动结束这台机的 daemon**，默认 **false（不主动结束）**。
//!
//! # 归属：为什么持久化不在这里
//!
//! `config.rs` 头注逐字「Rust 端**不解释配置内容**（schema 在前端定义）」。
//! 若这里也往 `config.json` 里读写，同一个文件就有了**两个写者** ——
//! 前端「读—改—写」整份的那一刻，会把 Rust 刚写进去的键按一份**陈旧副本**覆盖掉。
//! ⇒ 持久化归前端；本模块只持有**生效值**，由前端在改动时与启动时推进来。
//!
//! # ⚠ 「不主动结束」到底等不等于「继续跑」—— **今天要看它有没有真脱离**〔`K-P1` 08-26 翻面〕
//!
//! **翻面之前**（一直到 `K-P1`）：daemon 是**纯 stdio 子进程**，monitor 一退读端就断，
//! 它在 **153 毫秒**内自己 broken-pipe 退出（实测，08-11 P2s §0a）。所以那时本策略的真实语义是
//! **「立刻杀」与「让它自己死」之差**，不是「后台常驻」，而 `P2s-Y5` 据此立了一条
//! **无条件**禁令（原句已从这四处删干净，由
//! [`tests::the_unconditional_ban_is_gone_from_all_four_homes`] 钉着；要看原文去翻
//! `control-parity` 的 `P2s-Y5`）。
//!
//! ⚠ **这里刻意不把那句原文抄下来** —— 抄下来它就会命中那条判据自己，
//! 而「为了不命中判据把引号换成另一种」是**绕**，不是治。同族的自指陷阱本仓记过多次。
//!
//! **今天那条禁令的前提只在一半的情况下成立了。**`K-P1` 给了 daemon 一个监听口，
//! 并让它在 Linux 上**真脱离**（`process_group(0)` + stdio 全 null + 协议改走那个口）
//! ⇒ 「勾掉开关」在那一支上**真的**是「继续跑」。
//!
//! ⇒ 禁令换成**按状态分档**（`K-P1 KPY4`，由 `src/settings/daemon-section.vitest.ts` 机检）：
//! 用户可见的那四句话有**唯一一个家**（`src/daemon-policy.ts`），
//! Rust 这一侧把同样的四条字面量放在下面，由 [`tests::the_exit_copy_is_the_same_string_on_both_sides`]
//! 逐字对拍 —— 形状抄 `the_local_origin_is_the_same_string_on_both_sides`。
//!
//! ⚠⚠ **「无人监护」这半是用户裁定的一半，不许省**（`DECISIONS` `K14` 逐字：
//! 「第一档必须在 UI 上如实说『继续跑，无人监护』，这是本裁定的一半，
//! **不许只做常驻不做这句话**」）。脱离之后**没有任何东西在监护它** ——
//! 崩了不会自动重起（自愈整件摘出去成了 `K-P3`）。那是**如实登记的降级**，不是漏洞；
//! 而如实登记的意思就是**说出口**，不是写在一条源码注释里。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// ── `K-P1 KPY4`：退出行为的四句话。**用户可见文案的家在 TS 那侧**
/// （`src/daemon-policy.ts` 的 `EXIT_*`），这里这四条只为**逐字对拍**而存在。
///
/// ⚠ 别在这里加第五条而不动那边：对拍是**双向**的（两边条数与内容都比）。
///
/// ① 勾上「退出时结束它」。
///
/// ⚠ 它与那个复选框的**标签**刻意不是同一个串：标签说的是**这个开关是什么**，
/// 这一句说的是**接下来会发生什么**。写成同一个串的话，「那四句只许有一个家」那条判据
/// 会把复选框的标签算成第二个家 —— 而那**不是误报**：两处一模一样的串，
/// 下一次改文案时一定只会改到一处。
pub const EXIT_KILLS: &str = "monitor 退出时会结束它";
/// ② 勾掉 + **真脱离了**。★ 这一句里的「无人监护」是 `K14` 背书的那半。
pub const EXIT_UNATTENDED: &str =
    "monitor 退出后它继续跑，无人监护：崩了不会自动重起；下次开 monitor 会接上它，接不上才起一个新的";
/// ③ 勾掉 + **没脱离**（平台不支持 / 被关掉了 / 脱离失败）⇒ **保持今天那句，一字不改**。
pub const EXIT_SELF_DIES: &str =
    "monitor 不主动结束它；它仍会在 monitor 退出后很快自行退出";

/// 那三句的顺序**与 TS 那侧逐条对齐**。对拍判据按名字取、按内容比，条数也比。
///
/// ⚠ 曾经有过第四档（「已经脱离了 ⇒ 这个勾管不到它」）。它是**一个缺口的产物**：
/// 退出钩子当时只收被监护的那条路。缺口补上（`lib.rs` 的 `RunEvent::Exit` 现在两条都收）
/// 之后那一档成了假话 ⇒ 随缺口一起删掉。**留档是为了下一个人别把它当成「少了一档」。**
pub const EXIT_COPY: &[(&str, &str)] = &[
    ("EXIT_KILLS", EXIT_KILLS),
    ("EXIT_UNATTENDED", EXIT_UNATTENDED),
    ("EXIT_SELF_DIES", EXIT_SELF_DIES),
];

fn table() -> &'static Mutex<HashMap<String, bool>> {
    static T: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    T.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 缺省：**不主动结束**（`C8`③ 的前半句 —— 那半是站得住的）。
pub const DEFAULT_KILL_ON_EXIT: bool = false;

/// 这台机的 daemon，monitor 退出时要不要主动结束。
pub fn kill_on_exit(origin: &str) -> bool {
    table()
        .lock()
        .ok()
        .and_then(|t| t.get(origin).copied())
        .unwrap_or(DEFAULT_KILL_ON_EXIT)
}

/// 前端推进来的生效值（改动时 + 启动时各推一次）。
#[tauri::command]
pub fn set_daemon_kill_on_exit(origin: String, kill: bool) -> Result<(), String> {
    if origin.trim().is_empty() {
        return Err("origin 不许为空 —— 策略是 per-host 的，没有「全局」这一档".into());
    }
    let mut t = table().lock().map_err(|e| format!("锁毒化: {e}"))?;
    tracing::info!("daemon 策略：origin={origin} kill_on_exit={kill}");
    t.insert(origin, kill);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_origin_defaults_to_not_killing() {
        assert!(
            !kill_on_exit("这台机从来没被推过策略"),
            "缺省必须是「不主动结束」——`C8`③ 的前半句。\n\
             缺省若是 true，用户什么都没设就会被杀 daemon，而开关默认关着。"
        );
    }

    #[test]
    fn the_policy_is_per_origin_not_global() {
        set_daemon_kill_on_exit("甲机".into(), true).expect("设甲机");
        assert!(kill_on_exit("甲机"), "甲机设了 true 却读不回来");
        assert!(
            !kill_on_exit("乙机"),
            "改甲机把乙机也改了 —— 那就不是 per-host 而是全局一份（`C8`① 明说粒度是每台机各一个）"
        );
    }

    /// ★★ `K-P1 KPY6`：**那几句话不许只改一处** —— 跨语言逐字对拍。
    ///
    /// # 为什么是「逐字对拍」而不是 `grep -c`
    ///
    /// `brief` 第 11 条逐字：判「动没动某个闭集」**不许用 `grep` 数加行**（多行字面量会漏）。
    /// 这里两侧都是**具名常量**，所以量法是「按名字取那一行的字面量，整串相等」——
    /// 形状抄仓里已有的那条 `the_local_origin_is_the_same_string_on_both_sides`。
    ///
    /// # 它防的那个漂**不会有任何东西报错**
    ///
    /// 前端改了措辞而 Rust 这侧没跟：两边都编得过、都跑得起来，
    /// 只有「这一句到底是谁说了算」这件事悄悄没了 —— 下一个人只会看到两句不一样的话。
    #[test]
    fn the_exit_copy_is_the_same_string_on_both_sides() {
        let ts = include_str!("../../src/daemon-policy.ts");
        for (name, rust) in EXIT_COPY {
            let head = format!("export const {name} =");
            // TS 那侧允许换行（prettier 会把长串折下来）⇒ 从声明处起取到第一个分号。
            let at = ts
                .find(&head)
                .unwrap_or_else(|| panic!("前端那份里找不到 `{head}` —— 名字改了就来改这条"));
            let decl = &ts[at..];
            let end = decl.find(';').expect("那一行不是 `export const X = …;` 的形状");
            let lit = decl[..end]
                .split('"')
                .nth(1)
                .unwrap_or_else(|| panic!("`{name}` 的值不是一个双引号字面量"));
            assert_eq!(
                lit, *rust,
                "退出行为那句话两侧漂了（`{name}`）：\n  前端 {lit:?}\n  后端 {rust:?}\n\
                 ⚠ 这种漂**不会有任何东西报错** —— 两边都编得过、都跑得起来，\n\
                 只是「这一句谁说了算」悄悄没了。⇒ 改文案要**同一拍改两处**。"
            );
        }
        // 抽取器自检：条数变了也要红（少一条 = 上面的循环少跑一圈，那正是「空转」）。
        assert_eq!(EXIT_COPY.len(), 3, "退出行为的档数变了 —— 回来重判，别让本条在少数几档上绿着");
    }

    /// ★★ `K-P1 KPY6` 的另一半：**那条无条件禁令今天是假的，四处一处都不许留着。**
    ///
    /// 翻面之前那句话住四处（`P2s-Y5`）：`daemon_policy.rs` · `daemon_control.rs` ·
    /// `daemon-policy.ts` · `settings/daemon-section.ts`。它逐字是
    /// 「**UI 文案不许写「daemon 继续运行」**」，依据是「daemon 153ms 内自己走」。
    ///
    /// `K-P1` 之后那个依据**只在没脱离的那一支成立** ⇒ 这条**无条件**禁令是一句半假的全称。
    /// 留在四处中的任何一处，下一个人读到的就是「界面永远不许说继续跑」——
    /// 而那正好会把 `K14` 背书的那半（「如实说继续跑，无人监护」）读成违规。
    ///
    /// ⚠ 本条只查**那一句的字面**。它逮不到「换个说法写同一条无条件禁令」——
    /// 如实登记，不假装机检覆盖了它。
    #[test]
    fn the_unconditional_ban_is_gone_from_all_four_homes() {
        const HOMES: &[(&str, &str)] = &[
            ("daemon_policy.rs", include_str!("daemon_policy.rs")),
            ("daemon_control.rs", include_str!("daemon_control.rs")),
            ("src/daemon-policy.ts", include_str!("../../src/daemon-policy.ts")),
            (
                "src/settings/daemon-section.ts",
                include_str!("../../src/settings/daemon-section.ts"),
            ),
        ];
        // 针**运行时拼**：本条自己的散文里就有这几个字，写成一个整串的话它会命中自己
        //（同族自指陷阱本仓记过多次）。
        //
        // ⚠⚠ **08-26 死值验当场逮到这条针是错的**：第一版拼的是 `不许写daemon 继续运行`，
        //   而原句是 `不许写「daemon 继续运行」`——**中间隔着一对直角引号**。
        //   ⇒ 那一版**永远匹配不上原句**，本条是个安慰剂：把原句原样放回 `daemon_control.rs`，
        //   实测 `5 passed; 0 failed`（rc=0），**一点都不红**。
        //   ★ 这就是「acceptor 必须先证明它会失败」买到的东西：它红之前，我以为它有牙。
        let needle = format!(
            "{}{}{}daemon 继续运行{}",
            "不许",
            "写",
            '\u{300c}',
            '\u{300d}'
        );
        // ⚠ `.rs` 那两份**只看 `#[cfg(test)]` 之前那一段** —— 判据自己的解释性散文
        //   （包括本条的头注）住在测试段里，不切掉的话本条会**命中它自己、恒红**。
        //   ⚠ 这里不能用 `production_code`：它把 `//` 开头的行整行剥掉，
        //   而这条判据要查的**正是**那些头注散文（`//!` 也是 `//` 开头）。
        let before_tests = |s: &str| s.split("#[cfg(test)]").next().unwrap_or(s).to_string();
        let mut left: Vec<&str> = Vec::new();
        for (name, src) in HOMES {
            // 抽取器自检：四份都得真读到，切完也不能只剩个壳。
            assert!(src.len() > 500, "{name} 只读到 {} 字节 —— 人群坏了", src.len());
            let scan = if name.ends_with(".rs") {
                before_tests(src)
            } else {
                (*src).to_string()
            };
            assert!(
                scan.len() > 400,
                "{name} 切完只剩 {} 字节 —— 切法坏了，本条在空转",
                scan.len()
            );
            if scan.contains(&needle) {
                left.push(name);
            }
        }
        assert!(
            left.is_empty(),
            "这几处还留着那条**无条件**禁令：{left:?}\n\
             它的依据（「monitor 一退它 153ms 内自己走」）在 `K-P1` 之后**只对没脱离的那一支成立**。\n\
             ⇒ 留着它 = 下一个人会把 `K14` 背书的那半（如实说「继续跑，无人监护」）读成违规。\n\
             四处要**同一拍**改，这正是「不许只改一处」那条 DoD 的落点。"
        );
    }

    #[test]
    fn an_empty_origin_is_refused() {
        assert!(
            set_daemon_kill_on_exit("  ".into(), true).is_err(),
            "空 origin 必须拒。放过它等于悄悄造出一档「全局策略」，\n\
             而 `kill_on_exit(真 origin)` 永远读不到它 —— 设了没反应，且不报错。"
        );
    }
}
