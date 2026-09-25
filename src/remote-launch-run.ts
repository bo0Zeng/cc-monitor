/**
 * F41：远端 resume 拉起执行器（UI 侧）。连接 remote-launch 纯函数与后端
 * `launch_remote_terminal`（wt.exe/PowerShell 拉起 `ssh -t …`），tabs.ts 与
 * views/history.ts 共用此单一入口，防两处行为漂移。
 *
 * 失败回退 = F09 旧行为：复制命令 + toast 说明（非 Windows dev / 配置缺失 /
 * wt+PowerShell 都 spawn 失败时，用户仍拿得到可粘贴命令，功能永不变砖）。
 *
 * F03：6 个 executor 收敛为「构造 {ctx,plan} → `renderLaunchCommand` 挑渲染器 → 执行」。
 * **对外签名/返回值语义逐字不变**——`account-restart.ts`/`tabs.ts`/`views/history.ts` 零改动
 * （`runRemoteResumeTmux` 的位置参数签名被 `tests/e2e/restart-cmd-driver.ts` 经 `account-restart.ts`
 * 传递性锁死）。
 */
// 本机 origin（`"<local>"`，与 Rust `inbound_client::LOCAL_ORIGIN` 逐字节相同、有跨语言判据钉着）。
// 〔C4b〕`accounts.ts` 先前那个同名的 `"__local__"` 已退役 —— 全仓只剩这一个本机表示。
import { LOCAL_ORIGIN } from "./backend-policy";
import { commands } from "./ipc/commands";
import {
  planResumeDirect,
  planResumeTmux,
  planResumeIntoExistingTmux,
  planLauncher,
  planAttach,
} from "./launch-requests";
import type { LaunchModifiers } from "./launch-plan";
import { isValidRbindToken } from "./launch-dimensions";
// 🔴 〔步 22b·B 2026-09-20〕**这里原来 `import { renderFallback } from "./launch-render-fallback"`。**
// `设计/90 §4 E` 收官：外层 tmux 那三格切到 `backend::control::payload::render_tmux_outer`
// 之后，本文件是 `renderFallback` **最后一个生产消费者** —— 那一行随之退役。
// ⚠ 那两个文件（`launch-render-fallback.ts` / `session-backend.ts`）**没删**：
// 它们今天是逐字节金标准（`payload-golden.json` / `tmux-outer-golden.json`）的**左边**，
// 也就是「另一种语言的独立实现」；删它们等于把跨语言对拍降级成「Rust 没变」的冻结快照
// —— 与 `launch-render-cli.ts` 同一条先例（复裁过两次：不划算，不删）。
// 逐处住址与「还站不站在生产路上」两把尺子见 `launch_wire_f07_main_path_tests.rs` 的
// `TS_FALLBACK_KEEPERS` / `TS_FALLBACK_REACH`，**本刀两张都重裁过**。
// U8c-2c-2：`tryRenderCli` **不再是生产渲染器**（那一支已切到 Rust）——
// 它降级为「只供 `launch-cli-golden.ts` 生成夹具」。
// ⚠ 〔U8c-3-r2 08-14〕原写「删在 U8c-3」。**那个排期本身该被重问一次**：它今天是
// `cli-golden.json` 的**唯一生成者**，而那份夹具是「ccm 调用行该长什么样」在本仓的
// **独立说法**（Rust 侧 `ccm_invocation.rs` 的自测与实现同住一个文件，不是同一种独立）。
// 删它 = 把跨语言对拍降级成「Rust 没变」的冻结快照 —— `ccm_invocation.rs` 的 P1 头注
// 逐字预告过这个后果。⇒ 删它是**用一份独立说法换 ~500 行非生产代码**，
// 而它今天零生产调用、零漂移风险（夹具字节比对钉着）。本轮复裁：**不划算，不删**。
import type { CliRenderResult } from "./launch-render-cli";
import type { CliRenderRequest, PayloadRenderRequest } from "./launch-cli-wire.ts";
import type { CcmProbeResult } from "./ccm-probe.ts";
import { sanitizeRemoteLauncher } from "./shell-quote.ts";
import { probeCcm } from "./ccm-probe";
import { getBehavior } from "./behavior";
import { showActionFailureToast } from "./error-toast";
import { AGENT_PROFILE } from "./agent-profile";
import { deriveTmuxName, mintTmuxName } from "./remote-launch";
import { LOCAL_LAUNCH_ACCOUNT_WIRE as ACCOUNT_WIRE } from "./generated/launch-render-facts";
import type { LaunchContext, LaunchPlan } from "./launch-plan";
import { copyText } from "./copy-table";

/** P1：Rust 侧 `payload::refuse()` 给业务拒绝打的标。**跨语言双写点** ——
 *  Rust 那侧是 `backend::control::payload::REFUSE_TAG`，两处必须逐字一致。
 *  由 Rust 判据 `the_refuse_tag_is_the_same_string_on_both_sides` 钉住（改一侧会红）。
 *
 *  用途：区分「载荷渲染被拒」（坏输入，换条路渲染只会糊过去 ⇒ 不许回落）
 *  与「IPC/序列化异常」（通道问题，与载荷无关 ⇒ 可回落）。 */
const REFUSE_TAG = "REFUSE:";

// ═══════ 🔴 `设计/80 §8.7` 步 3：**启动期令牌的铸币口** ═══════════════════════════
//
// 步 1 把载荷侧的槽位铺到底（窄 `EnvOp` ＋ 两个渲染器 ＋ 黄金串），**刻意零生产铸币口**
// —— 那一刀的收尾话逐字：「铸币归 §8.7 步 3，本地半要先能记住 `token → HWND`，
// 令牌才有意义」。本刀两件一起落：`bind.rs` 记得住了（`lookup_hwnd_for_token`），
// 这里开始真的铸。
//
// ## 令牌为什么**不可猜**（`§8.6 ③` 的硬要求）
//
// 令牌会出现在远端的 `/proc/<pid>/environ`、`/proc/<pid>/cmdline` 与 shell 历史里
// ⇒ 它**只能是一个不可猜的关联 id，不许承载任何权限语义**。
// 拿到它顶多能让某人的 `↗` 拉错窗口，**不能越权**。
//
// ⇒ 熵取自**平台 CSPRNG**（`crypto.getRandomValues`，16 字节 = **128 位**），
//   渲成 32 个小写十六进制字符。
//
// 🔴 **拿不到 CSPRNG 就 throw，绝不回落 `Math.random()`**。这一条不是洁癖：
//   `Math.random()` 在 V8 里是 xorshift128+，**种子可从少量输出反推** ——
//   那会让「不可猜」这条性质静默失效，而失效的表现是**零**（令牌照样是 32 hex、
//   命令照样跑、判据照样绿）。⇒ 这是本仓 fail-closed 纪律的正典形态：
//   **说不出就放弃，不许近似**（`INVARIANTS §33`）。
//   代价是「没有 CSPRNG 的宿主上拉不起会话」—— 而那个宿主集合是**空的**
//   （Tauri 的 webview 与 Node ≥19 都有 `globalThis.crypto`），
//   真要出现，用户看到的是一条**说得准的**错误，不是一个悄悄变弱的令牌。
//
// ## 🔴 它连带的那个后果，写死在这里
//
// 从这一刀起，**每一次「起 agent 进程」的拉起都带令牌** ⇒ 经
// `renderLaunchCommand` 那道闸（步 1 加的），生产上那条 `ccm …` 调用行
// **只剩 `attach` 那一格还在走**（`planAttach` 不收 `mods`，永不带令牌）。
// 这不是回归、也不是偷偷换路：`RBIND_TOKEN_DIMENSION` 头注 ③ 逐字预告过
// （「带令牌的 plan 一律降级到载荷渲染器……等 `ccm` 学会了，这里改成吐 flag」），
// 两条路的产物都有**入库的逐字节金标准**（`cli-golden.json` / `payload-golden.json`）。
// ⇒ 换的是**渲染形态**，不是渲染语言、也不是校验口径（两条都在 Rust、同一排闸）。
// 判据在 `tests/remote-launch-run.vitest.ts` 的「铸币口」那一组（含一条点名钉住
// 「`attach` 仍走 `ccm …`」的正控 —— 少了它，「CLI 那条路整个死了」会读成一条绿）。

