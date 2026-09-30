//! 通信层边界登记表的判据本体 —— 模块头注（这张表为什么存在 · 成员怎么认 ·
//! 买到什么买不到什么）住 `src/frontend/shell/src/comm_boundary_registry.rs`，不在这里抄第二份。

use std::collections::BTreeSet;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    // 住址唯一源：`crate::guard_support`（头注写着 24 份副本怎么一起漂的）。
    crate::guard_support::repo_root()
}

// ════════════════════════════════════════════════════════════════════════════
//  一、边界登记表本体（`设计/05 §8` 步 1 的另一半：画出通信层的边界）
// ════════════════════════════════════════════════════════════════════════════

/// ★★ **边界登记表** —— `(仓根相对路径, 为什么它属于通信层)`。
///
/// 🔴 **今天 4 份，两拍进来的。** `步 3`（09-20）圈进头两份 —— 人群第一次非空；
/// `步 4`（09-21）圈进 `relay/` 那两份 —— **`C4` 第一次判得动它自己那句话点名的东西**。
/// 步 1 逐字「判据先对空集成立，随搬迁逐步收紧」——「收紧」从步 3 那一拍开始。
///
/// # ⚠ 步 3 的题面是「把**传输面**圈出来」，而传输面**一份都没进来**
///
/// `§8` 步 3 逐字：「把传输面（SSH / SFTP / 池 / 重连）从 `monitor` 的 Rust 半
/// **圈出来**，业务先不动」。现打的结论是：**那三份今天一份都圈不进来** ——
/// `ssh_source.rs` / `sftp.rs` / `sftp_pool.rs` 的**公开面**上都命名了业务概念，
/// 而 `设计/05 §2` 逐字「**`C1` 的豁免必须为零**」⇒ 不许开口子，只能不圈 ＋ 写清。
/// 逐份咬在哪（名字 · 处数 · 判词）写在 `真相源/` 那份读数里，**不在这里抄第二份**。
/// ⚠ 〔2026-09-21 换射程后现打〕**三份一份都没掉到零** —— 处数掉了约 91%，
/// 而剩下的那些是它们自己起的公开名字（`is_safe_remote_jsonl`〔散文墓碑〕（〔RW1〕已随 F11 删）· `SESSION_CHANNEL_CAP` ·
/// `InboundFrame` 的会话/tmux 变体一族…），**改得了，所以它是账不是命**。
///
/// ⇒ 下面**头两份**是**通信层自己的词汇**那一档（`§2` 逐字四样里的「地址」与
/// `§4.5.2` 的「失败语义」），不是传输面。**别把非空读成「传输面进来了」。**
///
/// # 🔴 〔`设计/05 §8` 步 4，2026-09-21〕后两份：`C4` 第一次有**真东西**可判
///
/// 步 4 逐字是「**`creds.rs` ＋ `table.rs` 搬去后端**，通信层改成『收一张表』
/// ｜ ✅ `C4` 从这一步起可以真断言零读盘」。现打两句：
///
/// 1. **搬家那半已经做完了**（`99 §4` 的 `14-i` 带走的）—— 那两份今天住
///    `src/backend/accounts/`（先住 `relay/accounts/`，2026-09-24 搬出中转层），与 `impl Destinations`（那张决策表的唯一实现）同一层，
///    中转手里只剩一个 `Arc<dyn Destinations>` ⇒「收一张表」的**接法**也在盘上了。
/// 2. **而「圈进来」那半一直没人做** ⇒ `C4` 一直绿着，却**一份 `relay/` 的文件都没扫过**：
///    它成立在一个不含被它点名的那个东西的人群上。这一拍补的就是那一半。
///
/// ⚠ **只进得来两份，不是整层。** `设计/05 §4.3` 把 `relay/` 按 `C4` 切开、归通信层那一列
/// 点了六份，加上后来的 `listen.rs` 共七份，而只有 `route.rs` 与 `http1.rs` 十一条全绿。
///
/// 🔴 **还进不来的那几份各被哪几条咬，从 2026-09-21 起不再写在这段散文里** ——
/// 它有了自己的判据（`the_relay_files_left_outside_are_blocked_by_exactly_the_criteria_the_prose_names`，
/// 逐份**两向集合相等**），编号与逐份理由住 [`RELAY_LEFT_OUTSIDE`]。
/// 先前这里逐字写着那几份的编号，而**没有任何东西在看它** —— 那一拍就抓到一处已经假了的：
/// 那句「`tee.rs` 带业务词并 `try_send`（`C1` ＋ `X4`）」在新射程下只剩 `X4`。
/// ⇒ 一个事实一个住址（`E12`），而这个住址现在是**机检的**那一侧。
/// **豁免仍为零** —— 变的是人群与射程，不是例外。
///
/// ⚠ 另有一格是**全绿而不该圈**（`真相源/100 §二` 那一形的第二例）：
/// `accounts/policy.rs` 十一条一条不咬，而它是**上游选择** —— 那份凭据文件的热重载，
/// 正是 `C4` 那句话要挡在外面的那一类。**不圈它**，理由写在这里而不是等人来问。
///
/// # 往里加一条要同拍做三件事，缺一当场红
///
/// 1. 给那份文件的**文件头注释**盖上 [`MARK`] 那枚标记（盘上那一侧）；
/// 2. 在这张表里加一行，理由写**为什么它是纯传输**，不是「先放这儿」；
/// 3. 把 `src/frontend/shell/src/comm_boundary_registry.rs` 头注里那句
///    「登记在册的通信层成员：N 份」的 N 改掉（散文那一侧）。
///
/// ⚠ 这张表**不是**豁免清单。进了这张表的文件从此被 `C1`–`C5` ＋ `X1`–`X6`
/// 十一条一起管着 —— 登记是**上锁**，不是**放行**。
const REGISTERED: &[(&str, &str)] = &[
    (
        "src/comms/inward/origin.rs",
        "面 A 的**寻址键**本体。`设计/05 §2` 逐字列了这一层认识的四样东西，第一样是\
         「**地址**（`origin` / 路由键）」；`§4` 那张一层两面图里面 A 的寻址逐字就是 `origin`。\
         它**只是**那个地址：零业务词 · 零读盘 · 零起进程 · 零期限字面量。\
         步 2 刚把它收得更紧（`Unspecified` 退役 ⇒ 地址只有一种线上形状，`null` 进不来）。",
    ),
    (
        "src/comms/inward/backend_route.rs",
        "面 A 的**失败语义**本体（`设计/05 §4.5.2`）。它把 `CallError` 翻成三态，\
         判准逐字是「能不能证明这条命令根本没发出去」—— 那就是 `§3.3.1` 的 `reach` \
         在今天这棵树上的样子，也是 `X1` 点名的三个线上类型之一（`CallError`）\
         今天唯一一处**穷尽**的 `match`。它不知道会话/账号/skill/agent/tmux，\
         不读盘、不起进程、不绑端口、不写期限。\
         ⚠ `CallError` 的**定义**所在（同目录 `inbound_client.rs`）**没有**跟着进来：\
         C1 在它身上咬到 `agent` 三处（cc-bus 的 `extras.agent`）、X4 咬到一处 `try_send`。\
         类型的家还在外面，而用它做分流的这一份先进来了 —— 那是 C1 指的下一刀，不是矛盾。",
    ),
    // ── 〔步 4，2026-09-21〕面 B 那一侧：凭据搬走之后**只收不取**的那两份 ──────────
    (
        "src/comms/outward/route.rs",
        "面 B 的**路由键**本体 —— `设计/05 §2` 四样里第一样「**地址**（`origin` / 路由键）」\
         在外向那一面的样子。更要紧的是它是 `设计/01 §2.1` 那条 🔴「怎么做到零豁免」的**现物**：\
         逐字「通信层的类型里用**位置**称呼它搬的东西（「路径的第 1/2 段」＋一个不透明的流标签），\
         业务名只出现在后端那一半的实现里」⇒ 盘上就是 `Route` ＋ `RouteKey{ seg1, seg2 }`（条 48）。\
         它**只切键、不解释键**：谁是 agent、谁是账号只在 `accounts/` 那一层才有名字。\
         零读盘 · 零环境变量 · 零期限字面量 —— 那正是步 4 要买的「只收不取」。",
    ),
    // ── 〔`P16` 2026-09-22 用户裁〕中转本体与它的契约本体 ────────────────────────
    (
        "src/comms/outward/mod.rs",
        "面 B 的**层间契约本体** —— `Destination` / `Destinations` / `Mode` / `RouteKey` / \
         `AuthSwap` 都住这儿，`设计/05 §4.3` 归通信层那一列也点名了它。\
         🔴 **那道「边界契约文件自己算不算成员」的题，用户 2026-09-22 裁「照圈」**，\
         裁词逐字：本文件里那行 `mod accounts;`（`accounts/` 是上游选择）是 **Rust 模块树的\
         机械产物** —— 子模块只能由父模块声明，语言里没有第二种写法 ⇒ \
         **让一行语言层面的声明去否决一份文件的架构归属，是让语言产物驱动架构裁决**。不办。\
         ⚠ 这一裁**配了一条硬判据**（同拍立的，见 [`assert_membership_does_not_inherit_down_the_module_tree`]）：\
         成员资格**不沿模块树往下传** —— `accounts/**` 一份都不许自称成员。\
         没有那一条，这一行登记在语义上就等于把上游选择一起圈进来。",
    ),
    (
        "src/comms/outward/server.rs",
        "面 B 的**交换面**本体（`20 §4` 要的那个名字正是 `exchange.rs`），\
         `设计/05 §4.3` 归通信层那一列的第一个。\
         它先前差两条，`P16` 同拍清掉：`C2`（`Destination::Substitute` 不再带 `creds-core` \
         的类型，连明文都碰不到了 —— 那一句搬去了上游选择）＋ `X2`（那个 30 秒的**值**搬去 \
         `listen.rs`，装它的那一手留在本文件、改成收入参）。\
         ⚠ 这枚标记**不买**「这一层做得对」：请求头拼得对不对由 `wire_golden` 的逐字节\
         金标准与 `server_tests` 那一族负责，它只买「没长业务、没伸手拿东西」。",
    ),
    // ── 〔`P16`，2026-09-22〕面 B 那一侧第三份：期限的**值**搬走之后才进得来 ──────
    (
        "src/comms/outward/upstream.rs",
        "面 B 的**上游那一跳**本体 —— 它头注第一行逐字「连出去、把请求**原样**递上去」，\
         那就是 `设计/05 §2` 四样里的「**载荷**（不透明字节）」在外向那一面的样子。\
         归属的依据是 `设计/01 §0` 那张图（面 B ＝ agent ↔ 上游 API，**本来就在通信层内**）\
         ＋ `设计/05 §4.3` 归通信层那一列点名了它。它既不是上游选择（那是 `accounts/`）、\
         也不是一张登记表 ⇒ **不属于** `真相源/100 §二` 那个「全绿而不该圈」的形状。\
         **它先前只差 `X2` 一条**：那个 600 秒的**值**住在它自己文件里；`P16` 按 \
         `设计/01 §2.1 C4`（「**期限值**全部由后端交给它」）把值搬去 `listen.rs`，\
         装它的那一手仍留在本文件（`connect` 收入参，`§3.3.2`「值归后端 · 执行归通信层」）\
         ⇒ 十一条现打全绿。\
         ⚠ **它带着 TLS 与 tee 明文进来，而一条豁免都没开**（`C1` 的豁免必须为零）：\
         TLS 是「怎么搬」（`裁-1`：上游是 `https://`，而 TLS 不能手写）、tee 是「搬完抄一份」，\
         两者都不需要它认识会话/账号/agent —— 它的**公开面**上一个业务名都没有。\
         ⚠ 这枚标记**不买**「TLS 配得对」（那归 `wire_golden` 与 `tls_config` 自己的判据），\
         也**不买**「tee 到的明文不该落盘」（那归 `creds_guard`）。",
    ),
    (
        "src/comms/outward/http1.rs",
        "面 B 的**协议编解码**本体 —— `设计/05 §4.2` 那张「四样不共享」表里，面 B 的「协议」\
         一栏逐字是「手写 HTTP/1.1 ＋ SSE」。它只解析中转必须懂的那几样（请求行 · 头 · \
         `Content-Length` · chunked 拆帧），别的一律当**不透明字节** —— 就是 `§2` 四样里的\
         「**载荷**（不透明字节）」。零 HTTP 框架（`K8`/`D4` 白名单一字没松）· 零业务 · 零读盘。\
         ⚠ 它**不买**「HTTP 解析对不对」—— 那由 `http1_tests.rs` 与 `wire_golden` 的\
         逐字节金标准负责；这枚标记只买「它没在这一层里长出业务、也没伸手去拿东西」。",
    ),
    // ── 〔SC1 · 第四波 4B〕面 B 那一侧：分帧从 `http1.rs` 里抽出来单住一份，同拍圈进来 ─────
    (
        "src/comms/outward/framer.rs",
        "面 B 协议编解码的**分帧**那一半 —— 先前就住在成员 `http1.rs` 的 `ChunkedView` 里\
         （外加 `tee.rs` 的一份手抄），`设计/17 §3.7` 把两份收成一个增量分帧器。\
         它只认**字节与一个分隔符**，不认里面是什么（`§2` 四样里的「载荷（不透明字节）」）：\
         零业务词 · 零读盘 · 零环境变量 · 零起进程 · 零绑端口 · 零期限 · 零尺寸常量（上限由调用方给）。\
         🔴 **不同拍圈进来就是变松**：那段代码在 `http1.rs` 里时受十一条管着，搬出来不盖标记就出了锁。\
         ⚠ 这枚标记**不买**「切得对 / 是 O(n)」—— 那由 `framer_tests.rs` 的次数与长度相等断言负责。",
    ),
    // ── 〔DEL〕面 B 那一侧：tee 的 NDJSON 行落点删了之后，挡它的 `X4` 清空，同拍圈进来 ─────
    (
        "src/comms/outward/tee.rs",
        "面 B 上「搬完抄一份」的那一半（`设计/05 §4.3` 归通信层那一列点名了它；`设计/20 §11` 挂载物 ①）：\
         拆 SSE 的 `data:` 行、给每件占号、交给宿主的 tap 口。它不认识会话 / 账号 / agent（流标签是不透明串，\
         路由那两段不进 tee —— `20 §11` I2「① 不问账号」）· 零读盘 · 零环境变量 · 零起进程 · 零绑端口 · 零期限。\
         先前挡它的 `X4`（NDJSON 行落点的 `try_send`：投不进就丢、不说）随独立 `--relay` 一起删了；\
         tap 那一形「丢必须说」由位置号 `n` 原位兑现（`05 §3.3.4` 的 `Gap` 那一形，纯算术）。\
         ⚠ 这枚标记**不买**「抄得全」—— 那由 `host_tests` 的逐件相等与缺口判据负责。",
    ),
    // ── 〔面 A 第一个外部客户端，2026-09-24〕通道那三份：进来那天就是十一条全绿 ─────────
    //    用户裁「甲, 窗口变成独立前端」：文件窗口是独立进程，够不着后端 ⇒ 它是又一个前端，
    //    说的正是 `01 §2.2` 那两个动作。同目录另两份（`host.rs` 绑口造钥匙、`dial.rs` 拨号）
    //    **刻意不圈** —— 它们做的正是 `C4`/`C5` 不许成员做的事，理由逐字住那两份的头注。
    (
        "src/comms/inward/chan/wire.rs",
        "面 A 的**线上词汇本体** —— `设计/05 §3.3.0` 那「五个不透明类型 ＋ 一个手柄 ＋ 一个跳号」、\
         `§3.3.1` 的三层错误、`§3.3.4` 的 `Item`、以及 `Comms`/`Sub` 两个 trait 第一次在盘上有了类型。\
         每一个公开名字都是位置名或传输词（`01 §2.1`：用**位置**称呼它搬的东西）。\
         载荷走帧体、不进 JSON ⇒ 对载荷形状零假设；帧长上限由调用方给（`C4`），本文件零尺寸常量、零期限常量。",
    ),
    (
        "src/comms/inward/chan/router.rs",
        "面 A 上**进程外前端**进来的那扇门：认证 ＋ 按 `origin` 把 `call`/`subscribe` 转给**注入的**句柄 ＋ \
         撤单 ＋ credit。它是一个**纯路由器**：`op`/`kind`/载荷原样交出去、一个都不解释（`C1`）；\
         钥匙、帧长上限、认证等待时长全由宿主交进来（`C4`）；**绑回环与 `accept` 在宿主那一份**，\
         它只有 `serve(stream)`（`C5`，照 `01 §2.1` 面 B 那个先例逐字同形）。\
         ⚠ 它**不买**「那个 `origin` 真有人服务」—— 那是句柄的活。",
    ),
    (
        "src/comms/inward/chan/client.rs",
        "`01 §2.2`「前端只有两个动作」在**进程外前端**手里的样子 —— 它实现 `05 §3.3.0` 的 `Comms`，\
         签名参数名与顺序一字不改（`budget` 是绝对时刻 · `from` 原样过线 · `want` 是 credit）。\
         它收的是一条**已经连好**的流与一把**已经交到手里**的钥匙：不拨号（拨号在 `dial.rs`，不是成员）、\
         不读钥匙、不造期限（`C4`/`C5`/`X2`）。⚠ 它**不买**自动重连：只有交给它的那一条流。",
    ),
    // ── 〔C4a · 第四波 · 2026-09-24〕通道在 **webview** 手里的那一半（主界面第一次说 `call`）──────
    (
        "src/comms/inward/chan.ts",
        "`01 §2.2`「前端只有两个动作」在**主界面**（webview）手里的样子 —— 与 `chan/client.rs`（进程外前端那一半）\
         是同一件东西的两个住址：`call(origin, op, payload, budget)` 参数名与顺序一字不改，`Budget.until` 是绝对时刻、\
         过线换成「还剩多少」，过期不发；本地撤单立即回；三层错误按 monitor 交回的线上形状解回（解不出就 `Broken`）。\
         载荷两个方向原样（不 `JSON.parse` / `stringify`，那是调用方 ＋ `ipc/chan-caller.ts` 的事）。\
         它经包装层 `chan_call` 过 Tauri IPC；那一跳的宿主 `chan/webview.rs` **不是成员**（碰 Tauri、注入生产句柄）。\
         ⚠ 它**不买**对端撤活与 `subscribe`（webview 这一侧本拍零条流）。",
    ),
    // ── 〔C2 · 2026-09-24〕`Q6` 选甲的收回：传输面洗干净的那两份（`设计/05 §13.7`）──────────
    //    ⚠ 四份候选里 `ssh_source.rs` / `pubkey.rs` **不收** —— 理由逐份住 `TRANSPORT_LEFT_OUTSIDE`；
    //    `sftp_pool.rs` 是 `F7c` 独占，下一拍。
    (
        "src/comms/inward/ssh_link.rs",
        "面 A 的 **SSH 链路**那一段：在一条**交给它的**管子上读拨号代理的阶段行与 ack、收全结果 —— \
         `05 §2` 四样里的「流」与「载荷」（ack 之后的字节它一个都不看）。它原来埋在 `ssh_source.rs` 里；\
         C2 把 SSH 的全部活搬进后端的拨号代理之后，界面侧与 SSH 有关的**传输**就只剩这一件。\
         起代理进程、读配置、定期限都在宿主 `dial_host.rs`（不是成员，做的正是 `C4`/`C5`/`X2` 不许成员做的事）。\
         ⚠ 它**不买**「代理拨得对」—— 那归后端 `dial_tests` 与读数脚本 `C2-dial-loopback.py`。",
    ),
    // 〔MIG-1 · `99 §2.1 ⑬`〕`port_forward.rs` 那一行随文件删了（不是摘标记）：三条命令与转发账进了本机常驻后端
    //   （`src/backend/dial/forwards.rs`），界面经通道直问 —— 界面 crate 里再没有端口转发这一面。
];

