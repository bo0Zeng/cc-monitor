// 起会话请求构造（`planXxx`）的单测。〔DUP1〕开头那组原来测 `validateLocalLaunch`〔散文墓碑〕（本地路径的前置校验：sid 字符集），
// 那个函数按 `设计/90 §3` 判据 2 删了；这里剩 `transport:{kind:"local"}` 走一遍维度注册表不抛异常，以及各请求的形状。纯函数，零 tauri/config 依赖，无需 mock。
//
// **R07 订正**：这段头注原写"证明本地路径真的在用同一套维度注册表（不是套了个类型皮的假装）"
// ——**那句是假的**。4 个生产调用点全部把返回值当语句丢弃，真命令由 Rust 独立构造
// （`history.rs::build_local_ps_command`）。本地路径**借** IR 做校验，但**不消费**它的输出，
// 而且这是 F06 论证过的设计（`Get-Command` 探测是 render-time 决策、只能在目标机做），
// 不是半成品。函数已随之改名并返回 `void`，见 `src/doc/INVARIANTS.md` §36。
import { describe, it, expect } from "vitest";
import {
  planResumeDirect,
  planResumeTmux,
  planResumeIntoExistingTmux,
  planLauncher,
  planAttach,
} from "../src/launch-requests";
import { buildLaunchPlan } from "../src/launch-plan.ts";
import { buildLaunchRenderRequest } from "../src/remote-launch-run.ts";
import type { LaunchAction, LaunchContext } from "../src/launch-types.ts";

/** `设计/80 §8` 步 1：形状合法的启动期令牌（32 个小写 hex）。 */
const TOK = "0f1e2d3c4b5a69788796a5b4c3d2e1f0";

// 〔DUP1 · `设计/90 §3` 判据 2〕这里原来是 `validateLocalLaunch`〔散文墓碑〕的两条（sid 字符集 throw · 合法输入不拦）。
// 它唯一的一格（sid）交 Rust 判（`history.rs` 本机决策 → `shell_quote_core::session_id_ok`），函数与四个调用点一起删了。

// R07：这一组的被测对象**不是** `validateLocalLaunch`，是**维度注册表在 `transport:local` 下的行为**
// ——它是 `src/doc/INVARIANTS.md` §36 那两条主张的证据。
//
// 为什么单独成组：`validateLocalLaunch` 现在**根本不构造 IR**（R07 Phase D 审计发现它内部那遍
// `buildLaunchPlan` 零门禁守护、且与生产无关，已删）。所以这些断言必须直接冲着 `buildLaunchPlan` 去，
// 不能借道那个函数——否则就是"通过一个已经不做这件事的函数去测这件事"。
describe("维度注册表在 transport:local 下的行为（INVARIANTS §36 的证据）", () => {
  const localCtx = (action: LaunchAction, cwd: string | null = "/p"): LaunchContext => ({
    transport: { kind: "local" },
    action,
    container: { kind: "none" },
    cwd,
    account: { kind: "base" },
    launcherOverride: undefined,
    ccmSid: undefined,
  });

  it("plan.env 因 nested-env-reset 维度恒非空（resume/new 都触发）——本地路径故意不消费它" +
     "（等价保护已在 lib.rs::scrub_env_vars 做完，见 features/F06-local-path-ir.md §0/§2）", () => {
    expect(buildLaunchPlan(localCtx({ kind: "resume", sid: "abc" })).env.length).toBeGreaterThan(0);
    expect(buildLaunchPlan(localCtx({ kind: "new" })).env.length).toBeGreaterThan(0);
  });

  it("account 维度对本地 base 态是无 env op 的 no-op（不因 F05 的 applies 恒真而误注入）", () => {
    const plan = buildLaunchPlan(localCtx({ kind: "new" }));
    expect(plan.env.some((op) => op.kind === "export-config-dir")).toBe(false);
  });

  it("本地 ctx 的其余字段照原样进 plan（transport/container/action/cwd）", () => {
    const plan = buildLaunchPlan(localCtx({ kind: "resume", sid: "abc-123" }));
    expect(plan.transport).toEqual({ kind: "local" });
    expect(plan.container).toEqual({ kind: "none" });
    expect(plan.action).toEqual({ kind: "resume", sid: "abc-123" });
    expect(plan.cwd).toBe("/p");
  });

  // Phase D 审计发现的**真覆盖丢失**：拆分前有一条钉 `cwd: null` 原样透传（不被转成 `""`），
  // 拆分后 `localCtx` 把 cwd 钉死成 `"/p"`，那条覆盖没了。审计实测：给 `buildLaunchPlan` 塞
  // `cwd: ctx.cwd ?? ""` 这个变异，改造前红、改造后**全仓 705 全绿**。
  // **这是共享代码**（`buildLaunchPlan` 远端路径也吃），不是本地专属，所以这条必须补回来。
  // （诚实标注：三个 `plan.cwd` 消费者今天都用真值判断，`""` 与 `null` 行为相同 → 当前是等价变异、
  // 低危；但哪天有人写 `plan.cwd !== null` 它就变成真缺口，钉住的成本近零。）
  it("cwd 为 null 时原样透传进 plan（不被悄悄转成空串）", () => {
    expect(buildLaunchPlan(localCtx({ kind: "new" }, null)).cwd).toBeNull();
  });
});

