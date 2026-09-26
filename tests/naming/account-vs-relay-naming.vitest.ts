/**
 * R3 · **账号 / 中转命名全量清账**的判据（用户裁「要, 所有东西都要准确, 清晰, 解耦清楚.
 * 不要把账号和中转混为一谈」；更早「中转层不要有账号, 账号就账号中转就中转」）。
 *
 * # 口径（唯一口径住 `调研/设计/20` 的「术语归属」表，本文件不另立）
 *
 * | 词 | 指什么 |
 * |---|---|
 * | **中转** / `relay` | 中转：HTTP 那一层本身 —— 让流量经过它、拿到 SSE 流。`--relay` 这个进程、它的端口、往会话里注的那个回环地址、注入前缀、「中转在不在跑」都是这一层的事 |
 * | **账号** / `apikey` | 上游选择：apikey 表（凭据文件）· 按 (agent, 账号) 查行 · 换 key · 每 agent 的默认上游。**不许叫中转** |
 *
 * `--relay` 这**一个进程**同时承载两层（上游选择挂在中转上，依赖只许上游选择 → 中转）。
 * 所以「中转进程」「中转起没起来」照旧叫中转；**它里面那张表、那把 key、那份凭据文件**不叫中转。
 *
 * # 判什么（两向）
 *
 * [`ACCOUNT_NAMES`] 是现打出来的全集：全仓里**账号含义却叫 relay / 中转**的每一个名字，
 * 逐条写了新名、它是哪一类（环境变量 / 文件名 / 命令 / 能力 id / 类型 / 判据名 / 散文词组）。
 * 每一条的状态是**登记出来的**，不是数出来的：
 *
 * - `done` ⇒ 旧名在扫描面上**零命中**，且新名**非零命中**（改名没把东西一起删掉）；
 * - `pending` ⇒ 旧名**非零命中**（这一行不是一条腐掉的登记）。
 *
 * ⇒ 登记与盘面逐条相等；收口那一拍 `pending` 为空，整张表就是「旧名全仓零命中」。
 *
 * [`RELAY_NAMES`] 是同一次现打里**确认是中转**的名字 —— 它们**刻意不改**，
 * 逐条写了为什么是中转。每条要求非零命中（表不腐）。
 *
 * # 扫描面与刻意不扫的（每一格写理由）
 *
 * 人群 = `git ls-files --cached --others --exclude-standard` 的全集（含没提交的新文件），去掉 [`NOT_SCANNED`]。
 *
 * # 正控
 *
 * - 把每条旧名种进一段内存里的文本，匹配器必须认出来（匹配式本身不是空转）；
 * - 扫描面必须含下面这几份锚文件（自定位：扫描器没跑到别的根上去）；
 * - [`RELAY_NAMES`] 每条非零命中 —— 同一个扫描器对中转的词**真的看得见**。
 *
 * # 死值验（2026-09-24 现打，每刀：锚串恰好 1 处 · 替换串不含原串 · 备份拷回后 sha256 逐字节一致 · 恢复后本文件全绿）
 *
 * | 刀 | 下在哪 | 红在哪 |
 * |---|---|---|
 * | K1 | `creds_core::store::FILE_NAME` 改回旧文件名 | 主判：「relay-credentials.json」done 却还有 1 处 |
 * | K2 | 本表 CSS 类那一行 done → pending | 主判：pending 却已零命中 |
 * | K3 | 本表 `CCM_RELAY_CREDENTIALS` 那行的新名匹配式指向不存在的名字 | 主判：新名全仓零命中 |
 * | K4 | `creds_store.rs` 头注「上游选择说没配」改回「中转说没配」 | 主判：散文词组 done 却还有 1 处 |
 * | K5 | 扫描面把 `src/` 整个排掉 | 正控 ①（锚文件不在）＋ 主判（新名零命中） |
 * | K6 | `relay::creds` 的匹配式去掉边界 | 正控 ②（吃进 `relay::creds_guard`）＋ 主判（5 处） |
 * | K7 | `account-chip.ts` 的字段改回 `relayRouting` | 主判：done 却还有 1 处 |
 *
 * # 不判什么（诚实段）
 *
 * - 判不了**新写的**一句散文把中转说成了账号的事：词组那几行只是这次现打里出现过的形状。
 * - 判不了设计文档（仓外，`调研/设计/`）：那里的旧名是「旧名 → 新名」对照表的左列，本来就该留着。
 */
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { REPO_ROOT } from "../test-support/repo-root.ts";

