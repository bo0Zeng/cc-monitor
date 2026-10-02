/**
 * **起会话的两件事各只有一个家**：铸 tmux 名（`src/frontend/ui/tmux-name-mint.ts`）·
 * 本机 resume 的编排（`src/frontend/ui/local-resume.ts`）。
 *
 * 守的要求（住址逐字）：
 * - 「**一个判定只有一个家**」——「同一条规则有两份实现，它们就会漂；而漂开的后果是静默的错，不是报错」；
 * - 「**一条都不许静默忽略**」——「要了、没做、也不说 —— 那是最坏的失效形态」。
 *
 * 出处：审计 B §2.6（本机 resume 编排抄 4 份；「列远端 tmux 再铸名」两份逐字副本、列不出就空集铸名，
 * 与本机「绝不退化成空集」（#76）相反）。FE1 现打另找出同形的两处（`resumeTabTmuxInner` 全新支 · 分叉远端）。
 *
 * # 判据
 *
 * | # | 性质 | 形状 |
 * |---|---|---|
 * | K1 | 铸名只有一个家 | 生产段发 `tmux-name-mint`（问那台后端铸名）的文件集合 == `{tmux-name-mint.ts}`；问 tmux 名单的文件集合 == 手写集合（两向） |
 * | K2 | 本机 resume 编排只有一个家 | 生产段以 resume 动作问本机后端起会话（`launchLocal(` / `planLocalLaunch(` ＋ `kind: "resume"`）的文件集合 == `{local-resume.ts}`（两向） |
 * | K3 | 问不到 ⇒ 不铸名 | `mintFreshTmuxName` 问不到 / 形状不认 ⇒ `ok:false`；本机 resume 问不到 ⇒ 交 `tmuxName: null` |
 *
 * | K4 | D-h：本机跟随时 pin 那个号选不了 ⇒ 不起、说清、给「用当前账号」的显式选择 | 零次本机 resume（`launch-local`）＋ 一条可点提示；点了 ⇒ 以当前号起；正控：pin 可选 ⇒ 带 pin 起 |
 *
 * K4 另守一处：D-h（「选不了原账号时 resume ⇒ 照 / D4：不静默换号，拒并说清、给「用当前账号」的显式选择」）
 * ＋「哪个账号」非有不可 —— 缺了 resume 会静默落到默认号，撞 `D4`」。远端那一半（`withAccount`）住 `accounts.vitest.ts`「〔FE1 · D-h〕」那几条。
 *
 * K3 的四个远端入口各有一条行为判据，住各自的测试文件（那里有现成的桩）：
 * `remote-launch-run.vitest.ts`「列不出会话 ⇒ 不起、出声」· `settings/remote-section.vitest.ts`「开新 Claude …」·
 * `tabs.vitest.ts`「tmux 全新 resume …」· `fork-start.vitest.ts`「铸不出名字且要进 tmux …」。每条都配正控。
 *
 * # 同波别的路长出新成员时会怎么红
 *
 * K1 / K2 的人群是**生产段全集**（`test-support/production-sources.ts`），不是登记表：
 * 别的路新写一处 `"tmux-name-mint"` / 以 resume 动作的 `launchLocal(` / `listTmux(` ⇒ 集合多一个 ⇒ 红，
 * 报文点名那个文件。它该不该存在，回来看它是不是本该走这两个家。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("../../../src/frontend/ui/error-toast", () => ({ showActionFailureToast: vi.fn() }));
vi.mock("../../../src/frontend/ui/behavior", () => ({
  getBehavior: vi.fn().mockResolvedValue({ resumeCommandLocal: "" }),
}));

import { invoke } from "@tauri-apps/api/core";
import { launchRenderShim, localLaunchCalls } from "../../test-support/chan-fake";
import { productionTsFiles, SCAN_TIMEOUT_MS } from "../../test-support/production-sources.ts";
import { stripComments } from "../../test-support/strip-comments.ts";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/ipc/origin";
import { mintFreshTmuxName } from "../../../src/frontend/ui/tmux-name-mint";
import { resumeLocalSession } from "../../../src/frontend/ui/local-resume";
import { showActionFailureToast } from "../../../src/frontend/ui/error-toast";
import type { Account, AccountsState } from "../../../src/frontend/ui/accounts";
import { __resetLocalLaunchSnapshotForTests, __setLocalLaunchSnapshotForTests } from "../../../src/frontend/ui/launch-account";

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
  // 派生 ＋ 避让搬进后端：前端铸名 = 问那台后端 `tmux-name-mint`，发这一问的只许一个家。
  it("★ 生产段发 `tmux-name-mint` 的文件 == {tmux-name-mint.ts}", () => {
    expect(
      filesMatching(/["']tmux-name-mint["']/),
      "有别的地方自己去问后端铸名了。起会话的 tmux 名只许经 `src/frontend/ui/tmux-name-mint.ts`" +
        "（它守着「问不到 ⇒ 不铸名」；别处抄一份，降级口径就又分叉了 —— B §2.6 的病）。",
    ).toEqual(["src/frontend/ui/tmux-name-mint.ts"]);
  }, SCAN_TIMEOUT_MS);

  it("★ 「这个会话在哪个 tmux 会话里」只问那台、只有一个问口（两向）；界面零处拿 tmux 名单自己判", () => {
    // 手写期望，不从实现生成：发 `sessions-tmux` 的只有 `tmux-sessions.ts`（菜单就绪 · 换号重启 · 分叉都经它）。
    expect(filesMatching(/["']sessions-tmux["']/), "有别的地方自己去问那台了").toEqual(["src/frontend/ui/tmux-sessions.ts"]);
    // 界面拿 tmux 名单自己判（按 sid / 按目录筛）那一族删了：读口定义处之外零处调它。
    expect(
      filesMatching(/\b(?:listTmux|list_(?:local|remote)_tmux)\s*\(/, ["src/frontend/ui/tmux-reads.ts"]),
      "又长出一处拿 tmux 名单自己判的 —— 判定只在那台后端（`sessions-tmux`）",
    ).toEqual([]);
  }, SCAN_TIMEOUT_MS);
});

describe("K2 · 本机 resume 编排只有一个家", () => {
  // 本机起会话的计划与渲染搬进本机后端（`launch-local`，`src/frontend/ui/launch-render.ts` 是那一问的唯一出口）⇒
  //   人群从「调 Tauri 命令 `resume_history_session(`」换成「以 resume 动作调那个出口」。
  it("★ 生产段以 resume 动作调 `launchLocal(` / `planLocalLaunch(` 的文件 == {local-resume.ts}", () => {
    expect(
      filesMatching(/\b(?:launchLocal|planLocalLaunch)\s*\(\s*\{\s*action:\s*\{\s*kind:\s*"resume"/),
      "有别的地方自己拼了一遍本机 resume。编排（校验 sid → 铸名 → 账号 → 起 → 记 pin）只许住 `src/frontend/ui/local-resume.ts`。",
    ).toEqual(["src/frontend/ui/local-resume.ts"]);
  }, SCAN_TIMEOUT_MS);
});

describe("K3 · 列不出 ⇒ 不铸名（三态不许压成两态）", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    vi.mocked(showActionFailureToast).mockReset();
  });

  /** 那台后端铸名那一问：名字 ⇒ 成品；`null` ⇒ 问不到；`"bad"` ⇒ 回了认不得的形状。记下被问的入参。 */
  const mintAsks: { origin: string; args: { cwd?: string; forkOf?: string } }[] = [];
  const replyMint = (minted: string | null | { bad: unknown }): void => {
    mintAsks.length = 0;
    invokeMock.mockImplementation(launchRenderShim((cmd: string, args: unknown) => {
      if (cmd !== "tmux_name_mint") return Promise.resolve(undefined);
      const { origin, ...rest } = args as Record<string, unknown>;
      mintAsks.push({ origin: String(origin), args: rest });
      return Promise.resolve(minted ?? undefined);
    }));
  };

  it("★ 问不到 ⇒ mintFreshTmuxName 回 ok:false 且带原因；形状不认 ⇒ 同样不铸；正控：那台铸了 ⇒ 用它铸的", async () => {
    replyMint(null);
    const no = await mintFreshTmuxName("devbox", "/home/u/proj");
    expect(no.ok).toBe(false);
    expect(no.ok ? "" : no.why).not.toBe("");
    replyMint({ bad: { name: "proj-cc", extra: 1 } });
    expect((await mintFreshTmuxName("devbox", "/home/u/proj")).ok).toBe(false);
    replyMint("proj-cc-2");
    expect(await mintFreshTmuxName("devbox", "/home/u/proj")).toEqual({ ok: true, name: "proj-cc-2" });
    expect(mintAsks).toEqual([{ origin: "devbox", args: { cwd: "/home/u/proj" } }]);
  });

  it("★ 本机 resume：问不到 ⇒ 交 `tmuxName: null`（后端如实不进容器）；正控：问到 ⇒ 交本机后端铸的名字", async () => {
    const sent = (): Record<string, unknown> =>
      localLaunchCalls(invokeMock.mock.calls, "resume_history_session")[0];
    replyMint(null);
    expect(
      await resumeLocalSession({ sid: "s1", cwd: "/home/u/proj", account: { kind: "explicit", configDir: null, name: null } }),
    ).toBe(true);
    expect(sent().tmuxName).toBeNull();
    invokeMock.mockReset();
    replyMint("proj-cc-2");
    await resumeLocalSession({ sid: "s1", cwd: "/home/u/proj", account: { kind: "explicit", configDir: null, name: null } });
    expect(sent().tmuxName).toBe("proj-cc-2");
    expect(mintAsks).toEqual([{ origin: LOCAL_ORIGIN, args: { cwd: "/home/u/proj" } }]);
    // 账号 0 是**显式 `base`**，不是省略（省略 = 没表态 = 被 shell rc 里的默认号顶掉）。
    expect(sent().account).toEqual({ kind: "base" });
  });

  // 这条原来断「sid 不合法 ⇒ 一次 IPC 都不发」（前端那道 `validateLocalLaunch`〔散文墓碑〕判的）。
  // 今天前端不判 sid：照发给本机后端，后端（`history.rs` 本机决策 → `shell_quote_core::session_id_ok`）拒 ⇒ 出声、回 false。
  it("本机 resume：sid 不合法 ⇒ 前端不判、照发，后端拒 ⇒ 出声、回 false", async () => {
    invokeMock.mockImplementation(launchRenderShim((cmd: string) =>
      cmd === "list_local_tmux"
        ? Promise.resolve([])
        : cmd === "resume_history_session"
          ? Promise.reject('refuse resume: invalid session_id "a b"')
          : Promise.resolve(undefined),
    ));
    expect(
      await resumeLocalSession({ sid: "a b", cwd: "/p", account: { kind: "explicit", configDir: null, name: null } }),
    ).toBe(false);
    expect(localLaunchCalls(invokeMock.mock.calls, "resume_history_session")).toHaveLength(1);
    expect(vi.mocked(showActionFailureToast)).toHaveBeenCalledTimes(1);
  });
});

