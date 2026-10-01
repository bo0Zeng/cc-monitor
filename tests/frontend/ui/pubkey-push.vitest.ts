/**
 * 要求住址：主会话 09-28 裁 MIG-3b 报备 2 ——「`push_public_key`〔散文墓碑〕：本机后端帧命令 `pubkey-push {machine}`，与 `remote-probe` 同形」。
 *
 * ① 应答两侧对拍：后端判据核键集 == 金样 `tests/__fixtures__/pubkey-push.golden.json`，这里的解码器读同一份（多一格 / 缺一格 / 值不在闭集 ⇒ 抛）；
 * ② 请求发给本机那一台、op 是 `pubkey-push`、体恰是金样那四格（已保存的同名那一份 · 跳板那一台从 `saved` 里找）、带期限。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

const sent: { origin: string; op: string; body: unknown; until: unknown }[] = [];
let reply: unknown = null;
vi.mock("../../../src/comms/inward/chan", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../src/comms/inward/chan")>()),
  chan: {
    call: async (origin: string, op: string, body: Uint8Array, budget: { until: number }) => {
      sent.push({ origin, op, body: JSON.parse(new TextDecoder().decode(body)), until: budget?.until });
      return new TextEncoder().encode(JSON.stringify(reply));
    },
  },
}));

import golden from "../../__fixtures__/pubkey-push.golden.json";
import { decodePush, pushPublicKey } from "../../../src/frontend/ui/pubkey-push";
import { LOCAL_ORIGIN } from "../../../src/frontend/ui/backend-policy";
import type { RemoteHostConfig } from "../../../src/frontend/ui/remote-config";

beforeEach(() => {
  sent.length = 0;
});

describe("MIG-3b 续 公钥推送走通道、本机后端办", () => {
  it("★ 金样的应答原样收下；多一格 / 缺一格 / 值不在闭集 ⇒ 抛", () => {
    expect(decodePush(golden.product)).toEqual(golden.product);
    for (const bad of [
      { ...golden.product, extra: 1 },
      { outcome: "added", pubPath: "/k" },
      { ...golden.product, outcome: "maybe" },
      { ...golden.product, via: "ssh" },
      null,
    ]) {
      expect(() => decodePush(bad), JSON.stringify(bad)).toThrow();
    }
  });

  it("发给本机那一台的 `pubkey-push`，体恰是金样那四格、带期限", async () => {
    reply = golden.product;
    const machine = { ...golden.request.machine, port: 22, addresses: [], jump: "", hostKeyFingerprint: null } as unknown as RemoteHostConfig;
    const r = await pushPublicKey(machine, [], null);
    expect(r).toEqual(golden.product);
    expect(sent).toHaveLength(1);
    expect(sent[0].origin).toBe(LOCAL_ORIGIN);
    expect(sent[0].op).toBe("pubkey-push");
    expect(Object.keys(sent[0].body as object).sort()).toEqual(Object.keys(golden.request).sort());
    expect((sent[0].body as { machine: unknown }).machine).toEqual(machine);
    expect(typeof sent[0].until).toBe("number");
  });
});