type Kind = "环境变量" | "文件名" | "tauri 命令" | "类型" | "能力 id" | "函数" | "字段" | "CSS 类" | "过期住址" | "判据名" | "散文词组";

interface AccountName {
  /** 旧名（人读）。 */
  old: string;
  /** 旧名的匹配式。**逐条写边界**：`relay::creds` 不许吃进 `relay::creds_guard`。 */
  re: RegExp;
  /** 新名（人读；可以是「按句改写」）。 */
  fresh: string;
  /** 新名的匹配式；散文词组那几行没有固定新名 ⇒ `null`（只判旧名零命中）。 */
  freshRe: RegExp | null;
  kind: Kind;
  /** 为什么它是账号（上游选择），不是中转。 */
  why: string;
  state: "done" | "pending";
}

/** 标识符边界：前后都不是 `[A-Za-z0-9_]`。 */
function ident(s: string): RegExp {
  return new RegExp(`(?<![A-Za-z0-9_])${s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?![A-Za-z0-9_])`);
}
/** 字面串（不加边界）。 */
function lit(s: string): RegExp {
  return new RegExp(s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"));
}

export const ACCOUNT_NAMES: AccountName[] = [
  // ── 盘上 / 进程环境 ──────────────────────────────────────────────────────────
  {
    old: "CCM_RELAY_CREDENTIALS",
    re: ident("CCM_RELAY_CREDENTIALS"),
    fresh: "CCM_APIKEY_CREDENTIALS",
    freshRe: ident("CCM_APIKEY_CREDENTIALS"),
    kind: "环境变量",
    why: "盖的是 apikey 凭据文件的路径 —— 读它的是上游选择（`accounts::upstream::creds`），中转一个字节都不读",
    state: "done",
  },
  {
    old: "relay-credentials.json",
    re: lit("relay-credentials"),
    fresh: "apikey-credentials.json",
    freshRe: lit("apikey-credentials.json"),
    kind: "文件名",
    why: "apikey 表（每个账号的第三方 key ＋ 端点）落盘那一份；`creds_core::store::FILE_NAME`",
    state: "done",
  },
  {
    old: "CCM_RELAY_UPSTREAM",
    re: ident("CCM_RELAY_UPSTREAM"),
    fresh: "CCM_AGENT_UPSTREAM_CLAUDE_CODE",
    freshRe: ident("CCM_AGENT_UPSTREAM_CLAUDE_CODE"),
    kind: "环境变量",
    why: "每 agent 默认上游（`agents::Adapter::upstream`，〔NT2 · V25〕跟着适配层）里 claude-code 那一格的旋钮；中转没有默认上游",
    state: "done",
  },
  // ── 命令面（前端 ↔ monitor；不上后端的线）─────────────────────────────────────
  {
    old: "read_relay_credentials_status",
    re: ident("read_relay_credentials_status"),
    fresh: "read_apikey_credentials_status",
    freshRe: ident("read_apikey_credentials_status"),
    kind: "tauri 命令",
    why: "读 apikey 凭据文件的状态（路径 · 权限 · 读没读坏）",
    state: "done",
  },
  {
    old: "write_relay_credentials_key",
    re: ident("write_relay_credentials_key"),
    fresh: "write_apikey_credentials_key",
    freshRe: ident("write_apikey_credentials_key"),
    kind: "tauri 命令",
    why: "往 apikey 凭据文件里给某个账号写一把 key",
    state: "done",
  },
  {
    old: "RelayCredentialsStatus",
    re: ident("RelayCredentialsStatus"),
    fresh: "ApikeyCredentialsStatus",
    freshRe: ident("ApikeyCredentialsStatus"),
    kind: "类型",
    why: "上一条命令的返回形状",
    state: "done",
  },
  {
    old: "relay.routing",
    re: ident("relay.routing"),
    fresh: "apikey.routing",
    freshRe: ident("apikey.routing"),
    kind: "能力 id",
    why: "`apikey_routing_for` 那条命令归的能力：问「这几个本机账号在 apikey 表里有没有行」",
    state: "done",
  },
  {
    old: "creds.relay-key",
    re: lit("creds.relay-key"),
    fresh: "creds.apikey",
    freshRe: ident("creds.apikey"),
    kind: "能力 id",
    why: "配第三方 API key 那一族命令归的能力",
    state: "done",
  },
  // ── 前端 ───────────────────────────────────────────────────────────────────
  {
    old: "fetchLocalRelayRouting",
    re: ident("fetchLocalRelayRouting"),
    fresh: "fetchLocalApikeyRouting",
    freshRe: ident("fetchLocalApikeyRouting"),
    kind: "函数",
    why: "`apikey_routing_for` 的前端取数口",
    state: "done",
  },
  {
    old: "localRelayStateFor",
    re: ident("localRelayStateFor"),
    fresh: "localApikeyEndpointStateFor",
    freshRe: ident("localApikeyEndpointStateFor"),
    kind: "函数",
    why: "把上一条的读数落到一个账号上：这个号的端点改写成不成（有没有行 ＋ 中转在不在跑）",
    state: "done",
  },
  {
    old: "AccountRelayState",
    re: ident("AccountRelayState"),
    fresh: "ApikeyEndpointState",
    freshRe: ident("ApikeyEndpointState"),
    kind: "类型",
    why: "api-key 号徽章那几档的入参：说的是「cc-monitor 能不能替这个号改写端点」",
    state: "done",
  },
  {
    old: "relayRouting",
    re: ident("relayRouting"),
    fresh: "apikeyRouting",
    freshRe: ident("apikeyRouting"),
    kind: "字段",
    why: "账号 chip 缓存的那份 `ApikeyRoutingView`",
    state: "done",
  },
  {
    old: "RelayKeyAccount",
    re: ident("RelayKeyAccount"),
    fresh: "ApikeyEditorAccount",
    freshRe: ident("ApikeyEditorAccount"),
    kind: "类型",
    why: "账号那一行「配 apikey」编辑格的入参",
    state: "done",
  },
  {
    old: "mountRelayKeyBlock",
    re: ident("mountRelayKeyBlock"),
    fresh: "renderApikeyFileBlock（今天的住址）",
    freshRe: ident("renderApikeyFileBlock"),
    kind: "函数",
    why: "散文里点着的旧住址（那个函数早已拆成 `renderApikeyFileBlock` ＋ `renderApikeyEditor`）",
    state: "done",
  },
  {
    old: "relay-key-*",
    re: /(?<![A-Za-z0-9_-])relay-key-[a-z]/,
    fresh: "apikey-file-* / accounts-row-apikey-*",
    freshRe: lit("apikey-file-block"),
    kind: "CSS 类",
    why: "设置页 apikey 凭据文件那一块 ＋ 账号行里配 key 那一格的类名",
    state: "done",
  },
  // ── 后端里过期的住址（上游选择早已搬出 `relay/`）──────────────────────────────────
  {
    old: "relay::table",
    re: ident("relay::table"),
    fresh: "accounts::upstream::table",
    freshRe: ident("accounts::upstream::table"),
    kind: "过期住址",
    why: "路由表住 `src/backend/accounts/upstream/table.rs`",
    state: "done",
  },
  {
    old: "relay::creds",
    re: ident("relay::creds"),
    fresh: "accounts::upstream::creds",
    freshRe: ident("accounts::upstream::creds"),
    kind: "过期住址",
    why: "读凭据文件住 `src/backend/accounts/upstream/creds.rs`（`relay::creds_guard` 是另一个名字，不在此列）",
    state: "done",
  },
  // ── 判据名 ─────────────────────────────────────────────────────────────────
  {
    old: "only_an_account_that_has_a_row_in_the_relay_table_gets_the_base_url_prefix",
    re: ident("only_an_account_that_has_a_row_in_the_relay_table_gets_the_base_url_prefix"),
    fresh: "only_an_account_that_has_a_row_in_the_apikey_table_gets_the_base_url_prefix",
    freshRe: ident("only_an_account_that_has_a_row_in_the_apikey_table_gets_the_base_url_prefix"),
    kind: "判据名",
    why: "「那张表」是 apikey 表",
    state: "done",
  },
  {
    old: "a_relay_started_with_only_a_file_on_disk_gets_the_key",
    re: ident("a_relay_started_with_only_a_file_on_disk_gets_the_key"),
    fresh: "upstream_selection_loads_the_key_from_a_hand_written_file_alone",
    freshRe: ident("upstream_selection_loads_the_key_from_a_hand_written_file_alone"),
    kind: "判据名",
    why: "它量的是上游选择的 `creds::load`，一个中转进程都没起；拿到 key 的也不是中转",
    state: "done",
  },
  {
    old: "a_launch_command_carrying_the_relay_env_prefix_reaches_the_relay_with_that_accounts_key",
    re: ident("a_launch_command_carrying_the_relay_env_prefix_reaches_the_relay_with_that_accounts_key"),
    fresh: "a_launch_command_carrying_the_relay_env_prefix_reaches_upstream_with_that_accounts_key",
    freshRe: ident("a_launch_command_carrying_the_relay_env_prefix_reaches_upstream_with_that_accounts_key"),
    kind: "判据名",
    why: "「带着那个号的 key」的是到上游的那一发（上游选择换的头），不是中转",
    state: "done",
  },
  // ── 散文词组（中转被说成了账号那一层的东西）──────────────────────────────────────
  {
    old: "中转 API key / 中转那把（第三方 API key）",
    re: /中转\s*(?:API key|那把|自己那把|的 ?key)/,
    fresh: "第三方 API key / apikey 表里那一把",
    freshRe: null,
    kind: "散文词组",
    why: "key 是账号的一格，中转手里没有任何 key",
    state: "done",
  },
  {
    old: "中转表 / 中转的路由表",
    re: /中转(?:的路由)?表/,
    fresh: "apikey 表",
    freshRe: null,
    kind: "散文词组",
    why: "那张表是上游选择的",
    state: "done",
  },
  {
    old: "中转 id",
    re: /中转\s*id/,
    fresh: "apikey 表里的账号 id",
    freshRe: null,
    kind: "散文词组",
    why: "账号 id",
    state: "done",
  },
  {
    old: "设置里说走中转",
    re: /说走中转/,
    fresh: "设置里说走 apikey 端点改写",
    freshRe: null,
    kind: "散文词组",
    why: "设置里说的是「这个号在 apikey 表里有行」—— 全量注入开关开着时，没有行的号也过中转",
    state: "done",
  },
  {
    old: "中转去读哪份凭据 / 中转说没配",
    re: /中转(?:去读|说没配)/,
    fresh: "上游选择去读哪份凭据 / 上游选择说没配",
    freshRe: null,
    kind: "散文词组",
    why: "读凭据文件、判「这个号配没配」的是上游选择",
    state: "done",
  },
];

/** 同一次现打里确认是**中转**的名字 —— 刻意不改。 */
export const RELAY_NAMES: { name: string; re: RegExp; why: string }[] = [
  { name: "--relay", re: /--relay(?![A-Za-z0-9_-])/, why: "中转进程的子命令；它承载两层，但进程本身是中转" },
  { name: "CCM_RELAY_PORT", re: ident("CCM_RELAY_PORT"), why: "中转监听的端口" },
  { name: "CCM_RELAY_ALL_SESSIONS", re: ident("CCM_RELAY_ALL_SESSIONS"), why: "全量注入开关：让订阅号的会话也过中转（`/t/`）" },
  { name: "RELAY_PORT", re: ident("RELAY_PORT"), why: "monitor 侧拼注入地址用的端口" },
  { name: "relay_endpoint_for", re: ident("relay_endpoint_for"), why: "「往 ANTHROPIC_BASE_URL 里写哪个中转地址」的唯一判断口（有行时它把那一格交给 `apikey_endpoint_for`）" },
  { name: "relay_route_path_in", re: ident("relay_route_path_in"), why: "拼中转路由键（中转的线格式）" },
  { name: "relay_base_url_in", re: ident("relay_base_url_in"), why: "拼中转地址" },
  { name: "relay_env_prefix_posix", re: ident("relay_env_prefix_posix"), why: "把中转地址拼成命令前缀" },
  { name: "relay_prefix_for_launch", re: ident("relay_prefix_for_launch"), why: "起会话那一刻挑中转前缀的接线口" },
  { name: "relay_running", re: ident("relay_running"), why: "本机中转进程在不在跑" },
  // 〔RL1 · V107〕`start_local_relay` / `LOCAL_RELAY` 两条随「monitor 另起中转」那一族删掉，换成接替它们的两个中转名字。〔散文墓碑〕
  { name: "relay_host_envs", re: ident("relay_host_envs"), why: "起本机后端时交给它的中转端口（与凭据路径 —— 那是上游选择要读的，但交接这一步是为中转在那个进程里起来）" },
  { name: "host_relay", re: ident("host_relay"), why: "常驻后端进程内起中转的装配口（中转的 host ＋ 上游选择那只手）" },
  { name: "RelayAsk", re: ident("RelayAsk"), why: "`relay_endpoint_for` 的入参" },
  { name: "relay_segment_is_safe", re: ident("relay_segment_is_safe"), why: "中转路由段的字符闸（中转线格式）" },
  { name: "RELAY_KEEPS_THE_OLD_PATH", re: ident("RELAY_KEEPS_THE_OLD_PATH"), why: "中转前缀在场时本机拉起走旧路的理由句" },
  { name: "run_relay", re: ident("run_relay"), why: "`--relay` 的装配口：跑中转 ＋ 把上游选择那只手递进去（住上游选择，依赖上游选择 → 中转）" },
  { name: "src/backend/relay/", re: lit("src/backend/relay/"), why: "中转的目录" },
  { name: "\"source\":\"relay\"", re: lit('"source":"relay"'), why: "tee 流的线上字段值（中转抄出来的 SSE 行）" },
];

/**
 * 〔RN1 · 第四波 4C〕**V114 · 「上游选择」改名**的判据。
 *
 * 要求住址：用户裁决 **V114**（`设计/99 §1`）。用户原话逐字：「中转（面 B）＋ 账号层是什么东西 / 怎么还有账号层? /
 * 账号应该包括订阅+api(即自选url和api)」＋〔选〕「上游选择」。裁决逐字：「原『层 2（账号）/ 账号层』改名『上游选择』
 * —— 账号域里按号决定这一发的上游与凭据的那个口」「纯命名清理，行为不变」。
 *
 * 与上面那张 [`ACCOUNT_NAMES`] 同一套机制（`done` ⇒ 旧名在扫描面零命中、新名非零；同一个扫描面），
 * 多一格：**讲旧叫法来历的那几行**挂 `structural_scan.rs::PROSE_NAME_TOMBSTONE` 那块标记，本表放行它们 ——
 * 而放行的住址集合与 [`V114_TOMBSTONED_FILES`] **两向相等**（新挂一块墓碑想逃过本表 ⇒ 红）。
 * 标记串不在本文件里写第二份字面量：运行期从 Rust 那一份常量的声明行里抠（一个标记一处真相）。
 *
 * 人群：同波别的路新写一句「账号层」/「层 2」/`accounts::apikey`/`account_layer…` ⇒ 本表当场红（合并时对上）。
 *
 * ⚠ 「层 1」**不整词禁**：它在别处另有其义（`设计/15 §3.2` 的层 1 · 只读护栏的层）。只禁中转义的那三种写法。
 * ⚠ 「层 2」按「前面不是 顶 / 卫」禁：`src/bridge/vendor/cc-acct-iso` 的测试里有「守卫层 2」（vendor，不是本仓的词）。
 */
export const UPSTREAM_SELECTION_NAMES: AccountName[] = [
  {
    old: "账号层",
    re: /账号层/,
    fresh: "上游选择（指账号域整体的那几处改成「账号域」）",
    freshRe: /上游选择/,
    kind: "散文词组",
    why: "V114：原「账号层」改名「上游选择」",
    state: "done",
  },
  {
    old: "层 2",
    re: /(?<![顶卫])层\s?2(?![0-9])/,
    fresh: "上游选择",
    freshRe: null,
    kind: "散文词组",
    why: "V114：原「层 2（账号）」改名「上游选择」",
    state: "done",
  },
  {
    old: "中转（层 1）/ 层 1（中转）/ 中转层 1",
    re: /中转（层\s?1）|层\s?1（中转）|中转层\s?1(?![0-9])/,
    fresh: "中转",
    freshRe: null,
    kind: "散文词组",
    why: "V114：中转只有一层 ⇒ 「层 1」是悬空编号",
    state: "done",
  },
  {
    old: "accounts::apikey / accounts/apikey",
    re: /accounts(?:::|\/|\\\\)apikey(?![A-Za-z0-9_])/,
    fresh: "accounts::upstream",
    freshRe: ident("accounts::upstream"),
    kind: "过期住址",
    why: "V114 落地列：代码 `accounts/apikey/` → `accounts/upstream/`",
    state: "done",
  },
  {
    old: "account_layer* / ACCOUNT_LAYER* / apikey_layer",
    re: /account_layer|ACCOUNT_LAYER|apikey_layer/,
    fresh: "upstream_selection_guard",
    freshRe: ident("upstream_selection_guard"),
    kind: "判据名",
    why: "判据文件 / 判据名 / 登记表里的「账号层」",
    state: "done",
  },
  {
    old: "layer_two* / LAYER_TWO* / layer_one*",
    re: /[Ll]ayer_?[Tt]wo|LAYER_TWO|[Ll]ayer_?[Oo]ne|LAYER_ONE/,
    fresh: "upstream_selection* / relay*",
    freshRe: ident("UPSTREAM_SELECTION_PREFIX"),
    kind: "判据名",
    why: "标识符里的「层 2 / 层 1」",
    state: "done",
  },
];

/** V114 那几条旧叫法**只许**活在这几份文件挂了墓碑标记的行里（讲来历）。两向相等。 */
export const V114_TOMBSTONED_FILES = ["src/backend/accounts/upstream/mod.rs", "src/backend/relay/mod.rs"];

/** 墓碑标记：从 `structural_scan.rs` 那一份常量声明里现抠（不在本文件写第二份字面量）。 */
export function proseTombstoneMark(): string {
  const src = readFileSync(resolve(REPO_ROOT, "src/bridge/src/structural_scan.rs"), "utf8");
  const m = /pub const PROSE_NAME_TOMBSTONE: &str = "([^"]+)";/.exec(src);
  if (!m) throw new Error("structural_scan.rs 里抠不出 PROSE_NAME_TOMBSTONE —— 常量改名或搬家了");
  return m[1];
}