describe("K4 · D-h：本机跟随时 pin 那个号选不了 ⇒ 不起、说清、给显式选择", () => {
  const acct = (name: string, ok: boolean): Account => ({
    name,
    email: `${name}@x`,
    configDir: `/h/${name}`,
    isDefault: false,
    mode: "isolated",
    exists: true,
    loggedIn: ok,
    authKind: "subscription",
    authReady: ok,
  });
  const snapshot = (accounts: Account[], defaultName: string | null): AccountsState =>
    ({
      origin: LOCAL_ORIGIN,
      available: true,
      error: null,
      notice: null,
      meta: null,
      accounts,
      defaultName,
    }) as unknown as AccountsState;
  const resumes = (): Array<Record<string, unknown>> =>
    localLaunchCalls(invokeMock.mock.calls, "resume_history_session");

  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockImplementation(launchRenderShim((cmd: string) => Promise.resolve(cmd === "list_local_tmux" ? [] : undefined)));
    vi.mocked(showActionFailureToast).mockReset();
    __resetLocalLaunchSnapshotForTests();
  });

  it("★ pin「z」选不了 ⇒ 零次拉起 ＋ 一条可点提示；点了 ⇒ 以当前号「b」起", async () => {
    __setLocalLaunchSnapshotForTests(snapshot([acct("z", false), acct("b", true)], "b"), { s1: "z" });
    expect(await resumeLocalSession({ sid: "s1", cwd: "/home/u/proj", account: { kind: "follow" } })).toBe(false);
    expect(resumes(), "pin 选不了还起了 —— 落到 shell rc 里的默认号，静默换号（E7 本机那一形）").toEqual([]);
    const calls = vi.mocked(showActionFailureToast).mock.calls;
    expect(calls).toHaveLength(1);
    expect(calls[0][0]).toBe("账号现在选不了，没有起会话");
    expect(calls[0][1]).toContain("「z」");
    expect(calls[0][1]).toContain("「b」");
    calls[0][2]!.onClick!();
    await vi.waitFor(() => expect(resumes()).toHaveLength(1));
    expect(resumes()[0].account).toEqual({ kind: "named", configDir: "/h/b", name: "b" });
  });

  it("正控：pin「z」可选 ⇒ 照起、带 z，不提示", async () => {
    __setLocalLaunchSnapshotForTests(snapshot([acct("z", true), acct("b", true)], "b"), { s1: "z" });
    expect(await resumeLocalSession({ sid: "s1", cwd: "/home/u/proj", account: { kind: "follow" } })).toBe(true);
    expect(resumes()[0].account).toEqual({ kind: "named", configDir: "/h/z", name: "z" });
    expect(vi.mocked(showActionFailureToast)).not.toHaveBeenCalled();
  });

  it("没有 pin ⇒ 当前号（没有原账号，谈不上换号）；快照冷 ⇒ 缺席（逐字节旧行为）", async () => {
    __setLocalLaunchSnapshotForTests(snapshot([acct("b", true)], "b"), {});
    await resumeLocalSession({ sid: "s1", cwd: "/p", account: { kind: "follow" } });
    expect(resumes()[0].account).toEqual({ kind: "named", configDir: "/h/b", name: "b" });
    invokeMock.mockClear();
    __resetLocalLaunchSnapshotForTests();
    await resumeLocalSession({ sid: "s1", cwd: "/p", account: { kind: "follow" } });
    expect(resumes()[0].account).toBeUndefined();
  });
});
