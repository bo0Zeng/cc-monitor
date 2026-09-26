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
 * 1. **人群两向（D5）**：盘上 `src/bridge/crates/*-core/src` 生产段的 `pub fn` / `pub const` / `pub static`
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
 * # 买不到
 *
 * - 标 `NONE` 的那些 `*-core` 项是**登记时**逐个读规则、按规则在 TS 里搜过得出的「没有孪生」——
 *   本判据只核它们在表里，**不核** TS 里真没有同义实现（语义判不动）。
 * - 指纹只抓「登记过的那几种写法」：换个名字、换种写法重写一遍，这里看不见。
 * - 不判 Rust 侧一条判定有几个家（`DUP1.md §0.3`：sid 五份、tmux 名三份……）—— 那是 Rust 内部的 D1。
 * - `guard-core` 的 dev-only 现核只看下面 [`CARGO_TOMLS`] 列的那几份；仓里新长一份 `Cargo.toml` 不会自动进来。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import { productionRsFiles, productionTsFiles } from "./test-support/production-sources.ts";
import { REPO_ROOT } from "./test-support/repo-root.ts";
import { stripComments } from "./test-support/strip-comments.ts";

type JudgmentId =
  | "J1" | "J2" | "J3" | "J4" | "J5" | "J6" | "J7" | "J8"
  | "J9" | "J10" | "J11" | "J12" | "J13" | "J14" | "J15" | "J16"
  | "J17" | "J18";

/** TS 孪生的规则指纹：一段字面子串（在**剥过注释**的生产代码里数）。`file` 缺席 = 全体生产段合计。 */
interface Needle {
  text: string;
  count: number;
  file?: string;
}

interface Judgment {
  /** 这条判定判的是什么（一句）。 */
  what: string;
  /** Rust 住址：`<crate>::<项>`（`*-core`）或 `<仓内相对路径>::<fn 名>`。第一个是「唯一住址」（待定的行列全部现住址）。 */
  homes: string[];
  status: "zero" | "open";
  /** TS 孪生的符号名（`function X` / `const X` …）。 */
  defs: string[];
  /** 已经删掉的孪生名（一行里删了一部分时用）：不论本行状态，都必须**零处定义**。 */
  gone?: string[];
  needles: Needle[];
  /** `open` 必填：谁在做 / 等谁拍。 */
  owner?: string;
  /** `open` 必填：为什么今天还在。 */
  why?: string;
}

