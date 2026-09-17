//! `K-W2D` · `R2` ④：**按需拉取**那条路的协议与判定。
//!
//! `DECISIONS.md` 的 `R2` 订正段把四样东西归到本拍，本模块逐一落在下面：
//! **拉取协议**（从哪拉 · 怎么定位那个资产 · 失败怎么退）· **哈希钉法**
//! （校验什么 · 哈希从哪来 · 对不上怎么办）· **断网 / 无写权限时的唯一失败面** ·
//! **别再一个值装两件事**。
//!
//! # 一 · 拉取协议
//!
//! 定位一个资产要三样东西，**三样都是这份 daemon 编译进来的常量，一样都不许在运行期发现**：
//! 资产基址 · release 的 tag · 这份 sidecar 的 `build_id`。缺任何一样，[`pin`] 回
//! [`Face::Unpinned`] —— **不猜、不兜底、不退回 `latest`**。
//!
//! ★ **「不许退回 `latest`」是这一节最贵的一条**，它不是洁癖：
//! 哈希是按 `build_id` 钉死的（见下一节），拉一份 `latest` 回来必然对不上 ⇒ 拉了也用不了；
//! 而它同时是一条**静默的第二退路** —— 有了它，「这份 daemon 没带钉」这个真正的病因
//! 就再也不会被人看见。⇒ [`crate::sidecar_fetch_guard`] 把这个词钉成本层生产段的禁词。
//!
//! 资产名照 daemon 自己那两个资产的形状（`release.yml` 的 `Append Linux artifacts`
//! 那一步逐字挂着 `cc-monitor-remote-<arch>` 与同名 `.build_id`）⇒ 见 [`asset_name`]。
//! 架构那一维的闭集住 [`ARCHES`]，与 monitor `build.rs` 内嵌两架构的那个循环同源。
//!
//! ⚠ **今天那三样一样都没有。** 本 crate 没有 `build.rs`，CI 也没有往里注任何一条 ——
//! 本轮**不动 CI**。⇒ 今天这条路的唯一答案就是 [`Face::Unpinned`]，而那正是
//! 「一种失败一个出口」要的样子：它说得出自己缺什么。
//!
//! # 二 · 哈希钉法
//!
//! **校验什么**：拉回来的那一整份字节，SHA-256。
//!
//! **哈希从哪来 —— 这一格只有一个答案是对的**：它必须**与 daemon 这份二进制同源**
//! （编译期常量），**不许**跟资产一起从网上取。理由是现打的：release 上确实挂着一份
//! `SHA256SUMS-linux.txt`，而它与资产**由同一处发出** ⇒ 拿它去校验资产，
//! 证明的只是「发资产的人前后一致」，一个字节的来历都没多证。
//! 而 daemon 这份二进制自己是怎么到这台机器上的，另有一条已经存在的路
//! （SFTP 自部署 + `build_id` 版本门控）⇒ 把钉挂在它身上，才接得上那条信任链。
//!
//! **对不上怎么办 —— 分两处，答案不同，这正是要两个判定函数的理由**：
//! 拉之前发现盘上那份对不上 ⇒ [`Step::Fetch`]（重拉就是修法）；
//! 刚拉回来的那份对不上 ⇒ [`Face::HashMismatch`]，**到此为止**，不重试、不落盘。
//!
//! # 二之二 · 为什么 [`sha256_hex`] 是手写的 —— **先把一条我引错的理由撤掉**
//!
//! 🔴 **订正（本轮自己打的读数）**：本段初版写的理由是「本 crate 的 `Cargo.toml` 逐字记着
//! ——依赖树钉在一个 **yanked** 的 `crypto-bigint 0.7.3` 上，**凡是让 cargo 重走解析的事
//! （含加或改任何一条依赖）都会撞墙** ⇒ 加一条哈希依赖会让本 crate 在断网门禁上当场编不过」。
//! **那是转述，不是读数，而且在这一条上不成立。** 实测（切刀 `M20`，跑在仓副本、断网容器里）：
//! 往本 crate 的 `[dependencies]` 加一行 `sha2 = "0.10"`，`cargo metadata --offline`
//! **退出码 0**，锁里干净地多出 4 个包（`block-buffer` / `crypto-common` / `digest` / `sha2`）。
//! ⇒ cargo 只重解析**新加的那一小片**，够得着本机缓存就过得去；那句话描述的是
//! 「**重解析整棵树**」那一类动作，不是「加任何一条依赖」。
//!
//! ⇒ 真正的取舍**只有这一格，如实摆着**：加依赖要付「daemon 的 lock 多 4 个包 +
//! `readonly_guard` 的依赖签字表多一行（那张表自带幽灵检查与重量判据）」；
//! 不加要付「自己写一个密码学原语」。
//!
//! **本轮选不加**，理由是这一处用哈希的方式窄得特别厉害：拿一整份字节去比一个
//! **编译期常量**，没有秘密、没有时序要求、没有增量/流式需求。接住正确性的是 [`tests`]
//! 里那三条 NIST 向量（空串 / `abc` / 跨块的 56 字节串）——**三条都不是本实现算出来的**，
//! 它们来自标准；再加一条非空对照（换一个字节，答案真的变）。
//! ⚠ **这个选择是可逆的、也值得复核**：换成 `sha2` 是一次局部替换（删掉本模块那 60 行、
//! 加一行依赖、去签字表上签一行），**不影响本模块任何一条判据的形状**。
//!
//! # 三 · 唯一失败面
//!
//! [`Face`] 是这条路上**全部**失败的闭集（成员就在下面，散文里不复述第二份，
//! 基数也不写死）。每一个成员恰好一个 `code`、恰好一句话，各自只有一处出产地：
//! [`Face::code`] 与 [`Face::sentence`] 两个穷尽 `match`。
//!
//! ★ **「一种失败只许有一个出口」在这里的具体含义**：四个事实
//! （[`Landed`] · [`Integrity`] · [`Transport`] · [`Landing`]）的每一个非成功取值，
//! 都由一个**总函数**映到 [`Face`] 的某一个成员上 —— 没有 `Option::None` 式的
//! 「悄悄跳过」，没有 `unwrap_or` 式的「配个默认值接着走」，没有「在 `PATH` 上再找一份」
//! 这种第二条路。后三样都被 [`crate::sidecar_fetch_guard`] 钉成本层生产段的禁词。
//!
//! ⚠ **那句话今天在线上没有住址，这一格本模块解决不了**：`R2` ④ 逐字要「握手帧
//! `unavailable` 里挂 `panorama` 族命令 + **明确的失败文案**」，而 `wire::Unavailable`
//! 只有两个字段、**没有 `message`**，它的头注还逐字论证过为什么不带
//! （「那句人话今天归 monitor」）。⇒ [`unavailable_for`] 把两半分开交出来，
//! 并由判据钉住那个结构今天真的是两个字段。**要不要给 wire 加一个字段归 PM**，不在本层。
//!
//! # 四 · 别再一个值装两件事（`K-W4` 那个坑的同族）
//!
//! `K-W4` 的病灶逐字是：`.build_id` 是**目录级**的标记，而部署判定只读它、
//! 从不 stat 那个二进制本身 ⇒ 一个读数同时被当成「版本对不对」**和**「那个文件在不在」。
//! 它的解法是把两件事拆成两个事实（版本那一维 + 落点四态），各自取样、合起来判。
//!
//! **本条路上同一个坑长在两个地方，两个都躲开了**：
//!
//! 1. **落点那个目录已经有两个写入方了。** sidecar 要落的 `~/.cc-monitor/bin/`
//!    正是远端自部署与本机释放**共用**的那一个（`local_backend::sweep_stale_partials`
//!    的注释逐字写着「目录是与远端自部署共用的」）；而远端那条路的标记
//!    **就叫 `.build_id`、就在这个目录里**。⇒ sidecar 若也写一份目录级 `.build_id`，
//!    两个产品的版本会互相覆盖，而 `R3` 逐字记着这一形 08-11 真发生过一次
//!    （两条写入路共用一个目录、互判 stale ⇒ **无限重装循环**）。
//!    ⇒ **本层一个标记文件都不写**：身份进**文件名**（[`landing_name`]），
//!    照的是本机释放那条路的先例（它的头注逐字「与远端那条**结构上不可能撞**
//!    （不是靠『配置别配成一样』）」）。禁词由判据钉住。
//! 2. **「拉过了」与「文件还在」不是同一件事，所以盘上没有任何东西记「拉过了」。**
//!    一个 `fetched: bool`（或一份「我拉过 X」的记录）会在文件被删 / 被截断 / 被换掉之后
//!    继续说「拉过了」—— 那正是 `K-W4` 那条病的形状。本层把它拆成**两个各自取样的事实**：
//!    [`Landed`]（那个文件在不在 / 有没有字节 / 问不问得出来）与 [`Integrity`]
//!    （那些字节是不是钉住的那一份）。两者都**每次现问**，都不缓存。
//!
//! ⚠ [`Integrity`] 自己还藏着同一族的第三处，也拆开了：**「没验过」不是「验过、不符」，
//! 也不是「验过、相符」。** 用一个 `bool` 装校验结果，`false` 会同时装下这两件事。
//! ⇒ [`Integrity::NotChecked`] 是独立的一档，而且它在类型上**进不了**落盘后的那个判定
//! （[`integrity_verdict`] 只吃 [`Checked`]）。
//!
//! # 接线那天要**同轮**做的事 —— 09-10 那一拍逐条对了一遍，**四件里做成一件半**
//!
//! - ❌ `panorama` 那一族命令要先进 `inbound::REGISTRY` —— 那是 PM 持有的文件，走上报口。
//!   **09-10 那一拍写区不含它 ⇒ 没做。** 这也是为什么这一层至今仍然零生产调用方。
//! - 🟡 真正那一跳（HTTP GET · 写盘 · 起进程）会让 `readonly_guard` 的两张表红。
//!   **09-10 做了那一跳（住 [`super::acquire`]），而两张表只签成一张**：
//!   · ✅ **写面白名单**从「一个 `&str`」变成一张逐条登记的表，本层那个落盘点签了字；
//!   · ❌ **起进程点表**没签成 —— 那一跳走的是 `K-W1A` 立的通用调用口，**没有新增
//!     `Command::new`**。两条理由都是现打的（`ratchet_guard` 逐字钉着那张表的相等断言那一行，
//!     而那份文件不在写区；`KW2D7` 09-04 选的又正是「不改通用口」），整段住
//!     [`super::acquire::ask`] 的头注。⇒ 件文件 `KW2D5` 预言的那一红**今天还没来**。
//! - ❌ 往 hello 帧上真填 `unavailable` 是一次跨仓契约变更，`wire.rs` 那个字段的头注
//!   列了要同轮做的三件事（含 bump `BUILD_ID`）。**没做，归 PM。**
//! - ❌ 本层的 `allow(dead_code)`（住 `sidecars/mod.rs`）同轮摘掉。
//!   **没做，而且条件一格都没到**：摘它的条件逐字是「那条『0 个生产调用点』的判据红了」，
//!   而第一条没做 ⇒ 它今天仍然绿。现打的代价与替代做法住 `sidecars/mod.rs` 头注。
//!
//! 🔴 **一句话别忘**：上面做成的那些**都在不出网的前提下**。「这条路能不能真拉下来」
//! **仍然一格都没买到** —— 那要一次带网的端到端，出了门禁沙箱，归 PM 另排。

