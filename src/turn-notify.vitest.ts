// Batch14-F42 TurnEndNotifier 判定链测试(依赖全注入,零 DOM/插件依赖)。
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("./behavior", () => ({
  getBehavior: vi.fn().mockResolvedValue({ notifyTurnEnd: true }),
}));

import { TurnEndNotifier, type TurnNotifyPayload } from "./turn-notify";

const T0 = 1_700_000_000_000;

function endTurnPayload(tsOffsetMs = 0): TurnNotifyPayload {
  return {
    message: {
      type: "assistant",
      timestamp: new Date(T0 + tsOffsetMs).toISOString(),
      message: { stop_reason: "end_turn" },
    },
  };
}

function makeNotifier(over?: {
  focused?: boolean;
  enabled?: boolean;
  now?: number;
}) {
  const send = vi.fn().mockResolvedValue(undefined);
  const state = { now: over?.now ?? T0 };
  const n = new TurnEndNotifier({
    isFocused: () => over?.focused ?? false,
    now: () => state.now,
    enabled: async () => over?.enabled ?? true,
    send,
  });
  return { n, send, state };
}

async function flush(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

describe("F42 TurnEndNotifier", () => {
  beforeEach(() => vi.clearAllMocks());

  it("happy path:实时 end_turn + 失焦 → 通知(标题带 tab 名)", async () => {
    const { n, send } = makeNotifier();
    n.observe("s1", "[aya] 项目甲", endTurnPayload(), false);
    await flush();
    expect(send).toHaveBeenCalledTimes(1);
    expect(send.mock.calls[0][0]).toContain("项目甲");
  });

  it("批量重放(inBatch)→ 不通知", async () => {
    const { n, send } = makeNotifier();
    n.observe("s1", "t", endTurnPayload(), true);
    await flush();
    expect(send).not.toHaveBeenCalled();
  });

  it("窗口聚焦 → 不通知", async () => {
    const { n, send } = makeNotifier({ focused: true });
    n.observe("s1", "t", endTurnPayload(), false);
    await flush();
    expect(send).not.toHaveBeenCalled();
  });

  it("陈旧时间戳(>90s)/缺时间戳 → 不通知", async () => {
    const { n, send } = makeNotifier();
    n.observe("s1", "t", endTurnPayload(-120_000), false);
    n.observe("s1", "t", { message: { type: "assistant", message: { stop_reason: "end_turn" } } }, false);
    await flush();
    expect(send).not.toHaveBeenCalled();
  });

  it("非 end_turn / 非 assistant → 不通知", async () => {
    const { n, send } = makeNotifier();
    n.observe("s1", "t", { message: { type: "assistant", timestamp: new Date(T0).toISOString(), message: { stop_reason: null } } }, false);
    n.observe("s1", "t", { message: { type: "user", timestamp: new Date(T0).toISOString(), message: {} } }, false);
    await flush();
    expect(send).not.toHaveBeenCalled();
  });

  it("同会话 10s 防抖;不同会话互不影响", async () => {
    const { n, send, state } = makeNotifier();
    n.observe("s1", "t", endTurnPayload(), false);
    state.now = T0 + 5_000;
    n.observe("s1", "t", endTurnPayload(5_000), false); // 5s 内再来 → 抑制
    n.observe("s2", "t2", endTurnPayload(5_000), false); // 另一会话不受影响
    state.now = T0 + 11_000;
    n.observe("s1", "t", endTurnPayload(11_000), false); // 过窗 → 放行
    await flush();
    expect(send).toHaveBeenCalledTimes(3);
  });

  it("开关关 → 不通知(且不因判定链前段短路而误发)", async () => {
    const { n, send } = makeNotifier({ enabled: false });
    n.observe("s1", "t", endTurnPayload(), false);
    await flush();
    expect(send).not.toHaveBeenCalled();
  });

  it("disable()(viewer 窗口)→ 永不通知", async () => {
    const { n, send } = makeNotifier();
    n.disable();
    n.observe("s1", "t", endTurnPayload(), false);
    await flush();
    expect(send).not.toHaveBeenCalled();
  });

  it("isSidechain 行(旧版 CC subagent 写主文件)→ 不通知", async () => {
    const { n, send } = makeNotifier();
    n.observe(
      "s1",
      "t",
      {
        message: {
          type: "assistant",
          timestamp: new Date(T0).toISOString(),
          isSidechain: true,
          message: { stop_reason: "end_turn" },
        },
      },
      false,
    );
    await flush();
    expect(send).not.toHaveBeenCalled();
  });

  // ★★ audit-0805 F12 / 报告 §4.1「turn-end 判定两份」。
  //
  // daemon 侧 `observe/turn_detect.rs` 的四条件里有 `!isApiErrorMessage`，TS 这份**少了一条** ——
  // 而那个字段在生成物 `generated/JsonlRecord.ts` 的 assistant 变体里一直存在：**数据在线上、没人看**。
  //
  // ⚠ V3 复核订正过报告的因果：monitor **不消费** daemon 的 TurnEnd 帧（全仓零帧消费点），
  //   通知是前端自己逐行算的 ⇒ 两者是**互不相通的两个探测器**，不是「一个不发另一个弹」。
  //   而且本机语料 107 条 isApiErrorMessage 记录里 end_turn **0 条** ⇒ 这是**潜伏缺口不是冒烟 bug**。
  //   修它是因为「哪天某个 CC 版本在错误记录上写 end_turn，就直接弹」，不是因为它现在在响。
  it("isApiErrorMessage 行(API 错误带 end_turn)→ 不通知", async () => {
    const { n, send } = makeNotifier();
    n.observe(
      "s1",
      "t",
      {
        message: {
          type: "assistant",
          timestamp: new Date(T0).toISOString(),
          isApiErrorMessage: true,
          message: { stop_reason: "end_turn" },
        },
      },
      false,
    );
    await flush();
    expect(send).not.toHaveBeenCalled();
  });

  // 反向：**不是**错误消息时照常通知（防把上面写成「凡带这个字段就不通知」）。
  it("isApiErrorMessage:false 仍然照常通知", async () => {
    const { n, send } = makeNotifier();
    n.observe(
      "s1",
      "t",
      {
        message: {
          type: "assistant",
          timestamp: new Date(T0).toISOString(),
          isApiErrorMessage: false,
          message: { stop_reason: "end_turn" },
        },
      },
      false,
    );
    await flush();
    expect(send).toHaveBeenCalled();
  });

  it("send 抛异常不外泄(console.warn 兜底)", async () => {
    const send = vi.fn().mockRejectedValue(new Error("no permission"));
    const n = new TurnEndNotifier({
      isFocused: () => false,
      now: () => T0,
      enabled: async () => true,
      send,
    });
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    n.observe("s1", "t", endTurnPayload(), false);
    await flush();
    expect(send).toHaveBeenCalled();
    expect(warn).toHaveBeenCalled();
    warn.mockRestore();
  });
});

// ★★ 跨语言对拍：**两份 turn-end 判定不许再各自漂**〔audit-0805 F12，定框 E3〕。
//
// 报告 §4.1 那张「同一职责多处落地」表里，turn-end 是「**是**（TS 少 `!isApiError`）」那一行。
// 修完只是把今天对齐了 —— **没有任何东西阻止它明天再漂开**。
// E3 要的不是「两份都改对」，是「**权威源恰好一个**」；两份实现天生做不到那个，
// 退而求其次就是**对拍**：daemon 那份的判别条件，TS 这份必须一个不少。
//
// ⚠⚠ **本条 08-07 被自己的变异打红过一次，人群改成派生**〔audit-0805 Phase G 第 35 件〕：
// 原版把 daemon 的四个条件**手写成一张表**。实测给 daemon 的 `is_turn_end` 加第五个合取项
// （`&& !is_compact_summary(v)`），**13 条 TS 测试 + 4 条 daemon 测试全绿** ——
// 也就是说「daemon 又排除了一类记录、TS 照发通知」这个方向，本条根本不看。
// 而 F12 那次漂移**正是这个方向**（daemon 有 `!isApiError`、TS 没有）⇒
// **为防某次漂移而建的判据，只挡住了那一次漂移的那一条，没挡住它所属的那一类。**
// 现在人群从 `is_turn_end` 的**合取项本身**派生，新条件不登记就红（默认拒绝）。
//
// ⚠ 另一半：断言看的是 TS 的**实现区**（`observe(` 之后），不是整份文件 ——
// `isApiErrorMessage` / `isSidechain` 在最小契约的**类型声明**里也各出现一次，
// 拿整份文件做主语的话，「删掉那行 if、留着类型字段」照样绿。
// （那半今天另有行为测试接住，两层失效模式不同 ⇒ 是真纵深；但断言本身要说对话。）
//
// 诚实边界：**反方向不钉** —— TS 比 daemon 多一条排除只会少发通知，不会误报「完成」；
// 且 TS 侧另有四道 daemon 没有的防线（inBatch / 新鲜度 / 防抖 / 聚焦），本就不是同一张表。
describe("turn-end 判定的跨语言对拍（audit-0805 F12）", () => {
  it("daemon 判词的每个合取项在 TS 侧都有对应判别", async () => {
    const { readFileSync } = await import("node:fs");
    const { resolve, dirname } = await import("node:path");
    const { fileURLToPath } = await import("node:url");
    const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
    const rs = readFileSync(
      resolve(ROOT, "remote-daemon-proto/src/observe/turn_detect.rs"),
      "utf8",
    );
    // ⚠ **必须剥注释**。第一版没剥，于是变异「连字段带判定一起删」照样绿 ——
    //   因为 `isApiErrorMessage` 在我自己写的那段注释里还出现着。
    //   这正是本仓记过的失效形态：**判据匹配到了文档注释里的关键字**。
    const stripComments = (src: string): string =>
      src.replace(/\/\*[\s\S]*?\*\//g, "").replace(/^\s*\/\/.*$/gm, "");
    const ts = stripComments(readFileSync(resolve(ROOT, "src/turn-notify.ts"), "utf8"));

    // 抽取器自检：拿错文件时下面会零命中地绿。
    expect(rs, "turn_detect.rs 里没有 is_turn_end —— 抽取器坏了").toContain("fn is_turn_end");
    // 抽取器自检之二：剥完注释还得剩下真代码，否则下面全是零命中地绿。
    expect(ts.length, "剥完注释 turn-notify.ts 只剩空壳 —— 剥法坏了").toBeGreaterThan(500);
    expect(ts, "剥完注释连 observe( 都没了 —— 剥法把代码也吃了").toContain("observe(");

    // TS 的**实现区**：`observe(` 之后。最小契约的类型声明全在它之前 ⇒ 被排除。
    const impl = ts.slice(ts.indexOf("observe("));
    expect(
      impl,
      "切片没把最小契约的类型声明切掉 —— 下面的断言会被「只声明不判」喂饱",
    ).not.toContain("isApiErrorMessage?:");
    expect(impl, "实现区里连 stop_reason 都没有 —— 切早了").toContain("stop_reason");

    // daemon 判词的合取项**从源码派生**（不再手写清单）：取 `is_turn_end` 的函数体、
    // 剥注释、按 `&&` 拆。rustfmt 会把长表达式折行 ⇒ 先把空白压平再拆。
    const fnAt = rs.indexOf("pub fn is_turn_end");
    const bodyFrom = rs.indexOf("{", fnAt) + 1;
    const bodyTo = rs.indexOf("\n}", bodyFrom);
    const conjuncts = stripComments(rs.slice(bodyFrom, bodyTo))
      .split("&&")
      .map((s) => s.replace(/\s+/g, " ").trim())
      .filter(Boolean);
    // 抽取器自检之三：拆出来的条数不合理 ⇒ 函数体没抓对（地板 = 今天的 4 条）。
    expect(
      conjuncts.length,
      `is_turn_end 只拆出 ${conjuncts.length} 个合取项（今天实测 4）—— 函数体抽取坏了，本条此刻无效`,
    ).toBeGreaterThanOrEqual(4);

    // 登记表：每个合取项对应 TS 侧哪个判别。**这一列是人的答案**（跨语言的语义映射
    // 机器判不了），但**谁进人群不是人说了算** —— 人群由上面派生，没登记的当场红。
    const REGISTERED: Array<{ rust: string; ts: string; what: string }> = [
      { rust: "is_assistant", ts: '"assistant"', what: "assistant 类型判别" },
      { rust: "stop_reason", ts: "end_turn", what: "stop_reason == end_turn" },
      { rust: "is_api_error", ts: "isApiErrorMessage", what: "API 错误消息要排除" },
      { rust: "is_sidechain", ts: "isSidechain", what: "subagent 行要排除" },
    ];
    const used = new Set<string>();
    for (const c of conjuncts) {
      const row = REGISTERED.find((r) => c.includes(r.rust));
      expect(
        row,
        `★ daemon 的 is_turn_end 长出了一个**没登记**的判别条件：\`${c}\`\n` +
          `两份实现里只有 daemon 那份排除了这类记录 ⇒ TS 侧会为它照发「完成」通知。\n` +
          `处置二选一：① 在 turn-notify.ts 的 observe() 里补上对应判别，并在本表登记；\n` +
          `② 若 TS 侧确实不需要（例如那是 daemon 独有的传输层顾虑），也在本表登记并写明为什么。\n` +
          `⚠ 别改本条去迁就它 —— F12 那次漂移就是这么长出来的。`,
      ).toBeTruthy();
      used.add(row!.rust);
      expect(
        impl,
        `★ TS 侧缺「${row!.what}」（实现区里找不到 \`${row!.ts}\`）。\n` +
          `daemon 的 turn_detect.rs 有这一条，TS 这份没有 ⇒ 同一件事两个口径。\n` +
          `报告 §4.1 记的正是这个：turn-end 判定两份、TS 少 !isApiError。\n` +
          `⚠ 后果是潜伏的而不是在响的（本机 107 条 API 错误记录里 end_turn 0 条），\n` +
          `但哪天某个 CC 版本在错误记录上写 end_turn，用户就会为一次失败收到「完成」通知。`,
      ).toContain(row!.ts);
    }
    // 反向锚点：登记表里不许留死行 —— daemon 删掉一条判别而本表照旧，
    // 会让「登记过」看起来仍然成立，实则那一条已经没人在守。
    for (const r of REGISTERED) {
      expect(
        used.has(r.rust),
        `登记表里的 \`${r.rust}\` 在今天的 is_turn_end 里已不存在 —— ` +
          `是 daemon 删了这条判别（那 TS 侧那道也该一起复核），还是改了名字？`,
      ).toBe(true);
    }
  });
});
