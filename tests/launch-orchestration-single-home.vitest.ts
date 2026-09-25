/**
 * 〔FE1 · 第四波 4D〕**起会话的两件事各只有一个家**：铸 tmux 名（`src/tmux-name-mint.ts`）·
 * 本机 resume 的编排（`src/local-resume.ts`）。
 *
 * 守的要求（住址逐字）：
 * - `设计/01 §5` D1「**一个判定只有一个家**」——「同一条规则有两份实现，它们就会漂；而漂开的后果是静默的错，不是报错」；
 * - `设计/01 §5` D4「**一条都不许静默忽略**」——「要了、没做、也不说 —— 那是最坏的失效形态」。
 *
 * 出处：审计 B §2.6（本机 resume 编排抄 4 份；「列远端 tmux 再铸名」两份逐字副本、列不出就空集铸名，
 * 与本机「绝不退化成空集」（#76）相反）。FE1 现打另找出同形的两处（`resumeTabTmuxInner` 全新支 · 分叉远端）。
 *
 * # 判据
 *
 * | # | 性质 | 形状 |
 * |---|---|---|
 * | K1 | 铸名只有一个家 | 生产段调 `mintSessionTmuxName(` 的文件集合 == `{tmux-name-mint.ts}`；问 tmux 名单的文件集合 == 手写集合（两向） |
 * | K2 | 本机 resume 编排只有一个家 | 生产段调 `resume_history_session(` 的文件集合 == `{local-resume.ts}`（两向） |
 * | K3 | 列不出 ⇒ 不铸名 | `readTmuxListing` 三态逐格 == 手写表；`mintFreshTmuxName` 在 unknown 上回 `ok:false`；本机 resume 在 unknown 上交 `tmuxName: null` |
 *
 * K3 的四个远端入口各有一条行为判据，住各自的测试文件（那里有现成的桩）：
 * `remote-launch-run.vitest.ts`「列不出会话 ⇒ 不起、出声」· `settings/remote-section.vitest.ts`「开新 Claude …」·
 * `tabs.vitest.ts`「tmux 全新 resume …」· `fork-start.vitest.ts`「名单没问到（null）…」。每条都配正控。
 *
 * # 同波别的路长出新成员时会怎么红
 *
 * K1 / K2 的人群是**生产段全集**（`test-support/production-sources.ts`），不是登记表：
 * 别的路新写一处 `mintSessionTmuxName(` / `resume_history_session(` / `list_*_tmux(` ⇒ 集合多一个 ⇒ 红，
 * 报文点名那个文件。它该不该存在，回来看它是不是本该走这两个家。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../src/error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../src/behavior", () => ({
  getBehavior: vi.fn().mockResolvedValue({ resumeCommandLocal: "" }),
}));

import { invoke } from "@tauri-apps/api/core";
import { productionTsFiles } from "./test-support/production-sources.ts";
import { stripComments } from "./test-support/strip-comments.ts";
import { LOCAL_ORIGIN } from "../src/ipc/origin";
import { readTmuxListing, mintFreshTmuxName, listingFromFetch } from "../src/tmux-name-mint";
import { resumeLocalSession } from "../src/local-resume";
import { showActionFailureToast } from "../src/error-toast";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

/** 生产段（剥注释）里匹配 `re` 的文件集合。 */
function filesMatching(re: RegExp, skip: readonly string[] = []): string[] {
  return productionTsFiles()
    .filter((f) => !skip.includes(f.file))
    .filter((f) => re.test(stripComments(f.text, "ts")))
    .map((f) => f.file)
    .sort();
}

