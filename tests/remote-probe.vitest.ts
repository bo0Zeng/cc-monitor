/**
 * 设计/99 §2.1 ⑬ 主会话裁「`test_remote_connection`：界面把表单里（未保存的）那台配置交给本机后端，后端组拨号请求、拨一次、回结局」——
 * 界面那一侧：交过去的是表单那一台 ＋ 已保存的同名那一份 ＋ 跳板那一台（问 `<local>` 的 `remote-probe`）；结局按形状严格收。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { decodeProbe, probeMachine } from "../src/remote-probe";
import type { RemoteHostConfig } from "../src/remote-config";
import { LOCAL_ORIGIN } from "../src/backend-policy";
import { chanArgsJson, chanReply, NO_CHANNEL, type ChanCallArgs } from "./test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const host = (over: Partial<RemoteHostConfig>): RemoteHostConfig => ({
  label: "devbox",
  host: "10.0.0.2",
  port: 22,
  user: "u",
  keyPath: "",
  hostKeyFingerprint: "",
  addresses: [],
  jump: "",
  resumeCommand: "",
  ...over,
});
const ok = {
  sshOk: true,
  fingerprint: "SHA256:k",
  endpoint: "10.0.0.2:22",
  backendOk: true,
  backendHello: "v=1 control=ok(3ms)",
  message: "SSH 与后端均正常。",
  stages: [{ kind: "dialing", endpoint: "10.0.0.2:22" }, { kind: "established" }],
};

beforeEach(() => invokeMock.mockReset());

describe("请求", () => {
  it("问 `<local>` 的 remote-probe：表单那一台 ＋ 已保存的同名那一份 ＋ 跳板那一台", async () => {
    let seen: { op: string; origin: string; body: unknown } | null = null;
    invokeMock.mockImplementation((cmd: string, args: ChanCallArgs) => {
      if (cmd !== "chan_call") return Promise.resolve(undefined); // 别的那一跳（撤单 / 记账）不看
      seen = { op: args.op, origin: args.origin, body: chanArgsJson(args) };
      return Promise.resolve(chanReply(ok));
    });
    const form = host({ host: "10.0.0.3", jump: "bastion" });
    const saved = [host({ label: "other" }), host({ label: "bastion", host: "b.example" }), host({ hostKeyFingerprint: "SHA256:old" })];
    const r = await probeMachine(form, saved);
    expect(seen).toEqual({
      op: "remote-probe",
      origin: LOCAL_ORIGIN,
      body: { machine: form, saved: saved[2], jump: saved[1] },
    });
    expect(r.stages.map((s) => s.kind)).toEqual(["dialing", "established"]);
  });
  it("本机后端不在 ⇒ 抛（不是一次失败的测试结局）", async () => {
    invokeMock.mockImplementation((cmd: string) => (cmd === "chan_call" ? Promise.reject(NO_CHANNEL) : Promise.resolve(undefined)));
    await expect(probeMachine(host({}), [])).rejects.toThrow();
  });
});

describe("严格收", () => {
  it.each([
    ["多一格", { ...ok, extra: 1 }],
    ["缺阶段行", { ...ok, stages: undefined }],
    ["阶段行不带 kind", { ...ok, stages: [{}] }],
    ["sshOk 不是布尔", { ...ok, sshOk: "yes" }],
  ])("%s ⇒ 抛", (_n, v) => {
    expect(() => decodeProbe(JSON.parse(JSON.stringify(v)))).toThrow();
  });
});
