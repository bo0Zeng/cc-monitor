/**
 * 〔DUP1 · 第四波 4D〕**判定只有一个家** —— `设计/90 §3` 判据 2 的机检。
 *
 * 守的要求（逐字）：
 * - `设计/90 §3`：「2. 前端不许出现「口径」—— 凡是有对应 `*-core` crate 的判定，TS 侧零实现；」
 * - `设计/01 §5` D1：「**一个判定只有一个家** | 同一条规则有两份实现，它们就会漂；而漂开的后果是静默的错，不是报错」
 * - `设计/01 §5` D5：「**判据的人群要从文件系统全集来**，不从「配置里已经承认的那批」来」
 *
 * # 为什么按登记表认、不按名字猜
 *
 * LR2 现打过「按名字配」：`*-core` 的 `pub fn` 从 snake 换成 camel 去 TS 里找定义，只配上两处，
 * 而真正的重复（`shell-quote.ts` 那几个校验器）名字一个都对不上 —— 那把尺子看不见它要量的东西。
 * ⇒ 这里是一张**显式登记表**：每条判定写明它的 Rust 住址、TS 孪生叫什么（`defs`）、
 * 孪生的规则长什么样（`needles`，规则指纹 —— 字符集、区间、回落写法）。逐条的来历在
 * `调研/第四波记录/DUP1.md §0.2 / §1`。
 *
 * # 四条
 *
 * 1. **人群两向（D5）**：盘上 `src/common/<crate>/src`（〔P1〕全部共享 crate，不再只看 `-core` 后缀）生产段的 `pub fn` / `pub const` / `pub static`
 *    名字全集 == [`CORE_ITEMS`] 的键全集（逐 crate）。新长一个 `*-core` 项而不登记 ⇒ 红 ——
 *    逼着回答「TS 里有没有它的孪生」。`guard-core` 整 crate 排出人群，理由现核：它在每一份提到它的
 *    `Cargo.toml` 里都只在 `[dev-dependencies]`（测试基础设施，不是产品判定）。
 * 2. **登记表自身两向**：`CORE_ITEMS` 指向的判定号都在、且那条判定的 `homes` 列了它；
 *    每条判定的每个 `homes` 都真的在（`crate::项` ⇒ 在 `CORE_ITEMS` 里且指回来；
 *    `路径::函数` ⇒ 那份 Rust 生产文件里恰好一处 `fn 函数`）。
 * 3. **TS 侧对登记名零实现**：状态 `zero` ⇒ `defs` 零处定义、`needles` 零处命中；
 *    状态 `open` ⇒ `defs` 各恰好一处定义、`needles` 命中数 == 登记数（恒等计数，纪律 7）。
 *    `open` 不是地板式豁免：孪生一旦没了（别路删了、或它自己漂没了）这一格就红，逼着翻成 `zero` —— 自退役。
 *    每条 `open` 写 `owner`（谁在做 / 等谁拍）与 `why`。
 * 4. **反空真**：扫到的 TS 生产文件非空；`open` 行的「恰好一处定义 / 指纹命中」就是探测器的阳性对照
 *    （探测器坏了 ⇒ `open` 行先红）；Rust 住址那一格同理。
 *
 * # 四个态（〔DUP2〕扩；`zero` / `open` 是 DUP1 的，`generated` / `mirror` 取自 `DUP1.md §4 ①` 甲 / 乙的原话）
 *
 * - `zero`：TS 侧没有这条判定 —— `defs` 零定义 · `needles` 零命中。
 * - `open`：孪生还在、等人 —— `defs` 恰一处 · `needles` 恒等计数 · 写 `owner` / `why`（自退役）。
 * - `generated`：「Rust 那侧把规则的**数据**导出成 `src/frontend/ui/generated/*.ts`，TS 只剩一个『按生成物求值』的薄壳」——
 *   `defs`（薄壳名，可空）恰一处 · `gone`（手写孪生）零定义 · `needles`（手写规则指纹）零命中 ·
 *   ⑤ 生成物里 `gen.exports` 真的导出 · 生产 TS 里从这份生成物 import 它们的文件集合 == `gen.importers`（两向）·
 *   ⑥ `parity`：两侧对同一份东西（`via`：金样或 Rust 源码）的测试都在、都提到它。
 * - `mirror`：「TS 那份原样留，登记为镜像 ＋ 跨语言对拍」，且有设计出处 —— `defs` 恰一处 · `needles` 恒等计数 ·
 *   ⑥ `parity` 同上 · `why` 必须点一个设计住址（`设计/NN §x` 或 `INVARIANTS §N`）。
 *
 * 另两格管 **Rust 内部并家之后不许长回来**（⑦）：`rustGone`（那份生产文件里 `fn 名` 零处）· `rustNeedles`（恒等计数）。
 *
 * # 买不到
 *
 * - `generated` 只证「薄壳读的是生成物、手写指纹不在」；薄壳自己那几行（例：空串先说「不能为空」）不判；
 *   生成物与 Rust 函数是两种写法（式子 vs 循环），一致性只由金样样本证。
 * - `mirror` 只证「对拍接着线」（测试在、提到了同一份金样），不证对拍本身写得对 —— 那是那几条测试自己的死值验。
 * - 标 `NONE` 的那些 `*-core` 项是**登记时**逐个读规则、按规则在 TS 里搜过得出的「没有孪生」——
 *   本判据只核它们在表里，**不核** TS 里真没有同义实现（语义判不动）。
 * - 指纹只抓「登记过的那几种写法」：换个名字、换种写法重写一遍，这里看不见。
 * - Rust 侧一条判定有几个家（`DUP1.md §0.3`）只判**登记过的并家**（⑦ `rustGone` / `rustNeedles`）；没登记的多家看不见。
 * - `guard-core` 的 dev-only 现核只看下面 [`CARGO_TOMLS`] 列的那几份；仓里新长一份 `Cargo.toml` 不会自动进来。
 */