describe("K1 · 铸名只有一个家", () => {
  it("★ 生产段调 `mintSessionTmuxName(` 的文件 == {tmux-name-mint.ts}（定义处 remote-launch.ts 除外）", () => {
    expect(
      filesMatching(/\bmintSessionTmuxName\s*\(/, ["src/remote-launch.ts"]),
      "有别的地方自己「列名单 → 铸名」了。起会话的 tmux 名只许经 `src/tmux-name-mint.ts`" +
        "（它守着「列不出 ⇒ 不铸名」；别处抄一份，降级口径就又分叉了 —— B §2.6 的病）。",
    ).toEqual(["src/tmux-name-mint.ts"]);
  });

  it("★ 问 tmux 名单的生产文件 == 手写集合（两向）", () => {
    // 手写期望，不从实现生成。每一格为什么在（包装层 `ipc/commands.ts` 是 `list_local_tmux: () =>` 这种签名，不带 `(` 直调，不在人群里）：
    // - `tmux-name-mint.ts`：铸名的家（本机 ＋ 远端）。
    // - `tab-session-actions.ts`：`fetchTmuxFresh`，TabManager 唯一取数点（attach / kill / 就地 resume 要**活的**那一份；
    //   它取回来的答案经 `listingFromFetch` 交给铸名的家，不自己铸）。
    const want = ["src/tab-session-actions.ts", "src/tmux-name-mint.ts"];
    expect(
      filesMatching(/\blist_(?:local|remote)_tmux\s*\(/),
      "问 tmux 名单的地方变了。新长的那一处是不是本该走 `tmux-name-mint.ts::readTmuxListing`？",
    ).toEqual(want);
  });
});

describe("K2 · 本机 resume 编排只有一个家", () => {
  it("★ 生产段调 `resume_history_session(` 的文件 == {local-resume.ts}（包装层除外）", () => {
    expect(
      filesMatching(/\bresume_history_session\s*\(/, ["src/ipc/commands.ts"]),
      "有别的地方自己拼了一遍本机 resume。编排（校验 sid → 铸名 → 账号 → 起 → 记 pin）只许住 `src/local-resume.ts`。",
    ).toEqual(["src/local-resume.ts"]);
  });
});

describe("K3 · 列不出 ⇒ 不铸名（三态不许压成两态）", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    vi.mocked(showActionFailureToast).mockReset();
  });

  const reply = (local: unknown, remote: unknown): void => {
    invokeMock.mockImplementation((cmd: string) => {
      const r = cmd === "list_local_tmux" ? local : cmd === "list_remote_tmux" ? remote : undefined;
      return r instanceof Error ? Promise.reject(r) : Promise.resolve(r);
    });
  };
  const S = (name: string) => ({ name, path: "/p", command: "claude", attached: false, windows: 1, sid: null });

  it("★ readTmuxListing 逐格 == 手写表", async () => {
    // 期望是手写的（`tmux.rs::list_remote_tmux` 头注那三档 ＋ `list_local_tmux` 的 None = 不知道）。
    const table: Array<[string, unknown, unknown, "known" | "unknown"]> = [
      ["本机 · 列表", [S("a")], undefined, "known"],
      ["本机 · null（后端还没报过）", null, undefined, "unknown"],
      ["本机 · 抛", new Error("x"), undefined, "unknown"],
      ["远端 · 列表", undefined, [S("a")], "known"],
      ["远端 · 空表（零会话）", undefined, [], "known"],
      ["远端 · null（没装 tmux）", undefined, null, "known"],
      ["远端 · 抛（没问到）", undefined, new Error("ssh 抖动"), "unknown"],
    ];
    for (const [what, local, remote, want] of table) {
      reply(local, remote);
      const origin = what.startsWith("本机") ? LOCAL_ORIGIN : "devbox";
      expect((await readTmuxListing(origin)).kind, what).toBe(want);
    }
  });

  it("★ 名单没问到 ⇒ mintFreshTmuxName 回 ok:false 且带原因；正控：问到了 ⇒ 避让到 -2", async () => {
    reply(undefined, new Error("ssh 抖动"));
    const no = await mintFreshTmuxName("devbox", "/home/u/proj");
    expect(no.ok).toBe(false);
    expect(no.ok ? "" : no.why).toContain("ssh 抖动");
    reply(undefined, [S("proj-cc")]);
    expect(await mintFreshTmuxName("devbox", "/home/u/proj")).toEqual({ ok: true, name: "proj-cc-2" });
  });

  it("listingFromFetch：undefined（调用方那一问抛了）⇒ unknown；远端 null ⇒ known 空表；本机 null ⇒ unknown", () => {
    expect(listingFromFetch("devbox", undefined).kind).toBe("unknown");
    expect(listingFromFetch("devbox", null)).toEqual({ kind: "known", sessions: [] });
    expect(listingFromFetch(LOCAL_ORIGIN, null).kind).toBe("unknown");
  });

  it("★ 本机 resume：名单不知道 ⇒ 交 `tmuxName: null`（后端如实不进容器）；正控：知道 ⇒ 交铸出来的名字", async () => {
    const sent = (): Record<string, unknown> =>
      invokeMock.mock.calls.find((c) => c[0] === "resume_history_session")![1] as Record<string, unknown>;
    reply(null, undefined);
    expect(
      await resumeLocalSession({ sid: "s1", cwd: "/home/u/proj", account: { kind: "explicit", configDir: null, name: null } }),
    ).toBe(true);
    expect(sent().tmuxName).toBeNull();
    invokeMock.mockReset();
    reply([S("proj-cc")], undefined);
    await resumeLocalSession({ sid: "s1", cwd: "/home/u/proj", account: { kind: "explicit", configDir: null, name: null } });
    expect(sent().tmuxName).toBe("proj-cc-2");
    // 账号 0 是**显式 `base`**，不是省略（省略 = 没表态 = 被 shell rc 里的默认号顶掉）。
    expect(sent().account).toEqual({ kind: "base" });
  });

  it("本机 resume：sid 不合法 ⇒ 一次 IPC 都不发、出声、回 false", async () => {
    reply([], undefined);
    expect(
      await resumeLocalSession({ sid: "a b", cwd: "/p", account: { kind: "explicit", configDir: null, name: null } }),
    ).toBe(false);
    expect(invokeMock).not.toHaveBeenCalled();
    expect(vi.mocked(showActionFailureToast)).toHaveBeenCalledTimes(1);
  });
});