// R03：**类型层防线**——`LaunchModifiers` 存在的首要理由不是"参数少几个"，而是让
// "传错顺序"这一整类 bug 在编译期不可表达。改造前三个尾参类型全是 `string | undefined`
// 且相邻，`configDir` 与 `accountName` 互换 tsc 照过、运行时却是"账号选择静默失效"
// （R11/R08 那一族"看起来生效了，只是用了错的号"的形状）。
//
// 这些断言靠 `@ts-expect-error` 生效：若哪天有人把签名改回位置参数（或给 bag 加了
// 索引签名之类使其重新接受裸字符串），`@ts-expect-error` 会因为"预期的错误没有发生"
// 而**让 tsc 报错**——即这条测试的守护力来自 tsc，不是运行时。
describe("R03：修饰只能以命名字段传入（类型层）", () => {
  it("旧的位置参数形态编译失败；bag 形态编译通过且等价", () => {
    // @ts-expect-error 尾部三元组已收进 LaunchModifiers，裸字符串不再是合法实参
    planResumeDirect("abc-123", "/p", "claude", "/h/z");
    // @ts-expect-error 同上：第 5 个位置参数已不存在
    planResumeTmux("abc-123", "/p", "claude", "cc-p", "/h/z");

    // 正确形态：命名字段，顺序无关（下面两种写法必须产出同一个 ctx.account）
    const a = planResumeDirect("abc-123", "/p", "claude", { configDir: "/h/z", accountName: "z" });
    const b = planResumeDirect("abc-123", "/p", "claude", { accountName: "z", configDir: "/h/z" });
    expect(a.ctx.account).toEqual({ kind: "account", name: "z", configDir: "/h/z" });
    expect(a.ctx.account).toEqual(b.ctx.account);
  });

  // R03 Phase D 对抗审计发现（重要）：`planXxx` 里"解包 bag → 填 ctx"这一层**全仓零覆盖**。
  // 审计实做变异 M5：让 `planResumeTmux` 不消费 `mods.modelOverride`（等价于"tmux resume 路径上
  // 每账号默认模型静默失效"）→ tsc 无输出、`npm test` 699 全绿、`ccm-print-parity` 12 全绿，
  // **三道门全瞎**。根因：`launch-dimensions.test.ts`/`launch-render-cli.test.ts` 都手搓 ctx、
  // 从不经 planXxx；`tabs.vitest.ts` 把 `remote-launch-run` 整个 mock 掉；
  // `remote-launch.ts` 的 builder 只传 `{ configDir }`。
  // 这是 F07 遗留的缺口（改造前同样没有），但 R03 是最该补它的功能——我上面那条只断言了
  // `ctx.account`，把手边最该钉的 `modelOverride` 漏了，计划 §5 的"抽 1-2 条做变异检查"因此没做到位。
  it("三个修饰字段都真的落进 ctx（不只是 account）", () => {
    const mods = { configDir: "/h/z", accountName: "z", modelOverride: "opus" };
    const builds = [
      planResumeDirect("abc-123", "/p", "claude", mods),
      planResumeTmux("abc-123", "/p", "claude", "cc-p", mods),
    ];
    for (const b of builds) {
      expect(b.ctx.modelOverride).toBe("opus");
      expect(b.ctx.account).toEqual({ kind: "account", name: "z", configDir: "/h/z" });
    }
  });

  // `设计/80 §8` 步 1：第四个修饰字段（启动期令牌）也要真落进 ctx。
  // ⚠ 上面那条判据的头注记着 M5 那个变异（`planResumeTmux` 不消费 `mods.modelOverride`
  // ⇒ 三道门全瞎）—— 令牌这一维的同形变异后果更重：`↗` 静默失效、归因指向别处。
  it("启动期令牌也真的落进 ctx（四条 planXxx 逐条，不只是 resume-direct）", () => {
    const mods = { rbindToken: TOK };
    const builds = [
      planResumeDirect("abc-123", "/p", "claude", mods),
      planResumeTmux("abc-123", "/p", "claude", "cc-p", mods),
      planResumeIntoExistingTmux("abc-123", "cc-p", "claude", mods),
      planLauncher("/p", "cc-p", "claude", mods),
    ];
    for (const b of builds) expect(b.ctx.rbindToken).toBe(TOK);
    // 对照组：不传 ⇒ `undefined`（诚实的没有），不是 `""`。
    expect(planResumeDirect("abc-123", "/p", "claude").ctx.rbindToken).toBeUndefined();
  });

  it("bag 缺省 = 基座（向下兼容：不传修饰等于今天不带账号的行为）", () => {
    expect(planResumeDirect("abc-123", "/p", "claude").ctx.account).toEqual({ kind: "base" });
    expect(planResumeDirect("abc-123", "/p", "claude", {}).ctx.account).toEqual({ kind: "base" });
  });
});