const PAYLOAD_RS = "src/bridge/src/backend/control/payload.rs";

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
    homes: [`${PAYLOAD_RS}::config_dir_command_safe`, "acct-core::is_deceptive_char"],
    // 〔DUP1 子步 4〕删 `shell-quote.ts::isValidConfigDir`（Rust 拒绝集的逐项手抄）；账号维度原样推，渲染侧判。
    status: "zero",
    defs: ["isValidConfigDir"],
    needles: [
      { text: "\\u2060-\\u2064", count: 0 },
      { text: 'includes("/../")', count: 0 },
    ],
  },
  J3: {
    what: "launcher 能不能裸拼进载荷",
    homes: [`${PAYLOAD_RS}::render_payload`],
    // 〔DUP1 子步 5〕删 `shell-quote.ts::sanitizeRemoteLauncher`（同一字符集、却静默换成 claude —— D4 禁）；前端只留「空白 ⇒ 默认」。
    status: "zero",
    defs: ["sanitizeRemoteLauncher"],
    needles: [{ text: "[;|&$`<>\\r\\n]", count: 0 }],
  },
  J4: {
    what: "POSIX 单引号",
    homes: ["shell-quote-core::posix_quote"],
    status: "open",
    defs: ["sq"],
    // 〔LR2 合入〕`shell-quote.ts::posixQuote` 随 TS 兜底一族删了 ⇒ 挪进 gone、指纹 2 → 1。
    gone: ["posixQuote"],
    needles: [{ text: "'\\\\''", count: 1 }],
    owner: "主会话拍（DUP1.md §4 ①）",
    why: "acct-deploy.ts::sq 驱动新建账号表单的逐字预览（即时反馈）",
  },
  J5: {
    what: "session id 形状",
    // 〔DUP1 子步 6 · 主会话 09-26 交「J5 那一族统一」〕唯一住址 `shell-quote-core::session_id_ok`（今天各处规则的交集）：
    //   `branch_core::is_plain_sid` 成它的再导出；monitor 载荷 `@ccm_sid` · 载荷线 `resumeSid` · `ccm …` 调用行 · 本机拉起 ·
    //   分叉 id，后端 ccm argv 都调它。⚠ 后端 `resolve_query.rs::is_valid_session_id` 刻意没收：行为冻结给仓外 aterm（V126）。
    //   TS：`isValidSessionId` 与只剩那一格的 `validateLocalLaunch` 删。
    homes: ["shell-quote-core::session_id_ok", "src/backend/control/resolve_query.rs::is_valid_session_id"],
    status: "zero",
    defs: ["isValidSessionId", "validateLocalLaunch"],
    needles: [{ text: "[A-Za-z0-9_-]{0,127}", count: 0 }],
  },
  J6: {
    what: "tmux 会话名形状",
    homes: ["src/backend/control/ccm/plan.rs::validate_tmux_name", `${PAYLOAD_RS}::check`],
    status: "open",
    defs: ["isValidTmuxName", "isValidNewTmuxName"],
    needles: [{ text: "/^[A-Za-z0-9_][A-Za-z0-9_-]*$/", count: 2, file: "src/launch-requests.ts" }],
    owner: "主会话拍（DUP1.md §4 ②）",
    why: "Rust 侧三份规则互不相同，唯一住址待定；载荷路上 F01「不建带 glob 的名字」只有 TS 这一道",
  },
  J7: {
    what: "tmux 名派生 ＋ 撞名避让",
    homes: [
      "src/backend/control/ccm/plan.rs::derive_tmux_name",
      "src/backend/control/ccm/plan.rs::next_free_name",
    ],
    status: "open",
    defs: ["tmuxNameSegment", "deriveTmuxName", "mintTmuxName"],
    needles: [],
    owner: "第五波阶段 E / G（主会话认可 FE1 报备：`90 §3` 下沉后端归那里）",
    why: "铸名要那台机器的 tmux 名单，下沉 = 一条新帧命令",
  },
  J8: {
    what: "启动期令牌形状",
    homes: [`${PAYLOAD_RS}::rbind_token_shape_ok`, "src/bridge/src/bind.rs::rbind_token_shape_ok"],
    status: "open",
    defs: ["isValidRbindToken"],
    needles: [],
    owner: "主会话拍（DUP1.md §4 ③）",
    why: "维度那格是纯副本；铸币口那格是 D7「归因要准确」的明写理由 —— D1 与 D7 顶着",
  },
  J9: {
    what: "Base URL 形状 ＋ 明文只许回环",
    homes: ["creds-core::check_base_url_shape", "src/backend/relay/upstream.rs::host_is_loopback"],
    status: "open",
    defs: ["checkBaseUrl"],
    needles: [],
    owner: "主会话拍（DUP1.md §4 ①）",
    why: "驱动新建 API 号表单的即时反馈（填错时「创建」逐字变灰）",
  },
  J10: {
    what: "用户消息里的 CLI 注入噪声",
    homes: ["search-core::clean_user_text"],
    status: "open",
    defs: ["stripInternalNoise"],
    needles: [{ text: 'startsWith("[Request interrupted by user")', count: 1, file: "src/branching.ts" }],
    owner: "主会话拍（DUP1.md §4 ④）· 渲染管线那一片（W5-RENDER / STC）",
    why: "两份规则不同（TS 多剥两句样板、少剥 stderr），用途也不同（渲染 vs 索引）",
  },
  J11: {
    what: "文案表取文 ＋ 插值",
    homes: ["copy-core::copy_text"],
    status: "open",
    defs: ["copyText"],
    needles: [],
    owner: "主会话拍（DUP1.md §4 ⑤）",
    why: "`01 §6.9` 明写两个读口（前端 copyText · Rust copy_text）",
  },
  J12: {
    what: "cc-bus agent id 形状",
    homes: ["src/bridge/src/backend/control/cc_bus.rs::is_valid_bus_id"],
    status: "open",
    defs: ["isValidBusId"],
    needles: [],
    owner: "主会话拍（DUP1.md §4 ⑥）",
    why: "`INVARIANTS §47`（V121，用户拍）那张「谁在守」表点名界面这一道；后端 bus-* 今天只核非空 —— 与判据 2 相抵",
  },
  J13: {
    what: "一次失败能否证明一个字节没发出",
    homes: ["src/bridge/src/backend/control/backend_route.rs::route_call_error"],
    status: "open",
    defs: ["provablyNotSent"],
    needles: [],
    owner: "主会话拍（DUP1.md §4 ⑦）",
    why: "调用方对自己那一次调用的归因，失败时问不了对端；两份由金样 reach-collapse.golden.json 钉",
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
    homes: ["search-core::DEFAULT_LIMIT", "search-core::sort_by_recency"],
    status: "open",
    defs: ["searchAllMachines"],
    needles: [{ text: "limit: 300", count: 1, file: "src/views/history.ts" }],
    owner: "`90` 阶段 F（4D LOC1b）",
    why: "多机合并今天在前端；收口到 search-core ＋ 后端是阶段 F",
  },
  J17: {
    what: "模型名能不能交出去（`ANTHROPIC_MODEL` · `--model`）",
    homes: ["shell-quote-core::model_name_ok"],
    status: "open",
    defs: ["isValidModelName"],
    needles: [{ text: "[A-Za-z0-9._-]{1,128}", count: 1 }],
    owner: "DUP1 子步 7",
    why: "TS 那份会拒 sonnet[1m] 与 Bedrock / Vertex 名；Rust 侧此前零判定（主会话 09-26 交：两侧同一份、住共享 crate）",
  },
  J18: {
    what: "账号名（`--account <名>`）",
    homes: ["shell-quote-core::account_name_ok"],
    status: "open",
    defs: ["validateAcctName"],
    needles: [],
    owner: "主会话拍（DUP1.md §4 ①：新建账号表单逐字反馈）",
    why: "TS 那份比建账号的工具宽（放行 `.` 与 33–64 位，建时由那个工具在终端里拒）；Rust 那份与工具逐字同",
  },
  J16: {
    what: "账号种类的取值集",
    homes: ["acct-core::AUTH_KINDS"],
    status: "open",
    defs: ["AUTH_KINDS"],
    needles: [],
    owner: "主会话拍（DUP1.md §4 ⑧）",
    why: "收成品时的白名单，金样 accounts.golden.json 钉着；要一个家可走 generated",
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
  "branch-core": {
    build_branch_records: NONE,
    find_session_file: NONE,
    // 〔DUP1〕`is_plain_sid` 成 `shell_quote_core::session_id_ok` 的再导出（`pub use`，不再是本 crate 的 `pub fn`）。
    SESSION_LOOKUP_DEPTH: NONE,
  },
  "codex-token-core": {
    codex_delta: NONE,
    is_noop: NONE,
  },
  "copy-core": {
    copy_text: "J11",
    TABLE_JSON: NONE,
  },
  "creds-core": {
    ACCOUNTS_FIELD: NONE,
    ALL: NONE,
    AUTH_STYLE_FIELD: NONE,
    BASE_URL_FIELD: NONE,
    check_base_url_shape: "J9",
    create_private: NONE,
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
    path_under_claude_home: NONE,
    probe: NONE,
    read_accounts: NONE,
    read_auth_style: NONE,
    read_key: NONE,
    TEMPLATE: NONE,
    to_pretty_json: NONE,
    WIDE_PRINCIPALS: NONE,
    wide_principals_in_sddl: NONE,
  },
  "gate-core": {
    allowed: NONE,
    as_str: NONE,
    gate2: NONE,
    is_ccm_tmux_name: NONE,
    needs_remote_sid: NONE,
  },
  "relay-route-core": {
    ALL: NONE,
    base_url: NONE,
    base_url_shape_ok: NONE,
    KEY_FILE_REL: "J14",
    key_shape_ok: NONE,
    parse_target: NONE,
    PORT: NONE,
    prefix: NONE,
    route_path: NONE,
    segment_is_safe: NONE,
    split_keyed_base_url: NONE,
  },
  "search-core": {
    clamp_limit: NONE,
    clean_user_text: "J10",
    collapse_ws: NONE,
    collapse_ws_keep_ellipsis: NONE,
    DEFAULT_LIMIT: "J15",
    extract_text_blocks: NONE,
    extract_tool_text: NONE,
    find_ci: NONE,
    head_chars: NONE,
    LIMIT_MAX: NONE,
    LIMIT_MIN: NONE,
    MAIN_CAP: NONE,
    make_snippet: NONE,
    new: NONE,
    PER_SESSION_CAP: NONE,
    session_title: NONE,
    SNIPPET_CTX: NONE,
    sort_by_recency: "J15",
    spent: NONE,
    starved: NONE,
    stringify_json: NONE,
    tail_chars: NONE,
    take: NONE,
    TOOL_CAP: NONE,
    truncate_excerpt: NONE,
    truncate_plain: NONE,
  },
  "shell-quote-core": {
    // 〔合并 TL3 续做〕`INVARIANTS §47` ② 自由文本那一层（拒绝集只收 NUL / CR / LF）进了本 crate：TS 侧零处拼 shell、零处判它 ⇒ NONE。
    FREE_TEXT_REFUSED: NONE,
    free_text_ok: NONE,
    posix_free_path_ok: NONE,
    posix_quote: "J4",
    // 〔DUP1〕`INVARIANTS §47` ① 标识符那一层。常量是规则的一部分（上界 · 放行的标点），TS 侧不抄它们：
    //   模型名那两格由 monitor 现生成进 `src/generated/judgment-rules.ts`（生成物不是孪生）⇒ NONE。
    SESSION_ID_MAX: NONE,
    session_id_ok: "J5",
    MODEL_NAME_MAX: NONE,
    MODEL_NAME_EXTRA: NONE,
    model_name_ok: "J17",
    ACCOUNT_NAME_MAX: NONE,
    account_name_ok: "J18",
  },
};