/** 令牌的熵：**16 字节 = 128 位**，渲成 32 个小写十六进制字符。
 *
 *  ⚠ 那个 `32` 是**跨三处的双写点**（本侧字符数 ·
 *  `payload.rs::RBIND_TOKEN_LEN` · `bind.rs::RBIND_TOKEN_LEN`），三处各有判据。
 *  这里只声明**字节数**，字符数由 `2 * 本值` 派生 —— 不再手抄一个 32 出来。 */
const RBIND_TOKEN_ENTROPY_BYTES = 16;

/**
 * 铸一个启动期令牌。
 *
 * **只有一个出口**（这是刻意的）：全仓所有「起 agent 进程」的拉起都从这里取令牌，
 * 于是「令牌怎么产的」这一问只有一个住址可查、只有一处会被死值验打。
 *
 * @throws 拿不到 CSPRNG（`crypto.getRandomValues` 不在）—— 见上方那段，**不回落**。
 * @throws 铸出来的串形状不对 —— 那是一次编程错误（进制/长度算错），
 *         宁可在铸币口当场炸，也不要让它一路滑到远端 shell 里去
 *         （`RBIND_TOKEN_DIMENSION.apply` 与 `payload.rs` 那两道闸会拒，
 *          但那时的错误信息说的是「拼命令时发现形状不对」，指不到这里）。
 */
export function mintRbindToken(): string {
  const c: Crypto | undefined = globalThis.crypto;
  if (!c || typeof c.getRandomValues !== "function") {
    throw new Error(
      copyText("remoteLaunchRun.token.noRandom"),
    );
  }
  const bytes = new Uint8Array(RBIND_TOKEN_ENTROPY_BYTES);
  c.getRandomValues(bytes);
  const token = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
  if (!isValidRbindToken(token)) {
    // 与维度侧**同一条形状**（不在这里写第二份正则）。
    throw new Error(copyText("remoteLaunchRun.token.badShape", { token: JSON.stringify(token) }));
  }
  return token;
}

/** 给一组修饰补上令牌。**已经有令牌就原样返回** —— 调用方显式传的优先（测试与将来
 *  「复用同一个令牌重连」那一档都要这个口子）。
 *
 *  ⚠ **`=== undefined` 不是 `??`/`!`**：与 `RBIND_TOKEN_DIMENSION.applies` 同一条纪律
 *  （**空值 ≠ 未设**，`Z01` 起的支点）。调用方真传了 `""` ⇒ 原样留着 ⇒
 *  维度那一层 `throw` ⇒ 一次铸币 bug 被**当场**看见，而不是静默退化成「这次不带令牌」。 */
function withMintedRbindToken(mods: LaunchModifiers): LaunchModifiers {
  return mods.rbindToken === undefined ? { ...mods, rbindToken: mintRbindToken() } : mods;
}

/** 〔RL1 · 第四波〕**这次拉起的中转地址**：问后端一次（`relay_endpoint_for_launch`），有就作为载荷里的一条
 *  `export-relay-base-url` 补进去；`null` ⇒ plan 原样（照旧直连，逐字节不变）。
 *
 *  - 判断只在后端一处（`payload::relay_endpoint_for`，`设计/20 §3.2` 那张表）—— 本函数**不判**，只转交；
 *  - 远端那台**用到才起**它的中转（后端那一侧做）；apikey 号的中转起不来 ⇒ 后端 reject，
 *    本函数原样抛给执行器那一格 catch（toast「无法构造…」＋ 那句说得出是哪台的理由）；
 *  - `attach` 不起 agent 进程 ⇒ 不问（没有「往中转上指」这回事）。
 *  「哪个号」那一格的键名取生成物（`K-R95`：前端不自己写后端载荷的键）。 */
export async function withRelayEndpoint(
  origin: string,
  ctx: LaunchContext,
  plan: LaunchPlan,
): Promise<LaunchPlan> {
  if (plan.action.kind === "attach") return plan;
  const account =
    ctx.account.kind === "account"
      ? {
          [ACCOUNT_WIRE.tag]: ACCOUNT_WIRE.named,
          [ACCOUNT_WIRE.configDir]: ctx.account.configDir,
          ...(ctx.account.name ? { [ACCOUNT_WIRE.name]: ctx.account.name } : {}),
        }
      : { [ACCOUNT_WIRE.tag]: ACCOUNT_WIRE.base };
  const url = await commands.relay_endpoint_for_launch({
    origin,
    account,
    sid: plan.action.kind === "resume" ? plan.action.sid : null,
  });
  return url === null ? plan : { ...plan, env: [...plan.env, { kind: "export-relay-base-url", value: url }] };
}

/** 挑渲染器：`forceLegacyLaunchRenderer` 手动逃生口（MASTERPLAN R2）短路到载荷那条；否则探测到 ccm
 *  且该 plan 的全部维度都能表达成 CLI 语法 → 走 CLI；探测失败/未装/能力不足/含 CLI 表达不了的
 *  维度（如账号、idle-tmux 复用）→ 安全降级，绝不因为渲染器选择本身而让启动失败。
 *
 *  🔴 〔步 22b·B 2026-09-20〕**两条分支今天都在 Rust 里**（`设计/90 §4 E` 收官）：
 *  `render_ccm_launch`（`ccm …` 调用行）与 `render_launch_payload`（内层载荷 ＋ 外层 tmux 三格）。
 *  ✅ 〔`P12` 2026-09-21 收完〕那个键此前叫 `forceLegacyLaunchRenderer`，而**名字里没有一个词是真的**
 *  （legacy 那一头在 `22b·B` 之后已经不存在）⇒ 改成 `forceLaunchPayloadRenderer`：
 *  保留「强制挑哪个渲染器」这个语义槽位，把宾语换成一个**真住址**（`render_launch_payload`）。
 *  🔴 那一拍顺带治了一个**比改名值钱得多**的毛病：config.json 里**未知键此前是静默忽略的**
 *  ——「关掉了」与「过了」在终端上一模一样。现在旧键出现会在设置面板顶上**指名喊出来**，
 *  并有会红的判据钉着（`tests/config-unknown-keys.vitest.ts`）。裁定住 `设计/99 §2.5 P12`。 */