/// 通信层**对前端的入口符号** —— `(符号名, 说明)`。`C3` 与 `X6` 的人群从这儿派生。
///
/// `设计/05 §3.1`：前端只有两个动作（`call` / `subscribe`）。
///
/// 🔴 〔2026-09-24，通道那一拍〕两个动作**在盘上有了**：
/// `src/comms/inward/chan/client.rs` 的 `Client` 实现了 `Comms`。
/// ⚠ **但本表仍然刻意为空**，理由是人群的形状对不上，不是入口不存在：
/// `X6` 的人群是「本表 × **TS** 前端语料」，而第一个用上它的外部前端是 **Rust**（egui 文件窗口，
/// 下一波 F2 才接）。把 `call` 填进来的话，`X6` 会去 TS 语料里数与本通道无关的 `call(`
/// （`fn.call(this, …)` 那一族），那是假红；而 Rust 那一侧今天**零个**真调用点。
/// ⇒ 这一格是一笔欠账：F2 落第一个真调用点时，`X6` 的人群要扩到 Rust 前端那一侧。
/// 住址在 `设计/05` 末尾「面 A 的第一个外部客户端：通道」的欠账那一节，不在这里抄第二份。
///
/// 🔴〔F2 · 2026-09-24〕**那一天到了**：文件窗口（`src/frontend/shell/src/filewin/`）是第一个真调用点
/// （`source::ask`）。表从两列扩成三列 —— 第二列是**这个入口的前端语料住在哪一种语言里**：
/// `call` / `subscribe` 这两个裸词在 TS 语料里另有与本通道无关的同名调用
/// （`launcher-diagnostics.ts` 的本地 `call(true)`、`session-accounts-poll.ts` 的 `subscribe(() => …)`），
/// 按语言分人群才不假红。Rust 那一侧的人群是 [`RUST_FRONTENDS`]。
const ENTRIES: &[(&str, &str, &str)] = &[
    (
        "chan.call",
        "ts",
        "〔C4a · 第四波〕**主界面**（webview）说 `call` 的入口（`src/comms/inward/chan.ts` 的 `chan.call`）。\
         入口名带着 `chan.` 前缀，是因为 TS 语料里另有与本通道无关的裸 `call(`（`fn.call(this, …)` 一族）——\
         按语言分人群之外再按全名收窄，才不假红。期限由调用方给（`Budget.within(…)`）。",
    ),
    (
        "call",
        "rs",
        "一次性请求（`05 §3.3.0` 的 `Comms::call`）—— 期限由调用方给（`Budget`，绝对时刻）",
    ),
    (
        "chan.subscribe",
        "ts",
        "〔CF2 · 第四波 4B〕**主界面**（webview）说 `subscribe` 的入口（`src/comms/inward/chan.ts` 的 `chan.subscribe`，会话内容流）。\
         同 `chan.call` 那一行按全名收窄（TS 语料里另有与本通道无关的裸 `subscribe(`）。它**没有期限参数**\
         （`05 §3.3.0`：订阅是长期意向）⇒ 只进调用点条数恒等，不进「显式给 `Budget`」那条。",
    ),
    (
        "subscribe",
        "rs",
        "订阅（`Comms::subscribe`）—— 〔F7c 09-24〕窗口恰好一处（`filewin/source.rs::watch`，传输进度流\
         `transfer/<id>`，生产上第一条流）。⚠ 它**没有期限参数**（`05 §3.3.0` 的签名逐字：订阅是长期意向，\
         不被一次调用的期限拴住）⇒ 本表这一格不进「显式给 `Budget`」那条，只进调用点条数恒等",
    ),
];

/// `X6` 的 **Rust 前端语料**：`(目录前缀, 摘掉的文件, 为什么摘)`。
///
/// ⚠ 摘掉的那一份是 **monitor 那一侧**的入口（`entry.rs`）：它调的是通道宿主注入给路由器的
/// 那个句柄（`Backends::call`，入参是「这一跳还剩多少」的 `Duration`），不是前端的 `Comms::call`。
const RUST_FRONTENDS: &[(&str, &[&str], &str)] = &[(
    "src/frontend/shell/src/filewin/",
    &["src/frontend/shell/src/filewin/entry.rs"],
    "`entry.rs` 住 monitor 进程，调的是宿主句柄 `Backends::call`，不是前端的 `Comms::call`",
)];