use crate::wire;

/// 架构那一维的闭集 —— 与发版真出的那两份资产同源。
///
/// ⚠ 它**不是**「这个世界上有哪些架构」，是「我们今天真出得来、也真挂上 release 的那几个」。
pub const ARCHES: &[&str] = &["x86_64", "aarch64"];

/// 收到这么多字节算不算超 —— **上限是入参，本层不持有那个数**。
///
/// 它挡的不是「大了一点」，是「对面换了个东西」（一个 HTML 错误页、一份 tar、
/// 一次重定向到别的站）。
///
/// # 🔴 为什么这个数不住在这里（本轮实测撞出来的，不是风格）
///
/// monitor 那棵树有一条判据 `byte_cap_registry`：**每一个字节上限都要登记它管什么量、
/// 超限怎么办**，而它的登记表在那棵树里、**不在本轮写区**。本轮实测：在这里写一个
/// `const … : u64 = …` 当场把门禁第三格打红（诊断逐字点名本文件与那个数）。
///
/// ⇒ 两条路，09-04 那一拍选后者：① 去那张表上登记一行（越界）；
/// ② **上限跟着真正会去下载的那一层走** —— 那一层当时不存在。
/// 选 ② 还有一条独立理由：本层是纯判定，它**执行不了**任何上限；
/// 一个自己管不住的数写在这里，是一句好看的空话。
///
/// # ✅ **那笔债 09-10 还掉了** —— 上面那句「没有住址」从这一拍起不成立
///
/// 接线那一拍的写区含了 monitor 树 ⇒ 两件事**同轮**做完了：
/// 上限落在真正会去下载的那一层（[`super::acquire::ASSET_BYTE_CAP`]），
/// 并且去 `byte_cap_registry` 上登记了。
/// ⚠ 而且落地时**发现它其实是两个量**：头与体共用一个上限会静默截断
/// （实测读数住 [`super::acquire::RESPONSE_HEAD_BYTE_CAP`] 的头注）⇒ 今天是**一对**数。
/// 本函数自己一个字没改：它仍然是纯判定，上限仍然是入参 —— 变的是**入参从哪儿来**。
pub fn within_cap(len: u64, cap: u64) -> Transport {
    if len > cap {
        Transport::Oversize(len)
    } else {
        Transport::Got(len)
    }
}