async function renderLaunchCommand(
  origin: string,
  ctx: LaunchContext,
  plan: LaunchPlan,
): Promise<string> {
  const behavior = await getBehavior();
  // 🔴 `设计/80 §8` 步 1：**带启动期令牌的 plan 不许去试 `ccm …` 调用行那条路。**
  //
  // `ccm` 今天没有承接 `CCM_RBIND_TOKEN` 的 flag，而**生产的 CLI 渲染在 Rust 那侧**
  // （`backend::control::ccm_invocation`）—— 它的 `CliSpec` 里没有 `rbind-token` 这个维度
  // ⇒ 不拦住的话它会**照常渲成功**，只是渲出来的 `ccm …` 里**没有令牌**。
  // 那是「静默丢一样东西」的形状：命令能跑、会话能起、只有 `↗` 从此拉不到窗口，
  // 而归因会指向别处（正是 `§8.5 ②`/`§6.2` 那四档猜要治的病）。
  //
  // ⚠ TS 的 `RBIND_TOKEN_DIMENSION.cliFlags` 也返回 `null`（诚实放弃，判据在
  // `tests/launch-render-cli.test.ts`），但 `tryRenderCli` **今天不是生产渲染器**
  // ⇒ 那一处拦不住这里。**两处都要有**，这一处是站在生产路上的那一处。
  //
  // ⚠ 判据依据的是**载荷里有没有这条 `EnvOp`**（不是「ctx 里有没有 rbindToken」）——
  // 判据必须读渲染器真吃的那个对象，否则「维度没把它推进 plan」这一类回归在这里是隐形的。
  // 〔RL1〕中转地址那一条同理：`ccm …` 调用行说不出它（`CliSpec` 里没有这一维）⇒ 带它就不试那条路。
  const payloadCarriesRbindToken = plan.env.some(
    (op) => op.kind === "export-rbind-token" || op.kind === "export-relay-base-url",
  );
  if (payloadCarriesRbindToken) {
    console.debug(
      `[launch] 载荷带启动期令牌，\`ccm …\` 调用行说不出它 ⇒ 直接走后端载荷渲染（origin=${origin}）`,
    );
  }
  if (!behavior.forceLaunchPayloadRenderer && ctx.transport.kind === "ssh" && !payloadCarriesRbindToken) {
    const probe = await probeCcm(origin);
    // R04①：一次调用同时回答"能不能"与"渲染成什么"。拿不到 `ok:true` 就走兜底——
    // 不存在"渲染出来了但悄悄丢了某个修饰"这个中间态（改造前 `renderCli` 对 `cliFlags` 返回
    // `null` 是静默跳过的，安全性全靠调用方记得先问 `canRenderCli`）。
    //
    // **U8c-2c-2：这一支已切到 Rust**（`backend::control::ccm_invocation::render_ccm_invocation`）。
    // 前端只发结构化请求，命令由后端渲染 —— 这是本工作区第一条真正切过去的渲染路径。
    // TS 的 `tryRenderCli` **没删**，降级为「只供夹具对拍」（删在 U8c-3）。
    //
    // 🔴 〔步 22b·B 2026-09-20〕**降级那条路今天也在 Rust 里了。**
    // 这里原来写着两段：「兜底那支仍在 TS（`container: tmux` 时它要外层 tmux 命令）」
    // 与「本行是 `renderFallback` 唯一有生产调用方的那个消费者（尺子B）」——
    // `设计/90 §4 E` 收官之后**两句都假了**：降级落到下面那两格，
    // 两格都是 `commands.render_launch_payload`（内层 ＋ 外层三格同一条命令）。
    // ⇒ 降级换的是**渲染形态**（`ccm …` 调用行 → 裸载荷/tmux 编排串），
    //   不再是**换一种语言**。
    // ⚠ §33b 那三问的**今天版**只有一个家：`src/doc/INVARIANTS.md §33b` 那张表
    //（由 `doc_claim_registry` 逐问与现场对拍）。**别在这儿复述，复述就会漂。**
    const r = await renderCliViaBackend(ctx, plan, probe);
    if (r.ok) return r.cmd;
    // R04① 的第二条收益（Phase D 审计指出它此前"只活在测试里"，生产侧零消费者）：
    // 把**为什么**降级说出来。刻意用 `console.debug` 而非 toast/`console.warn`——
    // 走兜底渲染器是**正常且预期**的路径（没装 ccm 的用户每次拉起都会走它），
    // 弹 toast 或 warn 等于对着正常行为报警，是净噪音。要查"为什么这台机没走 CLI 路径"时，
    // 这一行是唯一线索；不查的时候它不打扰任何人。
    console.debug(`[launch] CLI 渲染器降级 → 后端载荷渲染器（origin=${origin}）: ${r.reason}`);
  }
  // U8a-2c-pre（账本 S28）：兜底那支的 **`container:"none"` 那一格**先切到 Rust
  // （`backend::control::payload::render_payload`）。
  if (plan.container.kind === "none" && plan.action.kind !== "attach") {
    try {
      return await commands.render_launch_payload({ req: buildPayloadRenderRequest(plan) });
    } catch (e) {
      // 后端拒了（非法 configDir / 会裂的 arg）⇒ **不静默用 TS 版糊过去**：
      // 那等于把一次 fail-closed 变成 fail-open。原样抛给调用方的 catch（它会 toast）。
      throw new Error(copyText("remoteLaunchRun.render.payloadRefused", { e: String(e) }));
    }
  }
  // ★★★ `设计/90 §4 E` 收官那一刀〔步 22b·B 2026-09-20〕：**外层 tmux 那三格也走后端了。**
  //
  // 剩下的这三格是 `container:tmux` 的 `create` / `send-into` 加 `action:attach` ——
  // 它们要的是**外层 tmux 命令**（`new-session` / `send-keys` / `attach`），
  // 22b·A 把承接方补在了 `backend::control::payload::render_tmux_outer`
  // （并进「Rust 载荷」那一份，不单开模块 —— `设计/00 §2.5 ④` 要的是消灭副本），
  // 外加一份**入库的逐字节金标准**（`fixtures/tmux-outer-golden.json`，13 条）
  // 证明它与今天线上那一串一个字节都不差。本刀把生产接过来。
  //
  // 🔴 **不留回落、不留开关、不留双写**（条 80「不要管旧配置」的同一条纪律）：
  // 后端拒了就**诚实失败**。曾经那条回落（`renderFallback(plan)`）从此不在生产段里 ——
  // 保留它等于把 Rust 侧那一整排 fail-closed 闸（空会话名 · `Raw` 越出
  // `[A-Za-z0-9_-]` · 控制符/视觉欺骗字符 · 越界 `@ccm_sid` · 空串 cwd ·
  // create 少送载荷 · attach 多送载荷 · 两层 cwd 同时送）一次性变成 fail-open，
  // 因为 TS 座头注逐字「不做校验/转义」—— 同样的坏输入它**照拼**。
  //
  // ⚠ **这一跳不经网络、不依赖远端**：`render_launch_payload` 是 monitor 自己进程里的
  // tauri 命令（`src/bridge`），不是远端那份后端。⇒ 切过去不引入任何可用性前提。
  //
  // ⚠ 三格各自送什么、两层的 cwd 怎么分工，见 [`buildTmuxOuterRenderRequest`]；
  // **生产送出去的请求必须带 `outer`**（少送它后端会渲出一条只有内层载荷、
  // 没有 tmux 容器的串 —— 那时用户的会话根本不在 tmux 里，而两侧的闸一个都不响）。
  // 那一条由 `tests/remote-launch-run.vitest.ts` 的 `W22B` 组逐格钉着。
  try {
    return await commands.render_launch_payload({ req: buildTmuxOuterRenderRequest(plan) });
  } catch (e) {
    // 同上一格：带 `REFUSE:` 标的是坏输入（换条路渲染只会糊过去），不带标的是通道异常；
    // 两者在这一格的处置**相同** —— 因为这里已经没有第二条路了。
    throw new Error(copyText("remoteLaunchRun.render.outerRefused", { e: String(e) }));
  }
}

/** U8c-2c-2：把 `{ctx, plan, probe}` 摊成上线形状，交给 Rust 渲染 `ccm …` 调用行。
 *
 *  **`ok:false` 不是错误，是诚实降级**（§33）—— 调用方拿着 `reason` 去走载荷那条，
 *  与切换前 `tryRenderCli` 的语义逐字相同。
 *
 *  ⚠ IPC 本身失败（后端崩/参数被拒）与「渲染器说渲染不出来」是**两件事**：
 *  前者 catch 成一条带 `IPC` 字样的 reason，照样降级 —— 拉起功能永不因为渲染器选择而变砖。 */
