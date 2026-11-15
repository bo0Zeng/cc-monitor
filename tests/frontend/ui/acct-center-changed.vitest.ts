/**
 * 额度 / 会话轮换 / 策略目录 / 账号清单的推送：额度帧带了成品 ⇒ 用同一个解码器收、直接放进 store；没带 / 解不开 / 期间可能漏了 ⇒ 照旧重问。
 * 会话轮换不带成品 ⇒ 重问那一个会话的 `rotation-view` 与 `session-account`；策略目录变了 ⇒ 要问的会话 `rotation-view` 全部重问；
 * 额度账 · 账号清单变了 ⇒ 要问的会话 `session-account` 全部重问（用量 · 与默认号比都在那一份里）；中转没见过的会话 ⇒ `read: null`。
 * 要问的会话 ＝ 此刻有 tab、脸不是已结束的那几个。同一（台 · 会话）同一问一次只在飞一问，回来至多补一趟：那台答得慢、推得密也不堆。期望手写。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

/** 替身那台怎么答：`hold` ⇒ 压住、等 `release`；`slowMs` ⇒ 替身的钟（`now`）走过那么久才回；`fail` ⇒ 回错。 */
const there = vi.hoisted(() => ({
  hold: false,
  slowMs: 0,
  now: 0,
  due: [] as Array<{ at: number; r: () => void }>,
  fail: false,
  held: [] as Array<() => void>,
  /** 每一问（命令 台 会话）此刻在飞几问。 */
  flying: new Map<string, number>(),
}));
const asked: string[] = [];

/** 替身那一问：记一笔、照 `there` 答。 */
async function answer<T>(what: string, reply: () => T): Promise<T> {
  asked.push(what);
  there.flying.set(what, (there.flying.get(what) ?? 0) + 1);
  try {
    if (there.hold) await new Promise<void>((r) => there.held.push(r));
    else if (there.slowMs > 0) await new Promise<void>((r) => there.due.push({ at: there.now + there.slowMs, r }));
    if (there.fail) throw new Error("那台回错");
    return reply();
  } finally {
    there.flying.set(what, there.flying.get(what)! - 1);
  }
}

vi.mock("../../../src/frontend/ui/quota-reads", async (importOriginal) => {
  const real = await importOriginal<typeof import("../../../src/frontend/ui/quota-reads")>();
  return {
    ...real,
    readQuota: vi.fn(async () => {
      asked.push("quota-read");
      return null;
    }),
  };
});
vi.mock("../../../src/frontend/ui/rotation-view-reads", () => ({
  readSessionView: vi.fn((o: string, sid: string) =>
    answer(`rotation-view ${o} ${sid}`, () => (sid === "gone" ? null : { start: "a", current: `cur-${sid}` })),
  ),
}));
vi.mock("../../../src/frontend/ui/account-reads", () => ({
  fetchAccounts: vi.fn(async (o: string) => {
    asked.push(`accounts-list ${o}`);
    return null;
  }),
}));
vi.mock("../../../src/frontend/ui/session-account-reads", () => ({
  readSessionAccount: vi.fn((o: string, sid: string) => answer(`session-account ${o} ${sid}`, () => ({ account: `acct-${o}-${sid}` }))),
}));

import { onAccountsChanged, onQuotaChanged, refreshQuota, syncSessions } from "../../../src/frontend/ui/acct-center";
import { appStore, putIn, quotaOf } from "../../../src/frontend/ui/app-store";
import type { QuotaRead } from "../../../src/frontend/ui/generated/QuotaRead";
import { readQuota } from "../../../src/frontend/ui/quota-reads";
import { goldenFace } from "../../test-support/session-faces";

const flush = () => new Promise((r) => setTimeout(r, 0));
const LIVE = goldenFace("live-working-plain");
const ENDED = goldenFace("ended");
const tab = (sessionId: string, origin: string, face = LIVE) => ({ sessionId, origin, face });
/** 放回压住的那几问（放的过程中又压住的留到下一次）。 */
async function release(): Promise<void> {
  const now = there.held.splice(0);
  for (const r of now) r();
  await flush();
  await flush();
}
const count = (prefix: string) => asked.filter((a) => a.startsWith(prefix)).length;
const quotaPush = (all = false) => ({ cells: [{ key: null, rev: null, body: undefined }], all });