/// 这份 daemon 带着的**钉** —— 四样编译期事实，缺一样就没有钉。
///
/// ⚠ 刻意**没有**「上次拉的是哪一份」这一格：那会是一个跨进程存活的状态，
/// 而本模块头注第四节说的正是「盘上不许有任何东西记『拉过了』」。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Pin {
    /// 资产基址（release 下载前缀，不含 tag）。
    pub base: &'static str,
    /// 哪一个 release。
    pub tag: &'static str,
    /// 这一份 sidecar 的身份 —— **只用来起落点的名字**，不用来定位资产。
    pub build_id: &'static str,
    /// 这一份资产的 SHA-256（小写 hex）。
    pub sha256: &'static str,
}

/// 空串与「根本没定义」压成同一格 —— 两者对本层是同一件事：**没有这个钉**。
///
/// ⚠ 这**不是**「一个值装两件事」：两者的下游行为完全一致（都只能是
/// [`Face::Unpinned`]），而把它们分开会造出一个**没有人做得出不同处置**的区分。
/// 拆事实的判据是「单独为假时行为不同」，不是「字面上是两回事」。
fn pinned(v: Option<&'static str>) -> Option<&'static str> {
    v.filter(|s| !s.is_empty())
}

/// 这一架构的钉。**四样齐了才算有钉**，缺任何一样一律 [`Face::Unpinned`]。
///
/// 🔴 **没有默认值、没有 `latest`、没有「先拿个能跑的顶上」** —— 见模块头注第一节。
///
/// ⚠ **架构那一问先答，而且答案是另一张脸** —— 这一格是本轮自查逮到的同族：
/// 头一版把「这台机器的架构我们不出资产」并进了 [`Face::Unpinned`]，
/// 而两者的**处置完全不同**（前者用户做什么都没用，后者换一份正式发版的后端就好）。
/// 一个值装了两件事，说出来的那句话就有一半是错的。
pub fn pin(arch: &str) -> Result<Pin, Face> {
    if !ARCHES.contains(&arch) {
        return Err(Face::ArchUnsupported);
    }
    let sha = match arch {
        "x86_64" => pinned(option_env!("CCM_SIDECAR_SHA256_X86_64")),
        "aarch64" => pinned(option_env!("CCM_SIDECAR_SHA256_AARCH64")),
        // `ARCHES` 那一关已经挡在前面；这一支留着是因为两处必须同轮改（加一个架构
        // 只改上面那张表、不加这里的哈希来源 ⇒ 那个架构会永远报「没带钉」）。
        _ => None,
    };
    match (
        pinned(option_env!("CCM_SIDECAR_ASSET_BASE")),
        pinned(option_env!("CCM_SIDECAR_RELEASE_TAG")),
        pinned(option_env!("CCM_SIDECAR_BUILD_ID")),
        sha,
    ) {
        (Some(base), Some(tag), Some(build_id), Some(sha256)) => Ok(Pin {
            base,
            tag,
            build_id,
            sha256,
        }),
        _ => Err(Face::Unpinned),
    }
}

/// release 上那个资产叫什么 —— 形状照 daemon 自己那两份资产。
///
/// ⚠ **名字里不带 `build_id`，这是有意的**：资产的身份由 `tag` 定位、由哈希证明，
/// 名字只负责**找得到**。落点那个名字要的是另一件事（防同目录撞名），所以它带
/// —— 见 [`landing_name`]。两个名字解决两个问题，**别合成一个**。
pub fn asset_name(arch: &str) -> String {
    format!("code-picture-{arch}")
}

/// 那个资产的完整地址。
pub fn asset_url(pin: &Pin, arch: &str) -> String {
    format!("{}/{}/{}", pin.base, pin.tag, asset_name(arch))
}

/// 落到用户盘上的**文件名**（唯一真相源）。
///
/// 🔴 **身份进名字，不进目录级标记** —— 理由整段住模块头注第四节第 1 条。
/// 抽成函数的理由与 `local_backend::local_extract_name` 逐字同源：判据要断这条命名规则，
/// 而判据自己抄一份 `format!` 就成了「测自己的副本」。
///
/// ⚠ **今天不带可执行文件后缀**，如实记：算它要平台原语，而本 crate 的平台原语
/// 只许住 `platform/` 那一层；daemon 今天的运行目标是 Linux（后缀是空串）。
/// 哪天这一层要在 Windows 上真跑，补后缀与补一条平台例外是同一件活，不是本轮的。
pub fn landing_name(build_id: &str, arch: &str) -> String {
    format!("code-picture-{build_id}-{arch}")
}

// ───────────────────────────── 四个事实 ─────────────────────────────

/// 事实一：**落点那个文件本身**。
///
/// 与 `sftp::TargetBinary` 同形而**不是同一份**：那一份吃的是 SFTP 会话、住 monitor 那棵树的
/// 二进制 crate 里，本 crate 够不着它。⇒ 这是**第二个实例，如实登记**；要合并得先有一个
/// 共享 crate，那是一件活，不是本轮顺手能做的。
///
/// **「问不出来」不许读成上面任何一个确定答案** —— 这一条与那一份逐字同源。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Landed {
    /// 在，且有字节。
    Present,
    /// 明确不在。
    Missing,
    /// 在，但是 0 字节（崩在写一半、或被截断）。
    Empty,
    /// 问不出来（无权限 / IO 失败）。
    Unknown,
}