// ═══ audit-0805 F08 下半：远端 resume 的**会话容器** ═══════════════════════
//
// 本仓有三处散文（`README.md` · `src/doc/ARCHITECTURE.md` · `launch.rs` 的 POSIX 桩头注）
// 拿「会话容器反正是 tmux」当**理由**，去解释 POSIX 上为什么不开终端窗口。
// 代码说的相反：POSIX 远端 `↺` 走 `runRemoteResume` → `planResumeDirect`，
// 而那里逐字是 `container: { kind: "none" }`。
//
// ⚠ F08 上半订正过**两条**同源假头注，但漏了这三处 —— 它们在**另一条路**（远端）上，
// 看起来像是另一件事。复核时才发现（E1：台账是筛子不是免检章）。
//
// 「不开终端窗口」这个决定本身站得住（POSIX 没有唯一的终端）；站不住的是**那个理由**。
describe("F08 下半：远端 resume 的会话容器", () => {
  it("★ `planResumeDirect` 不进任何容器 —— 散文不能拿「反正在 tmux 里」当理由", () => {
    const { ctx } = planResumeDirect("abc-123", "/p", "claude");
    expect(
      ctx.container,
      "远端直连 resume 的容器不是 `none` 了。若这是**有意**改的，" +
        "那三处散文（README / ARCHITECTURE / launch.rs 的 POSIX 桩）要跟着改回来，" +
        "并把 `no_prose_claims_the_session_container_is_always_tmux` 一起调整。",
    ).toEqual({ kind: "none" });
  });

  it("对照组：`planResumeTmux` 才是 tmux（否则上面那条只是「所有 plan 都 none」）", () => {
    const { ctx } = planResumeTmux("abc-123", "/p", "claude", "cc-p");
    expect(
      ctx.container.kind,
      "连 tmux 那条路都不是 tmux 了 —— 那上面那条判据什么也没证明",
    ).toBe("tmux");
  });
});

