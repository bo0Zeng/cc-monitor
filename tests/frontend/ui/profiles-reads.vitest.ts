/**
 * 设置窗「别名与配置文件」五问（`profiles-*`）走通道、按类型问：后端现算对拍过的同一份金样（`profiles.golden.json`）原样交到调用方
 * （回包形状由握手时的形状指纹管，这里不逐格判）；请求问对那台、说对那条、参数原样；
 * 写那一口那台说 `stale` ⇒ 抛 ProfilesStale（界面据此重读、表单留着）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import {
  profileBases,
  profileImpact,
  ProfilesStale,
  readProfiles,
  resolveProfile,
  writeProfiles,
  type ProfileOp,
} from "../../../src/frontend/ui/profiles-reads";
import { REPO_ROOT } from "../../test-support/repo-root";
import { chanArgsJson, chanReply, refusedReply, type ChanCallArgs } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const G = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/profiles.golden.json"), "utf8")) as Record<string, Record<string, unknown>>;

beforeEach(() => {
  invokeMock.mockReset();
});

describe("金样：后端出的成品，原样交到调用方", () => {
  it("五口", async () => {
    invokeMock.mockResolvedValueOnce(chanReply(G.readReply));
    expect(await readProfiles("<local>")).toEqual(G.readReply);
    invokeMock.mockResolvedValueOnce(chanReply(G.resolveReply));
    expect(await resolveProfile("<local>", "teamcct", null, "/tmp")).toEqual(G.resolveReply);
    invokeMock.mockResolvedValueOnce(chanReply(G.impactReply));
    expect({ affected: await profileImpact("<local>", []) }).toEqual(G.impactReply);
    invokeMock.mockResolvedValueOnce(chanReply(G.basesReply));
    expect({ bases: await profileBases("<local>", "mycct") }).toEqual(G.basesReply);
    invokeMock.mockResolvedValueOnce(chanReply(G.writeReply));
    expect(await writeProfiles("<local>", [], null)).toEqual(G.writeReply);
  });
});

describe("请求：问对那台、说对那条、参数原样", () => {
  it("五问各一发", async () => {
    const changes = (G.impactArgs as { changes: ProfileOp[] }).changes;
    invokeMock.mockResolvedValueOnce(chanReply(G.readReply));
    await readProfiles("devbox");
    invokeMock.mockResolvedValueOnce(chanReply(G.resolveReply));
    await resolveProfile("devbox", "cc", null, null);
    invokeMock.mockResolvedValueOnce(chanReply(G.impactReply));
    await profileImpact("devbox", changes);
    invokeMock.mockResolvedValueOnce(chanReply(G.basesReply));
    await profileBases("devbox", "cc");
    invokeMock.mockResolvedValueOnce(chanReply(G.writeReply));
    await writeProfiles("devbox", changes, "fp");
    const calls = invokeMock.mock.calls.map((c) => c[1] as ChanCallArgs);
    expect(calls.map((x) => [x.origin, x.op, chanArgsJson(x)])).toEqual([
      ["devbox", "profiles-read", {}],
      ["devbox", "profiles-resolve", { name: "cc", edit: null, at: null }],
      ["devbox", "profiles-impact", { changes }],
      ["devbox", "profiles-bases", { name: "cc" }],
      ["devbox", "profiles-write", { changes, fingerprint: "fp" }],
    ]);
  });
});

describe("写那一口的拒绝", () => {
  it("那台说 stale ⇒ ProfilesStale；别的拒绝是普通的错", async () => {
    invokeMock.mockRejectedValueOnce(refusedReply("stale", "被别处改过"));
    await expect(writeProfiles("devbox", [], "fp")).rejects.toBeInstanceOf(ProfilesStale);
    invokeMock.mockRejectedValueOnce(refusedReply("refused", "多出坏处"));
    const e = await writeProfiles("devbox", [], "fp").catch((x: unknown) => x);
    expect(e).not.toBeInstanceOf(ProfilesStale);
  });
});