/// 一份文件是不是某个入口的前端语料（按 [`ENTRIES`] 第二列的语言分）。
fn is_frontend_for(rel: &str, lang: &str, member_paths: &BTreeSet<&str>) -> bool {
    if member_paths.contains(rel) {
        return false;
    }
    match lang {
        // 〔C4a · 第四波〕只算**生产**前端：`tests/` 那棵树不是调用方，是判据（`chan.vitest.ts` 自己就说了六次
        //   `chan.call(`）。测试文件整棵住 `tests/`（仓库重组 `设计/16`：src 与 tests 分离），`src/` 下没有 ——
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

/// 成员标记：一份文件属于通信层，当且仅当它的文本里带着这个词。
///
/// ⚠ **为什么是「文件自己带标记」而不是「住在某个目录下」**：恒等的两侧要异源。
/// 目录那种写法的两侧（「扫这个目录」与「表里写的路径」）都由改表的同一个人一次编辑改掉
/// ⇒ 退化成恒真。标记那一侧住在**成员文件自己的文本里**，是另一个人另一次编辑写的。
const MARK: &str = "COMM-LAYER-MEMBER";

// ════════════════════════════════════════════════════════════════════════════
//  二、语料（盘上那一侧）
// ════════════════════════════════════════════════════════════════════════════

/// 语料根 ＋ 每根**明写的**排除名单（`设计/16 §5.4b` 纪律 2、4）。
///
/// ⚠ 两个根**互不包含**（纪律 1）。排除的那两份是本判据自己的两半 ——
/// 它们里面逐字写着 [`MARK`]，收进语料就是「判据在自己的散文里找到了自己」
/// （`scanning_guard_registry` 头注治的那一族，实测栽过五次）。
/// ★ `guard_core::scan_tree_excluding` 自带「名单上每一条都必须真的摘到东西」的自检
/// ⇒ 这两份改名 / 搬走 ⇒ **当场 panic**，不会安静地多扫两份。
const CORPUS_ROOTS: &[(&str, &[&str])] = &[
    ("src", &["src/frontend/shell/src/comm_boundary_registry.rs"]),
    (
        "tests",
        &["tests/frontend/shell/comm_boundary_registry_tests.rs"],
    ),
];

/// 语料的后缀面。
///
/// ⚠ **诚实边界**：通信层哪天落一份别的后缀的文件（`.mts` / 无扩展名的脚本 / `.py`），
/// 本判据**一个字都看不见** —— 它既不在盘上那一侧，也就不会与登记表分叉。
/// 挡这一形的不是本条，是下面 [`CORPUS_WITNESS`]：每个后缀各钉一个真住址，
/// 后缀表被改空 / 改错时当场红，改**窄**则由那个后缀自己的见证接住。
const CORPUS_EXTS: &[&str] = &["rs", "ts", "toml"];

/// 语料里**刻意不收**的两块 —— `(路径片段, 为什么)`。
///
/// 照 `653b35eb` 那一拍的教训写成**明写的排除**，不靠目录位置：
/// 那次搬树之后 `evidence/` 跟着 `tests/` 自己回来了，而头注里那句「刻意不收」
/// 当时靠的是位置 ⇒ 审计记录里复述的探针串被喂给了探针。
///
/// ⚠ 下面 [`corpus`] 会**数每一条真的摘掉了几份**，摘到 0 份当场红 ——
/// 「排除悄悄失效」与「排除在生效」在终端上一模一样，那正是本仓反复记的那一形。
const CORPUS_DROP: &[(&str, &str)] = &[
    (
        "/tests/evidence/",
        "审计记录会逐字复述判据的串；喂给判据就是自己证明自己",
    ),
    (
        "/vendor/",
        "第三方代码不为我们的边界投票（它不会盖我们的标记，却会被我们的判据数进语料量）",
    ),
];

/// 语料见证：这几份**逐字的真住址**必须出现在这一趟的扫描面里。
///
/// 🔴 **这是反空真的第二样**（第一样是相等断言，第三样是阳性对照）。
/// 人群为空的日子里，「扫描面是活的」这件事没有任何别的东西能证明：
/// 根写错 / 后缀过滤打空 / 排除摘过头，三种都会让盘上那一侧安静地变成 0，
/// 而 `0 == 登记的 0` **照样绿**。
///
/// ⚠ 刻意**不是**地板（`>= N`）：地板在「变少」方向是瞎的 ——
/// 本仓逐字记过「第一版是 `checked >= 3`，而实测 `checked = 7` ⇒ 余量 2.3 倍，
/// 4 个块可以静默掉出采集面而地板照绿」。这里用的是**逐个住址的集合包含**。
/// ⚠ 也刻意**不是**「把 `CORPUS_ROOTS` 再抄一遍」：抄一份的话，
/// 「根少了一个」与「见证少了一条」会被同一次编辑一起改掉 ⇒ 恒真。
/// 形状照 `scanning_guard_registry` 的 `MUST_BE_IN_REACH`：拿**盘上真有的那一份**当见证。
const CORPUS_WITNESS: &[(&str, &str)] = &[
    ("src/frontend/shell/src/lib.rs", "壳那棵 Rust 树 · 后缀 rs"),
    (
        "src/backend/stream/wire.rs",
        "backend 那棵 Rust 树 · 后缀 rs",
    ),
    ("src/frontend/ui/tabs.ts", "前端那棵 TS 树 · 后缀 ts"),
    (
        "src/frontend/shell/Cargo.toml",
        "清单面（`C2` 的依赖图那一侧要读它）· 后缀 toml",
    ),
    (
        "tests/frontend/shell/guard_support_tests.rs",
        "tests 那棵树 —— 判据剖分之后半个仓的 `.rs` 住在这儿",
    ),
];

/// 走一遍两棵语料树，返回 `(仓根相对路径, 原文)`，已排序。
///
/// ⚠ 走的是 `guard_core::scan_tree_excluding`，**不裸 `read_dir`**
/// （`scanning_guard_registry` 那条递减棘轮明禁，且那条棘轮**只许往下拧**）。
fn corpus() -> Vec<(String, String)> {
    let root = repo_root();
    let mut raw: Vec<(String, String)> = Vec::new();
    for (sub, excluded) in CORPUS_ROOTS {
        for (p, text) in guard_core::scan_tree_excluding(&root.join(sub), CORPUS_EXTS, excluded) {
            let rel = p
                .strip_prefix(&root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            raw.push((rel, text));
        }
    }
    let mut kept: Vec<(String, String)> = Vec::new();
    let mut dropped = vec![0usize; CORPUS_DROP.len()];
    for (rel, text) in raw {
        let probe = format!("/{rel}");
        let mut keep = true;
        for (i, (pat, _)) in CORPUS_DROP.iter().enumerate() {
            if probe.contains(*pat) {
                dropped[i] += 1;
                keep = false;
            }
        }
        if keep {
            kept.push((rel, text));
        }
    }
    let dead: Vec<String> = CORPUS_DROP
        .iter()
        .zip(&dropped)
        .filter(|(_, n)| **n == 0)
        .map(|((pat, why), _)| format!("  `{pat}` —— 摘到 0 份（登记的理由：{why}）"))
        .collect();
    assert!(
        dead.is_empty(),
        "语料排除名单上有条目**一份都没摘到**：\n{}\n\n\
         ⇒ 它改名了 / 搬走了 / 片段写错了。这一格非红不可：\n\
         排除悄悄失效之后那批文件会**回到语料里**，而「排除在生效」与「排除是空转」\n\
         在终端上一模一样（`653b35eb` 那一拍的 `evidence/` 回流就是这么发生的）。",
        dead.join("\n")
    );
    kept.sort();
    kept
}

/// 一段文本是不是「自称通信层成员」。
///
/// 走 `guard_core::contains_word`（带边界），不是裸子串 ——
/// 否则 `XCOMM-LAYER-MEMBERS` 这种被撑大的写法会照样命中
/// （`needle_anchor_registry` 治的那一族：匹配单位比事实小）。
fn claims_membership(text: &str) -> bool {
    guard_core::contains_word(text, MARK)
}

/// 盘上那一侧：整棵语料里自称成员的那些路径（已排序、去重）。
fn members_on_disk() -> Vec<String> {
    corpus()
        .into_iter()
        .filter(|(_, text)| claims_membership(text))
        .map(|(rel, _)| rel)
        .collect()
}

/// 登记那一侧。
fn members_registered() -> Vec<String> {
    let mut out: Vec<String> = REGISTERED.iter().map(|(p, _)| (*p).to_string()).collect();
    out.sort();
    out
}

// ════════════════════════════════════════════════════════════════════════════
//  三、共用的那条相等断言 —— 每条判据的第一句
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

/// ★★ **每条判据的第一句**：先做那条相等断言，再把人群交出去。
///
/// `设计/05 §3.3.6` 逐字：绿的两条理由是 ①「盘上实际条数 == 登记表条数」
/// ②「人群里没有违例」。**① 在这里，② 在各条判据里** —— 一个事实一个住址（`E3`）：
/// 十一条判据不许各写一份自己的相等断言，那样十一份会各自漂。
fn boundary() -> Vec<Member> {
    assert_the_two_sides_agree();
    let root = repo_root();
    REGISTERED
        .iter()
        .map(|(rel, why)| {
            let p = root.join(rel);
            let raw = std::fs::read_to_string(&p).unwrap_or_else(|e| {
                panic!(
                    "登记表里写着 `{rel}`（理由：{why}），而它读不出来：{e}\n\
                     ⇒ 路径漂了 / 文件被删了。**不许当成「那就少判一份」** ——\n\
                     人群缩水与「全都合规」在终端上一模一样。"
                )
            });
            let prod = production_of(rel, &raw);
            Member {
                rel: (*rel).to_string(),
                prod,
            }
        })
        .collect()
}

/// 两侧**两向集合相等** ＋ 扫描面见证。[`boundary`] 与那条独立判据共用它。
fn assert_the_two_sides_agree() {
    let all = corpus();
    let seen: BTreeSet<&str> = all.iter().map(|(rel, _)| rel.as_str()).collect();
    let missing_witness: Vec<String> = CORPUS_WITNESS
        .iter()
        .filter(|(p, _)| !seen.contains(*p))
        .map(|(p, why)| format!("  {p} —— {why}"))
        .collect();
    assert!(
        missing_witness.is_empty(),
        "这几份**应当**在本趟的扫描面里，而一份都没扫到：\n{}\n\n\
         本趟语料共 {} 份（根：{:?} · 后缀：{CORPUS_EXTS:?}）。\n\n\
         🔴 别读成「那些文件没了」—— 它红的多半是**扫描面被改窄了**：\n\
         根少了一个 / 后缀表改了 / 排除名单摘过头。\n\
         ★ 而扫描面一旦打空，下面那条相等断言会变成 `0 == 0` 并**安静地全绿** ——\n\
         人群为空的日子里，这一条是唯一还在说话的东西。",
        missing_witness.join("\n"),
        all.len(),
        CORPUS_ROOTS.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
    );

    let on_disk: BTreeSet<String> = members_on_disk().into_iter().collect();
    let registered: BTreeSet<String> = members_registered().into_iter().collect();
    let unregistered: Vec<&String> = on_disk.difference(&registered).collect();
    let ghosts: Vec<&String> = registered.difference(&on_disk).collect();
    assert!(
        unregistered.is_empty(),
        "这几份文件**自称**通信层成员（文件里盖着那枚标记），却不在登记表里：{unregistered:?}\n\n\
         ⇒ 搬进来了而没人挡 —— `设计/05 §8` 逐字警告过的那一形：\n\
         「否则搬进来的东西没人挡，通信层当天就长业务」。\n\
         ⇒ 处置：往 `REGISTERED` 里加一行并写清**为什么它是纯传输**，\n\
         然后跑一遍 `C1`–`C5` / `X1`–`X6`（它们从此开始管这份文件）。"
    );
    assert!(
        ghosts.is_empty(),
        "登记表里这几条在盘上**找不到对应的成员**：{ghosts:?}\n\n\
         两种可能，都得有人看一眼：\n\
         ① 文件搬走 / 改名了 ⇒ 改登记表里的路径；\n\
         ② 文件还在，但那枚标记被删了 ⇒ 它是不是不该再算通信层了？\n\
         ⚠ **不许直接把这一行从表里删掉了事** —— 那会让人群静默缩水，\n\
         而人群缩水与「全都合规」在终端上一模一样。"
    );
    assert_eq!(
        on_disk.len(),
        registered.len(),
        "盘上 {} 份 · 登记 {} 份 —— 两向集合都查过还对不上，说明取法本身坏了（重复路径？）",
        on_disk.len(),
        registered.len()
    );
}

// ════════════════════════════════════════════════════════════════════════════
//  四、判据清单（元判据用它对拍「这十七条真的在跑」）
// ════════════════════════════════════════════════════════════════════════════

/// `(编号, 判据函数名, 它钉什么)`。**闭集**，与本文件里真实的 `#[test]` 两向相等。
const CRITERIA: &[(&str, &str, &str)] = &[
    (
        "锚",
        "the_boundary_registry_and_the_disk_agree_two_ways",
        "盘上自称成员的集合 == 登记表的集合（两向）＋ 扫描面见证",
    ),
    (
        "散文",
        "the_boundary_registry_says_out_loud_how_big_it_is_today",
        "模块头注那句「登记在册 N 份」== 登记表长度 == 盘上份数（三方）",
    ),
    (
        "识别器",
        "the_membership_scanner_would_see_a_new_member_and_ignore_a_bystander",
        "合成一棵小树：盖了标记的必须被看见，没盖的必须看不见",
    ),
    (
        "C1",
        "c1_no_business_concept_is_named_on_the_public_surface",
        "公开面上不许命名业务概念（`设计/05 §2` ＋ `§8.1.4`，豁免必须为零）",
    ),
    (
        "C2",
        "c2_no_business_crate_dependency_inside_the_boundary",
        "不许依赖任何业务 crate",
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
        "面 B（`relay/`）今天还进不来的那几份，逐份**被哪几条咬**与散文两向相等",
    ),
    (
        "传输面四份",
        "the_transport_candidates_left_outside_are_blocked_by_exactly_the_criteria_the_prose_names",
        "面 A（传输面）进不来的那四份，逐份**被哪几条咬**与散文两向相等",
    ),
    (
        "住址",
        "the_comms_tree_holds_exactly_the_registered_members",
        "`src/comms/` 下的文件集合 == 登记表（〔RE〕`99 §2.1 ⑰`）",
    ),
    (
        "元",
        "every_criterion_is_on_the_execution_chain",
        "上面这张表与本文件里真实的 `#[test]` 两向相等",
    ),
];

// ════════════════════════════════════════════════════════════════════════════
//  五、锚：相等断言 ＋ 散文对拍 ＋ 识别器阳性对照
// ════════════════════════════════════════════════════════════════════════════

/// ★ 〔RE · 收尾重排〕**住址那一腿：`src/comms/` 下的文件集合 == [`REGISTERED`]**（两向）。
///
/// 要求住址：`设计/99 §2.1 ⑰`「立两向判据：`comms/` 下文件集合 == 通信层登记表 == 带标记文件；非成员各回家」。
/// 「登记表 == 带标记文件」由主锚（下一条）钉着；本条只加住址那一腿 —— 三者两两相等。
/// 目录只是住址、成员资格仍由登记表认（`90 §0.5.3`）：成员搬出 `comms/`、非成员住进 `comms/`，都当场红。
/// 走的是**只按目录**的那一口（`guard_core::files_under`，不顺 `#[path]`：`comms/outward/mod.rs` 挂回的 door / listen 不许算进来）。
#[test]
fn the_comms_tree_holds_exactly_the_registered_members() {
    use std::collections::BTreeSet;
    let comms = crate::guard_support::repo_src_root().join("comms");
    let on_disk: BTreeSet<String> = guard_core::files_under(&comms)
        .into_iter()
        .map(|rel| format!("src/comms/{rel}"))
        .collect();
    let registered: BTreeSet<String> = REGISTERED.iter().map(|(p, _)| p.to_string()).collect();
    assert!(
        !on_disk.is_empty(),
        "`src/comms/` 下一份文件都没走到 —— 住址改了或遍历坏了，下面的相等会拿两个集合空转"
    );
    let stray: Vec<&String> = on_disk.difference(&registered).collect();
    let away: Vec<&String> = registered.difference(&on_disk).collect();
    assert!(
        stray.is_empty() && away.is_empty(),
        "`src/comms/` 的住户与通信层登记表对不上（`99 §2.1 ⑰`）：\n\
         住在 `comms/` 却没登记（非成员该回家）：{stray:?}\n\
         登记了却不住 `comms/`（成员该搬进来）：{away:?}"
    );
}

/// ★★ **主锚** —— 盘上那一侧与登记那一侧**两向集合相等**。
///
/// # 它为什么是主锚（而不是模块头注里那句手写的份数）
///
/// 两侧**异源**：盘上那一侧来自成员文件**自己的文本**（搬文件的人写的），
/// 登记那一侧来自 [`REGISTERED`]（立表的人写的）。同源的那种恒等会退化成恒真 ——
/// 一次编辑同时改掉两侧，断言一个字都不会说。
///
/// # 买到 / 买不到
///
/// **买到**：搬进来不登记 ⇒ 红 · 登记了盘上没有 ⇒ 红 · 路径漂了 ⇒ 红 ·
/// 标记被删了 ⇒ 红 · 扫描面被打空 ⇒ 红（见证那一段）。
/// **买不到**：「盖标记盖对了没有」。一份真的在做传输却不盖标记的文件，
/// 本条**一个字都看不见**（`src/frontend/shell/src/comm_boundary_registry.rs` 头注逐字登记过这条边界）。
#[test]
fn the_boundary_registry_and_the_disk_agree_two_ways() {
    // 🔴 同一件事的第二半〔`P16` 2026-09-22〕：**谁是成员**这条锚，还得管住
    //    「成员资格不沿模块树往下传」—— 理由整段写在那个函数的头注里。
    //    ⚠ 它刻意**不是**一条独立的 `#[test]`：门禁 `comm-boundary` 那一格的 `pin` 与
    //      本族条数是一条**三方恒等**腿，而 `tests/scripts/gate.sh` 本拍不在写区
    //      ⇒ 条数一个都不许动（同 `设计/99 §4 Q8` 那一格的处置：判据与 `pin` 同拍改，
    //      改不了 `pin` 就别偷偷加条数）。本条在执行链上，那才是要紧的。
    assert_membership_does_not_inherit_down_the_module_tree();
    assert_the_two_sides_agree();
    let n = REGISTERED.len();
    // 人群为空的那天，这一行是它**说得出口**的形态（`--nocapture` 可见）。
    println!(
        "〔通信层·边界登记表〕今天人群 {n} 份{}；语料 {} 份（根 {:?}）。\
         绿的理由是 `{n} == {n}`，不是「扫不到」。",
        if n == 0 { "（空集）" } else { "" },
        corpus().len(),
        CORPUS_ROOTS.iter().map(|(s, _)| *s).collect::<Vec<_>>(),
    );
}

/// 从模块头注里抠「登记在册的通信层成员：N 份」那个 N。
///
/// 针走 `guard_core::find_pinned`（**恰好一处 ＋ 两侧有边界**）：
/// 那句话在模块头注里出现第二次、或者被撑大，都当场红 ——
/// 「断言指不明是哪一处」正是 `needle_anchor_registry` 那一族。
/// ⚠ 「读的是不是那份文件」由调用方那条 `#[path]` 自检钉住（见下）。
fn population_claimed_in_prose(module_src: &str) -> usize {
    let needle = "登记在册的通信层成员：";
    let at = guard_core::find_pinned(module_src, needle).unwrap_or_else(|e| {
        panic!(
            "在 `src/frontend/shell/src/comm_boundary_registry.rs` 里钉不住那句「登记在册…」：{e}\n\
             ⇒ 那句话被改写 / 被删了 / 出现了第二处。它是散文那一侧的**唯一**住址。"
        )
    });
    let tail = &module_src[at + needle.len()..];
    let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().unwrap_or_else(|_| {
        panic!(
            "那句话后面跟的不是数字（读到 `{}`）",
            &tail[..tail.len().min(24)]
        )
    })
}

/// **上游选择的住址前缀** —— 下面那条「成员资格不传递」判据的人群。
const UPSTREAM_SELECTION_PREFIX: &str = "src/backend/accounts/upstream_select/"; // 〔`A3` 第二波〕上游选择从账号域根收窄到 `upstream/` 子树（`accounts/iso.rs` 不是上游选择）

/// 上游选择今天有几份文件。**相等，不是地板** —— 多一份就回来改这个数并重读下面那条。
const UPSTREAM_SELECTION_FILES: usize = 6; // 〔US1 · 4D〕5 → 6：多了 `endpoint.rs`（起会话那一发走哪、注入什么 · 界面「表里有没有行」，`设计/20 §3.2` 那张表从 monitor 搬来）。〔RM1a · 第四波〕4 → 5：多了 `file_face.rs`（这台机器上那份凭据文件的帧面读写口，上游选择自己的状态）。〔`A3` 第二波〕中途 4 → 5（`acct_iso.rs` 当时落在上游选择根底下）→ 回到 4：上游选择收进 `accounts/upstream_select/` 子树，`accounts/iso.rs` 不在这个前缀里。

/// ★★ **成员资格说的是「这份文件里的代码属于中转」，不是「它的模块子树都属于中转」。**
///
/// # 🔴 它为什么必须存在（`设计/99 §4 P16`，2026-09-22 用户裁「`mod.rs` 照圈」的同拍前置）
///
/// `relay/mod.rs` 入圈之后，它里面那行 `mod accounts;` 会让人读成「上游选择也跟着进来了」。
/// **没有这一条，那句读法就是对的** —— 而那正是 `C2`（不许依赖业务模块）存在的理由，
/// 并且 `C1`（公开面不许命名业务概念）在 `accounts/` 那一族上会**当场破**
/// （上游选择的整个活就是把两个不透明段读成 agent 与账号，它的公开面上必然有业务名）。
/// ⇒ 用户那一裁的裁词是「不许让语言产物驱动架构裁决」，而它配的硬条件就是本条：
/// **入圈只作用于这一份文件的文本，不沿模块树往下传。**
///
/// # 两向 ＋ 零命中，不用地板
///
/// 1. **盘上那一侧**：`accounts/**` 里**一份都没有**自称成员（零命中）；
/// 2. **登记那一侧**：[`REGISTERED`] 与 `accounts/**` 的**交集为空**；
/// 3. **反空真**：那个人群**非空且份数相等**（`UPSTREAM_SELECTION_FILES`）—— 否则前两条
///    是在一个空集上成立的，而「扫不到」与「都合规」在终端上一模一样；
/// 4. **识别器不是恒假**：拿一段**合成**文本（运行时拼出那枚标记）喂进去必须认出来
///    —— 没有这一条，识别器坏掉之后前两条照绿。
///
/// # 买不到
///
/// - **不买「上游选择真的没被当成中转用」** —— 它只判「有没有盖标记 / 有没有登记」。
///   中转反手去 `use` 上游选择的类型这一形，挡它的是 `C2` 与 `layering_guard`，不是本条。
/// - **不买「`accounts/` 就是上游选择的全部」** —— 人群是那个**住址前缀**给的。
///   上游选择哪天多一个目录，本条一个字都不说（那时 `UPSTREAM_SELECTION_FILES` 那条相等会先红）。
fn assert_membership_does_not_inherit_down_the_module_tree() {
    let all = corpus();
    let under_upstream_selection: Vec<&(String, String)> = all
        .iter()
        .filter(|(rel, _)| rel.starts_with(UPSTREAM_SELECTION_PREFIX))
        .collect();

    // 反空真③：人群非空且**份数相等**（不是地板）。
    assert_eq!(
        under_upstream_selection.len(),
        UPSTREAM_SELECTION_FILES,
        "`{UPSTREAM_SELECTION_PREFIX}` 下现扫到 {} 份文件，而判据里写的是 {UPSTREAM_SELECTION_FILES} 份 —— \
         上游选择加/减了文件就回来改这个数**并重读这一条**（人群缩水与「都合规」在终端上一模一样）",
        under_upstream_selection.len()
    );

    // 反空真④：识别器不是恒假 —— 合成一段**带那枚标记**的文本（运行时拼，本文件不写字面）。
    let synthetic = format!("// 这一份假装自己是成员：{MARK}\npub fn x() {{}}\n");
    assert!(
        claims_membership(&synthetic),
        "识别器认不出一段**故意**盖了标记的合成文本 —— 下面两条此刻在空转"
    );

    // ① 盘上那一侧：零命中。
    let claiming: Vec<&str> = under_upstream_selection
        .iter()
        .filter(|(_, text)| claims_membership(text))
        .map(|(rel, _)| rel.as_str())
        .collect();
    assert!(
        claiming.is_empty(),
        "上游选择的这几份文件盖了通信层的成员标记：{claiming:?}\n\n\
         🔴 **成员资格不沿模块树往下传。** `relay/mod.rs` 入圈说的是「**那一份文件里的\
         代码**属于中转」，不是「它的模块子树都属于中转」。\n\
         上游选择的整个活就是把两个不透明段读成 agent 与账号 ⇒ 它的公开面上必然有业务名\
         ⇒ 盖了标记，`C1` 当场破，而 `C1` 的豁免必须为零（`设计/01 §2.1`）。\n\
         ⇒ 处置只有一条：**把那枚标记摘掉**。不是往 `REGISTERED` 加一行。"
    );

    // ② 登记那一侧：交集为空。
    let registered_in_upstream_selection: Vec<&str> = REGISTERED
        .iter()
        .map(|(rel, _)| *rel)
        .filter(|rel| rel.starts_with(UPSTREAM_SELECTION_PREFIX))
        .collect();
    assert!(
        registered_in_upstream_selection.is_empty(),
        "登记表里有上游选择的文件：{registered_in_upstream_selection:?}\n\
         ⇒ 同上：入圈只作用于一份文件的文本，不传递。**删掉那几行。**"
    );
}

/// ★ **散文那一侧也得对得上** —— 「今天是空的」这句话由机器守着。
///
/// # 它治的是任务书点名的那一格
///
/// 「人群为空时，判据要能说出『今天是空的』而不是静默变绿 —— 两者在终端上一模一样」。
/// 本条把那句话变成**三方相等**：模块头注里的 N · [`REGISTERED`] 的长度 ·
/// 盘上现扫的份数。搬进来一份而忘了改散文 ⇒ 红；把散文改大而盘上没有 ⇒ 红。
///
/// ⚠ **诚实边界**：三条腿里只有「盘上」那条是异源的，另两条（散文与表）
/// 都由改这件事的同一个人写。它买的是「三处不许各说各的」，
/// **不买**「这个数是对的」—— 那一格由上面那条主锚买。
#[test]
fn the_boundary_registry_says_out_loud_how_big_it_is_today() {
    let module_src = include_str!("../../../src/frontend/shell/src/comm_boundary_registry.rs");
    let claimed = population_claimed_in_prose(module_src);
    let registered = REGISTERED.len();
    let on_disk = members_on_disk().len();
    assert_eq!(
        claimed, registered,
        "模块头注说「登记在册 {claimed} 份」，而 `REGISTERED` 里是 {registered} 行。\n\
         ⇒ 改表的时候忘了改那句话（或者反过来）。两处必须同拍。"
    );
    assert_eq!(
        registered, on_disk,
        "登记 {registered} 份，盘上自称成员的有 {on_disk} 份 —— 主锚那条会说得更细，先去看它。"
    );
    // 抽取器自检①：`include_str!` 指的真是那份模块，不是随便一份带那句话的文件。
    // 证据选的是**只有那份文件才会有的东西** —— 它把本判据挂进来的那行 `#[path]`。
    let attach = format!(
        "#[{} = \"../../../../tests/frontend/shell/comm_boundary_registry_tests.rs\"]",
        "path"
    );
    guard_core::find_pinned(module_src, &attach).unwrap_or_else(|e| {
        panic!(
            "读进来的这份文本里钉不住那行 `#[path]`：{e}\n\
             ⇒ `include_str!` 指错了地方（或者那条挂载改了）。\n\
             ★ 没有这一格的话，指到任何一份复述过那句话的文件上，本条都照样绿。"
        )
    });
    // 抽取器自检②：读进来的不是空文本。
    assert!(
        module_src.len() > 2_000,
        "读进来的模块源码只有 {} 字节 —— 本条在空转",
        module_src.len()
    );
}

/// ★ **识别器的阳性对照** —— 人群为空时，判据买到的全部就是这一条。
///
/// 合成一棵小树：一份盖了标记的、一份没盖的、一份把标记**撑大**了的
/// （`COMM-LAYER-MEMBERSHIP`，`needle_anchor_registry` 治的那一族）。
/// 走的是与真树**同一个** [`claims_membership`] 与 `guard_core::scan_tree_excluding`
/// —— 换一条路子的话，这条对照证明不了真树上那条还活着。
#[test]
fn the_membership_scanner_would_see_a_new_member_and_ignore_a_bystander() {
    // 识别器那一层（纯文本，不碰盘）。
    assert!(
        claims_membership(&format!("//! 一条传输线。{MARK}\n")),
        "识别器认不出盖了标记的文件 —— 上面那条主锚此刻在空转"
    );
    assert!(
        !claims_membership("//! 一条普通的业务线。\n"),
        "识别器把没盖标记的文件也算成了成员"
    );
    assert!(
        !claims_membership(&format!("//! {MARK}SHIP —— 这不是那枚标记\n")),
        "标记被**撑大**之后照样命中 —— 匹配单位比事实小（`needle_anchor_registry` 那一族）"
    );

    // 遍历 ＋ 识别器**合起来**那一层：真的走一趟文件系统。
    let dir = std::env::temp_dir().join(format!(
        "cc-monitor-comm-boundary-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("nested")).expect("造合成树");
    std::fs::write(
        dir.join("nested/a_member.rs"),
        format!("//! {MARK}\nfn f() {{}}\n"),
    )
    .expect("写成员");
    std::fs::write(dir.join("a_bystander.rs"), "fn g() {}\n").expect("写旁观者");
    std::fs::write(dir.join("not_scanned.md"), format!("{MARK}\n")).expect("写非语料后缀");
    let found: Vec<String> = guard_core::scan_tree_excluding(&dir, CORPUS_EXTS, &[])
        .into_iter()
        .filter(|(_, text)| claims_membership(text))
        .map(|(p, _)| {
            p.strip_prefix(&dir)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        found,
        vec!["nested/a_member.rs".to_string()],
        "合成树上的成员没被认准（实得 {found:?}）——\n\
         ⇒ 遍历与识别器**合起来**那一层坏了。真树今天人群为空，\n\
         这条对照是「它还认得出成员」的唯一证据。"
    );
}

// ════════════════════════════════════════════════════════════════════════════
//  六、C1–C5：`设计/05 §2` 的铁律
// ════════════════════════════════════════════════════════════════════════════

/// 业务词表 —— `设计/05 §2` 的 `C1` 逐字九个概念，**每个带它的复数形**
/// 〔用户 2026-09-21：「看怎么加」；`设计/05 §8.1.5`〕。
///
/// # 为什么是 `(单数, 复数)` 的**对**，不是一张摊平的十八词表
///
/// `真相源/103 §P5.3 ③` 现打了一个洞：词表全是单数，而匹配单位是标识符子词
/// ⇒ `tmux_sessions` 切出 `[tmux, sessions]`，而 `sessions ≠ session`。
/// 那一篇数出**十处真业务 `C1` 完全看不见**，含 `"/sessions/"`（那道 Claude 数据围栏
/// 自己的路径串）与 `reaper_tracked` 的形参 `announced_sids`（七行纯业务，一个词都不咬）。
///
/// 写成**对**买的是一件事：**加一个概念时不可能只加单数**。摊平的话
/// 「漏了复数」与「刻意只要单数」在盘上一模一样 —— 而那正是 `103` 逮到的那个洞的成因。
///
/// ⚠ `tmuxes` / `claudes` / `mcps` 这三个复数形**本拍现打全仓零命中**（`src/` 233 份 `.rs`
/// 的生产段）。留着是对的，理由同 [`BUSINESS_CRATES`] 那一条逐字：**本表是禁入名单，
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

/// ★★ **加复数买到了什么** —— `(标识符, 靠哪个复数形咬住, 它是什么)`〔本拍现打，`设计/05 §8.1.5`〕。
///
/// 🔴 **这张表是「加复数前后」那个读数唯一活着的住址，而它刻意不记处数。**
/// 处数（本拍：整个传输面 365 ⇒ 375 处，新增 10 处）会随任何一次编辑腐掉，
/// 而**标识符不会** —— 表里每一行都被两条断言夹着：
/// ① 今天的词表靠那个复数形真的咬住它 · ② **只拿单数去比抓不到它**
/// （⇐ 这一条就是「加复数之前它完全看不见」，做成了可机检的形状）。
///
/// ⚠ `tmux_sessions` / `TmuxSessions` **刻意不在表里**：它们本来就被 `tmux` 咬住，
/// 复数只是多给了一个判词，不是新咬住。`真相源/103 §P5.3 ③` 那十处里，
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
         那个函数**七行纯业务**而 `C1` 一个词都不咬，是 `真相源/103 §P5.3 ③` 最值钱的一格",
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

/// `C1` 的**匹配单位**：把一段文本切成标识符子词（小写、去重）〔`设计/05 §8.1.2`，09-21〕。
///
/// 两刀，顺序固定：
/// 1. 切**标识符** —— 连续的标识符字符（字母 / 数字 / `_` / `-`）算一个，其余一律是分隔。
///    ⇒ `path.ends_with(".jsonl")` 里那个 `jsonl` 也进得来：**字符串字面量里的业务词算数**，
///    它照样是这一层认识了业务（`设计/05 §2` 的铁律说的是「不知道」，不是「不直接命名」）。
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

// ── `C1` 的射程：**公开面上的声明名字**〔用户 2026-09-21 拍板，`设计/05 §8.1.4`〕 ──────

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
/// ⇒ `真相源/103 §P5.3 ①` 那条反驳（「要放过第三方名字就得解析类型」）在这个射程下
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

/// 〔C4a · 第四波〕按成员的语言取公开面：`.ts` 成员（`src/comms/inward/chan.ts`）走 [`ts_public_surface_names`]，
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

/// 〔C4a · 第四波〕**TS 成员的公开面** —— TS 的可见性规矩是 `export`：
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

/// ★ `C1` —— **公开面上不许命名业务概念**〔用户 2026-09-21 拍板，`设计/05 §8.1.4`〕。
///
/// `设计/05 §2` 逐字：「⚠ **`C1` 的豁免必须为零。** 一旦开始豁免，它就变成第三个业务的家」
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
/// 是同一个动作，`设计/01 §2.1` 那一次的措辞逐字就是这一句。
///
/// # 🔴 它为什么绕得开 `真相源/103 §P5.3` 那三条结构性反驳
///
/// `103` 否掉的是**另一条**候选（`设计/05 §8.1.1` 那个「第三方 crate 的导入名不算」），
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
/// # 匹配单位仍是**标识符子词**〔`设计/05 §8.1.2`〕，词表本拍加了复数〔`§8.1.5`〕
///
/// 走 [`identifier_subwords`]：先切标识符，再按 `_`/`-` 与驼峰拆。
/// · `sid` 当形参名 ⇒ 咬 · `SshSession` ⇒ `[ssh, session]` 咬 · `tmux_sessions` ⇒ `[tmux, sessions]` 咬
/// · `considered` ⇒ `[considered]` ≠ `sid` **不假红**
///
/// 🔴 **裸 `contains` 那条路是被否掉的，别退回去**（`设计/05 §8.1.2` 逐字）。
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
/// ③ 🔴 **实现里的业务判断** —— `真相源/103 §P5.3 ③` 那个 `reaper_tracked`
///    （七行纯业务：收割器要对账的 sid 集合）是个**私有** `fn`
///    ⇒ 上一版看不见它（复数），本版**照样看不见**（不在公开面）。
///    加复数买到的是它那两个形参在**别处**被数出来，不是这一条咬住了它。
/// ④ 🔴 **既没有分隔符、也没有驼峰的连写** —— `sessionid` / `session2` / `sidfoo`
///    拆不出子词 ⇒ 看不见。`设计/05 §8.1.2` 裁定逐字只给了「`_` 与驼峰」两刀，
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
         `设计/05 §2` 铁律：**通信层不知道什么是会话、账号、skill、agent。**\n\
         它只知道地址（`origin` / 路由键）· 操作名 · 载荷 · 流的订阅与分发。\n\
         ⚠ **豁免必须为零** —— 本条没有白名单，别来加。\n\
         ⚠ 处置不是「把名字挪进实现」那种凑绿：`设计/01 §2.1` 逐字给的是\n\
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
    //    `设计/05 §8.1.2` 逐字否掉了那条路。谁把 [`identifier_subwords`] 换回
    //    `text.contains(w)`，这里当场红。
    let stretched = "pub fn route(considered: u8, residual: u8, sessionize: u8, accounting: u8, \
         skillet: u8, claudette: u8, mcpx: u8, agentic: u8, sideline: u8) -> u8 { 0 }\n";
    assert!(
        public_surface_offences(stretched).is_empty(),
        "被撑大的标识符（`considered` / `sessionize` / `accounting` / `agentic` / `sideline` …）\
         被判成业务词 —— 匹配单位放宽过头了（裸 `contains` 那条路 `设计/05 §8.1.2` 逐字否掉过）。\
         假红比不查更坏：它会训练人绕过判据。实际命中：{:?}",
        public_surface_offences(stretched)
    );
    assert!(
        public_surface_offences("pub fn route(origin: &str, op: &str, payload: &[u8]) {}\n")
            .is_empty(),
        "一段**只用位置词**的干净公开面被判成有业务词 —— 假红比不查更坏"
    );

    // ── 🔴〔C4a〕TS 那一门的牙：导出面上的业务名必须咬（函数名 · 形参 · 导出接口的成员）；
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

    // ── 🔴 复数那一改补上的那个洞，逐个标识符钉住〔`设计/05 §8.1.5`〕。
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
             登记错了（`真相源/103 §P5.3 ③` 那十处里，`tmux_sessions` 一族正是靠 `tmux` 咬住的，\
             它们**不该**进这张表）"
        );
    }
    // 反向控制：单数那一侧还活着（否则上面那条「只拿单数抓不到」恒真）。
    assert!(
        !business_words_in("session_id").is_empty(),
        "连 `session_id` 都抓不到了 —— 单数那一侧整批瞎了，上面那组断言会恒真"
    );

    assert_eq!(
        pop.len(),
        REGISTERED.len(),
        "人群与登记表对不上 —— 主锚那条会说得更细"
    );
}

/// 业务 crate —— `设计/05 §2` 的 `C2` 逐字五个（`creds-core` 是它特意加粗的那个）。
///
/// ⚠ `usage-core` 今天盘上**不存在**（`设计/50` 把用量整删了）。留在表里是对的：
/// 本表是**禁入名单**，不是现存清单 —— 哪天它回来，这条闸已经在那儿了。
const BUSINESS_CRATES: &[&str] = &[
    "branch-core",
    "usage-core",
    "acct-core",
    "search-core",
    "creds-core",
];

/// 一份文本里引到了哪几个业务 crate（连字符形与下划线形都认）。
fn business_crates_in(text: &str) -> Vec<&'static str> {
    BUSINESS_CRATES
        .iter()
        .filter(|c| {
            let underscored = c.replace('-', "_");
            guard_core::contains_word(text, c) || guard_core::contains_word(text, &underscored)
        })
        .copied()
        .collect()
}

/// ★ `C2` —— 不许依赖任何业务 crate。
///
/// **买到**：登记成员（`.rs` 的 `use` / 路径，`.toml` 的依赖表）里引到那五个 crate ⇒ 红。
/// **买不到**：① **传递依赖** —— 本条读的是文本，不是 `cargo metadata` 的依赖图；
/// 经由第三个 crate 间接吃进 `creds-core` 它看不见。那一格要等通信层真有自己的
/// `Cargo.toml` 之后才判得了（缺的证据：一份该 crate 的 `cargo tree` 读数）。
/// ② **别的业务 crate** —— 名单是 `设计/05 §2` 那五个，第六个业务 crate 不在人群里。
#[test]
fn c2_no_business_crate_dependency_inside_the_boundary() {
    let pop = boundary();
    let offenders: Vec<String> = pop
        .iter()
        .filter_map(|m| {
            let hits = business_crates_in(&m.prod);
            (!hits.is_empty()).then(|| format!("  {} —— {hits:?}", m.rel))
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "通信层成员引到了业务 crate：\n{}\n\n\
         `设计/05 §2` `C2`：通信层是纯基础设施，业务 crate 一个都不许依赖。\n\
         ⇒ 真需要那份数据，让**后端交给它**（`C4` 是同一句话的另一面）。",
        offenders.join("\n")
    );
    for c in BUSINESS_CRATES {
        let dashed = format!("{c} = {{ path = \"../{c}\" }}\n");
        assert!(
            business_crates_in(&dashed).contains(c),
            "禁入名单里写着 `{c}`，识别器认不出它的清单形 —— 名单与识别器脱钩了"
        );
        let used = format!("use {}::Thing;\n", c.replace('-', "_"));
        assert!(
            business_crates_in(&used).contains(c),
            "禁入名单里写着 `{c}`，识别器认不出它的 `use` 形 —— 名单与识别器脱钩了"
        );
    }
    assert!(
        business_crates_in("use guard_core::scan_tree;\nshell-quote-core = \"1\"\n").is_empty(),
        "非业务 crate 被判成业务 crate —— 假红比不查更坏"
    );
}

/// 一行里既有公开面的记号、又有 `transport` 这个词。
fn public_line_leaks_transport(line: &str) -> bool {
    let public = line.contains("pub ") || line.contains("export ");
    public && guard_core::contains_word(line, "transport")
}

/// ★ `C3` —— 前端发出的请求里不许含 `transport`。
///
/// `设计/05 §3.2` 逐字：「`transport` 是通信层的**内部**选择，前端不知道」
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
         `设计/05 §3.2`：**`transport` 是通信层的内部选择，前端不知道。**\n\
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
/// 前三条是 `设计/05 §2` 逐字点名的（`read_to_string` / `File::open` / `env::var`）；
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
/// `设计/05 §2.1`：这是用户那句「**key 什么的这些应该要归后端管，通信只负责流量**」
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
/// 读的是 `connect_and_exec_cmd` 交回来的那条 SSH 流 —— 也就是 `设计/05 §2.1 C5` 逐字
/// 「**只使用别人交给它的通道**」正要它干的那件事。
/// ⇒ **「本条咬了几处」不等于「这一层伸手拿了几次东西」**，报数时别把两者读成一个。
///
/// ⚠ **今天刻意不收窄**：`设计/05 §2` 那三条判词是**逐字**点名的，收窄成
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
         `设计/05 §2.1` 把用户那句话操作化成这一条：\n\
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

/// `C5` 的形状表 —— 前两条是 `设计/05 §2` 逐字点名的，其余是同族别名。
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
/// `设计/05 §2.1`：用户那句「**我不希望一个 app 占用三个端口、三个进程**」的操作化 ——
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
         `设计/05 §2.1`：「我不希望一个 app 占用三个端口、三个进程」\n\
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
//  七、X1–X6：`设计/05 §3.3.6` 的签名判据（与 C1–C5 共用上面那张表）
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

/// `设计/05 §3.3.0` 那三个线上类型 —— `X1` 的人群按它们认。
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
         `设计/05 §3.3.6` `X1`：三个类型的每一处 `match` 必须**穷尽、零 `_ =>`**。\n\
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
/// `设计/05 §3.3.2`：**值归后端 · 执行归通信层 · 说法归调用方**。
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
         `设计/05 §3.3.2`：**值归后端**。期限从 `Budget` 里进来，不在这一层里写死。",
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
/// `设计/05 §3.3.1`：错误分三层（传输错 · 对端错 · 我们自己错），
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
         `设计/05 §3.3.6` `X3`：**无默认值、无 `..Default::default()`**。\n\
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
/// `设计/05 §3.3.4`：**回推优先 · 推不动才丢 · 丢必须说**。
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
         `设计/05 §3.3.4`：**回推优先 · 推不动才丢 · 丢必须说**。\n\
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
/// `设计/05 §3.3.2`：期限沿着调用链**只许越来越紧**。
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
         `设计/05 §3.3.6` `X5`：**每一处派生都是 `min` —— 零 `+` / `max` / 重新 `now() + …`**。\n\
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
/// `设计/05 §3.3.2`：**说法归调用方**。一个藏在库里的默认期限意味着
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
    let all = corpus();
    // 抽取器自检：两种语言的前端语料都真的在（人群为空不等于语料为空）。
    assert!(
        all.iter().any(|(rel, _)| rel == "src/frontend/ui/tabs.ts"
            && is_frontend_for(rel, "ts", &member_paths)),
        "前端语料里找不到 `src/frontend/ui/tabs.ts` —— 语料面坏了，本条此刻在空转"
    );
    assert!(
        all.iter()
            .any(|(rel, _)| rel == "src/frontend/shell/src/filewin/source.rs"
                && is_frontend_for(rel, "rs", &member_paths)),
        "Rust 前端语料里找不到 `filewin/source.rs`（窗口进程那一处 `call`）—— 语料面坏了"
    );
    let mut offenders: Vec<String> = Vec::new();
    let mut sites = 0usize;
    // 〔F7c 09-24〕按入口分开数：`subscribe` 进来了（窗口里第一处），而它**没有期限参数**
    //   （`05 §3.3.0` 签名逐字）⇒ 「显式给 `Budget`」只对带期限的入口判；条数两个入口各自恒等。
    let mut per_entry: std::collections::BTreeMap<&str, usize> = Default::default();
    // 〔C4a · 第四波〕`chan.call`（主界面那一侧）同样带期限 —— 死值验现打：只写 `call` 的话，
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
         `设计/05 §3.3.2`：**说法归调用方**。库里的默认期限 =\n\
         「这条路该等多久」没有任何调用方想过（`§9`：那 8 条无期限路径连实测分布都没有）。",
        offenders.join("\n")
    );
    // 🔴〔F2〕调用点**条数恒等**（不是地板）：`call` 恰好 1 处（`filewin/source.rs::ask`）；
    //    〔F7c 09-24〕`subscribe` 恰好 1 处（`filewin/source.rs::watch`）。
    //    变多 ＝ 窗口里长出了第二处说它的地方（期限 / 撤的住址跟着分家）；
    //    变少 ＝ 那一处没了 —— 上面那条零违例会在零个调用点上**恒绿**。
    // 〔C4a · 第四波〕`chan.call`（TS，主界面）恰好 2 处：`account-reads.ts::fetchSessionAccounts`（`accounts-sessions`）·
    //    `views/history-search.ts` 逐台那一问（`history-search`）。**X6 的 TS 人群第一次非空。**
    // 〔C4b · 第四波 4B〕2 → 5：`session-reads.ts` 的三问（`history-index` / `history-user-inputs` / `history-find`，
    //    会话读面那三条从 monitor 的 Tauri 命令改走通道；每处显式给期限）。5 → 6：`settings/plugins-section.ts::fetchSurvey`
    //    （`plugins-marketplaces`）。
    // 〔C4c · 第四波 4B〕6 → 8：`account-reads.ts::fetchAccounts`（`accounts-list`）· `account-reads.ts::checkTrust`（`accounts-trust`）——
    //    账号清单与信任预检从 monitor 的三条 Tauri 命令改走通道；每处显式给期限。
    //    8 → 9：`session-reads.ts::probeSessionRecord`（`history-record`，resume 之前问记录还在不在）。
    //    9 → 11：`settings/backend-section.ts::askExitPolicy` / `putExitPolicy`（「退出行为」问 / 交写）。
    //    〔合并 AS2〕11 → 12：`settings/assets-section.ts` 问那台的资产目录（`assets-catalog`，显式给期限）。
    // 〔C4d · 第四波 4B〕12 → 18：`history-reads.ts` 六处（`history-projects` 本机 · 逐台远端两处 · `history-sessions` ·
    //    `history-annotate` · `history-forget` · `history-last-accounts`）—— 历史清单与注解从 monitor 的五条 Tauri 命令改走通道，
    //    一律问本机常驻后端（远端那台由它去问）；每处显式给期限。
    // 〔SU1 · 第四波 4C〕18 → 20：`settings/assets-section.ts` 问那台记着的「从别处装来的 skill」（`skill-installs`）·
    //    点「卸」之后问那台的卸判定（`skill-uninstall-plan`）；两处都显式给期限。
    // 〔C4e · 第四波 4C〕18 → 19：`tmux-control.ts::capturePane`（`capture-pane`，预览窗抓一屏从 monitor 的 Tauri 命令改走通道；
    //    显式给期限）。
    //    〔C4e〕19 → 22：`tmux-control.ts` 的 `killSession`（`kill`）· `sendKeys` · `sendInto`（都是 `launch`）——
    //    杀会话 / 送键 / 就地 resume 三条 Tauri 命令改走通道；每处显式给期限（操作名留在调用点写字面量，见 `settle` 头注）。
    //    〔C4e〕22 → 27：`cc-bus-control.ts` 五处（`bus-list` 查在线 · `bus-send` · `bus-kill` · `bus-spawn` · `bus-broadcast`）——
    //    cc-bus 驾驶舱的写面从 monitor 的五条 Tauri 命令改走通道；每处显式给期限。
    // 〔MG1 · 合并 SU1 ＋ C4e〕基数 18 ＋ SU1 增量 2 ＋ C4e 增量 9 = 29（两路各自从 18 起算；上面两段各写各的增量）。
    // 〔US1 · 第四波 4D〕〔合并 US1 × 主线〕主线 29 ＋ 2：`apikey-reads.ts::readApikeyStatus`（`apikey-read`）· `fetchApikeyRouting`（`apikey-routing`）——
    //    API key 那两问从 monitor 的两条 Tauri 命令改走通道；每处显式给期限。
    // 〔HX1 · 4D · D-f〕31 → 32：`account-reads.ts::fetchSessionAccountsOrNull`（`accounts-sessions`，机器页「停」本机后端之前
    //    现问一次走中转的活会话；不走缓存、问不到回 `null`）；显式给期限。
    // 〔LOC1a · 第四波 4D〕〔合并 LOC1a × 主线 4837d0bd〕主线 31 ＋ 1：`tasks-panel.ts::fetchSessionTasks`（`tasks-list`）——
    //    任务快照从 monitor 的 Tauri 命令改走通道（C4e 批 4）；显式给期限。
    // 〔合并 HX1 × LOC1a〕两路各自 31 ＋ 1 ⇒ 31 ＋ 2 = 33。
    // 〔HX2 · 4D〕〔合并 HX2 × 主线 99b8adb6〕主线 33 ＋ 1 ⇒ 34：`apikey-reads.ts::writeApikeyKey`（`apikey-key-set`，写 key 从 monitor 那条 Tauri 命令改走通道）；显式给期限。
    // 〔STC · 第四波 4D〕〔合并 STC × 主线 aa8c29f3〕主线 35 ＋ 1 ⇒ 36：`session-reads.ts::readSessionFacts`（`history-facts`，会话事实出成品 ——
    //    此前是前端 `onLine` 旁路自己攒的，不是替掉一条 Tauri 命令）；显式给期限（`READ_BUDGET_MS`）。
    // 〔CF2 · 第四波 4B〕`chan.subscribe`（TS，主界面）恰好 1 处：`events.ts::bindEvents` 按 `streams` 订会话内容流
    //    （主窗口每台机器一条、独立窗口一条，都经这一处）。
    // 〔TAP〕`session-tap` 与会话行走同一处（`plan` 里多一种流），仍是 1。
    // 〔DL1 · 第五波〕`accounts-changed`（替掉裸事件 `remote-backend-ready`；`设计/01 §2.2`「前端只有两个动作」）同样经这一处
    //    （合并 TAP 时从单独一处 `watchAccountsChanged` 收回 `bindEvents` 的 `plan`，照 TAP 那一形）⇒ 仍是 1。
    // 〔W5-ALIAS · 第五波先行〕＋1（合并主线 3c815828 之后 34 → 35；那一拍两边各自写成 34、git 当同一行合了，现打 35）：`settings/machine-aliases.ts::previewAlias` 一处（别名预览 `ccm-print`，
    //    问本机常驻后端「这条别名实际会执行什么」，`设计/71 §2.3`）；显式给期限（`PREVIEW_BUDGET_MS`）。
    // 〔DUP2 · J4〕36 → 37：`settings/acct-deploy.ts::askAcctIsoCmd` 一处（cc-acct-iso 步骤那一行问那台后端 `acct-iso-cmd`；
    //    新建表单预览 · 启用向导预览 · 弹终端三个用处都经这一处）；显式给期限（`CMD_BUDGET_MS`）。
    // 〔SH1 · 4D〕37 → 39：`cc-bus-control.ts::readState` / `readInbox`（`bus-state` / `bus-inbox`，驾驶舱读面从 monitor 那两条 Tauri 命令改走通道）；显式给期限（`READ_BUDGET_MS`）。
    // 〔GAP1 · `设计/15 §4.7 S1`〕39 → 40：`settings/backend-section.ts::askBackendLog`（`backend-log`，那台后端的诊断文件尾部）；显式给期限。
    // 〔RESYNC · V149〕基数 40 → 增量 +1 ⇒ 41：`resync.ts::resync`（`resync`，机器一行「重新对齐」与关卡 2「对齐后重试」共用这一处）；显式给期限（`RESYNC_BUDGET_MS`）。
    // 〔MIG-3a〕基数 41 → 增量 +6 ⇒ 47：`mcp-reads.ts` 三处（`mcp-read` · `mcp-server-put` / `-remove`）＋ `mcp-sync-reads.ts` 三处（`mcp-sync-source` / `-preview` / `-apply`），
    //    MCP 读写与推拉从 monitor 那八条 Tauri 命令改走通道；显式给期限（`MCP_BUDGET_MS` / `SYNC_BUDGET_MS`）。
    // 〔MIG-3a〕基数 47 → 增量 +5 ⇒ 52：`skill-install-reads.ts` 四处（`skill-read` · `skill-install-plan` · `-apply` · `skill-uninstall-apply`）
    //    ＋ `assets-sync-reads.ts` 一处（`assets-sync`）；显式给期限（`SKILL_BUDGET_MS` / `SYNC_BUDGET_MS`）。
    // 〔合并 MIG-3a × 主线 5b52f042〕基数 52 ＋ MIG-3a +11 ＋ MIG-2 +4 ⇒ 67。
    // 〔MIG-3a · 子步 3〕基数 61 → 增量 +2 ⇒ 63：`cc-bus-install-reads.ts` 两处（`cc-bus-install` / `-state`）。
    // 〔MIG-3a · 主会话 09-28 裁〕基数 63 → 增量 −2 ⇒ 61：MCP 推拉 3 → 2、skill 装 3 → 2（经前端中继那一形改成只问本机枢纽一次）。
    // 〔MIG-3a〕基数 57 → 增量 +6 ⇒ 63：`alias-reads.ts` 六处（`aliases-*`）；显式给期限（`ALIAS_BUDGET_MS`）。
    // 〔MIG-3a〕基数 54 → 增量 +3 ⇒ 57：`skill-inbox-reads.ts` 三处（`skill-host-list` / `-read` / `-write`）；显式给期限（`INBOX_BUDGET_MS`）。
    // 〔MIG-3a〕基数 52 → 增量 +2 ⇒ 54：`acct-iso-reads.ts` 两处（`acct-iso-status` · `acct-iso-shellinit`）；显式给期限（`ACCT_ISO_BUDGET_MS`）。
    // 〔MIG-3a · 09-28 裁 2〕基数 67 → 增量 +1 ⇒ 68：`acct-iso-reads.ts` 一处（`acct-iso-install`）；显式给期限（`ACCT_ISO_BUDGET_MS`）。
    // 〔MIG-1 · `99 §2.1 ⑯`〕基数 41 → 增量 +3 ⇒ 44：`ssh-config-reads.ts` 三处（`ssh-config-aliases` · `-resolve` · `-import`，`~/.ssh/config` 导入从 monitor 三条 Tauri 命令改问本机常驻后端）；各自显式给期限。
    // 〔合并 MIG-1 × 主线 b9818369〕基数 41 ＋ MIG-3a 11 ＋ MIG-1 3 ⇒ 55。
    // 〔MIG-2〕基数 52 → 增量 +4 ⇒ 56：`launch-render.ts` 四处（`launch-render-cli` · `launch-render-payload` · `launch-endpoint` · `launch-local`），
    //    起会话的渲染 / 中转地址 / 本机计划从 monitor 那几条 Tauri 命令改走通道；显式给期限（`budgetWithin(...)`）。
    // 〔合并 MIG-1 × 主线 bc175f33〕主线 56 ＋ MIG-1 本路 6（ssh 配置三问 ＋ 端口转发三问）⇒ 62。
    assert_eq!(
        per_entry,
        [
            ("call", 1usize),
            ("chan.call", 95usize), // 〔P5〕+1：`terminal-open.ts::openTerminal` 本机那一支问本机后端 `terminal-local`（令牌握手前奏由后端接）；显式给期限（`RENDER_BUDGET_MS`） // 〔WF1 · L〕+1：`alias-reads.ts::allowLocalScripts`（`powershell-policy-set`，用户确认后改执行策略）；显式给期限（`ALIAS_BUDGET_MS`） // 〔FIX4 · J15〕+1：`views/history-search.ts::searchAllMachines`（`history-search-merge`，各台结果合一份问本机后端）；显式给期限（`MERGE_BUDGET_MS`） // 〔FIX4 · ⑬〕+1：`terminal-open.ts::openTerminal` 远端那一支（`terminal-ssh`，开终端那一行问本机后端渲）；显式给期限（`RENDER_BUDGET_MS`） // 〔FIX4 · J7〕+1：`tmux-name-mint.ts::askMint`（`tmux-name-mint`，起会话要的 tmux 名问那台后端铸）；显式给期限（`MINT_BUDGET_MS`） // 〔FIX4〕+1：`settings/panorama-section.ts::uninstallPanorama`（`panorama-uninstall`，全景小程序卸口）；显式给期限（`UNINSTALL_BUDGET_MS`） // 〔MOD〕+5：`record-reads.ts` 五处（`history-page` 两处 · `history-lines` · `history-subagent` · `drift-report`；会话正文四条从 monitor 那几条 Tauri 命令改走通道，漂移账记录那两面问那台后端）// 〔MIG-3b 续〕+1：`settings/footprint-reads.ts` 的 `ask`（`footprint-report`，足迹从 monitor 那条 Tauri 命令改走通道）// 〔MIG-3b 续〕+2：`panorama/api.ts` 的 `remote`（`panorama`）· `edit`（`panorama-edit`），全景从 monitor 那三条 Tauri 命令改走通道 // 〔MIG-3b 续〕+1：`pubkey-push.ts::pushPublicKey` 问本机 `pubkey-push` // 〔OSA〕基数 79 → 增量 +1：`settings/profile-backups.ts` 问本机 `files-ls`（`$PROFILE` 备份那一格） // 〔合并 MIG-3a × 主线 f5a294e1〕基数 67 ＋ 主线 +11（78）＋ MIG-3a +1（`acct-iso-install`）⇒ 79
            ("chan.subscribe", 2usize), // 〔MIG-1 收尾〕1 → 2：`remote-probe.ts::probeMachine` 订那一趟测试连接的进度流（`probe-progress/<票>`，一次一条、结局到了就撤）—— 它不是长活的会话流，不进 `bindEvents` 的 `plan`
            ("subscribe", 1usize)
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

/// `relay/` 里**进不来**的那几份 ——
/// `(仓根相对路径, 咬它的判据编号, 它的 `C4` 判词标签, 为什么它今天圈不进来)`。
///
/// ⚠ 第三列〔`C4` 判词标签〕是 2026-09-22 加的，理由住 [`assert_left_outside`] ——
/// 只比判据编号的话，「一份文件少了一处读盘」在盘上看不出来。标签的住址是
/// [`disk_and_env_needles`]（本表**不写那个串本身**，理由见 [`disk_and_env_tags_in`]）。
///
/// 🔴 **这张表是把一段散文搬成机检的。** `REGISTERED` 上方那段头注先前逐字写着
/// 「余下五份各被哪几条咬（`C2` / `X2` / `C1` ＋ `X4` / `C4` ＋ `C5`）」——〔那句散文立表那天就是五份〕
/// 那是**散文**：判据一条都没在看它，谁哪天把 `tee.rs` 的 `try_send` 换成 `send().await`、
/// 或者往 `upstream.rs` 里再塞一个期限常量，那句话就悄悄假了而**没有任何东西会红**。
/// 本条把它换成**逐份两向集合相等**：盘上现扫的咬人判据集合 == 本表这一行。
///
/// # 它治什么（而这不是「人群扩大」）
///
/// ⚠ **表里这几份不是通信层成员** —— 它们没盖 [`MARK`]，`C1`–`C5`/`X1`–`X6` 不管它们。
/// 本条判的是**另一件事**：那段解释「为什么它们还进不来」的散文有没有腐。
/// ⇒ 一份文件的阻塞清空了（表里那一行变成空集）⇒ 本条当场红，
/// 而那时该做的是**把它圈进来**（盖标记 ＋ 加进 `REGISTERED` ＋ 改散文那个 N），
/// 不是回来把这一行删掉了事。
///
/// ⚠ **它不买「圈进来就对了」** —— `真相源/100 §二` 那一形（十一条全绿而语义上不该圈）
/// 本条一个字都不说。归属判断永远是人做的，本表只保证那段理由不是假的。
const RELAY_LEFT_OUTSIDE: &[(&str, &[&str], &[&str], &str)] = &[
    // 〔DEL〕`relay/tee.rs` 那一行摘了：挡它的 `X4`（NDJSON 行落点的 `try_send`）随独立 `--relay` 删了 ⇒ 圈进 `REGISTERED`。
    (
        "src/backend/relay/listen.rs",
        &["C5", "X2"],
        &[],
        "自己 `TcpListener::bind` 端口（`C5`）。〔DEL〕先前还自己 `std::env::var` 读环境（`C4`，`--relay` 入口那一处）—— 那一形删了，\
         进程内那一形的取值器是宿主递进来的。\
         按 `设计/05 §2.1` `C5` 括号里那条，端口与端口号本来就归后端 ⇒ \
         这一份**语义上就该在外面**，不是「等它变干净」。\
         🔴 **`X2` 是 `P16`（2026-09-22）新加的一条，而它是「变干净」的反面**：\
         中转那两个期限**常量**搬到了这一份里（`设计/01 §2.1 C4` 逐字把「期限值」\
         算进「全部由后端交给它」）⇒ 这一份**更**该在外面了，而中转少了两处 `X2`。\
         这一格是那张表少见的「多一条反而是对的」—— 别顺手把它改回去。",
    ),
];

/// **面 A 的传输面候选**里进不来的那几份 ——
/// `(仓根相对路径, 咬它的判据编号, 它的 `C4` 判词标签, 为什么它今天圈不进来)`。
///
/// 🔴 **这张表治的病与 [`RELAY_LEFT_OUTSIDE`] 逐字相同，只是换了一个面。**
/// 那一拍（2026-09-21）给**面 B** 把「为什么进不来」从散文换成了机检，而**面 A 一直没有** ——
/// 它那几份的判词只住 `真相源/100`，也就是一份**会腐的读数**。
///
/// # 🔴 它不是「多一张表」，它补的是一个现打出来的洞
///
/// 〔步 4 剩余那一路，2026-09-22〕死值验：把 `ssh_source.rs` 那处读环境变量**摘掉**
/// ⇒ 这一族当时那十六条**一条都没红**（那几份不是成员、也不在任何一张表里）。
/// ⇒ 「面 A 还剩几处读盘」这件事**此前完全不在执行链上**：清掉一处、或者再长出一处，
/// 都没有任何东西会说话。本条就是那条缺掉的腿。
/// （同一刀真红的是**另一族** `ssh_source_dial_move_judge`，而它是**正着**钉住那处
/// 环境变量必须在 ⇒ 清它是改设计，不是做清理。这一格的判词写在下面 `ssh_source.rs` 那一行。）
///
/// # 人群从哪来（**不是**「扫哪个目录」）
///
/// `设计/05 §8.1.4` 那张「传输面在新射程下还咬」的表点了四份，**减去** `sftp_move_ledger.rs`
/// （它今天一条都不咬），**加上** `pubkey.rs`（`真相源/100 §一` 逐份试圈的第 5 行，
/// 也是 `设计/99 §4 P16` 完成判据里点名的三份之一）。
///
/// ⚠ **`sftp_move_ledger.rs` 为什么不列进来**：它是 `真相源/100 §二` 那一形的第三例
/// —— 十一条一条不咬，**而它仍然不该圈**。列进来的话这张表第一天就红，
/// 而那条红指向的处置（「阻塞清空了 ⇒ 把它圈进来」）**恰好是错的**。
/// 归属判断永远是人做的 ⇒ 照 [`RELAY_LEFT_OUTSIDE`] 对 `accounts/policy.rs` 的处置办：
/// **不列，理由写在这里而不是等人来问。**
///
/// # 🔴 `设计/99 §4 P16` 那个「**5 处读盘**」，机检住址就在这张表的第三列
///
/// 那个数**不写在任何一句散文里** —— 它是这张表第三列的**处数合计**，
/// 由下面那条判据与 `expected_c4_sites` 做**相等**断言（不是地板）。
/// ⇒ 清掉一处、或者再多长一处，两个方向都当场红。
/// 〔本拍死值验：摘掉 `ssh_source.rs` 那处读环境变量 ⇒ 逐字
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
        "src/frontend/shell/src/ssh_source.rs",
        &["C1", "X2"],
        &[],
        "〔C2 · 2026-09-24，`设计/05 §13`〕**传输那一段已经搬出去了**：SSH 的全部活进了后端的拨号代理，\
         界面侧读应答的那一段是新的通信层成员 `ssh_link.rs`，起代理的是宿主 `dial_host.rs`。\
         今天咬它的两条**全是业务该做的事**，不是传输面没洗干净：`C1` 公开面上是会话/tmux/agent 那一族\
         （远端数据源本来就是业务）· `X2` 重连退避与快照重试的期限值。\
         ⇒ **这一份不是「还差一点就进来」，是「本来就不该进来」**：登记它等于把业务家圈进通信层。\
         〔`C4` 原来还有一个判词「读环境OS」—— 拨号代理二进制的解析搬去了宿主，那一处随之离开。\
          〔MIG-1〕`C4`「读文本」（读 `~/.ssh/config`）与 `C5`（起 `ssh -G`）随「从 ssh config 导入」搬进后端 `dial/ssh_config.rs` 一起离开。〕",
    ),
    (
        "src/frontend/shell/src/sftp.rs",
        &["X2"],
        &[],
        "〔MIG-3b · 4d-lanes 子步 1〕**今天咬 `X2` 一条**：部署判定进了本机常驻后端（`deploy-plan`），本文件问它要计划那一问         定了一个期限值（`PLAN_BUDGET`）—— 期限值归宿主（`05 §3.3.2`），而它就是宿主那一侧的调用方（形状同下一行 `sftp_pool.rs`），         照实登记、不圈。         〔RW1 · 第四波 09-24〕**原来只差 `C1` 一条，后来一条都不咬了** —— 咬它的那个词随 F11 那条 SFTP 直删\
         （连同它的结构守卫〔散文墓碑〕）改经远端后端删一起走了（`设计/05 §8.1.3` 说的「要清掉那个词得连它一起搬」，\
         搬的是用户裁的 RW1）。🔴 **而它仍然不圈**，这是一次归属判断、不是判据没跑：\
         本文件今天剩下的是 F08 的**部署**（后端二进制 · 入口 shim · 卸载）（〔W5-ALIAS〕远端 rc 别名块的**规划**\
         `merge_profile_block` / `strip_profile_block` / `CCM_WRAPPER_SNIPPET` 搬去了 `profile_installer.rs`）—— 都是业务，不是传输；\
         SFTP 整体进常驻后端是 4B 的 `SR1b`，这一份的去留归它裁。十一条全绿不等于该圈（`真相源/100 §二` 那一形）。",
    ),
    (
        "src/frontend/shell/src/sftp_pool.rs",
        &["X2"],
        &[],
        "〔SR1b · 2026-09-24〕**传输本体整段搬进了本机常驻后端**（SFTP 客户端 `dial/sftp.rs`、传输台 `control/transfer.rs`），\
         这份只剩中继：开单 / 起跑 / 撤原样转给本机后端，`transfer` 帧翻成窗口那几格。\
         〔墓碑 —— 从前咬它的是 `C1`（公开面上的传输业务词）与 `C4` 两个判词「开文件」「以选项开」（用户那次传输的本地那一头）；\
         两样都跟着传输本体走了。〕今天只剩 `X2`：它给本机后端那几条**就地记账**命令的应答定了一个期限值（`CALL_BUDGET`）—— \
         期限值归宿主（`05 §3.3.2`），而它**就是**宿主那一侧的中继，不是传输面候选 ⇒ 不圈，照实登记。",
    ),
    // 〔MIG-3b 续 · ⑬〕`pubkey.rs` 那一行摘了：它「正解是搬去后端」—— 公钥推送整件进了本机后端（`pubkey-push`，`src/backend/assets/pubkey.rs`：
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
    if !business_crates_in(prod).is_empty() {
        out.insert("C2");
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
        // 〔F2〕人群与 `X6` 同一个口径（按入口的语言分前端语料），不另写一份。
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
/// 第一版只比**判据编号**的集合。死值验第一刀当场证否：`ssh_source.rs` 有两个 `C4` 判词，
/// 摘掉其中一个之后编号那个集合**一个字都不变** ⇒ 十七条**全绿**。
/// ⇒ 「集合粒度」在「少了一处」那个方向与**地板**一样瞎，而本仓正是反复栽在那上面。
/// 本条因此多两层：逐份的 `C4` **判词**两向集合相等 ＋ 全表判词**处数**相等，
/// 另有一条两列互相的自检（写了 `C4` 就必须逐个判词点出来，反之也不许凭空多一列）。
///
/// # 买不到（如实登记）
///
/// - **只有 `C4` 收到了判词那一层。** `C2`（业务 crate 名）· `C5`（起进程/绑端口）·
///   `X1`–`X6` 今天仍**只比编号集合** ⇒ 那几条各自「少了一处」的方向**照样瞎**。
///   这一格是有意的：`设计/99 §4 P16` 的完成判据点名的是「那 5 处读盘」，
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
        let p = root.join(rel);
        let raw = std::fs::read_to_string(&p).unwrap_or_else(|e| {
            panic!(
                "表里写着 `{rel}`（理由：{why}），而它读不出来：{e}\n\
                 ⇒ 路径漂了 / 文件搬走了。**不许当成「那就少判一份」** ——\n\
                 人群缩水与「全都合规」在终端上一模一样。"
            )
        });
        let prod = production_of(rel, &raw);
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
        //    〔本拍死值验第一刀逮到的正是这一形：摘掉 `ssh_source` 那处读环境变量，
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
            阻塞清空 ⇒ 盖 [`MARK`] ＋ 往 `REGISTERED` 加一行 ＋ 改模块头注那个份数 N。\n\
            ⚠ 而「十一条全绿」不等于「该圈」（`真相源/100 §二` 那一形），归属判断仍是人做的。",
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
         是「那一份现在圈得进来了吗」（阻塞清空 ⇒ 盖标记 ＋ 进 `REGISTERED` ＋ 改份数 N）。"
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

// 🪦 **这里先前住着一张「十一条全绿、而归属待裁」的表 ＋ 它的断言**〔散文墓碑〕
//
// **它存在过，为什么，被谁裁掉的** —— 三句话记全，别只留一句「已删」：
//
// · **为什么有**：`P16㈡`（2026-09-22）把 `C2` ＋ `X2` 从中转清掉，**三份**文件同拍变成
//   十一条全绿，而那一拍只裁了一份（`upstream.rs`）。余下两份落进一个「进不来」那张表
//   装不下的状态：既不被任何判据咬着，也还没有归属裁决。
//   ⇒ 留在那张表里它会**以一个假理由红**（没人「清掉阻塞」，是阻塞本来就没了）；
//   直接删掉这件事就**从盘上消失**。所以当时移出来单独登记并钉住，两个方向都钉
//   （「今天真全绿」＋「真没盖标记」）。**那不是豁免，是一张等裁的账。**
// · **谁裁的**：用户 2026-09-22 裁「**两份都圈**」，裁词逐字住 [`REGISTERED`] 里
//   `relay/mod.rs` 那一行（「不许让语言产物驱动架构裁决」），并配了同拍的硬条件 ——
//   即 [`assert_membership_does_not_inherit_down_the_module_tree`] 那一条「不传递」。
// · **所以它走了**：账结清了，表跟着走。⚠ **不是**因为它太吵或没人看 ——
//   一张等裁的账被裁掉才能删，**别把这条墓碑读成「这种表不该有」**。
//   下一次再出现「全绿而没裁」的文件，仍然照那个形状立一张，然后拿去要一个裁决。

/// ★ **面 B 剩下的那几份** —— `relay/` 进不来的那几份，逐份**被哪几条咬**与 [`RELAY_LEFT_OUTSIDE`] 两向相等。
///
/// `设计/05 §4.3` 把 `relay/` 按 `C4` 切开、归通信层那一列点了六份，加上后来的 `listen.rs`
/// 共七份。**为什么其余的进不来**先前只是散文。
///
/// ⚠ 〔`P16` 2026-09-22〕这张表从 5 行掉到 **2 行**：`upstream.rs` · `server.rs` · `mod.rs`
/// 三份**都圈进来了**（`C2` ＋ `X2` 一清，它们十一条全绿；归属逐份已裁，理由住
/// [`REGISTERED`] 各自那一行）。⇒ 面 B 归通信层那一列七份里，今天**只剩这两份**在外面。
///
/// 反空真那三样与面 A 那条**共用同一份实现**（[`assert_left_outside`]，`D1`）。
///
/// # 买不到
///
/// - **不买「表里那几份该不该进来」** —— 见 [`RELAY_LEFT_OUTSIDE`] 头注。
/// - **不买「`relay/` 就是这七份」** —— 人群是 `设计/05 §4.3` 那张表给的，本条不去数目录。
///   哪天 `relay/` 多一份文件，本条**一个字都不说**（挡那一形的是 `BACKEND_FILES` 那张表）。
#[test]
fn the_relay_files_left_outside_are_blocked_by_exactly_the_criteria_the_prose_names() {
    assert_left_outside(
        RELAY_LEFT_OUTSIDE,
        "`relay/` 今天还进不来的那几份（面 B）",
        // 〔DEL〕份数 2 → 1：`tee.rs` 圈进来了，只剩 `listen.rs`。
        1,
        // 〔DEL〕`C4` 判词处数 1 → 0：`relay/listen.rs` 那一处读环境（`--relay` 入口）随那一形删了；
        //   那一份仍被 `C5` / `X2` 咬 ⇒ 仍圈不进来（份数 2 不变）。
        0,
    );
}

/// ★ **传输面那四份** —— 面 A 的候选逐份**被哪几条咬**与 [`TRANSPORT_LEFT_OUTSIDE`] 两向相等。
///
/// 〔`设计/99 §4 P16`「步 4 的剩余」，2026-09-22 立〕面 B 那张表 2026-09-21 就有了，
/// **面 A 一直没有** ⇒ 「面 A 还剩几处读盘」这件事此前**完全不在执行链上**
/// （死值验：摘掉一处读环境变量，这一族当时那十六条一条都没红）。人群与逐份理由住
/// [`TRANSPORT_LEFT_OUTSIDE`]，反空真那三样与面 B 那条共用 [`assert_left_outside`]。
///
/// # 买不到（别读大了）
///
/// - **不买「这四份该不该进来」** —— 见 [`TRANSPORT_LEFT_OUTSIDE`] 头注；
///   其中 `pubkey.rs` 与 `sftp_move_ledger.rs` 两格的正解都**不是**「圈进来」。
/// - **不买「传输面就是这四份」** —— 人群是 `设计/05 §8.1.4` 那张表 ＋ `真相源/100 §一` 给的，
///   本条**不去数目录**。哪天传输面多一份文件，本条一个字都不说。
/// - **不买「这几处读盘清得掉」** —— 它只买「那段解释为什么清不掉的理由不是假的」。
///   逐处的写区外前置（另一族判据正着钉住那处环境变量 · 6 条互锁登记表 · 一道未拍的设计题）
///   写在表里各自那一行。
#[test]
fn the_transport_candidates_left_outside_are_blocked_by_exactly_the_criteria_the_prose_names() {
    // 〔C2 · 2026-09-24〕`C4` 判词处数 5 → **4**：少的是 `ssh_source.rs` 的「读环境OS」——
    //   拨号代理二进制的解析（`CCM_DIAL_PROXY`）随拨号搬去了宿主 `dial_host.rs`（不是成员，那一处本来就归它）。
    // 〔SR1b · 2026-09-24〕`C4` 判词处数 4 → **2**：少的是 `sftp_pool.rs` 的「开文件」「以选项开」——
    //   用户那次传输的本地那一头随传输台搬进了本机常驻后端（`control/transfer.rs`）。份数仍是 4（它还是候选，只剩 `X2`）。
    // 〔MIG-1 · `99 §2.1 ⑯`〕`C4` 判词处数 2 → **1**：少的是 `ssh_source.rs` 的「读文本」（读 `~/.ssh/config`）——
    //   「从 ssh config 导入」搬进后端 `dial/ssh_config.rs`。份数仍是 4。
    // 〔MIG-3b 续〕4 → 3：`pubkey.rs` 随公钥推送进本机后端删了。
    // 〔MIG-3b 续〕判词处数 1 → 0：那一处读盘就是 `pubkey.rs` 读本机 `.pub`（它搬进了本机后端）。
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
/// （`设计/05` 的编号与盘上的东西对不上）· 把 `X4` 悄悄改名（闭集按名字认）。
///
/// # ⚠ 它**接不住**的那一格（如实登记，这是本件已知的洞）
///
/// **整个模块被从 `lib.rs` 摘掉**。那时本文件一条都不跑，本条也不跑 ——
/// 「摘掉了」与「全绿」在终端上一模一样。挡这一形要在 `tests/scripts/gate.sh` 上
/// 给这一族开一个**自己的格**（形状照 `f3-copy`：pin · declared · ran 三方相等）。
/// ⚠ 〔2026-09-22〕**那一格今天已经在了** —— `tests/scripts/gate.sh` 的 `comm-boundary`，
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
        "`设计/05 §2` 的铁律恰好五条（C1–C5），盘上是 {got_c:?}"
    );
    assert_eq!(
        got_x, want_x,
        "`设计/05 §3.3.6` 的签名判据恰好六条（X1–X6），盘上是 {got_x:?}"
    );
}
