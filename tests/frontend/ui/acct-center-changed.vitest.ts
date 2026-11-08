/**
 * 额度 / 会话轮换 / 策略目录 / 账号清单的推送：额度帧带了成品 ⇒ 用同一个解码器收、直接放进 store、不再问；没带 / 解不开 / 期间可能漏了 ⇒ 照旧重问。
 * 会话轮换不带成品 ⇒ 重问那一个会话的 `rotation-view` 与 `session-account`；策略目录变了 ⇒ 那台有 tab 的会话 `rotation-view` 全部重问；
 * 额度账 · 账号清单变了 ⇒ 那台有 tab 的会话 `session-account` 全部重问（用量 · 与默认号比都在那一份里）；中转没见过的会话 ⇒ `read: null`。期望手写。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const asked: string[] = [];
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
  readSessionView: vi.fn(async (o: string, sid: string) => {
    asked.push(`rotation-view ${o} ${sid}`);
    return sid === "gone" ? null : { start: "a", current: `cur-${sid}` };
  }),
}));
vi.mock("../../../src/frontend/ui/account-reads", () => ({
  fetchAccounts: vi.fn(async (o: string) => {
    asked.push(`accounts-list ${o}`);
    return null;
  }),
}));
vi.mock("../../../src/frontend/ui/session-account-reads", () => ({
  readSessionAccount: vi.fn(async (o: string, sid: string) => {
    asked.push(`session-account ${o} ${sid}`);
    return { account: `acct-${o}-${sid}` };
  }),
}));

import { onAccountsChanged, onQuotaChanged, refreshQuota, syncSessions } from "../../../src/frontend/ui/acct-center";
import { appStore, putIn, quotaOf } from "../../../src/frontend/ui/app-store";
import type { QuotaRead } from "../../../src/frontend/ui/generated/QuotaRead";
import { readQuota } from "../../../src/frontend/ui/quota-reads";

const flush = () => new Promise((r) => setTimeout(r, 0));

describe("acct-center 收推送", () => {
  beforeEach(() => {
    asked.length = 0;
  });

  it("★ tab 来了 ⇒ 每个会话在它那台问一次 rotation-view 与 session-account，进 store；中转没见过 ⇒ read 为 null", async () => {
    syncSessions([
      { sessionId: "s1", origin: "box-a" },
      { sessionId: "gone", origin: "box-a" },
      { sessionId: "sb", origin: "box-b" },
    ]);
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

  it("★ 会话轮换变了 ⇒ 只重问那一个；不认得的会话不问；策略目录变了 ⇒ 那台有 tab 的 rotation-view 全部重问", async () => {
    syncSessions([
      { sessionId: "s2", origin: "box-c" },
      { sessionId: "s3", origin: "box-c" },
    ]);
    await flush();
    asked.length = 0;
    onQuotaChanged("box-c", "rotation", { cells: [{ key: "s2", rev: null, body: undefined }, { key: "zz", rev: null, body: undefined }], all: false });
    await flush();
    expect(asked.sort()).toEqual(["rotation-view box-c s2", "session-account box-c s2"]);
    asked.length = 0;
    onQuotaChanged("box-c", "rotation_strategies", { cells: [{ key: null, rev: null, body: undefined }], all: false });
    await flush();
    expect(asked.sort()).toEqual(["rotation-view box-c s2", "rotation-view box-c s3"]);
  });

  it("★ 额度账 · 账号清单变了 ⇒ 那台有 tab 的会话 session-account 全部重问；别的台不问", async () => {
    syncSessions([
      { sessionId: "q1", origin: "box-d" },
      { sessionId: "q2", origin: "box-d" },
      { sessionId: "e1", origin: "box-e" },
    ]);
    await flush();
    asked.length = 0;
    onQuotaChanged("box-d", "quota", { cells: [{ key: null, rev: null, body: undefined }], all: true });
    await flush();
    expect(asked.filter((a) => a.startsWith("session-account")).sort()).toEqual(["session-account box-d q1", "session-account box-d q2"]);
    asked.length = 0;
    onAccountsChanged("box-e");
    await flush();
    expect(asked).toEqual(["session-account box-e e1"]);
    asked.length = 0;
    onAccountsChanged();
    await flush();
    expect(asked.filter((a) => a.startsWith("session-account")).length).toBeGreaterThanOrEqual(3);
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