/// 事实二之内核：**真的去算过一遍之后**得到的结论。三档，没有「没算」这一档。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Checked {
    /// 现打的 SHA-256 与钉住的那个逐字相符。
    Verified,
    /// 现打过，不符。
    Mismatch,
    /// 字节读不出来（权限 / IO）——不许读成上面任何一个。
    Unreadable,
}

/// 事实二：**那些字节是不是钉住的那一份**。
///
/// 🔴 [`Integrity::NotChecked`] 是本类型存在的全部理由：用 `bool` 装校验结果，
/// 「没算过」就会被压进 `false`（当成不符 ⇒ 每次都重拉）或压进 `true`（静默信任一份
/// 来历不明的字节）。⇒ 它必须是**第三档**，而且不许有人把它读成前两档之一。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Integrity {
    /// 算过了。
    Checked(Checked),
    /// **没算过。**
    NotChecked,
}

/// 事实三：**网络那一跳**。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Transport {
    /// 拿到了这么多字节。
    Got(u64),
    /// 连不上（断网 / DNS / 连接被拒 / 超时）。
    Offline,
    /// 对面答了，但不是 200。
    Status(u16),
    /// 对面给的东西比调用方给的上限还大（见 [`within_cap`]）。
    ///
    /// ⚠ 里面那个数是**下界**（「我们停在这儿」），不一定是对面那份东西的真长度 ——
    /// 真长度要读完才知道，而读完正是上限要拦的事。[`Face::Oversize`] 那句话逐字说「至少」。
    Oversize(u64),
}

/// 事实四：**落盘那一跳**。
///
/// ⚠ **「磁盘满」与别的 IO 错今天分不开，如实记**：分得开要 `raw_os_error` 与一个
/// 平台常量，而平台原语只许住 `platform/` 那一层。⇒ 它们共用 [`Landing::Io`] 那一格，
/// 那一句话把两种可能都说出来。
///
/// ⚠ 〔09-10 接线那一拍补〕[`Landing::Io`] 那一格今天还装着**第三种**，一并写明：
/// 落点上已经躺着一份同名文件（多半是上次写一半崩下的 0 字节残骸）。
/// 本层的落盘只有 `O_EXCL` 新建一条路，撞上既有文件就失败 —— 而删 / 改名 / 截断
/// 三个动词**全被只读白名单层禁着**。⇒ **本层修不好那种残骸，只能出声。**
/// 这是「daemon 不许改动用户既有数据」这条性质的**直接代价**，不是缺陷；
/// 那一句话把这一种也说出来了。
///
/// # 🔴 〔`K-R55` 09-11〕第四种从 [`Landing::Io`] 里**拆出来了** —— [`Landing::Unsupported`]
///
/// `K-R52` 那一拍在这里逐字登记过一笔债：「非 unix 平台上这一跳没有实现」当时并进了
/// [`Landing::Io`]，而 [`Face::LandingIo`] 那一句用户文案说的是磁盘满 / 目录不存在 / 残骸，
/// **一个字都没提这第四种** ⇒ 它自己把状态写成「**不撒谎，但说得不够准**」。
/// 加成员要同拍改 `src/doc/IPC-PROTOCOL.md`（[`crate::sidecar_fetch_guard`] 双向对账钉着），
/// 而那份文件不在 `K-R52` 的写区。
///
/// ⇒ `K-R55` 把它开成独立成员，**不是把那一句文案改宽**：
/// 把一个可判别的状态压进笼统状态，是本区最贵的那条病 ——
/// 用户看到「磁盘满 / 去把残骸拿走」会去做一件**在这台机器上永远不会有用**的事。
/// 新那一格的话逐字说「这台机器上这条路没有实现」，处置也不同（不是清盘，是换台机器 / 等实现）。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Landing {
    /// 落住了。
    Landed,
    /// 写不进去（无写权限 / 只读挂载）。
    Denied,
    /// 别的 IO 错（磁盘满也落这一格）。
    Io,
    /// 🔴 **这台机器上这一跳没有实现** —— 与「去写了、写失败了」不是一件事。
    ///
    /// 今天唯一的来路是非 unix 平台（[`crate::platform::landing::LandFailed::Unsupported`]）：
    /// 那里没有「可执行位」这个概念，而 [`landing_name`] 也不带 `.exe`
    /// ⇒ 就算把字节原样写下去，那一份**也 exec 不起来** ⇒ 那一臂不写盘、直接说没有。
    Unsupported,
}