async function renderCliViaBackend(
  ctx: LaunchContext,
  plan: LaunchPlan,
  probe: CcmProbeResult,
): Promise<CliRenderResult> {
  const req = buildCliRenderRequest(ctx, plan, probe);
  try {
    const res = await commands.render_ccm_launch({ req });
    return res.ok && res.cmd !== null
      ? { ok: true, cmd: res.cmd }
      : { ok: false, reason: res.reason ?? copyText("remoteLaunchRun.renderCli.noReason") };
  } catch (e) {
    return { ok: false, reason: copyText("remoteLaunchRun.renderCli.fallback", { e: String(e) }) };
  }
}

/** U8a-2c-pre 复盘（判据体系审计）：**请求构造抽出来，好让夹具用同一份代码产它**。
 *
 *  ⚠ 抽之前，`renderCliViaBackend` 里这 22 行**零判据** —— 审计实测三个变异全绿：
 *  成功分支整个作废 · `isSsh` 恒 false（CLI 路径永久死掉）· `ccmSid`/`model` 恒 null
 *  （两个维度静默消失）。它们静默的形态都一样：**回落到另一条渲染路，功能不变砖、门禁全绿**
 *  〔那时那一条是 TS 的兜底渲染器；步 22b·B 之后是 Rust 的载荷渲染器 —— **形状没变**：
 *  「静默换一条路」永远长得和「本来就该走那条」一模一样，所以它需要一条自己的判据〕。
 *
 *  现在 `launch-cli-golden.ts` 用这同一个函数产夹具里的 `req`，Rust 侧拿**生产 wire 类型**
 *  反序列化它、跑**生产命令**、与 TS 渲染器的产物逐字节比 ⇒ 上面那三个变异各自会让
 *  `req` 变形 ⇒ Rust 产出变 ⇒ 与 `out` 不一致 ⇒ 红。 */
export function buildCliRenderRequest(
  ctx: LaunchContext,
  plan: LaunchPlan,
  probe: CcmProbeResult,
): CliRenderRequest {
  return {
    isSsh: plan.transport.kind === "ssh",
    // `K-R53` `KR53D3`：**肯定式** —— `null` 的两种来历（真没装 · 没探出来）在这条 wire 上
    // 本来就同形（都是「拿不到能力集」⇒ Rust 侧 `installed = caps.is_some()` 为 false ⇒ 降级），
    // 而**值**那一侧已经分得开了（`ccm-probe.ts` 的三态）。这里刻意不把 `unknown` 写成
    // `installed:false` 再传下去 —— 降级是处置，不是事实。
    caps: probe.state === "installed" ? [...probe.capabilities] : null,
    action:
      plan.action.kind === "resume"
        ? { kind: "resume", sid: plan.action.sid }
        : plan.action.kind === "attach"
          ? { kind: "attach", name: plan.action.name }
          : { kind: "new" },
    container:
      plan.container.kind === "tmux"
        ? { kind: "tmux", name: plan.container.name, send_into: plan.container.mode === "send-into" }
        : { kind: "none" },
    cwd: plan.cwd,
    account:
      ctx.account.kind === "account"
        ? { kind: "account", name: ctx.account.name ?? null }
        : { kind: "base" },
    ccmSid: ctx.ccmSid ?? null,
    model: ctx.modelOverride ?? null,
    launcher: sanitizeRemoteLauncher(plan.launcher),
    defaultLauncher: AGENT_PROFILE.defaultLauncher,
  };
}

/** 同 [`buildCliRenderRequest`]：抽出来好让夹具用同一份代码产 `req`。 */
/**
 * `设计/90 §4 E`：把一个**要外层 tmux 命令**的 plan 摊成上线形状。
 *
 * 三格：`container:tmux` 的 `create` / `send-into`，加上 `action:attach`。
 * 与 `buildPayloadRenderRequest` 的两处分工差别，都是**真差别**不是口味：
 *
 * 1. **cwd 归外层** —— tmux 那两格的内层没有 `cd`（cwd 是 `new-session -c` 的实参）。
 *    顶层 `cwd` 因此恒 `null`；两个都送后端会 fail-closed 拒。
 * 2. **attach 那一格不带载荷** —— 它一个 agent 进程都不起，
 *    `env` / `args` / `launcher` 一律空；带了后端会拒（而不是静默丢）。
 *
 * 🔴 **〔步 22b·B 2026-09-20〕生产接过来了。** 这里原来逐字写着「生产调用方今天是 0
 * 〔本轮只搬了『后端产得出』那一半〕，唯一的调用者是金标准发生器」——
 * 那是 22b·A 的读数。今天的调用方有两个：`renderLaunchCommand` 最后那一格（**生产**）
 * 与 `src/launch-tmux-outer-golden.ts`（金标准发生器）。
 *
 * ⚠ **最要紧的一条**：生产那一格送出去的 `req` **必须带 `outer`**。
 * 少送它不会有任何一道闸响 —— 后端会老老实实渲一条**只有内层载荷**的串
 *（那是 `container:"none"` 的合法形态），而用户的会话于此根本不在 tmux 里。
 * ⇒ 「三格各自送的是哪个 `outer.mode`」由 `tests/remote-launch-run.vitest.ts`
 * 的 `W22B` 组逐格钉着（死值验：把这一格换成 `buildPayloadRenderRequest` ⇒ 三条红）。
 */
export function buildTmuxOuterRenderRequest(plan: LaunchPlan): PayloadRenderRequest {
  const base = buildPayloadRenderRequest(plan);
  if (plan.action.kind === "attach") {
    if (plan.container.kind !== "tmux") throw new Error("programmer error: attach needs a tmux container"); // 〔CP2b〕程序员错误，刻意英文（不是对外文案，同 copy-table.ts 的两条抛错）
    return {
      ...base,
      env: [],
      args: [],
      launcher: "",
      cwd: null,
      wrap: [],
      outer: {
        mode: "attach",
        name: plan.container.name,
        quoting: plan.container.nameQuoting,
      },
    };
  }
  if (plan.container.kind !== "tmux") throw new Error("programmer error: these launch shapes need a tmux container");
  if (plan.container.mode === "attach-only") {
    throw new Error("unreachable: attach-only is handled by the action.kind === 'attach' branch");
  }
  return {
    ...base,
    cwd: null, // ★ 内层不加 `cd` —— cwd 交给外层的 `new-session -c`
    outer:
      plan.container.mode === "create"
        ? {
            mode: "create",
            name: plan.container.name,
            quoting: plan.container.nameQuoting,
            cwd: plan.cwd,
            ccmSid: plan.identity?.ccmSid ?? null,
          }
        : {
            mode: "send-into",
            name: plan.container.name,
            quoting: plan.container.nameQuoting,
          },
  };
}

export function buildPayloadRenderRequest(plan: LaunchPlan): PayloadRenderRequest {
  return {
    env: plan.env,
    cwd: plan.cwd,
    launcher: sanitizeRemoteLauncher(plan.launcher),
    args:
      plan.action.kind === "resume"
        ? [AGENT_PROFILE.resumeFlag, plan.action.sid, ...plan.args]
        : [...plan.args],
    nestedEnv: [...AGENT_PROFILE.nestedEnvVars],
    wrap: plan.wrap.map((w) => ({ order: w.order, prelude: w.prelude })),
  };
}

interface LaunchToasts {
  success: string;
  successDetail: string;
  failureCopied: string;
  failureNotCopied: string;
}

/** MASTERPLAN §3 账本对 `remote-launch-run.ts` 的既定最终形态之一：「剪贴板回退集中一处」。
 *  6 个 executor 的 invoke→toast/剪贴板回退骨架逐字相同，只有文案与 `origin` 不同——收敛成
 *  这一个函数，返回「IPC 是否真的被接受」（true=拉起成功；false=已走剪贴板回退）。 */
