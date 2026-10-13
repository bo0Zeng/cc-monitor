/**
 * 额度 / 会话轮换 / 策略目录的推送：额度帧带了成品 ⇒ 用同一个解码器收、直接放进 store、不再问；没带 / 解不开 / 期间可能漏了 ⇒ 照旧重问。
 * 会话轮换不带成品 ⇒ 重问那一个会话的 `rotation-view`；策略目录变了 ⇒ 那台有 tab 的会话全部重问；中转没见过的会话 ⇒ `read: null`。期望手写。
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

import { onQuotaChanged, syncSessions } from "../../../src/frontend/ui/acct-center";
import { appStore } from "../../../src/frontend/ui/app-store";

const flush = () => new Promise((r) => setTimeout(r, 0));

describe("acct-center 收推送", () => {
  beforeEach(() => {
    asked.length = 0;
  });

  it("成品解不开 ⇒ 不放、照旧问", () => {
    onQuotaChanged("box-b", "quota", { cells: [{ key: null, rev: null, body: { nope: 1 } }], all: false });
    expect(appStore.quota.get().has("box-b")).toBe(false);
    expect(asked).toEqual(["quota-read"]);
  });

  it("★ tab 来了 ⇒ 每个会话问一次 rotation-view，在用的号进 store；中转没见过 ⇒ read 为 null", async () => {
    syncSessions([
      { sessionId: "s1", origin: "box-a" },
      { sessionId: "gone", origin: "box-a" },
    ]);
    await flush();
    expect(asked).toEqual(["quota-read", "rotation-view box-a s1", "rotation-view box-a gone"]);
    expect(appStore.sessionRotation.get().get("s1")?.read?.current).toBe("cur-s1");
    expect(appStore.sessionRotation.get().get("gone")).toEqual({ origin: "box-a", read: null });
  });

  it("★ 会话轮换变了 ⇒ 只重问那一个；不认得的会话不问；策略目录变了 ⇒ 那台有 tab 的全部重问", async () => {
    syncSessions([
      { sessionId: "s2", origin: "box-c" },
      { sessionId: "s3", origin: "box-c" },
    ]);
    await flush();
    asked.length = 0;
    onQuotaChanged("box-c", "rotation", { cells: [{ key: "s2", rev: null, body: undefined }, { key: "zz", rev: null, body: undefined }], all: false });
    await flush();
    expect(asked).toEqual(["rotation-view box-c s2"]);
    asked.length = 0;
    onQuotaChanged("box-c", "rotation_strategies", { cells: [{ key: null, rev: null, body: undefined }], all: false });
    await flush();
    expect(asked.sort()).toEqual(["rotation-view box-c s2", "rotation-view box-c s3"]);
  });
});