/** 同 [`hitsIn`]，但把带墓碑标记的行分开：`[未挂标记的命中, 挂了标记的命中所在文件]`。 */
export function hitsSplitByTombstone(texts: Map<string, string>, re: RegExp, mark: string): [string[], Set<string>] {
  const once = new RegExp(re.source, re.flags.replace("g", ""));
  const live: string[] = [];
  const tomb = new Set<string>();
  for (const [p, t] of texts) {
    if (!once.test(t)) continue;
    for (const line of t.split("\n")) {
      if (!once.test(line)) continue;
      if (line.includes(mark)) tomb.add(p);
      else live.push(`${p}: ${line.trim().slice(0, 160)}`);
    }
  }
  return [live, tomb];
}

/**
 * 刻意不扫的（逐格写理由）。
 *
 * - `tests/evidence/` 里的**历史读数与一次性量具** —— 它们记的是**当时**盘上的样子（改了就是改史）。
 *   ⚠ 例外两份**活的**：`K-R117-ruler.py`（门禁 `installface` 那一格的判据本体，它逐条列着能力 id）·
 *   `CP1-copy-verdicts.tsv`（界面文字台账，`CP1-copy-verdicts.py` 现读）—— 这两份照扫。
 * - `CHANGELOG.md` —— 已发版的发版记录（当时界面上的字就是那几个字）；改不改由主会话裁。
 * - 本文件 —— 旧名是这张表的左列。
 */