/** 整 crate 排出人群的那一个，与现核它的依据。 */
const TEST_INFRA_CRATE = "guard-core";

/**
 * 提到 `guard-core` 的那几份 `Cargo.toml` 之外还有哪些（现核「只作 dev 依赖」的人群）：
 * 两份顶层工作区 ＋ 全景引擎 ＋ 每个 `crates/*` 自己那份（按盘上 crate 目录现生成，见下）。
 */
const CARGO_TOMLS_TOP = ["src/bridge/Cargo.toml", "src/backend/Cargo.toml", "src/panorama-engine/Cargo.toml"];
/** 那几份里 `guard-core = …` 依赖行的总数（恒等计数：今天 monitor · 后端 · 全景引擎 · creds-core 各一）。 */
const GUARD_CORE_DEP_LINES = 4;

const CRATES_ROOT = "src/bridge/crates";

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

// ─────────────────────────────── 判据 ───────────────────────────────

describe("DUP1 判定只有一个家（设计/90 §3 判据 2）", () => {
  const byCrate = rsByCrate();
  const ts = productionTsFiles("src").map((s) => ({ file: s.file, code: stripComments(s.text, "ts") }));
  const tsAll = ts.map((s) => s.code).join("\n");
  const tsByFile = new Map(ts.map((s) => [s.file, s.code]));

  it("① 人群两向：盘上 *-core 的 pub 项 == 登记表（逐 crate）", () => {
    const diskCrates = [...byCrate.keys()].filter((c) => c.endsWith("-core")).sort();
    expect(diskCrates.length, "一个 *-core 都没扫到 —— 路径断了，下面会零命中地绿").toBeGreaterThan(0);
    expect(diskCrates, "盘上 *-core crate 集合 ≠ 登记表 crate 集合 ∪ {guard-core}").toEqual(
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
      if (j.status === "zero" && j.needles.some((n) => n.count !== 0)) bad.push(`${id} 是 zero，指纹的登记数必须是 0`);
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
  });
});