import { existsSync, readFileSync } from "node:fs";
import { posix, resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { productionRsFiles, productionTsFiles } from "../../test-support/production-sources.ts";
import { REPO_ROOT } from "../../test-support/repo-root.ts";
import { stripComments } from "../../test-support/strip-comments.ts";

type JudgmentId =
  | "J1" | "J2" | "J3" | "J4" | "J5" | "J6" | "J7" | "J8"
  | "J9" | "J10" | "J11" | "J12" | "J13" | "J14" | "J15" | "J16"
  | "J17" | "J18" | "J19" | "J20" | "J21";

/** TS 孪生的规则指纹：一段字面子串（在**剥过注释**的生产代码里数）。`file` 缺席 = 全体生产段合计。 */
interface Needle {
  text: string;
  count: number;
  file?: string;
}

/** 〔DUP2〕Rust 生产文件里的一段字面子串（剥注释后数），恒等计数。 */
interface RustNeedle {
  file: string;
  text: string;
  count: number;
}

/** 〔DUP2〕`generated` 态：规则数据住哪份生成物、导出了哪几个名字、生产 TS 里谁读它（两向）。 */
interface Gen {
  file: string;
  exports: string[];
  importers: string[];
}

/** 〔DUP2〕`generated` / `mirror` 态：两侧对的是同一份东西（`via`：共用金样或 Rust 源码），`tests` 是两侧（或一侧）读它的测试。 */
interface Parity {
  via: string;
  tests: string[];
}

interface Judgment {
  /** 这条判定判的是什么（一句）。 */
  what: string;
  /** Rust 住址：`<crate>::<项>`（`*-core`）或 `<仓内相对路径>::<fn 名>`。第一个是「唯一住址」（待定的行列全部现住址）。 */
  homes: string[];
  status: "zero" | "open" | "generated" | "mirror";
  /** TS 孪生的符号名（`function X` / `const X` …）。 */
  defs: string[];
  /** 已经删掉的孪生名（一行里删了一部分时用）：不论本行状态，都必须**零处定义**。 */
  gone?: string[];
  needles: Needle[];
  /** `generated` 必填。 */
  gen?: Gen;
  /** `generated` / `mirror` 必填。 */
  parity?: Parity;
  /** 〔DUP2〕Rust 内部并家之后不许长回来：`路径::fn 名` ⇒ 那份生产文件里零处 `fn 名`。 */
  rustGone?: string[];
  /** 〔DUP2〕同上，按字面指纹（恒等计数）。 */
  rustNeedles?: RustNeedle[];
  /** `open` 必填：谁在做 / 等谁拍。 */
  owner?: string;
  /** `open` / `mirror` 必填：为什么今天还在（`mirror` 要点一个设计住址）。 */
  why?: string;
}

/** `mirror` 的 `why` 必须点的设计住址形状。 */
const DESIGN_ADDRESS = /设计\/\d+ §|INVARIANTS §\d+/;

const PAYLOAD_RS = "src/backend/control/launch_render/payload.rs"; // 〔MIG-2〕随载荷渲染搬进后端

/**
 * ★ 登记表：判定 → 唯一住址，以及 TS 侧的孪生今天在不在。
 *
 * 行号即 `调研/第四波记录/DUP1.md §0.2` 的 J 号。
 */
const JUDGMENTS: Record<JudgmentId, Judgment> = {
  J1: {
    what: "鉴权方式是否阻塞可选 · 账号种类缺省",
    homes: ["acct-core::auth_ready", "acct-core::auth_kind_from_manifest"],
    // 〔DUP1 子步 3〕删的是 `accounts.ts` 里「缺席 = 旧后端 ⇒ 回落 loggedIn / 当订阅号」那一形：
    //   `authReady()` 包装（`a.authReady ?? a.loggedIn`）＋ `Account` 两格可缺。解码器早已逐键要求这两格。
    status: "zero",
    defs: ["authReady"],
    needles: [
      { text: "authReady ??", count: 0 },
      { text: "authReady?:", count: 0 },
      { text: "authKind?:", count: 0 },
    ],
  },
  J2: {
    what: "configDir 能不能拼进命令",
    // 〔DUP1 子步 8〕规则整份（POSIX 形 ＋ 任一平台形 ＋ 拒绝集）搬进 `acct-core`：`payload.rs::config_dir_command_safe`
    //   与后端 `accounts_query.rs::is_safe_config_dir` 成转手的薄壳，后端 `control/ccm` 直接用 `config_dir_ok`。
    homes: [
      "acct-core::config_dir_posix_ok",
      "acct-core::config_dir_ok",
      "acct-core::config_dir_char_unsafe",
      "acct-core::is_deceptive_char",
    ],
    // 〔DUP1 子步 4〕删 `shell-quote.ts::isValidConfigDir`（Rust 拒绝集的逐项手抄）；账号维度原样推，渲染侧判。
    status: "zero",
    defs: ["isValidConfigDir"],
    needles: [
      { text: "\\u2060-\\u2064", count: 0 },
      { text: 'includes("/../")', count: 0 },
    ],
  },
  J3: {
    what: "启动器（命令片段）能不能拼进命令（本机 · 远端载荷 · ccm 同一张白名单）",
    // 〔DUP3 · 主会话 09-26 裁 · INVARIANTS §47 ③〕三处三条规则收成一张白名单，住共享 crate；本机那份私有函数删。
    homes: ["shell-quote-core::launcher_refused_char"],
    // 〔DUP1 子步 5〕删 `shell-quote.ts::sanitizeRemoteLauncher`（同一字符集、却静默换成 claude —— D4 禁）；前端只留「空白 ⇒ 默认」。
    status: "zero",
    defs: ["sanitizeRemoteLauncher"],
    needles: [{ text: "[;|&$`<>\\r\\n]", count: 0 }],
    rustGone: ["src/frontend/shell/src/history.rs::sanitize_launcher"],
    rustNeedles: [
      { file: PAYLOAD_RS, text: "';' | '|' | '&'", count: 0 },
      { file: "src/frontend/shell/src/history.rs", text: "'-' | '_' | '.' | ' '", count: 0 },
      // 〔MIG-2〕本机起会话的计划与渲染搬进本机后端 ⇒ 私有那份最可能长回来的地方跟着挪。
      { file: "src/backend/control/launch_render/local.rs", text: "'-' | '_' | '.' | ' '", count: 0 },
      { file: "src/backend/control/ccm/plan.rs", text: "free_text_ok(&o.launcher)", count: 0 },
    ],
  },
  J4: {
    what: "POSIX 单引号 ＋ cc-acct-iso 那几条命令串",
    // 〔DUP2 · 主会话 09-26 裁 J4〕后端出这条命令（帧命令 `acct-iso-cmd`，`src/backend/accounts/iso.rs::render_cmd`，值过唯一的 quote），
    //   界面的逐字预览与弹终端都经 `chan.call` 问它 —— TS 零拼 shell 串。`acct-deploy.ts` 的 `sq` · `buildAcctIsoCmd` ·
    //   快照路径那道 `validatePathArg` 一起删；账号名的即时反馈读生成物（J18）。
    homes: ["shell-quote-core::posix_quote", "src/backend/accounts/iso.rs::render_cmd"],
    status: "zero",
    defs: ["sq", "buildAcctIsoCmd", "validatePathArg"],
    // 〔LR2 合入〕`shell-quote.ts::posixQuote` 随 TS 兜底一族删了 ⇒ 挪进 gone。
    gone: ["posixQuote"],
    needles: [
      { text: "'\\\\''", count: 0 },
      { text: "cc-acct-iso init", count: 0 },
    ],
  },
  J5: {
    what: "session id 形状",
    // 〔DUP1 子步 6 · 主会话 09-26 交「J5 那一族统一」〕唯一住址 `shell-quote-core::session_id_ok`（今天各处规则的交集）：
    //   `branch_core::is_plain_sid` 成它的再导出（〔THIN〕那个再导出随 branch-core 收进后端删了，调用方直呼这一条）；monitor 载荷 `@ccm_sid` · 载荷线 `resumeSid` · `ccm …` 调用行 · 本机拉起 ·
    //   分叉 id，后端 ccm argv 都调它。⚠ 后端 `resolve_query.rs::is_valid_session_id` 刻意没收：行为冻结给仓外 aterm（V126）。
    //   TS：`isValidSessionId` 与只剩那一格的 `validateLocalLaunch` 删。
    homes: ["shell-quote-core::session_id_ok", "src/backend/control/resolve_query.rs::is_valid_session_id"],
    status: "zero",
    defs: ["isValidSessionId", "validateLocalLaunch"],
    needles: [{ text: "[A-Za-z0-9_-]{0,127}", count: 0 }],
  },
  J6: {
    what: "tmux 会话名形状",
    // 〔DUP2 · 主会话 09-26 裁 J6〕唯一一份进 `gate-core`：**新建**（本工具铸的名，`§47` ①）· **已有会话**（attach / 送进已在的，
    //   V131 ②：拒绝集 ＋ 非空，寻址恒 `=<名>:`）。monitor 载荷外层（`payload.rs::check`）· `ccm …` 调用行 · 后端
    //   `ccm/plan.rs::validate_tmux_name`（成只管「说哪一句」的薄壳）都调它；TS 两个谓词与两处内联式子删。
    // 〔THIN〕共享 crate `gate-core` 收成后端模块 `control/gate_rules.rs`（monitor 那一侧的门删了）⇒ homes 换成路径形。
    homes: ["src/backend/control/gate_rules.rs::new_tmux_name_issue", "src/backend/control/gate_rules.rs::existing_tmux_name_issue"],
    status: "zero",
    // 〔DUP3 · 主会话 09-26 裁〕§34 Gate 1「只拒空」的两个住址（界面 `tmux-control.ts::rejectEmptyTarget` · monitor `tmux.rs` 的私有谓词）
    //   并进本行：目标都是已有会话 ⇒ 判定就是 `existing_tmux_name_issue`；界面那一份删（TS 零，空目标原样交给后端，后端入口拒），
    //   monitor 那一份改调 gate-core。
    defs: ["isValidTmuxName", "isValidNewTmuxName", "rejectEmptyTarget"],
    needles: [
      { text: "/^[A-Za-z0-9_][A-Za-z0-9_-]*$/", count: 0, file: "src/frontend/ui/launch-requests.ts" },
      { text: "[*?=]", count: 0 },
      { text: 'target === ""', count: 0, file: "src/frontend/ui/tmux-control.ts" },
      // 〔FIX · `99 §2 ㊹`〕`sendInto` 那一格也零（空名 / 空载荷交后端，回落那一跳由 Rust 渲染器拒）。
      { text: '.trim() === ""', count: 0, file: "src/frontend/ui/tmux-control.ts" },
    ],
    // 禁字集字面量只住 gate-core 一处；后端 plan.rs 那份自己的不许长回来。
    rustNeedles: [
      { file: "src/backend/control/gate_rules.rs", text: '"*?.:="', count: 1 },
      { file: "src/backend/control/ccm/plan.rs", text: '"*?.:="', count: 0 },
      // 〔THIN〕monitor `tmux.rs` 那一格（Gate 1 调 gate-core 恰一处 · 私有谓词不许长回来）随整份文件删了：
      //   monitor 生产段零处够门，由 `tests/frontend/shell/gate_singleton_guard_tests.rs::the_monitor_holds_no_gate_of_its_own` 钉。
    ],
  },
  J7: {
    what: "tmux 名派生 ＋ 撞名避让",
    // 〔FIX4 · `设计/90 §3` J7〕翻 `zero`：派生（`name_segment` → `<段>-cc` / `<段>-fork-cc`）＋ 避让只在后端，
    //   帧命令 `tmux-name-mint`（`control/ccm/mod.rs::answer_tmux_name_mint`，按那台的会话快照避让）；
    //   界面要名字就问它（`src/frontend/ui/tmux-name-mint.ts`）。TS 那份（`remote-launch.ts` 整份 · `shell-quote.ts::tmuxNameSegment` ·
    //   `fork-launch.ts::forkTmuxName`）删。
    homes: [
      "src/backend/control/ccm/plan.rs::derive_tmux_name",
      "src/backend/control/ccm/plan.rs::fork_tmux_base",
      "src/backend/control/ccm/plan.rs::next_free_name",
    ],
    status: "zero",
    defs: ["tmuxNameSegment", "deriveTmuxName", "mintTmuxName", "mintSessionTmuxName", "forkTmuxName"],
    needles: [
      { text: "session-cc", count: 0 },
      { text: "-fork-cc", count: 0 },
    ],
  },
  J8: {
    what: "启动期令牌形状",
    // 〔DUP2 · 主会话 09-26 裁 J8 → 甲〕唯一住址 `payload.rs::rbind_token_shape_ok`（字母表 ＋ 长度两个常量同住）；
    //   `bind.rs` 那份逐字同的副本并进来（那边是再导出，`rustGone` 钉它不许长回来）。
    //   铸币口按生成物（字母表 × 长度）**造**，构造上造不错 ⇒ 界面两处自检（维度 `apply` · 铸币口）与 TS 副本删。
    // 〔DUP3 · 主会话 09-26 裁〕后端 `control/identity_tag.rs::token_is_safe` 是**同一个令牌**（`CCM_RBIND_TOKEN`）的第三份（读侧、跨半边）⇒
    //   三份收成一份进共享 crate `shell_quote_core::rbind_token_ok`（§47 ① 标识符那一层；两半都要）；monitor `payload.rs`（`bind.rs` 再转）
    //   与后端 `identity_tag.rs` 都成它的再导出，`rustGone` 钉三处都不许再长出自己的 `fn`。
    homes: ["shell-quote-core::rbind_token_ok"],
    status: "generated",
    defs: [],
    gone: ["isValidRbindToken"],
    needles: [{ text: "[0-9a-f]{32}", count: 0 }],
    gen: {
      file: "src/frontend/ui/generated/judgment-rules.ts",
      exports: ["RBIND_TOKEN_ALPHABET", "RBIND_TOKEN_LEN"],
      importers: ["src/frontend/ui/remote-launch-run.ts"],
    },
    parity: { via: "src/common/shell-quote-core/src/lib.rs", tests: ["tests/frontend/ui/rbind-token-shape-parity.vitest.ts"] },
    rustGone: [
      "src/frontend/shell/src/bind.rs::rbind_token_shape_ok",
      `${PAYLOAD_RS}::rbind_token_shape_ok`,
      "src/backend/control/identity_tag.rs::token_is_safe",
    ],
    // 字母表字面量只住共享 crate 一处；两半各自的写法不许长回来。
    rustNeedles: [
      { file: "src/common/shell-quote-core/src/lib.rs", text: '"0123456789abcdef"', count: 1 },
      { file: PAYLOAD_RS, text: '"0123456789abcdef"', count: 0 },
      { file: "src/backend/control/identity_tag.rs", text: "(b'a'..=b'f')", count: 0 },
    ],
  },
  J9: {
    what: "上游 base URL 能不能用（形状 ＋ 协议闭集 ＋ 明文只许回环）",
    // 〔DUP3 · 主会话 09-26 裁 J9 → 甲〕三截（写口 creds-core 形状关 · 中转 `Base::parse` · 装表的回环判定）收成一份进新 crate
    //   `upstream-url-core`；界面 `checkBaseUrl` 成读生成物的薄壳（只剩「空 ⇒ 默认」与「按理由挑一句」）。
    homes: ["upstream-url-core::usable", "upstream-url-core::parse", "upstream-url-core::upstream_is_loopback"],
    status: "generated",
    defs: ["checkBaseUrl"],
    needles: [
      { text: "new URL(s)", count: 0, file: "src/frontend/ui/settings/account-new-form.ts" },
      { text: "127(\\.\\d{1,3}){3}", count: 0 },
    ],
    gen: {
      file: "src/frontend/ui/generated/judgment-rules.ts",
      exports: ["BASE_URL_ISSUE_PATTERNS", "baseUrlIssue", "BaseUrlIssue"],
      importers: ["src/frontend/ui/settings/account-new-form.ts"],
    },
    parity: {
      via: "tests/__fixtures__/upstream-url.golden.json",
      tests: ["tests/frontend/shell/payload_judgment_rules.rs", "tests/frontend/ui/upstream-url-parity.vitest.ts"],
    },
    rustGone: ["src/comms/outward/upstream.rs::host_is_loopback", "src/common/creds-core/src/store.rs::check_base_url_shape"],
    // 中转那份不许再有自己的协议表 / 回环判定。
    rustNeedles: [
      { file: "src/comms/outward/upstream.rs", text: '("https", true)', count: 0 },
      { file: "src/comms/outward/upstream.rs", text: "is_loopback()", count: 0 },
    ],
  },
  J10: {
    what: "用户消息里的 CLI 注入噪声",
    // 〔RENDER2〕规则合成一份住 `user_text`（`clean_user_text` 取它的 `clean`）；后端读者（搜索 · 历史摘要 · 大纲）都走它。
    //   前端（主会话 09-27 裁乙）：monitor 解析 user 记录时调同一个 `user_text` 填 `userText`（`messages.rs::UserText::of`），
    //   渲染 / compact 检测 / 分叉折叠只读成品；TS 那份 `stripInternalNoise` 与 `branching.ts` 的前缀判断删了。
    // 〔P1〕`search-core` 拆进后端：这条规则是 Claude 记录格式的知识 ⇒ 住适配层（通用层经注册表 `agents::clean_user_text` 够）。
    homes: ["src/backend/agents/claudecode/text.rs::user_text", "src/backend/agents/claudecode/text.rs::clean_user_text"],
    status: "zero",
    defs: ["stripInternalNoise"],
    needles: [
      { text: 'startsWith("[Request interrupted by user")', count: 0, file: "src/frontend/ui/branching.ts" },
      { text: "Request interrupted by user", count: 0 },
    ],
  },
  J11: {
    what: "文案表取文 ＋ 插值",
    homes: ["copy-core::copy_text"],
    // 〔DUP2 · 主会话 09-26 裁 J11 → 甲〕设计认可的双读口：登记为镜像 ＋ 插值对拍（只对拍合法插值）。
    //   两侧有意不同的几形（缺键 TS 抛 / Rust 回 `〔key〕` · 参数对不上）照现状登记在金样 `_differences`。
    // 〔DUP3 · 主会话 09-26 裁〕「值里含别的占位符」那一形不再不同：Rust 读口改成单趟（值不再被扫），金样 `cases` 多一条钉它。
    status: "mirror",
    defs: ["copyText"],
    needles: [],
    parity: {
      via: "tests/__fixtures__/copy-interpolation.golden.json",
      tests: ["tests/common/copy-core/lib_tests.rs", "tests/copy/copy-table.vitest.ts"],
    },
    why: "`设计/01 §6.9` 逐字「前端读口 `copy-table.ts::copyText`；Rust 读口只有一份实现 `copy-core::copy_text`」—— 设计明写两个读口",
  },
  J12: {
    what: "cc-bus agent id 形状",
    // 〔DUP2 · 主会话 09-26 裁 J12 → 乙「挪进后端」〕唯一一份进共享 crate（`shell_quote_core::bus_id_ok`，规则逐字不变）：
    //   monitor 读收件箱（`cc_bus.rs::is_valid_bus_id` 是它的再导出）· 后端 `bus-send` / `bus-kill` / `bus-spawn` 入口
    //   在交给 `cc-send` / `cc-kill` / `cc-spawn` 之前判（拒码 `bad_id`）。界面那一份（发 / 收 / 查在线 / 派生账号名）删了；
    //   `INVARIANTS §47` 那两格改写成「后端交给 cc-bus 之前」（报用户，用户可推翻）。
    homes: ["shell-quote-core::bus_id_ok"],
    status: "zero",
    defs: ["isValidBusId", "refuseBadId"],
    needles: [{ text: "/^[A-Za-z0-9_][A-Za-z0-9_-]*$/", count: 0, file: "src/frontend/ui/cc-bus-control.ts" }],
    // 两半各自接的是共享那一个（后端入口 · monitor 再导出），谁也没有自己再写一份字符集。
    rustNeedles: [
      { file: "src/backend/control/cc_bus.rs", text: "shell_quote_core::bus_id_ok(v)", count: 1 },
      { file: "src/frontend/shell/src/cc_bus.rs", text: "pub use shell_quote_core::bus_id_ok as is_valid_bus_id;", count: 1 },
      { file: "src/frontend/shell/src/cc_bus.rs", text: "c.is_ascii_alphanumeric() || c == '_' || c == '-'", count: 0 },
    ],
  },
  J13: {
    what: "一次失败能否证明一个字节没发出",
    homes: ["src/comms/inward/backend_route.rs::route_call_error"],
    // 〔DUP2 · 主会话 09-26 裁〕登记「镜像 ＋ 金样」：Rust 侧把每一种失败分层上线、连同判出的「可回落」写成金样，TS 读同一份逐行判。
    status: "mirror",
    defs: ["provablyNotSent"],
    needles: [],
    parity: {
      via: "tests/__fixtures__/reach-collapse.golden.json",
      // Rust 侧读金样对拍 `route_call_error` 的是 `chan/webview_tests.rs`（`backend_route_tests.rs` 只在注释里提到它 ——
      //   ⑥ 只认代码里的提及，第一版登记成那一份时 ⑥ 没逮到，改成只认代码之后才逮到）。
      tests: ["tests/frontend/shell/chan/webview_tests.rs", "tests/frontend/ui/tmux-control.vitest.ts"],
    },
    why: "`设计/01 §5` D7「失败要显式、归因要准确」：这是调用方对自己那一次调用的归因，失败时恰恰问不了对端 —— 只能在调用方判，两份由金样钉",
  },
  J14: {
    what: "中转钥匙文件的相对路径",
    homes: ["relay-route-core::KEY_FILE_REL"],
    // 〔LR2 合入〕`launch-render-fallback.ts` 整份删了 ⇒ open → zero。
    status: "zero",
    defs: ["RELAY_KEY_FILE_REL"],
    needles: [],
  },
  J15: {
    what: "全文搜索条数上限 ＋ 多机合并排序",
    // 〔FIX4 · `设计/90 §3` J15 · 主会话 09-28 裁 B〕翻 `zero`：界面照旧逐台扇出（各台常驻索引保热），合并排序问本机后端
    //   `history-search-merge`（`observe/search_query.rs::answer_merge` → `search_rules::sort_by_recency`）；前端 `mergeSearchResults` 删。
    //   条数上限：界面不交 `limit` ⇒ 每台用 `search_rules::DEFAULT_LIMIT`（`history.ts` 那份 300 的副本删了）。
    // 〔P1〕`search-core` 拆进后端：通用口径住 `observe/search_rules.rs`（`DEFAULT_LIMIT` 与收 limit 的 `clamp_limit` 同一份文件；住址格只认 `fn`）。
    homes: ["src/backend/observe/search_rules.rs::sort_by_recency", "src/backend/observe/search_rules.rs::clamp_limit"],
    status: "zero",
    defs: ["mergeSearchResults"],
    needles: [
      { text: "limit: 300", count: 0, file: "src/frontend/ui/views/history.ts" },
      { text: "b.updatedAt - a.updatedAt", count: 0 },
    ],
  },
  J17: {
    what: "模型名能不能交出去（`ANTHROPIC_MODEL` · `--model`）",
    // 〔DUP1 子步 7 · 主会话 09-26「两侧同一份、住共享 crate，真实模型名都放行」〕唯一住址 `shell-quote-core::model_name_ok`；
    //   monitor 载荷 `ExportModel` · `ccm …` 的 `--model` · 后端 ccm argv 调它。TS 手写那份删：设置里写入点那一句读
    //   **生成物** `src/frontend/ui/generated/judgment-rules.ts`（生成物目录不在本判据的扫描面里 —— 它是规则的投影，不是孪生），
    //   两侧由共用金样 `identifier-rules.golden.json` 逐条对。
    homes: ["shell-quote-core::model_name_ok"],
    // 〔DUP2〕这一行本来就是「生成物」那一形（DUP1 当时只有 zero / open 两态可记）⇒ 记成 `generated`，读者两向钉住。
    status: "generated",
    defs: [],
    gone: ["isValidModelName"],
    needles: [{ text: "[A-Za-z0-9._-]{1,128}", count: 0 }],
    gen: {
      file: "src/frontend/ui/generated/judgment-rules.ts",
      exports: ["modelNameOk", "MODEL_NAME_PATTERN", "MODEL_NAME_MAX"],
      importers: ["src/frontend/ui/account-prefs.ts"],
    },
    parity: {
      via: "tests/__fixtures__/identifier-rules.golden.json",
      tests: ["tests/frontend/shell/payload_judgment_rules.rs", "tests/frontend/ui/identifier-rules-parity.vitest.ts"],
    },
  },
  J18: {
    what: "账号名（`--account <名>`）",
    // 〔DUP1 子步 8〕Rust 侧接上：`ccm …` 的 `--account` · 后端 ccm argv（此前只靠 quote ＋ 名单成员检查）。
    homes: ["shell-quote-core::account_name_ok"],
    // 〔DUP2 · 主会话 09-26 裁 J18 → 甲〕新建账号表单那一句读生成物（`validateAcctName` 成薄壳：只多说一句「空」）；
    //   顺带修掉「比 cc-acct-iso 宽」—— 旧的手写规则放行 `.` 与 33–64 位，两根指纹钉它不许回来。
    status: "generated",
    defs: ["validateAcctName"],
    needles: [
      { text: "name.length > 64", count: 0 },
      { text: 'name.startsWith(".")', count: 0 },
    ],
    gen: {
      file: "src/frontend/ui/generated/judgment-rules.ts",
      exports: ["accountNameOk", "ACCOUNT_NAME_PATTERN", "ACCOUNT_NAME_MAX"],
      importers: ["src/frontend/ui/settings/acct-deploy.ts"],
    },
    parity: {
      via: "tests/__fixtures__/identifier-rules.golden.json",
      tests: ["tests/frontend/shell/payload_judgment_rules.rs", "tests/frontend/ui/identifier-rules-parity.vitest.ts"],
    },
  },
  J19: {
    what: "工具词表：哪些工具名算「展开 = 子会话」（agent 工具）· 〔THIN〕哪个工具名画成哪一种卡 · tmux 前台命令哪几个算 agent 的会话",
    // 〔DUP2 · 主会话 09-26 裁 J19〕两个 Rust 住址（后端 `observe/facts_query.rs` 喂会话事实 · monitor `adapter.rs` 喂渲染 agent 卡，
    //   `调研/第四波记录/STC.md §1.3`）收成一份进新立的共享 crate `agent-tools-core`（两半编译期不许互咬 —— `设计/90 §0` C2 ——
    //   共享 crate 是唯一合法的形；现有 core 没有一个的身份是「工具词表」，理由见 `调研/第四波记录/DUP2.md §2`）。
    //   两边都成它的别名，异源对拍（后端常量 == 生成物里 claude 那一行）随之退役。
    //   〔THIN · `设计/00 §1.2` 判定只在后端〕界面不再按工具名判（卡型随 assistant 记录的 `toolCards` 带来、tmux 那一格随 `tmux-list`
    //   的 `agent` 带来）⇒ 那几张词表连同 `agent-tools-core` 收进后端适配层 `agents/claudecode/cards.rs`，翻 `zero`：
    //   TS 那几个按生成物求值的薄壳（`isAgentTool` · `isInteractiveTool` · `isDiffTool` · `isClaudeTmuxCommand` · `defaultModeForTool`）删了。
    homes: ["src/backend/agents/claudecode/cards.rs::tool_card"],
    status: "zero",
    defs: ["isAgentTool", "isInteractiveTool", "isDiffTool", "isClaudeTmuxCommand", "defaultModeForTool"],
    needles: [
      { text: 'new Set(["Agent", "Task"])', count: 0 },
      { text: "agentTools", count: 0 },
      { text: "livenessProcessNames", count: 0 },
    ],
    rustGone: ["src/backend/observe/facts_query.rs::is_agent_tool"],
    rustNeedles: [
      { file: "src/backend/observe/facts_query.rs", text: '&["Agent", "Task"]', count: 0 },
      // 〔P1 · 第 4 件〕monitor `adapter.rs` 那一行摘了：那份文件随 monitor 的适配表整删（起会话事实只住后端适配层）。
      { file: "src/backend/agents/claudecode/cards.rs", text: '&["Agent", "Task"]', count: 1 },
    ],
  },
  J16: {
    what: "账号种类的取值集",
    homes: ["acct-core::AUTH_KINDS"],
    // 〔DUP2 · 主会话 09-26 裁 J16 → 甲〕解码白名单与 `AuthKind` 类型都读生成物（`acct_core::AUTH_KINDS` 现生成）；手写字面量两形钉零。
    status: "generated",
    defs: [],
    gone: ["AUTH_KINDS"],
    needles: [
      { text: '"subscription", "api-key"', count: 0 },
      { text: '"subscription" | "api-key"', count: 0 },
    ],
    gen: {
      file: "src/frontend/ui/generated/judgment-rules.ts",
      exports: ["AUTH_KINDS", "AuthKind"],
      importers: ["src/frontend/ui/accounts-decode.ts", "src/frontend/ui/accounts.ts"],
    },
    parity: {
      via: "tests/__fixtures__/accounts.golden.json",
      tests: ["tests/backend/observe/accounts_query_tests.rs", "tests/frontend/ui/accounts-decode.vitest.ts"],
    },
  },
  J20: {
    what: "中转进门请求的 Host 头是不是回环字面量（防 DNS 重绑）",
    // 〔DUP3 · 主会话 09-26 裁（丙）〕与 J9 里上游那条回环判定是两个判定：这里只认三个字面量是设计，改名说清、分两行登记，不许并。
    homes: ["src/backend/relay/door.rs::host_header_is_loopback_literal"],
    status: "zero",
    defs: [],
    needles: [],
    rustGone: ["src/backend/relay/door.rs::host_is_loopback"],
  },
  J21: {
    what: "后端健康读数落哪一档（无记录 / 没崩过 / 崩过）、每一档给界面什么",
    // 〔PB1 · `设计/90 §4` 阶段 B · `设计/70 §2.2`〕判准是「账上一条记录都没有」不是 `crashed === 0`。原来两份：
    //   TS `backend-policy.ts::describeBackendHealth` ＋ `describeHealthDetail`（界面）与 monitor `backend_policy.rs::describe_health`
    //   （同逻辑，只有测试读）。收成后端一份 `health_face`：`backend_status` 交成品（状态 ＋ 一句 ＋ ⓘ ＋ [详情]），
    //   界面只排版（`settings/backend-section.ts::decodeHealthFace` 按形状收、不判）。TS 那两个判定 · 取文口 · 缺格当无记录的
    //   `readHealth` 一起删；Rust 那份并进来（`rustGone`）。形状两侧同读金样 `tests/__fixtures__/backend-health.golden.json`。
    homes: ["src/frontend/shell/src/backend_policy.rs::health_face"],
    status: "zero",
    defs: ["describeBackendHealth", "describeHealthDetail", "healthCrashed", "readHealth"],
    needles: [
      { text: "seen === 0", count: 0 },
      { text: "crashed === 0", count: 0 },
    ],
    rustGone: ["src/frontend/shell/src/backend_policy.rs::describe_health"],
  },
};

/** `NONE` = 登记时逐个读过规则、在 TS 生产段按规则搜过，没有孪生。 */
const NONE = "—" as const;
type Entry = JudgmentId | typeof NONE;

/**
 * ★ 人群：`*-core` 每个 crate 的全部 `pub` 项（`guard-core` 除外，见头注）。
 * 加一项要回答一句：TS 里有没有它的孪生？有 ⇒ 指向一条判定；没有 ⇒ `NONE`。
 */
const CORE_ITEMS: Record<string, Record<string, Entry>> = {
  "acct-core": {
    ACCTS_DIR_NAME: NONE,
    // 〔DUP1 子步 8〕账号配置目录的全表（从 monitor `payload.rs` 与后端 `accounts_query.rs` 收进来）。
    CONFIG_DIR_SHELL_META: NONE,
    config_dir_char_unsafe: "J2",
    config_dir_ok: "J2",
    config_dir_posix_ok: "J2",
    apikey_account_id_of_dir: NONE,
    apikey_routed_subset: NONE,
    AUTH_KINDS: "J16",
    AUTH_KIND_API_KEY: NONE,
    auth_kind_from_manifest: "J1",
    AUTH_KIND_PARITY_CASES: NONE,
    auth_kind_parity_manifest: NONE,
    AUTH_KIND_SUBSCRIPTION: NONE,
    auth_kind_with_apikey_table: NONE,
    auth_ready: "J1",
    CREDENTIALS_NAME: NONE,
    is_deceptive_char: "J2",
    MANIFEST_NAME: NONE,
    SUPPORTED_SCHEMA: NONE,
  },
  // 〔THIN〕`agent-tools-core` 那一格删了：它收进后端适配层（`agents/claudecode/cards.rs`），不再是共享 crate。
  // 〔THIN〕`branch-core` 那一格删了：它收进后端适配层（`agents/claudecode/branch.rs`），不再是共享 crate。
  // 〔RE〕`codex-token-core` 那一格删了：它搬进后端成模块（`agents/codex/token.rs`），不再是共享 crate，不在本表人群里。
  // 〔P4〕通道（面 A）的客户端 · 路由器 · 线上词汇 · 拨号与交接件：契约类，零判定；TS 侧那一半是 `src/comms/inward/chan.ts`（同一套线上形状，两侧由通道判据对拍）。
  "chan-core": {
    mint_key: NONE,
  },
  "copy-core": {
    copy_text: "J11",
    TABLE_JSON: NONE,
  },
  "creds-core": {
    // 〔TAIL〕monitor 数据目录的规则搬进这里（远端常驻后端按同一份推默认路径）；TS 侧没有孪生。
    DATA_DIR_ENV: NONE,
    HISTORY_METADATA_FILE: NONE,
    monitor_data_dir: NONE,
    // 〔P5 · 主会话 09-29 裁〕哪个环境变量算家（按平台惯例）：两侧 Rust 都调它；TS 侧零处读家目录 ⇒ NONE。
    home_dir: NONE,
    home_dir_from: NONE,
    home_dir_on: NONE,
    ACCOUNTS_FIELD: NONE,
    ALL: NONE,
    AUTH_STYLE_FIELD: NONE,
    BASE_URL_FIELD: NONE,
    create_private: NONE,
    // 〔DATA-HOME · V160〕凭据文件住数据目录根上（原先按 claude 家拼的那个函数改名成它）。
    credentials_path: NONE,
    DEFAULT: NONE,
    expose_for_auth_header: NONE,
    expose_for_persisting: NONE,
    field_value: NONE,
    FILE_NAME: NONE,
    from_field_value: NONE,
    is_configured: NONE,
    is_empty: NONE,
    judge: NONE,
    KEY_FIELD: NONE,
    LEGACY_ACCOUNT_ID: NONE,
    len: NONE,
    make_private: NONE,
    masked: NONE,
    MASK_KEEP: NONE,
    merge_account_base_url: NONE,
    merge_account_key: NONE,
    merge_key: NONE,
    needs_attention: NONE,
    new: NONE,
    ordered_keys: NONE,
    ordered_value: NONE,
    parse: NONE,
    probe: NONE,
    read_accounts: NONE,
    read_auth_style: NONE,
    read_key: NONE,
    // 〔FIX · COPY ④〕常量 `TEMPLATE` → 运行期拼的 `template()`（三句说明住文案表）；报错的话 `StoreError::said`（带路径）。
    said: NONE,
    template: NONE,
    to_pretty_json: NONE,
    WIDE_PRINCIPALS: NONE,
    wide_principals_in_sddl: NONE,
  },
  // 〔THIN〕`gate-core` 那一格删了：它收成后端模块（`control/gate_rules.rs`），不再是共享 crate，不在本表人群里。
  // 〔P4〕monitor ↔ 文件窗口进程的契约：开窗种子 · 就绪行 · 开终端那一问 · 远端路径的两个切法（窗口与开窗入口共用；TS 侧不切远端路径 ⇒ 无孪生）。
  "filewin-contract": {
    BIN_ENV: NONE,
    TERMINAL_OPEN_OP: NONE,
    decode_ready: NONE,
    decode_request: NONE,
    encode_ready: NONE,
    encode_request: NONE,
    parent_dir: NONE,
    remote_basename: NONE,
    terminal_open_args: NONE,
    terminal_open_cwd: NONE,
  },
  // 〔P4〕前端宿主原语（两个前端共用、后端不链）：自有状态文件的原子写 · 窗口夹进工作区；TS 侧零孪生。
  "host-core": {
    atomic_write_json: NONE,
    fit_into_work_area: NONE,
  },
  "relay-route-core": {
    // 〔P3 · 主会话 09-29 裁〕后端住在 `~/.cc-monitor` 里的那几样的相对路径（后端各写者引它、monitor 数据位置页按它列）；TS 侧没有孪生。
    ACCT_ISO_REL: NONE,
    ALL: NONE,
    ASSET_CATALOG_REL: NONE,
    BACKEND_POLICY_REL: NONE,
    file_name_of: NONE,
    listen_pid_file_name: NONE,
    POSIX_ALIASES_REL: NONE,
    PS_ALIASES_REL: NONE,
    SKILL_LEDGER_REL: NONE,
    // 〔E2 · V28〕后端落点（`~/.cc-monitor/bin/ccm`，两半同一份）；TS 侧没有孪生（界面那句话里的路径在文案表里）。
    BACKEND_LANDING_REL: NONE,
    BACKEND_LANDING_SHELL: NONE,
    base_url: NONE,
    base_url_shape_ok: NONE,
    KEY_FILE_REL: "J14",
    key_shape_ok: NONE,
    // 〔HOST〕常驻监听口的门牌（本机宿主与远端 `--resident-ensure` 同一个函数）；TS 侧没有孪生。
    listen_port_for: NONE,
    LISTEN_TOKEN_FILE_REL: NONE,
    // 〔P3 · V160〕后端住在家里的暂存区与全景索引（后端按它落盘、monitor 的数据位置页按它列）；TS 侧没有孪生。
    PANORAMA_INDEX_REL: NONE,
    parse_target: NONE,
    PORT: NONE,
    prefix: NONE,
    route_path: NONE,
    segment_is_safe: NONE,
    split_keyed_base_url: NONE,
    STAGING_DIR_REL: NONE,
  },
  // 〔P1〕`search-core` 出列：crate 删了（通用口径 → 后端 `observe/search_rules.rs` · Claude 记录文本 → `agents/claudecode/text.rs`），J10 / J15 的住址改成路径形。
  "shell-quote-core": {
    // 〔合并 TL3 续做〕`INVARIANTS §47` ② 自由文本那一层（拒绝集只收 NUL / CR / LF）进了本 crate：TS 侧零处拼 shell、零处判它 ⇒ NONE。
    FREE_TEXT_REFUSED: NONE,
    free_text_ok: NONE,
    // 交给 agent 的参数 / 登记备注那一格（可以跨行，只拒 NUL / CR）：TS 侧零处拼 shell、零处判它 ⇒ NONE。
    ARG_TEXT_REFUSED: NONE,
    arg_text_ok: NONE,
    posix_free_path_ok: NONE,
    posix_quote: "J4",
    // 〔FILES2〕唯一 quote 的字节形（`$'…'`，非 UTF-8 目录开终端）与它的判定：TS 侧零处拼 shell、零处判它 ⇒ NONE。
    posix_quote_bytes: NONE,
    posix_free_path_bytes_ok: NONE,
    // 〔DUP1〕`INVARIANTS §47` ① 标识符那一层。常量是规则的一部分（上界 · 放行的标点），TS 侧不抄它们：
    //   模型名那两格由 monitor 现生成进 `src/frontend/ui/generated/judgment-rules.ts`（生成物不是孪生）⇒ NONE。
    SESSION_ID_MAX: NONE,
    session_id_ok: "J5",
    MODEL_NAME_MAX: NONE,
    MODEL_NAME_EXTRA: NONE,
    model_name_ok: "J17",
    ACCOUNT_NAME_MAX: NONE,
    // 〔DUP2〕规则的一部分（放行的标点），现生成进 `judgment-rules.ts`（生成物不是孪生）⇒ NONE。
    ACCOUNT_NAME_EXTRA: NONE,
    account_name_ok: "J18",
    // 〔DUP2 · J12〕cc-bus agent id（从 monitor `cc_bus.rs` 搬来，两半共用）。
    bus_id_ok: "J12",
    // 〔DUP3 · J8〕启动期令牌形状（两半三份收成一份）；两个常量现生成进 `judgment-rules.ts`（生成物不是孪生）⇒ NONE。
    rbind_token_ok: "J8",
    RBIND_TOKEN_LEN: NONE,
    RBIND_TOKEN_ALPHABET: NONE,
    // 〔P5〕令牌握手的 marker 与目录名（写侧后端前奏 · 读侧 monitor `bind.rs` 共用的契约）：TS 侧零处拼 marker、零处写握手文件 ⇒ NONE。
    RBIND_TOKEN_MARKER_PREFIX: NONE,
    AWAIT_SUBDIR: NONE,
    rbind_token_marker: NONE,
    rbind_token_from_marker: NONE,
    // 〔DUP3 · J3〕启动器命令片段白名单（§47 ③）；两个常量是规则的一部分，TS 不抄 ⇒ NONE。
    launcher_refused_char: "J3",
    LAUNCHER_EXTRA: NONE,
    LAUNCHER_HOME_PREFIX: NONE,
  },
  // 〔DUP3 · J9〕上游 base URL 能不能用（新立；中转解析 · 上游选择装表 · 写口 · 界面生成物共用）。
  "upstream-url-core": {
    SCHEMES: NONE,
    LOOPBACK_NAME: NONE,
    code: NONE,
    parse: "J9",
    upstream_is_loopback: "J9",
    usable: "J9",
  },
  // 〔MIG-3b · P1〕部署那一族的契约（`deploy-core` 拆开：判定那一半进了后端 `control/deploy_plan.rs`，这里只剩两侧对上的形状）。TS 侧零处判部署 ⇒ 全 NONE。
  "deploy-contract": {
    LAUNCHER_MARK: NONE, // 〔P1〕旧入口两形的记号（文件格式；认不认得出是后端判）
    SHIM_MARK: NONE,
    LEGACY_BACKEND_REL: NONE,
    LEGACY_ENTRY_REL: NONE, // 〔THIN〕旧入口路径（契约：后端判去向 · monitor 照删 · 足迹一行）
    LEGACY_BACKEND_WORD: NONE,
    LINES: NONE,
    UNAME_CMD: NONE,
    build_order: NONE,
    identity_of_bytes: NONE,
    interpret_stamp_scan: NONE,
    key_from_uname: NONE,
    key_of: NONE,
    label: NONE,
    say: NONE,
    stamp_scan_cmd: NONE,
    this_machine: NONE,
    // 〔THIN〕产物在线上的那个词（帧命令 `deploy-slot` 的 `product`，两侧对上的契约）；TS 侧不说它 ⇒ NONE。
    wire: NONE,
    of_wire: NONE,
  },
};

/** 整 crate 排出人群的那一个，与现核它的依据。 */
const TEST_INFRA_CRATE = "guard-core";

/**
 * 提到 `guard-core` 的那几份 `Cargo.toml` 之外还有哪些（现核「只作 dev 依赖」的人群）：
 * 两份顶层工作区 ＋ 全景引擎 ＋ 每个 `crates/*` 自己那份（按盘上 crate 目录现生成，见下）。
 */
// 〔P4〕文件窗口独立成包，它的清单也是顶层清单（它的 guard-core 只在 dev 侧）。
const CARGO_TOMLS_TOP = [
  "src/frontend/shell/Cargo.toml",
  "src/frontend/filewin/Cargo.toml",
  "src/backend/Cargo.toml",
  "src/panorama-engine/Cargo.toml",
];
/** 那几份里 `guard-core = …` 依赖行的总数（恒等计数：今天 monitor · 文件窗口 · 后端 · 全景引擎 · creds-core 各一；〔P4〕4 → 5 多的是文件窗口包）。 */
const GUARD_CORE_DEP_LINES = 5;

const CRATES_ROOT = "src/common";

// ─────────────────────────────── 抽取 ───────────────────────────────

/** 盘上 `crates/<名>/src/**` 的生产 `.rs`，按 crate 分组。 */
function rsByCrate(): Map<string, string[]> {
  const out = new Map<string, string[]>();
  for (const s of productionRsFiles(CRATES_ROOT)) {
    const m = new RegExp(`^${CRATES_ROOT}/([^/]+)/src/`).exec(s.file);
    if (!m) continue;
    const code = stripComments(s.text, "rust");
    out.set(m[1], [...(out.get(m[1]) ?? []), code]);
  }
  return out;
}

/** 一个 crate 生产段的 `pub fn` / `pub const` / `pub static` 名字（`pub(crate)` 等不算）。 */
function pubItems(codes: string[]): string[] {
  const names = new Set<string>();
  for (const code of codes) {
    for (const m of code.matchAll(/^\s*pub\s+(?:const\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)/gm)) names.add(m[1]);
    for (const m of code.matchAll(/^\s*pub\s+(?:const|static)\s+([A-Za-z_][A-Za-z0-9_]*)\s*:/gm)) names.add(m[1]);
  }
  return [...names].sort();
}

/** 在剥过注释的代码里数 `fn <名>` 出现几次。 */
function rustFnCount(file: string, name: string): number {
  const code = stripComments(readFileSync(resolve(REPO_ROOT, file), "utf8"), "rust");
  return code.match(new RegExp(`\\bfn\\s+${name}\\b`, "g"))?.length ?? 0;
}

/** TS 定义：`function X` / `const|let|var|class X`。 */
function tsDefCount(code: string, name: string): number {
  return (
    code.match(new RegExp(`(?:\\bfunction\\s+${name}\\b|\\b(?:const|let|var|class)\\s+${name}\\b)`, "g"))?.length ?? 0
  );
}

function substrCount(hay: string, needle: string): number {
  let n = 0;
  for (let i = hay.indexOf(needle); i >= 0; i = hay.indexOf(needle, i + needle.length)) n++;
  return n;
}

/** `[section]` 下的 `guard-core = …` 行，逐行报出所在节。 */
function guardCoreDepSections(toml: string): string[] {
  const out: string[] = [];
  let section = "";
  for (const line of toml.split("\n")) {
    const h = /^\s*\[([^\]]+)\]/.exec(line);
    if (h) section = h[1];
    else if (/^\s*guard-core\s*=/.test(line)) out.push(section);
  }
  return out;
}