// ═══ `设计/80 §8.4`：**`EnvOp` 容器无关 ⇒ 两条起法同一套机制** ════════════════
//
// 🔴 这一组是 `§8.4` 那张表在本仓的**现打读数**，也是整个方案 E 最要紧的那句主张：
// 「『直接起和 tmux 用同一套机制』不是要额外做的事，**是这个设计的自动结果**」。
// 它之所以成立，是因为令牌走 `plan.env`（载荷），而 env 注入发生在**容器之外**。
//
// ⇒ 判据形状：**同一个 `rbindToken` 喂给四条 planXxx，四条交给生产渲染命令的请求里
// 都带着那一条 `export-rbind-token`，而且它在内层载荷（`env`）里、不在外层（`outer`）。**
// `container:{kind:"none"}`（`planResumeDirect` —— `§6.1`/`§5 方案 A` 明确不覆盖、
// 今天 `↗` 做不到的那一档）与 `container:{kind:"tmux"}` 的三格**一视同仁**。
//
// 〔LR2〕这一组原来比的是 TS 兜底渲染器渲出的**字节**；那份渲染器零生产调用、按 `设计/00 §2.5 ④`
// 删了。现在比的是**生产**那一跳的请求（`buildLaunchRenderRequest`，`renderLaunchCommand` 用的同一个）；
// 请求 → 字节那一段归 Rust：`payload-golden.json`「只有启动期令牌」与
// `tmux-outer-golden.json`「create：启动期令牌」两条逐字节钉着（`launch_*_parity.rs`）。
//
// ⚠ **反空真**：每一条都配一个「不传令牌 ⇒ 零命中」的对照组。少了对照组，
// 「请求构造把令牌硬编码进去」与「令牌真的从 ctx 流过来」在这把尺子上同形。
describe("设计/80 §8.4：EnvOp 容器无关 —— 两条起法都自动带上启动期令牌", () => {
  const TOKEN_OP = { kind: "export-rbind-token", value: TOK } as const;
  const hasToken = (env: readonly { kind: string }[]): boolean =>
    env.some((op) => op.kind === "export-rbind-token");

  it("★ `container:\"none\"`（planResumeDirect）—— 今天 ↗ 做不到的那一档，自动带上了", () => {
    const { plan } = planResumeDirect("abc-123", "/w", "claude", { rbindToken: TOK });
    expect(plan.container).toEqual({ kind: "none" });
    const req = buildLaunchRenderRequest(plan);
    // 相等不是包含：排在全部 unset 之后（`RBIND_TOKEN_DIMENSION` 的 order 契约）。
    expect(req.env).toEqual([{ kind: "unset-nested-env" }, TOKEN_OP]);
    expect(req.outer).toBeUndefined();
  });

  it("★ tmux 那三格也带（create / send-into / new）—— 一行容器相关的代码都没写", () => {
    const tmuxBuilds = [
      planResumeTmux("abc-123", "/w", "claude", "cc-p", { rbindToken: TOK }),
      planResumeIntoExistingTmux("abc-123", "cc-p", "claude", { rbindToken: TOK }),
      planLauncher("/w", "cc-p", "claude", { rbindToken: TOK }),
    ];
    for (const { plan } of tmuxBuilds) {
      expect(plan.container.kind).toBe("tmux");
      const req = buildLaunchRenderRequest(plan);
      // 令牌进的是**内层载荷**（`env`），外层 `outer` 那一格结构上就没有 env 字段（`§8.4`）。
      expect(req.env).toContainEqual(TOKEN_OP);
      expect(req.outer).toBeDefined();
      expect(JSON.stringify(req.outer)).not.toContain(TOK);
    }
  });

  it("★ 对照组：不传令牌 ⇒ 四条起法的请求里零 `export-rbind-token`", () => {
    const plans = [
      planResumeDirect("abc-123", "/w", "claude").plan,
      planResumeTmux("abc-123", "/w", "claude", "cc-p").plan,
      planResumeIntoExistingTmux("abc-123", "cc-p", "claude").plan,
      planLauncher("/w", "cc-p", "claude").plan,
    ];
    for (const plan of plans) {
      expect(hasToken(buildLaunchRenderRequest(plan).env)).toBe(false);
    }
  });

  it("★ attach 那一格不带（不收 mods，且维度的 action 闸是第二道同向的闸）", () => {
    const { ctx, plan } = planAttach("cc-p");
    expect(ctx.rbindToken).toBeUndefined();
    expect(plan.env).toEqual([]);
    expect(buildLaunchRenderRequest(plan).env).toEqual([]);
  });
});