/// `std::io` 那一侧的错怎么翻成 [`Landing`] —— 纯映射，不碰世界。
pub fn classify_write_error(kind: std::io::ErrorKind) -> Landing {
    match kind {
        std::io::ErrorKind::PermissionDenied => Landing::Denied,
        _ => Landing::Io,
    }
}

// ───────────────────────────── 唯一失败面 ─────────────────────────────

/// 这条路上**全部**的失败。闭集，唯一住址就是本枚举。
///
/// 每个成员的 `code` 与那一句话各自只有一处出产地（[`Face::code`] / [`Face::sentence`]，
/// 两个穷尽 `match`）⇒ 加一个成员，两处编译期就会逼着一起加。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Face {
    /// 这份 daemon 没带钉。
    Unpinned,
    /// 这台机器的架构我们根本不出资产 —— 与「没带钉」**不是同一件事**（处置不同）。
    ArchUnsupported,
    /// 连不上。
    Offline,
    /// 对面答了，但不是 200。
    HttpStatus(u16),
    /// 对面给的东西超过上限。
    Oversize(u64),
    /// 拉回来的字节与钉住的哈希不符。
    HashMismatch,
    /// 盘上那一份的字节读不出来。
    BytesUnreadable,
    /// 落点写不进去。
    LandingDenied,
    /// 落盘撞上 IO 错。
    LandingIo,
    /// 落点那个文件问不出来。
    LandingUnreadable,
    /// 🔴 `K-R55`：**这台机器上落盘那一跳没有实现**（[`Landing::Unsupported`]）。
    /// 与 [`Face::LandingIo`] 刻意分开：那一格说的是「去写了、撞上 IO 错」，
    /// 处置是清盘 / 腾地方；这一格说的是「这条路在这台机器上根本没修好」，清盘没有用。
    LandingUnsupported,
}

impl Face {
    /// 线上那个词 —— 取值空间与命令级错误码同一套（`wire::Unavailable::code` 的口径）。
    pub fn code(&self) -> &'static str {
        match self {
            Face::Unpinned => "sidecar_unpinned",
            Face::ArchUnsupported => "sidecar_arch_unsupported",
            Face::Offline => "sidecar_offline",
            Face::HttpStatus(_) => "sidecar_http_status",
            Face::Oversize(_) => "sidecar_oversize",
            Face::HashMismatch => "sidecar_hash_mismatch",
            Face::BytesUnreadable => "sidecar_bytes_unreadable",
            Face::LandingDenied => "sidecar_landing_denied",
            Face::LandingIo => "sidecar_landing_io",
            Face::LandingUnreadable => "sidecar_landing_unreadable",
            Face::LandingUnsupported => "sidecar_landing_unsupported",
        }
    }

    /// 那**一句**话。每个成员恰好一句，说清「是什么坏了」与「这台机器上该动哪儿」。
    pub fn sentence(&self) -> String {
        match self {
            Face::Unpinned => "这份后端没有带着代码全景 sidecar 的下载钉（资产基址 / release / 版本 / 哈希，四样缺至少一样）——它是发版时注进来的，换一份正式发版的后端即可。".to_string(),
            Face::ArchUnsupported => "我们没有为这台机器的 CPU 架构出过代码全景 sidecar——这不是配置问题，换后端也没用。".to_string(),
            Face::Offline => "取代码全景 sidecar 时连不上下载地址：这台机器现在到不了外网，或被出网策略挡住了。".to_string(),
            Face::HttpStatus(s) => format!("取代码全景 sidecar 时对面回了 HTTP {s}：那个资产在这一版的 release 上没挂出来，或地址被中间层改写了。"),
            Face::Oversize(n) => format!("取代码全景 sidecar 时对面给的东西至少有 {n} 字节，超过了上限——收到的多半不是那个资产本身（错误页 / 重定向）。"),
            Face::HashMismatch => "取回来的代码全景 sidecar 与这份后端钉住的哈希不符：字节已丢弃、没有落盘。要么下载被改写过，要么那个 release 上挂的不是这一版。".to_string(),
            Face::BytesUnreadable => "盘上那份代码全景 sidecar 读不出来（权限或 IO），因此无法确认它是不是钉住的那一份。".to_string(),
            Face::LandingDenied => "代码全景 sidecar 写不进落点目录：那个目录没有写权限，或挂在只读文件系统上。".to_string(),
            Face::LandingIo => "代码全景 sidecar 落盘时撞上 IO 错：磁盘满、落点目录不存在、或者那个落点上已经躺着一份同名文件——最后那种多半是上次写到一半留下的残骸，而这一层只许新增、删不掉它，要人去把它拿走。".to_string(),
            Face::LandingUnreadable => "问不出落点上那份代码全景 sidecar 在不在（stat 失败）——这一格刻意不猜：既不当它在，也不当它不在。".to_string(),
            Face::LandingUnsupported => "这台机器上还没有实现「把代码全景 sidecar 落到盘上」这一跳：那一步要在新建文件的同时给它可执行位，而这个平台上没有这个概念，落点的文件名也不带 .exe——写下去也跑不起来，所以一个字节都没写。这不是磁盘或权限的问题，腾地方、改权限都没有用。".to_string(),
        }
    }
}