/**
 * 〔DUP2〕一份（剥过注释的）TS 里从某个模块 import / re-export 进来的名字：`import { a, b as c, type T } from "…"` ·
 * `import type { T } from "…"` · `export { x } from "…"` · `export type { T } from "…"`。回 `[模块串, 名字]` 对。
 */
function importedNames(code: string): [string, string][] {
  const out: [string, string][] = [];
  for (const m of code.matchAll(/\b(?:import|export)\s+(?:type\s+)?\{([^}]*)\}\s*from\s*["']([^"']+)["']/g)) {
    for (const raw of m[1].split(",")) {
      const name = raw.trim().replace(/^type\s+/, "").split(/\s+as\s+/)[0].trim();
      if (name) out.push([m[2], name]);
    }
  }
  return out;
}

/** 〔DUP2〕`from` 那个模块串（相对 `file`）落到哪份仓内文件（去掉 `.ts` 后缀比）。 */
function resolvesTo(file: string, spec: string, target: string): boolean {
  if (!spec.startsWith(".")) return false;
  const joined = posix.normalize(posix.join(posix.dirname(file), spec)).replace(/\.ts$/, "");
  return joined === target.replace(/\.ts$/, "");
}

/**
 * 〔DUP2〕⑥ 在对拍测试的代码里找的那一截：`via` 路径的末两段（`__fixtures__/accounts.golden.json` · `control/payload.rs`）；
 * 文件名太泛（`lib.rs` / `mod.rs` / `main.rs`）时取末三段（`copy-core/src/lib.rs`）。
 * 用后缀不用全路径：Rust 侧读金样常是 `include_str!("../../__fixtures__/…")` 这种相对写法。
 */