/** U8b：后端在 POSIX 上回的那句话里的**稳定标记**。
 *
 *  它不是「随便找个子串」——后端 `launch.rs::POSIX_NO_TERMINAL_WINDOW` 是那句话的唯一出处，
 *  两边由 Rust 侧的 `the_posix_marker_is_the_one_the_frontend_matches_on`
 *  逐字对拍（`include_str!` 读本文件）。改一边不改另一边 ⇒ 红。
 *
 *  **为什么按错误文本判、而不是按 `hostOs` 判**：`hostOs !== "windows"` 会把
 *  **真失败**（配置缺失 / 命令被拒 / spawn 崩）也一起软化成「这是设计」——那是另一种撒谎。
 *  按后端自己的声明判，只软化后端明说「这是既定设计」的那一种。 */
export const POSIX_NO_WINDOW_MARKER = "刻意不替你挑终端模拟器";

/** 〔`设计/80 §8.7` 步 3 收尾，第二波 T4〕**这份 plan 真正要渲进载荷的那个令牌**。
 *
 *  从 `plan.env` 取、不从 `ctx`/`mods` 取：渲染器吃的是 `plan`，于是「交给本地窗口去登记的令牌」
 *  与「注进远端进程环境的令牌」**在构造上是同一个串** —— 从中间态取的话，「维度没把它推进 plan」
 *  那一类回归会造出一个本地登记了、远端却没有的令牌（join 静默失配，表现与「没令牌」同形）。 */
function rbindTokenOf(plan: LaunchPlan): string | null {
  const op = plan.env.find((o) => o.kind === "export-rbind-token");
  return op && op.kind === "export-rbind-token" ? op.value : null;
}

async function invokeLaunchOrCopyFallback(
  origin: string,
  cmd: string,
  toasts: LaunchToasts,
  // 〔第二波 T4〕这次拉起的启动期令牌（`rbindTokenOf`）；`attach` 那一格恒 `null`。
  //   交给后端，让新开的窗口以它为 marker 登记进本地表（`launch.rs::with_rbind_bind_prelude`）。
  rbindToken: string | null,
): Promise<boolean> {
  try {
    await commands.launch_remote_terminal({ origin, remoteCmd: cmd, rbindToken });
    showActionFailureToast(toasts.success, toasts.successDetail, { level: "info", durationMs: 6000 });
    return true;
  } catch (err) {
    // 回退：复制命令让用户自己粘贴（保留 F09 语义）。
    let copied = true;
    try {
      await navigator.clipboard.writeText(cmd);
    } catch {
      copied = false; // 命令在 toast 里仍可见，可手动复制
    }
    // U8b：**POSIX 上不开终端窗口是既定设计，不是失败。**
    // 原来这里一律报「拉起失败」，在 Linux 上每次点 ↗ 都会读到 —— 那是把一个正常状态
    // 训练成「坏了」。标题按后端的声明分档；正文原样带上后端那句话（它自己会解释为什么）。
    const byDesign = String(err).includes(POSIX_NO_WINDOW_MARKER);
    const headline = byDesign
      ? copied
        ? copyText("remoteLaunchRun.copyFallback.noWindowCopied")
        : copyText("remoteLaunchRun.copyFallback.noWindowManual")
      : copied
        ? toasts.failureCopied
        : toasts.failureNotCopied;
    // ★ 本机没有 ssh 那一跳，文案不能照抄远端那句〔08-12〕。
    const where =
      origin === LOCAL_ORIGIN ? copyText("remoteLaunchRun.copyFallback.runLocal") : copyText("remoteLaunchRun.copyFallback.runRemote", { machine: origin });
    showActionFailureToast(
      headline,
      `${String(err)}\n${where}\n${cmd}`,
      { level: "info", durationMs: 10000 },
    );
    return false;
  }
}

/** 一键 resume 远端会话：拉起成功 toast 告知；失败回退复制命令。 */
export async function runRemoteResume(
  origin: string,
  sid: string,
  cwd: string,
  launcher: string,
  mods: LaunchModifiers = {}, // R03：正交修饰 bag（configDir/accountName/modelOverride），见 launch-plan.ts
  // Phase G（branch-anywhere）：返回值从 `void` 改成 `boolean`，与 `runRemoteResumeTmux`
  // 对齐（那边的头注逐字记着为什么要有返回值：account-ux 那次把「走到了第⑤步」当成
  // 「已 resume」）。既有调用点忽略返回值 ⇒ 行为逐字不变。
): Promise<boolean> {
  let cmd: string;
  let token: string | null;
  try {
    const { ctx, plan: bare } = planResumeDirect(sid, cwd, launcher, withMintedRbindToken(mods));
    const plan = await withRelayEndpoint(origin, ctx, bare);
    cmd = await renderLaunchCommand(origin, ctx, plan);
    token = rbindTokenOf(plan);
  } catch (err) {
    showActionFailureToast(copyText("remoteLaunchRun.resume.buildFailed"), String(err));
    return false;
  }
  return invokeLaunchOrCopyFallback(origin, cmd, {
    success: copyText("remoteLaunchRun.resume.started"),
    successDetail: copyText("remoteLaunchRun.resume.startedBody", { machine: origin }),
    failureCopied: copyText("remoteLaunchRun.resume.failedCopied"),
    failureNotCopied: copyText("remoteLaunchRun.copyFallback.failedManual"),
  }, token);
}

/** F52：tmux 版 resume——在远端 tmux 会话 `<sid8>-cc` 里幂等 resume Claude;失败回退复制命令。
 *
 *  @returns 是否**真的把终端拉起来了**。false = 命令构造失败 / `launch_remote_terminal` 失败
 *  （此时已走剪贴板回退，需用户手动粘贴）。
 *  account-ux Phase G 审计:此前返回 void 且两条失败路径都自己吞掉,于是 `restartWithAccount`
 *  把"走到了第⑤步"当成"已 resume"——会话被 kill、没起来,却照样记 pin、照样弹「已用新账号重启」、
 *  照样 return true,批量对齐还把它计成成功。而失败是**确定性**的（如 F34 launcher 含双引号被
 *  launch.rs 拒、tmux 名不合白名单、缺 OpenSSH），不是概率事件。 */
export async function runRemoteResumeTmux(
  origin: string,
  sid: string,
  cwd: string,
  launcher: string,
  // F13：`name` 改必填（原为 `name?`）。生产三个调用点本来都传，但类型允许省略 ——
  // 而省略就意味着走一个**不做撞名避让**的默认值。让 `tsc` 把「碰巧没人省略」变成「不可能省略」。
  name: string,
  mods: LaunchModifiers = {}, // R03：正交修饰 bag（configDir/accountName/modelOverride），见 launch-plan.ts
): Promise<boolean> {
  let cmd: string;
  let token: string | null;
  try {
    const { ctx, plan: bare } = planResumeTmux(sid, cwd, launcher, name, withMintedRbindToken(mods));
    const plan = await withRelayEndpoint(origin, ctx, bare);
    cmd = await renderLaunchCommand(origin, ctx, plan);
    token = rbindTokenOf(plan);
  } catch (err) {
    showActionFailureToast(copyText("remoteLaunchRun.resumeTmux.buildFailed"), String(err));
    return false;
  }
  return invokeLaunchOrCopyFallback(origin, cmd, {
    success: copyText("remoteLaunchRun.resumeTmux.started"),
    successDetail: copyText("remoteLaunchRun.resumeTmux.startedBody", { machine: origin }),
    failureCopied: copyText("remoteLaunchRun.resumeTmux.failedCopied"),
    failureNotCopied: copyText("remoteLaunchRun.copyFallback.failedManual"),
  }, token);
}