// ───────────────────────────── 判定 ─────────────────────────────

/// 拿到两个事实之后，**下一步做什么**。
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Step {
    /// 直接用盘上这一份。
    Use,
    /// 先把盘上这一份算一遍哈希 —— [`Integrity::NotChecked`] 的**唯一**出口。
    Verify,
    /// 去拉一趟，附人读原因。
    Fetch(String),
    /// 到此为止，带上那唯一的失败面。
    GiveUp(Face),
}

/// 拉之前那一问：**盘上这一份能不能直接用**。
///
/// 两个事实各自独立取样，在这里合起来判 —— 形状与 `sftp::deploy_decision_at` 同源。
///
/// ⚠ **落点不是 `Present` 时，第二个入参一个字都不看**（那时它根本没有意义）。
/// 这一条由 [`tests::the_integrity_fact_is_not_read_when_the_file_is_not_there`] 钉住，
/// 否则「顺手把它也纳进条件」会悄悄造出一个「文件不在但哈希对」的荒唐格。
pub fn use_or_fetch(landed: Landed, integrity: Integrity) -> Step {
    match landed {
        Landed::Missing => Step::Fetch("落点没有代码全景 sidecar".to_string()),
        Landed::Empty => Step::Fetch("落点那份代码全景 sidecar 是 0 字节".to_string()),
        // 「问不出来」这一格与 `K-W4` 那条**刻意不同**，理由是那边有第二个事实可退、这边没有：
        // 那边 stat 失败还能退回版本标记（保守 = 与今天同答）；这里退无可退 ——
        // 当它「不在」就每次重拉，当它「在」就拿一份来历不明的字节去 exec。
        // ⇒ 唯一诚实的出口是出声。
        Landed::Unknown => Step::GiveUp(Face::LandingUnreadable),
        Landed::Present => match integrity {
            Integrity::NotChecked => Step::Verify,
            Integrity::Checked(Checked::Verified) => Step::Use,
            Integrity::Checked(Checked::Mismatch) => {
                Step::Fetch("落点那份代码全景 sidecar 与钉住的哈希不符".to_string())
            }
            Integrity::Checked(Checked::Unreadable) => Step::GiveUp(Face::BytesUnreadable),
        },
    }
}

/// 网络那一跳的判定：要么拿到字节数，要么恰好一个失败面。
pub fn transport_verdict(t: Transport) -> Result<u64, Face> {
    match t {
        Transport::Got(n) => Ok(n),
        Transport::Offline => Err(Face::Offline),
        Transport::Status(s) => Err(Face::HttpStatus(s)),
        Transport::Oversize(n) => Err(Face::Oversize(n)),
    }
}

/// 刚拉回来那份字节的判定。
///
/// 🔴 入参是 [`Checked`] 而不是 [`Integrity`]：**刚拉回来的东西不可能「没算过」**，
/// 而让「没算过」在类型上进不来，比在这里写一条 `unreachable!()` 强一档。
/// ⚠ 这里 [`Checked::Mismatch`] 的答案与 [`use_or_fetch`] 里**不同**（那边重拉、
/// 这边到此为止）：同一个事实，两处不同的处置，所以是两个函数。
pub fn integrity_verdict(c: Checked) -> Result<(), Face> {
    match c {
        Checked::Verified => Ok(()),
        Checked::Mismatch => Err(Face::HashMismatch),
        Checked::Unreadable => Err(Face::BytesUnreadable),
    }
}

/// 落盘那一跳的判定。
pub fn landing_verdict(l: Landing) -> Result<(), Face> {
    match l {
        Landing::Landed => Ok(()),
        Landing::Denied => Err(Face::LandingDenied),
        Landing::Io => Err(Face::LandingIo),
        Landing::Unsupported => Err(Face::LandingUnsupported),
    }
}

/// `R2` ④ 要的那一半：把一次失败翻成**握手帧上的负面能力项 + 那一句话**。
///
/// 🔴 **两半是分开回的，因为线上只装得下前一半**：`wire::Unavailable` 只有两个字段。
/// 那一句话今天没有线上住址 —— 这不是本函数漏了，是那个结构今天就这样，
/// 由 [`crate::sidecar_fetch_guard`] 钉住（它一红，就是有人给 wire 加了字段，
/// 那天回来把这里改成一个值）。
///
/// ⚠ 命令名是**入参**，本层不写死 —— 与 `plugin::discover::find` 同一条纪律：
/// 哪几条命令属于全景那一族，是 `inbound::REGISTRY` 的知识，不是本层的。
pub fn unavailable_for(commands: &[&str], face: &Face) -> (Vec<wire::Unavailable>, String) {
    let entries = commands
        .iter()
        .map(|c| wire::Unavailable {
            command: (*c).to_string(),
            code: face.code().to_string(),
        })
        .collect();
    (entries, face.sentence())
}

// ───────────────────────────── SHA-256 ─────────────────────────────

