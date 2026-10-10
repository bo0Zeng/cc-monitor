/**
 * 额度 / 规则表 / 会话轮换的推送带了成品（同重问那条命令的应答）⇒ 用同一个解码器收、直接放进 store、不再问；
 * 没带 / 解不开 / 期间可能漏了 ⇒ 照旧重问。期望手写。
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
    readRules: vi.fn(async () => {
      asked.push("rotation-rules-read");
      return null;
    }),
    readSessionRotation: vi.fn(async (_o: string, sids: string[]) => {
      asked.push(`rotation-session-read ${sids.join(",")}`);
      return { now: 1, sessions: {} };
    }),
  };
});

import { onQuotaChanged } from "../../../src/frontend/ui/acct-center";
import { appStore } from "../../../src/frontend/ui/app-store";

const rules = { state: "present", defaultRule: "r1", rules: [] };
const rotation = { now: 7, sessions: { s1: { state: "absent", inPlace: "follow" } } };

describe("acct-center 收推送里的成品", () => {
  beforeEach(() => {
    asked.length = 0;
  });

  it("规则表带了成品 ⇒ 进 store、不问；没带 ⇒ 问；可能漏了 ⇒ 带了也问", () => {
    onQuotaChanged("box-a", "rotation_rules", { cells: [{ key: null, rev: null, body: rules }], all: false });
    expect(appStore.rotationRules.get().get("box-a")?.defaultRule).toBe("r1");
    expect(asked).toEqual([]);
    onQuotaChanged("box-a", "rotation_rules", { cells: [{ key: null, rev: null, body: undefined }], all: false });
    expect(asked).toEqual(["rotation-rules-read"]);
    onQuotaChanged("box-a", "rotation_rules", { cells: [], all: true });
    expect(asked).toEqual(["rotation-rules-read", "rotation-rules-read"]);
  });

  it("成品解不开 ⇒ 不放、照旧问", () => {
    onQuotaChanged("box-b", "quota", { cells: [{ key: null, rev: null, body: { nope: 1 } }], all: false });
    expect(appStore.quota.get().has("box-b")).toBe(false);
    expect(asked).toEqual(["quota-read"]);
  });

  it("会话轮换带了那个会话的成品 ⇒ 进 store、不问那一个", () => {
    onQuotaChanged("box-a", "rotation", { cells: [{ key: "s1", rev: null, body: rotation }], all: false });
    expect(appStore.sessionRotation.get().get("s1")).toEqual({ origin: "box-a", now: 7, read: rotation.sessions.s1 });
    expect(asked).toEqual([]);
  });
});