/** U8a-2c-1 + **F14**：把 `send-keys` 那半边交给**远端 backend**（`control/launch.rs`，`mode:"send-into"`）。
 *
 *  @returns 三态 —— `"typed"` = 载荷**真的键入了** ⇒ 终端只需 `attach`；
 *  `"fallback"` = **能证明什么都没发出去** ⇒ 照今天那条整串走；
 *  `"refused"` = backend 说过话了（或我们无法证明它没执行）⇒ **绝不许回落**。
 *
 *  # ★★ F14 为什么把两态改成三态
 *
 *  原来「任何一步不顺都回落」。而那条整串（`session-backend.ts` 的
 *  `tmux send-keys -t '=name:' … ; tmux attach …`）**没有 §34 的门** —— 既无 `display-message`
 *  探测也无 `CCM_GUARD_REJECTED`。于是：
 *
 *  - 一次 `wrong_owner`（门说「这不是本工具的会话」）会被那条**无门**的路重做一遍；
 *  - 更实的一条：backend **已经过门并键入成功**，但应答排在出方向帧后面、慢链路下 >10s
 *    ⇒ monitor 侧超时 ⇒ 回落 ⇒ **载荷第二次被键入**，而这次落进一个**已经在跑 claude 的 pane**
 *    ⇒ 那条 `env … claude --resume …` 被当成 **prompt 提交**、写进对话历史、**不可撤销**。
 *
 *  ⇒ 分流判定**不在这里**，它住 Rust 侧的 `backend/control/backend_route.rs`（与 `kill`/`send-keys`
 *  共用一份），这里只读它翻出来的 `mayFallBack`。
 *
 *  ⚠ **`"refused"` 要 toast**（改了原来那条「绝不 toast」的纪律）：回落是用户看不出区别的，
 *  所以不该吵；而**拒绝**意味着这次就地 resume 没做成，用户必须知道 —— 否则他会以为成功了。
 *
 *  🔴 **〔步 22b·B 2026-09-20〕那条登记在案的 fail-open 关掉了。**
 *  原文逐字：「⚠ **诚实登记一处仍然 fail-open**：载荷渲染被 Rust 拒（非法 configDir /
 *  会裂的 arg）时走 `"fallback"`，而兜底渲染器（TS）对同样输入**未必拒** ……
 *  收成 fail-closed 要连兜底渲染器一起收 ⇒ U8c-3。」
 *  ⇒ `设计/90 §4 E` 收官之后，`"fallback"` 那一跳去渲染整串走的也是
 *  `commands.render_launch_payload`（同一个 Rust 渲染器、同一排闸）⇒
 *  **同样的坏输入在回落那条路上照样被拒**，不再有「换条路糊过去」这个出口。
 *  ⚠ 诚实边界：本条说的是「两条路的拒绝口径同源」，**不是**「回落那一跳不会重做」——
 *  重做安不安全仍由 `mayFallBack` 那条判定（F14）负责，一个字没动。 */
async function sendIntoViaBackend(
  origin: string,
  name: string,
  plan: LaunchPlan,
): Promise<{ verdict: "typed" | "fallback" | "refused"; reason?: string }> {
  try {
    const payload = await commands.render_launch_payload({
      req: buildPayloadRenderRequest(plan),
    });
    const res = await commands.backend_send_into({ req: { origin, name, payload } });
    if (res.typed) return { verdict: "typed" };
    const reason = res.reason ?? copyText("remoteLaunchRun.sendInto.noReason");
    if (res.mayFallBack) {
      console.debug(`[F14] send-into 回落到整串（证明没发出去）：${reason}`);
      return { verdict: "fallback", reason };
    }
    // ★ 不许回落：backend 说过话，或我们无法证明它没执行。
    console.debug(`[F14] send-into 被拒，**不回落**：${reason}`);
    return { verdict: "refused", reason };
  } catch (e) {
    // ★★ P1：这里原来把**两件事**混成一件，注释是这么写的 ——
    //   「两者都在后端那一跳之前 ⇒ 能证明什么都没发出去 ⇒ 可回落」
    // **对一半错一半**：「没发出去 ⇒ 重做不会重复执行」对；「所以可以回落」错 ——
    // 没发出去只说明**重做是安全的**，**不说明重做走的那条路也会拒**。
    // 🔴 〔步 22b·B 2026-09-20 订正一句〕原文写「而回落那条路是 TS 兜底渲染器，它对同样输入
    // **未必拒**」——`设计/90 §4 E` 收官之后那条路也是 Rust 渲染（同一排闸），
    // **口径已经同源**。⇒ 今天保留这个分法的理由**换人了，而结论不变**：
    // 带标说明这是一次**业务拒绝**，重做只会被同一道闸再拒一次（净噪音 ＋ 一次假成功的机会），
    // 而 `refused` 会让用户看见「这次就地 resume 没做成」。**不是因为另一条路更松。**
    //
    // 分法：Rust 的**业务拒绝**都经 `payload::refuse()` 打了 `REFUSE:` 标
    //（那侧有判据 `every_business_rejection_is_tagged` 钉住「一条都不许裸写」）。
    // 带标 ⇒ 坏输入，换条路渲染只会把坏输入糊过去 ⇒ **refused，不回落**。
    // 不带标 ⇒ IPC/序列化异常 ⇒ 通道问题，与载荷本身无关 ⇒ 照旧 fallback。
    //
    // ⚠ 诚实边界：这是**字符串约定不是类型**（全仓 70 个 tauri command 的错误都是 `String`，
    // 本件不在这里开第一个结构化的口 —— 那是 `U6`）。手写一个带同样前缀的普通错误串会被误判。
    const raw = String(e);
    if (raw.includes(REFUSE_TAG)) {
      console.debug(`[P1] send-into 载荷渲染被拒，**不回落**（同一道闸只会再拒一次）：${raw}`);
      return { verdict: "refused", reason: raw };
    }
    console.debug(`[F14] send-into 回落到整串（通道异常，尚未发出）：${raw}`);
    return { verdict: "fallback", reason: raw };
  }
}

/** F03：往一个**已存在的空 tmux**（idle-tmux：claude 已退、只剩交互 shell 的 `<sid8>-cc`）就地
 *  resume——send-keys 载荷 + attach，复用原会话名（不产孤儿，治 #76）。签名/返回值与
 *  `runRemoteResumeTmux` 对齐：true=真拉起来了；false=命令构造失败/拉起失败（已回退剪贴板）。
 *  **CLI 那条渲染器对这类 plan（`mode==="send-into"`）恒返回 `ok:false`**——ccm 没有就地
 *  复用能力，本函数因此恒走载荷那条（诚实放弃，见 F03 计划 §2「#76 防线」）。
 *  🔴 〔步 22b·B 2026-09-20〕「载荷那条」今天是 **Rust**（`render_launch_payload` 带
 *  `outer:{mode:"send-into"}`），不再是 TS 的座。 */