describe("acct-center 收推送", () => {
  beforeEach(async () => {
    there.hold = false;
    there.slowMs = 0;
    there.fail = false;
    await release();
    asked.length = 0;
  });

  it("★ tab 来了 ⇒ 每个会话在它那台问一次 rotation-view 与 session-account，进 store；中转没见过 ⇒ read 为 null", async () => {
    syncSessions([tab("s1", "box-a"), tab("gone", "box-a"), tab("sb", "box-b")]);
    await flush();
    expect(asked.sort()).toEqual(
      [
        "quota-read",
        "quota-read",
        "rotation-view box-a s1",
        "rotation-view box-a gone",
        "rotation-view box-b sb",
        "session-account box-a s1",
        "session-account box-a gone",
        "session-account box-b sb",
      ].sort(),
    );
    expect(appStore.sessionRotation.get().get("s1")?.read?.current).toBe("cur-s1");
    expect(appStore.sessionRotation.get().get("gone")).toEqual({ origin: "box-a", read: null });
    expect(appStore.sessionAccount.get().get("sb")).toEqual({ origin: "box-b", read: { account: "acct-box-b-sb" } });
  });

  it("★ 会话轮换变了 ⇒ 只重问那一个；不认得的会话不问；策略目录变了 ⇒ 要问的会话 rotation-view 全部重问", async () => {
    syncSessions([tab("s2", "box-c"), tab("s3", "box-c")]);
    await flush();
    asked.length = 0;
    onQuotaChanged("box-c", "rotation", { cells: [{ key: "s2", rev: null, body: undefined }, { key: "zz", rev: null, body: undefined }], all: false });
    await flush();
    expect(asked.sort()).toEqual(["rotation-view box-c s2", "session-account box-c s2"]);
    asked.length = 0;
    onQuotaChanged("box-c", "rotation_strategies", quotaPush());
    await flush();
    expect(asked.sort()).toEqual(["rotation-view box-c s2", "rotation-view box-c s3"]);
  });

  it("★ 额度账 · 账号清单变了 ⇒ 要问的会话 session-account 全部重问；别的台不问", async () => {
    syncSessions([tab("q1", "box-d"), tab("q2", "box-d"), tab("e1", "box-e")]);
    await flush();
    asked.length = 0;
    onQuotaChanged("box-d", "quota", quotaPush(true));
    await flush();
    expect(asked.filter((a) => a.startsWith("session-account")).sort()).toEqual(["session-account box-d q1", "session-account box-d q2"]);
    asked.length = 0;
    onAccountsChanged("box-e");
    await flush();
    expect(asked).toEqual(["session-account box-e e1"]);
    asked.length = 0;
    onAccountsChanged();
    await flush();
    expect(asked.filter((a) => a.startsWith("session-account")).sort()).toEqual([
      "session-account box-d q1",
      "session-account box-d q2",
      "session-account box-e e1",
    ]);
  });

  it("★ 额度账变了 ⇒ 那台的账号清单读过就重读一次（每号那几格在那一行里）；没读过的台不问", async () => {
    putIn(appStore.accounts, "box-g", { origin: "box-g", available: true, error: null, oldBackend: false, meta: null, accounts: [], notice: null });
    asked.length = 0;
    onQuotaChanged("box-g", "quota", { cells: [], all: false });
    onQuotaChanged("box-h", "quota", { cells: [], all: false });
    await flush();
    expect(asked.filter((a) => a.startsWith("accounts-list"))).toEqual(["accounts-list box-g"]);
  });

  it("★ quota-read 整问失败 ⇒ 那一格记下出错那一层写好的那一句与复制详情；再问成了 ⇒ 换成成品", async () => {
    const e = Object.assign(new Error("连接层那一句"), { detail: "D-detail" });
    vi.mocked(readQuota).mockRejectedValueOnce(e);
    expect(quotaOf("box-f")).toEqual({ state: "pending" });
    await refreshQuota("box-f");
    expect(quotaOf("box-f")).toEqual({ state: "failed", said: "连接层那一句", detail: "D-detail" });
    const read = { state: "present", accounts: [] } as unknown as QuotaRead;
    vi.mocked(readQuota).mockResolvedValueOnce(read);
    await refreshQuota("box-f");
    expect(quotaOf("box-f")).toBe(read);
  });
});