const LIVE_EVIDENCE = new Set(["tests/evidence/K-R117-ruler.py", "tests/evidence/CP1-copy-verdicts.tsv"]);
const SELF = "tests/naming/account-vs-relay-naming.vitest.ts";
export function notScanned(path: string): boolean {
  if (path === SELF) return true;
  if (path === "CHANGELOG.md") return true;
  if (path.startsWith("tests/evidence/")) return !LIVE_EVIDENCE.has(path);
  if (path === "package-lock.json") return true;
  return false;
}

/** 扫描面必须含的锚文件（自定位）。 */
const ANCHORS = [
  "src/backend/relay/mod.rs",
  "src/backend/accounts/upstream/mod.rs",
  "src/bridge/src/backend/control/payload.rs",
  "src/accounts.ts",
  "src/doc/IPC-PROTOCOL.md",
  "tests/evidence/K-R117-ruler.py",
];

function trackedFiles(): string[] {
  const out = execFileSync("git", ["ls-files", "-z", "--cached", "--others", "--exclude-standard"], {
    cwd: REPO_ROOT,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  return [...new Set(out.split("\0").filter((p) => p.length > 0))].sort();
}

interface Corpus {
  paths: string[];
  texts: Map<string, string>;
}

function loadCorpus(): Corpus {
  const paths = trackedFiles().filter((p) => !notScanned(p));
  const texts = new Map<string, string>();
  for (const p of paths) {
    let buf: Buffer;
    try {
      buf = readFileSync(resolve(REPO_ROOT, p));
    } catch {
      continue; // 工作树里删了还没提交的
    }
    if (buf.includes(0)) continue; // 二进制
    texts.set(p, buf.toString("utf8"));
  }
  return { paths, texts };
}

export function hitsIn(texts: Map<string, string>, re: RegExp): string[] {
  // 先整份判一次（快），命中了才逐行切 —— 全仓两千多份文件 × 几十条匹配式，逐行全切会超时。
  const once = new RegExp(re.source, re.flags.replace("g", ""));
  const out: string[] = [];
  for (const [p, t] of texts) {
    if (!once.test(t)) continue;
    for (const line of t.split("\n")) {
      if (once.test(line)) out.push(`${p}: ${line.trim().slice(0, 160)}`);
    }
  }
  return out;
}

describe("R3 · 账号 / 中转命名清账（`设计/20` 术语归属）", () => {
  const corpus = loadCorpus();

  it("正控 ①：扫描面含锚文件（自定位）", () => {
    for (const a of ANCHORS) expect(corpus.texts.has(a), `扫描面里没有 ${a} —— 扫描器跑到别处去了`).toBe(true);
  });

  it("正控 ②：每条旧名的匹配式认得出种进去的那一份", () => {
    for (const row of ACCOUNT_NAMES) {
      const sample = row.kind === "散文词组" ? row.old.split(" / ")[0].replace(/（.*$/, "") : row.old.replace("*", "block");
      const planted = new Map([["planted.txt", `前缀 ${sample} 后缀`]]);
      expect(hitsIn(planted, row.re), `「${row.old}」的匹配式认不出它自己`).toHaveLength(1);
    }
    // `relay::creds` 不许吃进 `relay::creds_guard`（边界写对了）。
    const guard = ACCOUNT_NAMES.find((r) => r.old === "relay::creds")!;
    expect(hitsIn(new Map([["x", "relay::creds_guard::LOG_ROOTS"]]), guard.re)).toHaveLength(0);
  });

  it("正控 ③：中转那几个名字逐条看得见（同一个扫描器，表不腐）", () => {
    const blind = RELAY_NAMES.filter((r) => hitsIn(corpus.texts, r.re).length === 0).map((r) => r.name);
    expect(blind, "这几条登记为中转的名字全仓零命中 —— 表腐了或扫描器瞎了").toEqual([]);
  }, 60_000);

  it("每条账号名的状态与盘面逐条相等：done ⇒ 旧名零命中且新名在；pending ⇒ 旧名还在", () => {
    const wrong: string[] = [];
    for (const row of ACCOUNT_NAMES) {
      const olds = hitsIn(corpus.texts, row.re);
      if (row.state === "done") {
        if (olds.length > 0) wrong.push(`「${row.old}」登记 done，盘上还有 ${olds.length} 处：\n    ${olds.slice(0, 12).join("\n    ")}`);
        if (row.freshRe && hitsIn(corpus.texts, row.freshRe).length === 0) wrong.push(`「${row.old}」登记 done，新名「${row.fresh}」全仓零命中`);
      } else if (olds.length === 0) {
        wrong.push(`「${row.old}」登记 pending，盘上已零命中 —— 把它改成 done`);
      }
    }
    expect(wrong, wrong.join("\n")).toEqual([]);
  }, 60_000);
});

describe("V114 · 「上游选择」改名（RN1）", () => {
  const corpus = loadCorpus();
  const mark = proseTombstoneMark();

  it("正控：每条旧名认得出种进去的样本、认不出别义；墓碑行放行、同一行去掉标记就红", () => {
    const want: [string, string][] = [
      ["账号层", "中转里的账号层按这一行换 key"],
      ["层 2", "问层 2 要答案"],
      ["层 2", "层2 的根"],
      ["中转（层 1）/ 层 1（中转）/ 中转层 1", "中转（层 1）住后端"],
      ["中转（层 1）/ 层 1（中转）/ 中转层 1", "R2 让中转层 1 的传输失败"],
      ["accounts::apikey / accounts/apikey", "use crate::accounts::apikey::Accounts;"],
      ["accounts::apikey / accounts/apikey", "src/backend/accounts/apikey/mod.rs"],
      ["account_layer* / ACCOUNT_LAYER* / apikey_layer", "relay::account_layer_guard"],
      ["layer_two* / LAYER_TWO* / layer_one*", "fn render_via_layer_two()"],
    ];
    for (const [old, sample] of want) {
      const row = UPSTREAM_SELECTION_NAMES.find((r) => r.old === old)!;
      expect(hitsIn(new Map([["x", sample]]), row.re), `「${old}」认不出样本「${sample}」`).toHaveLength(1);
    }
    // 别义：不许认。
    const notMine: [string, string][] = [
      ["层 2", "cc-acct-iso：守卫层 2:计划执行器"],
      ["层 2", "顶层 2 处"],
      ["层 2", "层 23 格"],
      ["中转（层 1）/ 层 1（中转）/ 中转层 1", "`设计/15 §3.2` 层 1 那一搬"],
      ["accounts::apikey / accounts/apikey", "accounts::apikey_routed_subset"],
    ];
    for (const [old, sample] of notMine) {
      const row = UPSTREAM_SELECTION_NAMES.find((r) => r.old === old)!;
      expect(hitsIn(new Map([["x", sample]]), row.re), `「${old}」把别义「${sample}」认进来了`).toHaveLength(0);
    }
    // 墓碑那一格两向：带标记 ⇒ 进放行集、不进命中；同一行去掉标记 ⇒ 进命中。
    const row = UPSTREAM_SELECTION_NAMES[0];
    const [liveT, tombT] = hitsSplitByTombstone(new Map([["t.rs", `原叫账号层。${mark}`]]), row.re, mark);
    expect(liveT).toEqual([]);
    expect([...tombT]).toEqual(["t.rs"]);
    const [liveU, tombU] = hitsSplitByTombstone(new Map([["t.rs", "原叫账号层。"]]), row.re, mark);
    expect(liveU).toHaveLength(1);
    expect(tombU.size).toBe(0);
    // 标记是从 Rust 那份常量抠出来的，不是空串（空串会让每一行都「带标记」）。
    expect(mark.length).toBeGreaterThan(2);
  });

  it("每条旧名：未挂墓碑的行零命中且新名在；挂了墓碑的住址 == V114_TOMBSTONED_FILES（两向）", () => {
    const wrong: string[] = [];
    const tombAll = new Set<string>();
    for (const row of UPSTREAM_SELECTION_NAMES) {
      const [live, tomb] = hitsSplitByTombstone(corpus.texts, row.re, mark);
      for (const t of tomb) tombAll.add(t);
      if (row.state === "done") {
        if (live.length > 0) wrong.push(`「${row.old}」登记 done，盘上还有 ${live.length} 处：\n    ${live.slice(0, 12).join("\n    ")}`);
        if (row.freshRe && hitsIn(corpus.texts, row.freshRe).length === 0) wrong.push(`「${row.old}」新名「${row.fresh}」全仓零命中`);
      } else if (live.length === 0) {
        wrong.push(`「${row.old}」登记 pending，盘上已零命中 —— 改成 done`);
      }
    }
    expect(wrong, wrong.join("\n")).toEqual([]);
    expect([...tombAll].sort(), "讲 V114 旧叫法来历、挂了墓碑的住址对不上登记（新挂一块要来这里登记并说清为什么）").toEqual(
      [...V114_TOMBSTONED_FILES].sort(),
    );
  }, 60_000);
});