export async function runRemoteResumeIntoExistingTmux(
  origin: string,
  sid: string,
  name: string,
  launcher: string,
  mods: LaunchModifiers = {}, // R03：正交修饰 bag（configDir/accountName/modelOverride），见 launch-plan.ts
): Promise<boolean> {
  let cmd: string;
  let viaBackend = false;
  let token: string | null;
  try {
    const { ctx, plan: bare } = planResumeIntoExistingTmux(sid, name, launcher, withMintedRbindToken(mods));
    const plan = await withRelayEndpoint(origin, ctx, bare);
    // ★ 〔第二波 T4〕两条出路都用**这份 send-into plan** 的令牌：`typed` 那条开的窗口只跑 `attach`，
    //   但它接上的正是刚被键入、环境里带着这个令牌的那个 claude —— 本地要登记的就是这一个。
    token = rbindTokenOf(plan);
    // ★ U8a-2c-1：**先试 backend**。这一格今天的整串是
    //   `tmux send-keys -t '=name:' '<载荷>' Enter; tmux attach -t '=name:'` —— 两半干干净净：
    //   `send-keys` 交给远端 `control/`，`attach` **必须**留在用户自己的终端（§1.3）。
    //   拿不到控制通道 / backend 回报未键入 ⇒ 原样回落到整串（**行为逐字不变**），
    //   所以这条切换在没有后端的远端上是零影响的。
    const sent = await sendIntoViaBackend(origin, name, plan);
    if (sent.verdict === "refused") {
      // ★ F14：**不许回落**。那条整串没有 §34 的门 ⇒ 回落等于用一条无门的路把
      //   「被门拒绝」或「可能已经键入过」重做一遍（后者会把载荷第二次提交给正在跑的 claude）。
      //   ⇒ 就地失败，并且**要让用户看见** —— 这次就地 resume 没做成。
      showActionFailureToast(copyText("remoteLaunchRun.inPlace.notRun"), sent.reason ?? copyText("remoteLaunchRun.inPlace.refusedUnsure"));
      return false;
    }
    if (sent.verdict === "typed") {
      const attach = planAttach(name);
      cmd = await renderLaunchCommand(origin, attach.ctx, attach.plan);
      viaBackend = true;
    } else {
      cmd = await renderLaunchCommand(origin, ctx, plan);
    }
  } catch (err) {
    showActionFailureToast(copyText("remoteLaunchRun.inPlace.buildFailed"), String(err));
    return false;
  }
  return invokeLaunchOrCopyFallback(origin, cmd, {
    success: copyText("remoteLaunchRun.inPlace.done"),
    successDetail: viaBackend
      ? copyText("remoteLaunchRun.inPlace.doneByBackend", { name, machine: origin })
      : copyText("remoteLaunchRun.inPlace.doneBody", { machine: origin, name }),
    failureCopied: copyText("remoteLaunchRun.inPlace.failedCopied"),
    failureNotCopied: copyText("remoteLaunchRun.copyFallback.failedManual"),
  }, token);
}

/**
 * ★★ P3 刀 3：**本机**就地 resume —— 往一个已存在的空 tmux 送载荷，不 new-session。
 *
 * 治的是 issue #76 的根因：不复用就会产 `<sid8>-cc-2` 孤儿，
 * 而用户以为自己回到了原会话。远端那半（`runRemoteResumeIntoExistingTmux`）早就在做这件事。
 *
 * # 与远端那条的差别只有两处，其余逐字共用
 *
 * ① **载荷那半共用**：同一个 `planResumeIntoExistingTmux` + 同一个 `sendIntoViaBackend`
 *    + 同一条 `backend_send_into`（它 `client_for(&origin)`，**本来就传输无关**）。
 *    这就是 `C1`「差别只允许出现在传输这一跳」的样子。
 * ② **attach 那半本机做不到，而且是结构性的**：`launch_remote_terminal` 只会
 *    `ssh + PowerShell`，而 POSIX 本机 `launch.rs` 逐字「**不开 GUI 终端窗口**」——
 *    「开窗口要先猜用户用哪个终端模拟器，是平白引入一个会在别人机器上错的决定」。
 *    ⇒ 送完载荷就把 attach 命令交给用户（与远端 POSIX 宿主上**同一种**处置：
 *    `POSIX_NO_WINDOW_MARKER` 那条路早就在这么做）。
 *
 * # 为什么本机这条**一条回落都没有**
 *
 * 远端那条在 `verdict === "fallback"` 时会回落去渲染整串（走 ssh 重做一遍）。
 * 本机没有那条路 —— 也**不该造**一条：`C1` 逐字排除「给本地单写一套控制逻辑」。
 * ⇒ 只要不是 `typed`，就诚实失败并让用户看见，绝不用另一条路把「可能已经键入过」重做一遍
 *（那会把载荷第二次提交给正在跑的 claude —— F14 逐字记着这个后果）。
 */
export async function runLocalResumeIntoExistingTmux(
  sid: string,
  name: string,
  launcher: string,
  mods: LaunchModifiers = {},
): Promise<boolean> {
  let plan: LaunchPlan;
  try {
    const { ctx, plan: bare } = planResumeIntoExistingTmux(sid, name, launcher, withMintedRbindToken(mods));
    // 〔RL1〕本机这一格同样问一次（本机的两个事实走起会话那一侧的缝）：
    //   「就地 resume」不经 `launch_local`，先前这里的 apikey 号**不走**中转 —— `01 §2.5` 入口纪律的一个漏口。
    plan = await withRelayEndpoint(LOCAL_ORIGIN, ctx, bare);
  } catch (err) {
    showActionFailureToast(copyText("remoteLaunchRun.inPlace.buildFailed"), String(err));
    return false;
  }
  const sent = await sendIntoViaBackend(LOCAL_ORIGIN, name, plan);
  if (sent.verdict !== "typed") {
    // `fallback` 与 `refused` 在本机是**同一种处置** —— 见头注：本机没有第二条路，
    // 而造一条就是 `C1` 排除的那件事。两者的 `reason` 都原样交给用户。
    showActionFailureToast(
      copyText("remoteLaunchRun.inPlace.notRun"),
      sent.reason ?? copyText("remoteLaunchRun.inPlaceLocal.noBackend"),
    );
    return false;
  }
  // ★ attach 那半**与远端共用同一条路**〔用户裁定 08-12：「attach 暂时就用纯 linux bash
  //   以及 windows 的 PowerShell + Windows Terminal」〕。
  //
  //   `invokeLaunchOrCopyFallback` 里那两条分档正好就是裁定的两侧：
  //   · Windows → `launch_remote_terminal` 走 `launch_powershell_window`（PowerShell + WT）；
  //   · Linux   → 那条回 `POSIX_NO_TERMINAL_WINDOW`，前端按 `POSIX_NO_WINDOW_MARKER`
  //     把标题分档成「本机不开终端窗口，命令已复制」，正文给出在自己 bash 里执行的命令。
  //   ⇒ 本机不再自己写一份复制逻辑 —— 写第二份就是 `C1` 排除的那件事。
  //   命令用 `=name:` 精确形态（§31a）。
  //
  // ★★ 〔`P9` 08-12〕这里原本**手写** `tmux attach -t '=<name>:'` —— 那违反
  //   `src/doc/INVARIANTS.md §31` 最终形态第①条逐字「**前端绝不硬编码后端命令**
  //   （不准出现可执行的字面 `tmux attach` / `tmux new-session` / `tmux send-keys`）
  //   → **问一层要**」。改成问座要，产物**逐字同构**（座的 `attach({kind:"quoted"})`
  //   出的就是 `tmux attach -t '=<name>:'`），换的是**谁拥有这条语法**。
  //   ⚠ 那条门禁此前只是散文里的一条手工 grep，**且只盯 `remote-launch.ts` 一个文件** ——
  //   本文件是后来从它拆出去的，门禁没跟着拆 ⇒ 这处违反因此躺了下来。
  //   现在它有机检了（`session-backend-gate.vitest.ts`），扫**整个前端生产段**。
  //
  // 🔴🔴 〔`K-R109` 2026-09-13〕**接过去了 —— 这一处不再问座要。**
  //   用户逐字裁「新起一个会话之后，把你的终端接进那个会话那一句 `tmux attach`，
  //   归谁产？」→「**归本机后端就好了啊**」（`DECISIONS.md#R61` 裁定三）。
  //   本机后端那一侧 `K-R106` 就产得出了（`src/bridge/src/history.rs::render_local_attach`
  //   ⇒ `ccm attach <名>`，走本机 `new`/`resume` 同一条渲染路）；`K-R109` 补的是**注册面**
  //   （`generate_handler!` ＋ `parity_ledger::LEDGER` ＋ 上面那个包装层，三处同一拍）。
  //   ⚠ 那条「本模块**不 attach**，一次都不」仍然对，它说的是**远端**后端
  //   （在远端，开不了你面前的窗）；本机后端就在用户面前那台机器上。
  //
  // 🔴 **渲不出来就诚实失败，不许回落到前端自己拼一条** —— 两条理由，都不是偏好：
  //   ① §31 最终形态第①条逐字禁「前端硬编码后端命令」，回落等于把它请回来；
  //   ② **走到这一行时后端刚刚证明过自己在**（上面那个 `sent.verdict === "typed"` 是
  //      本机后端通道真的把载荷键进去了才有的结论）。而「有后端、没有 ccm」是
  //      `DECISIONS.md#R64` 判过的**幽灵态**（用户逐字「不存在什么没装 ccm 装了后端的情况」）
  //      ⇒ 这一行真的 reject 的时候，那是一条**该让人看见的**读数，不是该被糊过去的边角。
  let attachCmd: string;
  try {
    attachCmd = await commands.render_local_attach({ tmuxName: name });
  } catch (err) {
    showActionFailureToast(
      copyText("remoteLaunchRun.inPlaceLocal.attachFailed"),
      copyText("remoteLaunchRun.inPlaceLocal.attachFailedBody", { err: String(err) }),
    );
    // ★ 与下面那条同一个道理：**就地 resume 已经成了**，attach 这一跳的成败不改变它。
    return true;
  }
  await invokeLaunchOrCopyFallback(LOCAL_ORIGIN, attachCmd, {
    success: copyText("remoteLaunchRun.inPlaceLocal.done"),
    successDetail: copyText("remoteLaunchRun.inPlaceLocal.doneBody", { name }),
    failureCopied: copyText("remoteLaunchRun.inPlaceLocal.copied"),
    failureNotCopied: copyText("remoteLaunchRun.inPlaceLocal.manual"),
  }, rbindTokenOf(plan));
  // ★ 就地 resume 本身已经成了（`typed`）——**attach 开不开得了窗口不改变这个结论**。
  //   返回 `false` 会让调用方以为这次 resume 没做成，那是把两件事混成一件。
  return true;
}

