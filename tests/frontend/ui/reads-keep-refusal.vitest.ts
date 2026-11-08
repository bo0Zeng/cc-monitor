/**
 * 界面那几个读口问失败时原样抛通道交的那个错，不再包成 `new Error(那一句)`：包了就丢了那台回的码与复制详情，
 * 照码挑格的那几处（账号页配 key：`bad_args` 是地址那一格）永远落空。那一句不变（`saidOf` 取的就是它的 `message`）。
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { refusalIn } from "../../../src/frontend/ui/ipc/chan-caller";
import { detailOf, saidOf } from "../../../src/frontend/ui/kit/detail";
import { writeApikeyKey } from "../../../src/frontend/ui/apikey-reads";
import { fetchHooksReport } from "../../../src/frontend/ui/cc-bus-hooks-reads";
import { pushPublicKey } from "../../../src/frontend/ui/pubkey-push";
import { syncAssets } from "../../../src/frontend/ui/assets-sync-reads";
import { fetchImage } from "../../../src/frontend/ui/cards/attach-strip";
import { markChore, readChores, readDataReport } from "../../../src/frontend/ui/settings/data-reads";
import { readBranch, readRecordDrift } from "../../../src/frontend/ui/record-reads";
import { importSshHosts } from "../../../src/frontend/ui/ssh-config-reads";
import { listForwards, startForward, stopForward } from "../../../src/frontend/ui/port-forward-reads";
import { profileBases, profileImpact, readProfiles, resolveProfile } from "../../../src/frontend/ui/profiles-reads";
import { refusedReply } from "../../test-support/chan-fake";

const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;
const SAID = "核心·地址那一格不对";

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => (cmd === "chan_call" ? Promise.reject(refusedReply("bad_args", SAID)) : Promise.resolve(null)));
});

const readers: [string, () => Promise<unknown>][] = [
  ["writeApikeyKey", () => writeApikeyKey("devbox", "/h/.claude-alt/work", "sk-FIXTURE", "https://up.example/v1")],
  ["fetchHooksReport", () => fetchHooksReport("devbox")],
  ["pushPublicKey", () => pushPublicKey({} as never, null)],
  ["syncAssets", () => syncAssets("devbox")],
  ["fetchImage", () => fetchImage("devbox", "/p/s.jsonl", "i1", { at: 0, n: 0 } as never)],
  ["markChore", () => markChore("devbox", {} as never)],
  ["readDataReport", () => readDataReport("devbox")],
  ["readChores", () => readChores()],
  ["readBranch", () => readBranch("devbox", "/p/s.jsonl")],
  ["readRecordDrift", () => readRecordDrift("devbox")],
  ["importSshHosts", () => importSshHosts([])],
  ["startForward", () => startForward({} as never)],
  ["stopForward", () => stopForward("f1")],
  ["listForwards", () => listForwards()],
  ["readProfiles", () => readProfiles("devbox")],
  ["resolveProfile", () => resolveProfile("devbox", "p", null, null)],
  ["profileImpact", () => profileImpact("devbox", [])],
  ["profileBases", () => profileBases("devbox", "p")],
];

describe("读口失败原样抛通道那个错（码 · 那一句 · 复制详情都在）", () => {
  it.each(readers)("★ %s：被拒 `bad_args` ⇒ 抛出的错认得出码，那一句与详情照旧", async (_name, ask) => {
    const err = await ask().then(
      () => null,
      (e: unknown) => e,
    );
    expect(err).not.toBeNull();
    expect(refusalIn(err)?.code).toBe("bad_args");
    expect(saidOf(err)).toBe(SAID);
    expect(detailOf(err)).not.toBe("");
  });
});