function viaNeedle(via: string): string {
  const seg = via.split("/");
  const k = /^(lib|mod|main)\.rs$/.test(seg[seg.length - 1] ?? "") ? 3 : 2;
  return seg.slice(-k).join("/");
}

/** 〔DUP2〕剥过注释的 Rust 生产文件文本。 */
function rustCode(file: string): string {
  return stripComments(readFileSync(resolve(REPO_ROOT, file), "utf8"), "rust");
}

// ─────────────────────────────── 判据 ───────────────────────────────

describe("DUP1 判定只有一个家（设计/90 §3 判据 2）", () => {
  const byCrate = rsByCrate();
  const ts = productionTsFiles("src").map((s) => ({ file: s.file, code: stripComments(s.text, "ts") }));
  const tsAll = ts.map((s) => s.code).join("\n");
  const tsByFile = new Map(ts.map((s) => [s.file, s.code]));

  it("① 人群两向：盘上 `src/common/` 各 crate 的 pub 项 == 登记表（逐 crate）", () => {
    // 〔P1〕人群是 `src/common/` 下的全部 crate（D5：从文件系统全集来）—— 从前按 `*-core` 后缀筛，`deploy-contract` 会静默出列。
    const diskCrates = [...byCrate.keys()].sort();
    expect(diskCrates.length, "一个共享 crate 都没扫到 —— 路径断了，下面会零命中地绿").toBeGreaterThan(0);
    expect(diskCrates, "盘上 `src/common/` 的 crate 集合 ≠ 登记表 crate 集合 ∪ {guard-core}").toEqual(
      [...Object.keys(CORE_ITEMS), TEST_INFRA_CRATE].sort(),
    );
    const diff: string[] = [];
    for (const [crate, items] of Object.entries(CORE_ITEMS)) {
      const disk = pubItems(byCrate.get(crate) ?? []);
      const reg = Object.keys(items).sort();
      for (const d of disk) if (!reg.includes(d)) diff.push(`${crate}::${d} 在盘上、没登记（TS 里有没有它的孪生？）`);
      for (const r of reg) if (!disk.includes(r)) diff.push(`${crate}::${r} 登记了、盘上没有（改名 / 删了）`);
    }
    expect(diff).toEqual([]);
  });

  it("① guard-core 只作 dev 依赖（它被整 crate 排出人群的依据）", () => {
    const tomls = [
      ...CARGO_TOMLS_TOP,
      ...[...byCrate.keys()].map((c) => `${CRATES_ROOT}/${c}/Cargo.toml`),
    ];
    const lines = tomls.flatMap((f) =>
      guardCoreDepSections(readFileSync(resolve(REPO_ROOT, f), "utf8")).map((sec) => ({ f, sec })),
    );
    expect(lines.length, "guard-core 依赖行的条数变了（恒等计数：多了 / 少了谁？）").toBe(GUARD_CORE_DEP_LINES);
    expect(
      lines.filter((l) => l.sec !== "dev-dependencies").map((l) => `${l.f} [${l.sec}]`),
      "guard-core 进了生产依赖 —— 它不再只是测试基础设施，得回到人群里逐项登记",
    ).toEqual([]);
  });

  it("② 登记表自身两向：CORE_ITEMS ⇄ homes，路径住址真的在", () => {
    const bad: string[] = [];
    for (const [crate, items] of Object.entries(CORE_ITEMS)) {
      for (const [item, entry] of Object.entries(items)) {
        if (entry === NONE) continue;
        const j = JUDGMENTS[entry];
        if (!j) bad.push(`${crate}::${item} 指向不存在的 ${entry}`);
        else if (!j.homes.includes(`${crate}::${item}`)) bad.push(`${crate}::${item} → ${entry}，但 ${entry}.homes 没列它`);
      }
    }
    for (const [id, j] of Object.entries(JUDGMENTS)) {
      expect(j.homes.length, `${id} 没有住址`).toBeGreaterThan(0);
      for (const h of j.homes) {
        const [where, name] = [h.slice(0, h.lastIndexOf("::")), h.slice(h.lastIndexOf("::") + 2)];
        if (where.endsWith(".rs")) {
          const n = rustFnCount(where, name);
          if (n !== 1) bad.push(`${id}：${where} 里 \`fn ${name}\` 出现 ${n} 次（要恰好 1）`);
        } else if (CORE_ITEMS[where]?.[name] !== id) {
          bad.push(`${id}：${h} 不在 CORE_ITEMS 里、或没指回 ${id}`);
        }
      }
    }
    expect(bad).toEqual([]);
  });

  it("③ TS 侧对登记名：zero 行零实现，open 行恰如登记（自退役）", () => {
    expect(ts.length, "一个 TS 生产文件都没扫到 —— 下面零命中地绿").toBeGreaterThan(0);
    const bad: string[] = [];
    for (const [id, j] of Object.entries(JUDGMENTS)) {
      if (j.status === "open" && !(j.owner && j.why)) bad.push(`${id} 是 open，但没写 owner / why`);
      if ((j.status === "zero" || j.status === "generated") && j.needles.some((n) => n.count !== 0)) {
        bad.push(`${id} 是 ${j.status}，手写规则的指纹登记数必须是 0`);
      }
      if (j.status === "generated" && !(j.gen && j.parity)) bad.push(`${id} 是 generated，但没写 gen / parity`);
      if (j.status === "mirror" && !(j.parity && j.why && DESIGN_ADDRESS.test(j.why))) {
        bad.push(`${id} 是 mirror，但没写 parity，或 why 没点设计住址（设计/NN §x · INVARIANTS §N）`);
      }
      if (j.status !== "generated" && j.gen) bad.push(`${id} 不是 generated，却写了 gen`);
      for (const d of j.defs) {
        const n = tsDefCount(tsAll, d);
        const want = j.status === "zero" ? 0 : 1;
        if (n !== want) bad.push(`${id}（${j.status}）：TS 生产段里 \`${d}\` 定义 ${n} 处（要 ${want}）`);
      }
      for (const d of j.gone ?? []) {
        const n = tsDefCount(tsAll, d);
        if (n !== 0) bad.push(`${id}：已删的孪生 \`${d}\` 又在 TS 生产段里定义了 ${n} 处`);
      }
      for (const nd of j.needles) {
        const hay = nd.file ? (tsByFile.get(nd.file) ?? "") : tsAll;
        if (nd.file && !tsByFile.has(nd.file)) bad.push(`${id}：指纹点着 ${nd.file}，它不在扫到的生产文件里`);
        const n = substrCount(hay, nd.text);
        if (n !== nd.count) bad.push(`${id}（${j.status}）：指纹 ${JSON.stringify(nd.text)} 命中 ${n} 次（登记 ${nd.count}）`);
      }
    }
    expect(
      bad,
      "zero 行出红 ⇒ TS 里又长出了那条判定的一份；open 行出红 ⇒ 孪生已经没了（翻成 zero）或多了一份。逐行来历见 DUP1.md §0.2。",
    ).toEqual([]);
  });

  it("⑤ generated 行：生成物真的导出那几个名字 · 生产 TS 里读它们的文件 == 登记（两向）", () => {
    const bad: string[] = [];
    let seen = 0;
    for (const [id, j] of Object.entries(JUDGMENTS)) {
      if (!j.gen) continue;
      seen++;
      const genPath = resolve(REPO_ROOT, j.gen.file);
      if (!existsSync(genPath)) {
        bad.push(`${id}：生成物 ${j.gen.file} 不在盘上`);
        continue;
      }
      const genCode = readFileSync(genPath, "utf8");
      for (const e of j.gen.exports) {
        if (!new RegExp(`\\bexport\\s+(?:const|function|type)\\s+${e}\\b`).test(genCode)) {
          bad.push(`${id}：生成物 ${j.gen.file} 没导出 ${e}（生成器漂了，或登记错名）`);
        }
      }
      const readers = ts
        .filter((f) => importedNames(f.code).some(([spec, n]) => j.gen!.exports.includes(n) && resolvesTo(f.file, spec, j.gen!.file)))
        .map((f) => f.file)
        .sort();
      const want = [...j.gen.importers].sort();
      for (const r of readers) if (!want.includes(r)) bad.push(`${id}：${r} 读了生成物里这条规则，没登记`);
      for (const w of want) if (!readers.includes(w)) bad.push(`${id}：登记 ${w} 读生成物，它没读（薄壳没接上，或又手写了一份）`);
    }
    expect(seen, "一条 generated 行都没有 —— 本条零命中地绿").toBeGreaterThan(0);
    expect(bad).toEqual([]);
  });

  it("⑥ generated / mirror 行：对拍接着线（via 在盘上，每个对拍测试都在、都在**代码里**提到它）", () => {
    const bad: string[] = [];
    let seen = 0;
    for (const [id, j] of Object.entries(JUDGMENTS)) {
      if (!j.parity) continue;
      seen++;
      if (!existsSync(resolve(REPO_ROOT, j.parity.via))) bad.push(`${id}：对拍的对象 ${j.parity.via} 不在盘上`);
      expect(j.parity.tests.length, `${id} 的 parity 没有测试`).toBeGreaterThan(0);
      for (const t of j.parity.tests) {
        const p = resolve(REPO_ROOT, t);
        if (!existsSync(p)) bad.push(`${id}：对拍测试 ${t} 不在盘上`);
        // 剥掉注释再找：只在注释里提到金样的文件不算对拍（它没读它）。字符串留着 —— 读金样的那一句就是一个路径串。
        else if (!stripComments(readFileSync(p, "utf8"), t.endsWith(".rs") ? "rust" : "ts").includes(viaNeedle(j.parity.via))) {
          bad.push(`${id}：对拍测试 ${t} 的代码里没提到 ${viaNeedle(j.parity.via)}（只在注释里提到不算；线断了）`);
        }
      }
    }
    expect(seen, "一条带 parity 的行都没有 —— 本条零命中地绿").toBeGreaterThan(0);
    expect(bad).toEqual([]);
  });

  it("⑦ Rust 内部并家之后不许长回来（rustGone 零处 fn · rustNeedles 恒等计数）", () => {
    const bad: string[] = [];
    for (const [id, j] of Object.entries(JUDGMENTS)) {
      for (const g of j.rustGone ?? []) {
        const [file, name] = [g.slice(0, g.lastIndexOf("::")), g.slice(g.lastIndexOf("::") + 2)];
        const n = rustFnCount(file, name);
        if (n !== 0) bad.push(`${id}：并掉的那一份 ${file} 里 \`fn ${name}\` 又出现了 ${n} 处`);
      }
      for (const rn of j.rustNeedles ?? []) {
        const n = substrCount(rustCode(rn.file), rn.text);
        if (n !== rn.count) bad.push(`${id}：${rn.file} 里指纹 ${JSON.stringify(rn.text)} 命中 ${n} 次（登记 ${rn.count}）`);
      }
    }
    expect(bad).toEqual([]);
  });

  it("④ 反空真：探测器对每一个登记名 / 指纹都真的认得出来（阳性对照不靠 open 行在不在）", () => {
    // ③ 的 zero 行是「零命中」—— 探测器若对某个名字 / 指纹根本认不出来，那一行恒绿。
    // ⇒ 每个登记名各造一份最小定义、每根指纹各夹进一段代码，喂给同一个探测器，必须恰好抓到一次。
    const blind: string[] = [];
    for (const [id, j] of Object.entries(JUDGMENTS)) {
      for (const d of [...j.defs, ...(j.gone ?? [])]) {
        for (const form of [`export function ${d}(x: string) {}`, `const ${d} = 1;`]) {
          if (tsDefCount(`let a = 0;\n${form}\nlet b = 0;`, d) !== 1) blind.push(`${id}：认不出 ${JSON.stringify(form)}`);
        }
      }
      for (const nd of j.needles) {
        if (substrCount(`x(${nd.text})y`, nd.text) !== 1) blind.push(`${id}：认不出指纹 ${JSON.stringify(nd.text)}`);
      }
    }
    expect(blind).toEqual([]);
    // 剥注释别剥过头：一个已知还在的定义得剩在代码里（`copyText` 是全部界面文字的出口，删不掉）。
    expect(tsDefCount(tsAll, "copyText"), "剥完注释连 copyText 的定义都没了 —— 剥过头，③ 会零命中地绿").toBe(1);
    // 〔DUP2〕⑤ 的 import 抽取器：四种写法各认得出、别的模块不算（它瞎了 ⇒ importers 两向在空集上成立）。
    const snippet = [
      'import { a, b as c } from "./generated/judgment-rules";',
      'import type { T } from "../generated/judgment-rules.ts";',
      'export type { U } from "./generated/judgment-rules";',
      'import { z } from "./elsewhere";',
    ].join("\n");
    expect(importedNames(snippet)).toEqual([
      ["./generated/judgment-rules", "a"],
      ["./generated/judgment-rules", "b"],
      ["../generated/judgment-rules.ts", "T"],
      ["./generated/judgment-rules", "U"],
      ["./elsewhere", "z"],
    ]);
    expect(resolvesTo("src/frontend/ui/x.ts", "./generated/judgment-rules", "src/frontend/ui/generated/judgment-rules.ts")).toBe(true);
    expect(resolvesTo("src/frontend/ui/settings/y.ts", "../generated/judgment-rules.ts", "src/frontend/ui/generated/judgment-rules.ts")).toBe(true);
    expect(resolvesTo("src/frontend/ui/x.ts", "./elsewhere", "src/frontend/ui/generated/judgment-rules.ts")).toBe(false);
    // ⑥ 的「代码里提到」：注释里的提及剥掉、字符串里的留着（两种语言各一格）。
    expect(stripComments('// 读 accounts.golden.json\nlet x = 1;', "rust").includes("accounts.golden.json")).toBe(false);
    expect(stripComments('let p = "tests/__fixtures__/accounts.golden.json";', "rust").includes("accounts.golden.json")).toBe(true);
    expect(stripComments('/** accounts.golden.json */\nconst x = 1;', "ts").includes("accounts.golden.json")).toBe(false);
    expect(stripComments('readFileSync("tests/__fixtures__/accounts.golden.json")', "ts").includes("accounts.golden.json")).toBe(true);
    expect(viaNeedle("tests/__fixtures__/accounts.golden.json")).toBe("__fixtures__/accounts.golden.json");
    expect(viaNeedle("src/common/copy-core/src/lib.rs")).toBe("copy-core/src/lib.rs");
    // `mirror` 的设计住址形状：认得出两种、认不出空话。
    expect(DESIGN_ADDRESS.test("`设计/01 §6.9` 逐字")).toBe(true);
    expect(DESIGN_ADDRESS.test("INVARIANTS §47")).toBe(true);
    expect(DESIGN_ADDRESS.test("主会话拍")).toBe(false);
  });
});