/**
 * F96：历史页「在该目录起新会话」——远端分支。tmux 会话名由 cwd 派生、默认拉起命令由
 * `AGENT_PROFILE` 兜底，**让调用方（history.ts）既不必知道底下用不用 tmux、也不必知道
 * 默认拉起是哪个 agent**（用户 2026-07-15 硬约束）——history.ts 只传 F34 配置命令（可空）。
 * 薄封装 F53 的 `runRemoteLauncher`，不写第二份拉起逻辑。
 * （`buildLauncherCmd` 只对 `undefined` 套默认、空串不触发，故默认在此显式兜。）
 */
export async function runNewSessionRemote(
  origin: string,
  cwd: string,
  command: string,
  mods: LaunchModifiers = {}, // R03：正交修饰 bag（configDir/accountName/modelOverride），见 launch-plan.ts
): Promise<void> {
  // ★★ **默认名必须过铸名口**（F13）—— 08-12 补上，此前这里直接拿 `deriveTmuxName(cwd)`
  // 当最终名。`deriveTmuxName` 只产**基名建议**（它自己的头注逐字这么写），最终名一律过
  // `mintTmuxName`，那是全仓唯一带撞名避让的铸造口。
  //
  // 不过它会怎样：同一个 cwd 点两次「起新会话」⇒ 派生出**同一个名字** ⇒ 撞上远端
  // `create-or-attach` 的幂等闸（`tmux new-session … 2>/dev/null && send-keys` 里那个 `&&`
  // 短路）⇒ **静默接进第一个会话，而用户以为开了新的** = issue #76 那一族。
  //
  // ⚠ **同一个坑仓里修过一次**：`settings/machine-card.ts:941` 的「开新 Claude」逐字记着
  // 「此前直接拿 `deriveTmuxName(cwd)` 当最终名，**不查撞名**」并已改为过铸造口。
  // 而全仓 `deriveTmuxName` 的生产调用点**只有两个** —— 那次修了一个、漏了这一个。
  // ⇒ 「修过了」不等于「这一族修完了」：按**症状**修会漏，按**调用点人群**修才不会。
  //
  // 拿不到会话列表 ⇒ 用空集合**诚实降级**（与改之前逐字同行为，不更差），
  // 不因为查询失败挡住起会话。这一格照抄那处先例。
  let taken: ReadonlySet<string> = new Set();
  try {
    const sessions = await commands.list_remote_tmux({ origin });
    taken = new Set((sessions ?? []).map((x) => x.name));
  } catch {
    // 诚实降级：列不出来就不避让
  }
  await runRemoteLauncher(
    origin,
    cwd,
    mintTmuxName(deriveTmuxName(cwd), taken),
    command || AGENT_PROFILE.defaultLauncher,
    mods,
  );
}

/** F53：「在这台机开新 Claude」——在远端 tmux 会话里启动全新 Claude;失败回退复制命令。 */
export async function runRemoteLauncher(
  origin: string,
  cwd: string,
  tmuxName: string,
  command: string,
  mods: LaunchModifiers = {}, // R03：正交修饰 bag（configDir/accountName/modelOverride），见 launch-plan.ts
): Promise<void> {
  let cmd: string;
  let token: string | null;
  try {
    const { ctx, plan: bare } = planLauncher(cwd, tmuxName, command, withMintedRbindToken(mods));
    const plan = await withRelayEndpoint(origin, ctx, bare);
    cmd = await renderLaunchCommand(origin, ctx, plan);
    token = rbindTokenOf(plan);
  } catch (err) {
    showActionFailureToast(copyText("remoteLaunchRun.launcher.buildFailed"), String(err));
    return;
  }
  await invokeLaunchOrCopyFallback(origin, cmd, {
    success: copyText("remoteLaunchRun.launcher.started"),
    successDetail: copyText("remoteLaunchRun.launcher.startedBody", { machine: origin, name: tmuxName }),
    failureCopied: copyText("remoteLaunchRun.launcher.failedCopied"),
    failureNotCopied: copyText("remoteLaunchRun.copyFallback.failedManual"),
  }, token);
}

/** F51：一键 attach 到远端 tmux 会话:拉起 `ssh -t … tmux attach -t <名>`;失败回退复制命令。
 *  ccm 已装且能力齐全时走 CLI 渲染器（`ccm attach <名>`，与兜底输出逐字同构，无 #76 歧义）。 */
export async function runRemoteAttach(origin: string, name: string): Promise<void> {
  let cmd: string;
  try {
    const { ctx, plan } = planAttach(name);
    cmd = await renderLaunchCommand(origin, ctx, plan);
  } catch (err) {
    showActionFailureToast(copyText("remoteLaunchRun.attach.buildFailed"), String(err));
    return;
  }
  await invokeLaunchOrCopyFallback(origin, cmd, {
    success: copyText("remoteLaunchRun.attach.started"),
    successDetail: copyText("remoteLaunchRun.attach.startedBody", { machine: origin, name }),
    failureCopied: copyText("remoteLaunchRun.attach.failedCopied"),
    failureNotCopied: copyText("remoteLaunchRun.copyFallback.failedManual"),
  }, null); // `attach` 不铸币（`planAttach` 不收 `mods`）⇒ 新窗口不做令牌握手
}