/// SHA-256 的那 64 个轮常量（FIPS 180-4）。
const ROUND: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// 压一个 64 字节的块。
fn compress(h: &mut [u32; 8], block: &[u8]) {
    let mut w = [0u32; 64];
    for (i, quad) in block.chunks_exact(4).enumerate() {
        w[i] = u32::from_be_bytes([quad[0], quad[1], quad[2], quad[3]]);
    }
    for i in 16..64 {
        let a = w[i - 15];
        let b = w[i - 2];
        let s0 = a.rotate_right(7) ^ a.rotate_right(18) ^ (a >> 3);
        let s1 = b.rotate_right(17) ^ b.rotate_right(19) ^ (b >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = *h;
    for (k, wi) in ROUND.iter().zip(w.iter()) {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ ((!e) & g);
        let t1 = hh
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(*k)
            .wrapping_add(*wi);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        hh = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (slot, add) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
        *slot = slot.wrapping_add(add);
    }
}

/// 一整份字节的 SHA-256，小写 hex。
///
/// 为什么是手写的、接住它的是什么、以及被撤掉的那条错理由 —— 见模块头注「二之二」。
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut blocks = bytes.chunks_exact(64);
    for b in &mut blocks {
        compress(&mut h, b);
    }
    let rest = blocks.remainder();
    let mut tail = [0u8; 128];
    tail[..rest.len()].copy_from_slice(rest);
    tail[rest.len()] = 0x80;
    let tail_len = if rest.len() + 1 + 8 <= 64 { 64 } else { 128 };
    let bits = (bytes.len() as u64).wrapping_mul(8);
    tail[tail_len - 8..tail_len].copy_from_slice(&bits.to_be_bytes());
    for b in tail[..tail_len].chunks_exact(64) {
        compress(&mut h, b);
    }
    h.iter().map(|w| format!("{w:08x}")).collect()
}

/// 拿一份字节去对钉住的那个哈希。
///
/// ⚠ 比的是**小写 hex 的逐字相等**：钉是我们自己编进去的常量，不是用户输入，
/// 这里要挡的是「字节不是那一份」，不是一个会被计时的攻击者。
pub fn verify_bytes(bytes: &[u8], pinned_sha256: &str) -> Checked {
    if sha256_hex(bytes) == pinned_sha256 {
        Checked::Verified
    } else {
        Checked::Mismatch
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SHA-256 的三条标准向量 —— **它们来自 FIPS 180-4 / NIST 的公开例，不是本实现算出来的**。
    ///
    /// 三条各有各的活：空串（长度 0 的填充）· `abc`（一块之内）·
    /// 56 字节那条（填充恰好挤到第二块，正是手写实现最容易写错的那一格）。
    #[test]
    fn the_hand_written_sha256_matches_the_published_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    /// 反向那半：喂一份差一个字节的输入，它真的会给出别的答案（不是恒答一个串）。
    #[test]
    fn the_hash_really_depends_on_the_bytes() {
        assert_ne!(sha256_hex(b"abc"), sha256_hex(b"abd"));
        assert_eq!(verify_bytes(b"abc", &sha256_hex(b"abc")), Checked::Verified);
        assert_eq!(verify_bytes(b"abc", &sha256_hex(b"abd")), Checked::Mismatch);
    }

    /// 🔴 今天这份 daemon **没有带钉** —— 而它给出的是一句说得出缺什么的话，不是一个默认值。
    ///
    /// 这一格红的那天 = CI 真开始往里注钉的那天。那时把这条改成正向断言，
    /// 并**同轮**回答「注的那个 tag 是谁算的」。
    #[test]
    fn this_build_carries_no_pin_and_says_so_instead_of_guessing() {
        for arch in ARCHES {
            assert_eq!(
                pin(arch),
                Err(Face::Unpinned),
                "{arch} 这一格拿到了钉 —— 本条的期望该翻面了"
            );
        }
        assert_eq!(
            pin("mips64"),
            Err(Face::ArchUnsupported),
            "「我们不出这个架构」被压进了「没带钉」—— 那句话会给出一条做了也没用的建议"
        );
    }

    /// 两个名字解决两个问题：资产名**不带** `build_id`，落点名**带**。
    ///
    /// 🔴 后者是 `K-W4` 那个坑的解法本体（身份进文件名，不进目录级标记）——
    /// 两台不同版本的 daemon 落在同一个目录里也不会撞。
    #[test]
    fn the_landing_name_carries_the_identity_and_the_asset_name_does_not() {
        assert_eq!(asset_name("x86_64"), "code-picture-x86_64");
        assert!(
            !asset_name("x86_64").contains("p2e"),
            "资产名不该带版本：它只负责找得到"
        );
        let a = landing_name("p2e-dial", "x86_64");
        let b = landing_name("p9z-next", "x86_64");
        assert_ne!(
            a, b,
            "两个版本必须落成两个文件名，否则就是 08-11 那次互相覆盖"
        );
        assert!(a.contains("p2e-dial"), "落点名里没有版本 ⇒ 撞名防不住");
    }

    /// 落点不是 `Present` 时，**第二个事实一个字都不看**。
    ///
    /// 反向那半在这里很要紧：只断「Missing ⇒ Fetch」的话，一个把两个事实
    /// `&&` 在一起的实现照样绿。这条把四种 `Integrity` 全喂一遍，答案必须一模一样。
    #[test]
    fn the_integrity_fact_is_not_read_when_the_file_is_not_there() {
        let every = [
            Integrity::NotChecked,
            Integrity::Checked(Checked::Verified),
            Integrity::Checked(Checked::Mismatch),
            Integrity::Checked(Checked::Unreadable),
        ];
        for absent in [Landed::Missing, Landed::Empty, Landed::Unknown] {
            let answers: Vec<Step> = every.iter().map(|i| use_or_fetch(absent, *i)).collect();
            assert!(
                answers.windows(2).all(|w| w[0] == w[1]),
                "{absent:?} 这一格的答案被第二个事实改变了：{answers:?}"
            );
        }
    }

    /// 「没算过」既不是「相符」也不是「不符」—— 三个取值给三个不同的下一步。
    #[test]
    fn not_checked_is_its_own_answer() {
        assert_eq!(
            use_or_fetch(Landed::Present, Integrity::NotChecked),
            Step::Verify
        );
        assert_eq!(
            use_or_fetch(Landed::Present, Integrity::Checked(Checked::Verified)),
            Step::Use
        );
        assert!(matches!(
            use_or_fetch(Landed::Present, Integrity::Checked(Checked::Mismatch)),
            Step::Fetch(_)
        ));
        assert_eq!(
            use_or_fetch(Landed::Present, Integrity::Checked(Checked::Unreadable)),
            Step::GiveUp(Face::BytesUnreadable)
        );
    }

    /// 同一个「不符」，拉之前与拉之后是**两个不同的答案** —— 这正是两个判定函数的理由。
    #[test]
    fn a_mismatch_means_refetch_before_and_stop_after() {
        assert!(matches!(
            use_or_fetch(Landed::Present, Integrity::Checked(Checked::Mismatch)),
            Step::Fetch(_)
        ));
        assert_eq!(
            integrity_verdict(Checked::Mismatch),
            Err(Face::HashMismatch)
        );
    }

    /// 四个事实的每一个非成功取值，都恰好落到一个 [`Face`] 上，**而且互不相同**。
    ///
    /// 「互不相同」是这条真正在买的东西：两种坏法共用一个 code，用户看到的就是一句
    /// 说不清是哪一种的话 —— 那等于没有失败面。
    #[test]
    fn every_failing_fact_has_its_own_single_exit() {
        let mut faces = vec![
            use_or_fetch(Landed::Unknown, Integrity::NotChecked),
            use_or_fetch(Landed::Present, Integrity::Checked(Checked::Unreadable)),
        ]
        .into_iter()
        .filter_map(|s| match s {
            Step::GiveUp(f) => Some(f),
            _ => None,
        })
        .collect::<Vec<_>>();
        for t in [
            Transport::Offline,
            Transport::Status(404),
            Transport::Oversize(1),
        ] {
            faces.push(transport_verdict(t).unwrap_err());
        }
        for c in [Checked::Mismatch, Checked::Unreadable] {
            faces.push(integrity_verdict(c).unwrap_err());
        }
        for l in [Landing::Denied, Landing::Io] {
            faces.push(landing_verdict(l).unwrap_err());
        }
        faces.push(pin("x86_64").unwrap_err());
        faces.push(pin("mips64").unwrap_err());

        let mut codes: Vec<&str> = faces.iter().map(|f| f.code()).collect();
        codes.sort_unstable();
        let n = codes.len();
        codes.dedup();
        assert_eq!(
            codes.len(),
            n - 1, // `BytesUnreadable` 从两条路上各来一次：盘上那份读不出来，与校验时读不出来
            "失败面出现了共用的 code：{codes:?}"
        );
        for f in &faces {
            assert!(!f.sentence().is_empty(), "{f:?} 没有那一句话");
            assert!(
                f.code().starts_with("sidecar_"),
                "{f:?} 的 code 不在本层的命名空间里"
            );
        }
    }

    /// 上限这一关：边界两侧各一格，等于上限那一格**不算超**。
    #[test]
    fn the_cap_is_an_argument_and_it_bites_on_both_sides() {
        assert_eq!(within_cap(10, 10), Transport::Got(10));
        assert_eq!(within_cap(11, 10), Transport::Oversize(11));
        assert_eq!(
            transport_verdict(within_cap(11, 10)),
            Err(Face::Oversize(11))
        );
    }

    /// 成功那一侧也要有格，否则上面那几条可以靠「恒 Err」全过。
    #[test]
    fn the_success_arms_really_succeed() {
        assert_eq!(transport_verdict(Transport::Got(7)), Ok(7));
        assert_eq!(integrity_verdict(Checked::Verified), Ok(()));
        assert_eq!(landing_verdict(Landing::Landed), Ok(()));
        assert_eq!(
            use_or_fetch(Landed::Present, Integrity::Checked(Checked::Verified)),
            Step::Use
        );
    }

    /// 写错那一侧的分类：无写权限有自己的一格，别的 IO 错落另一格。
    #[test]
    fn a_denied_write_is_not_folded_into_the_generic_io_face() {
        assert_eq!(
            classify_write_error(std::io::ErrorKind::PermissionDenied),
            Landing::Denied
        );
        assert_eq!(classify_write_error(std::io::ErrorKind::Other), Landing::Io);
        assert_ne!(
            landing_verdict(Landing::Denied),
            landing_verdict(Landing::Io)
        );
    }

    /// 地址是三样钉拼出来的，缺一样这个函数根本调不到（[`pin`] 先挡住）。
    #[test]
    fn the_url_is_assembled_from_the_pin_and_nothing_else() {
        let p = Pin {
            base: "https://example.invalid/download",
            tag: "v9.9.9",
            build_id: "b9",
            sha256: "00",
        };
        assert_eq!(
            asset_url(&p, "aarch64"),
            "https://example.invalid/download/v9.9.9/code-picture-aarch64"
        );
    }

    /// 那一句话与线上那一半是**分开**交出来的。
    #[test]
    fn the_wire_half_and_the_sentence_half_come_back_separately() {
        let (entries, sentence) =
            unavailable_for(&["panorama-overview", "panorama-search"], &Face::Offline);
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().all(|e| e.code == "sidecar_offline"));
        assert!(sentence.contains("连不上"), "那一句话丢了：{sentence}");
    }
}