describe("那台答得慢、推得密时不堆（壳到本机后端在等堆满 256 的根因）", () => {
  beforeEach(async () => {
    there.hold = false;
    there.slowMs = 0;
    there.fail = false;
    await release();
    asked.length = 0;
  });

  it("★ 每问 10 s 才回、100 个会话、quota 每 4 s 推一声持续 60 s ⇒ 同一会话 session-account 在飞至多 1 问，那台这一问在飞 ≤ 100", async () => {
    // 钟是替身自己的一格（`there.now`）：每过 1 s 放回到点的那几问，不睡真时长。
    there.slowMs = 10_000;
    there.now = 0;
    const tick = async () => {
      there.now += 1_000;
      for (const d of there.due.filter((x) => x.at <= there.now)) d.r();
      there.due = there.due.filter((x) => x.at > there.now);
      await flush();
      await flush();
    };
    {
      const tabs = Array.from({ length: 100 }, (_, i) => tab(`p${i}`, "box-slow"));
      syncSessions(tabs);
      let perSid = 0;
      let total = 0;
      const look = () => {
        let sum = 0;
        for (const [k, n] of there.flying) {
          if (!k.startsWith("session-account box-slow ")) continue;
          perSid = Math.max(perSid, n);
          sum += n;
        }
        total = Math.max(total, sum);
      };
      for (let t = 0; t < 60; t += 4) {
        onQuotaChanged("box-slow", "quota", { cells: [{ key: null, rev: null, body: { state: "present", accounts: [] } }], all: false });
        look();
        for (let s = 0; s < 4; s++) {
          await tick();
          look();
        }
      }
      expect(perSid).toBe(1);
      expect(total).toBeLessThanOrEqual(100);
      for (let s = 0; s < 30; s++) await tick();
      there.slowMs = 0;
    }
  });

  it("★ session-account 压住、推 5 声 quota ⇒ 每会话只 1 问；放回 ⇒ 恰补 1 问；再放回 ⇒ 不再问", async () => {
    syncSessions([tab("h1", "box-hold"), tab("h2", "box-hold")]);
    await flush();
    await release();
    asked.length = 0;
    there.hold = true;
    for (let i = 0; i < 5; i++) onQuotaChanged("box-hold", "quota", quotaPush(true));
    await flush();
    expect(asked.filter((a) => a.startsWith("session-account")).sort()).toEqual(["session-account box-hold h1", "session-account box-hold h2"]);
    asked.length = 0;
    await release();
    expect(asked.filter((a) => a.startsWith("session-account")).sort()).toEqual(["session-account box-hold h1", "session-account box-hold h2"]);
    asked.length = 0;
    await release();
    expect(count("session-account")).toBe(0);
  });

  it("★ 压住的那一问回错、期间推过 quota ⇒ 不往外抛，恰补 1 问；补的那一问回来以后不再问", async () => {
    syncSessions([tab("f1", "box-fail")]);
    await flush();
    await release();
    asked.length = 0;
    there.hold = true;
    there.fail = true;
    onQuotaChanged("box-fail", "quota", quotaPush(true));
    onQuotaChanged("box-fail", "quota", quotaPush(true));
    await flush();
    expect(count("session-account")).toBe(1);
    asked.length = 0;
    there.fail = false;
    await release();
    expect(asked.filter((a) => a.startsWith("session-account"))).toEqual(["session-account box-fail f1"]);
    asked.length = 0;
    await release();
    expect(count("session-account")).toBe(0);
    expect(appStore.sessionAccount.get().get("f1")).toEqual({ origin: "box-fail", read: { account: "acct-box-fail-f1" } });
  });

  it("★ rotation-view 压住、推 5 声 rotation（all）⇒ 每会话只 1 问；放回 ⇒ 恰补 1 问", async () => {
    syncSessions([tab("r1", "box-rot"), tab("r2", "box-rot")]);
    await flush();
    await release();
    asked.length = 0;
    there.hold = true;
    for (let i = 0; i < 5; i++) onQuotaChanged("box-rot", "rotation", { cells: [], all: true });
    await flush();
    expect(asked.filter((a) => a.startsWith("rotation-view")).sort()).toEqual(["rotation-view box-rot r1", "rotation-view box-rot r2"]);
    asked.length = 0;
    await release();
    expect(asked.filter((a) => a.startsWith("rotation-view")).sort()).toEqual(["rotation-view box-rot r1", "rotation-view box-rot r2"]);
    asked.length = 0;
    await release();
    expect(count("rotation-view")).toBe(0);
  });

  it("★ tab 关了 · 会话结束了 ⇒ 再推不问它；又出现 ⇒ 问一次；压着时关掉的不补", async () => {
    syncSessions([tab("k1", "box-k"), tab("k2", "box-k"), tab("k3", "box-k")]);
    await flush();
    await release();
    asked.length = 0;
    syncSessions([tab("k1", "box-k"), tab("k3", "box-k", ENDED)]);
    onQuotaChanged("box-k", "quota", quotaPush(true));
    onQuotaChanged("box-k", "rotation", { cells: [], all: true });
    await flush();
    await release();
    expect(asked.filter((a) => !a.startsWith("quota-read")).sort()).toEqual(["rotation-view box-k k1", "session-account box-k k1", "session-account box-k k1"]);
    asked.length = 0;
    syncSessions([tab("k1", "box-k"), tab("k2", "box-k"), tab("k3", "box-k")]);
    await flush();
    await release();
    expect(asked.sort()).toEqual(["rotation-view box-k k2", "rotation-view box-k k3", "session-account box-k k2", "session-account box-k k3"]);
    // 压着时关掉 ⇒ 回来不补
    asked.length = 0;
    there.hold = true;
    onAccountsChanged("box-k");
    onAccountsChanged("box-k");
    await flush();
    syncSessions([tab("k1", "box-k"), tab("k2", "box-k")]);
    asked.length = 0;
    await release();
    expect(asked.sort()).toEqual(["session-account box-k k1", "session-account box-k k2"]);
  });

  it("★ 那台第一次出现 ⇒ quota-read 恰 1 问；tab 全走了再推 rotation · 账号清单变了 ⇒ 不问 quota-read，又来 tab 也不问", async () => {
    syncSessions([tab("m1", "box-m"), tab("m2", "box-m")]);
    await flush();
    await release();
    expect(count("quota-read")).toBe(1);
    syncSessions([tab("m1", "box-m")]);
    syncSessions([]);
    onQuotaChanged("box-m", "rotation", { cells: [], all: true });
    onAccountsChanged("box-m");
    syncSessions([tab("m3", "box-m")]);
    await flush();
    await release();
    expect(count("quota-read")).toBe(1);
  });
});
