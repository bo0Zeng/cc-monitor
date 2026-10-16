// 起会话执行器（`remote-launch-run.ts`）：每条起会话路径问那台后端要那一行 `ccm …`、原样交给终端（或键进 pane），
// 失败怎么说。渲得对不对归 Rust（`cli-golden.json` 逐字节 ＋ 每条路径都以 `ccm ` 开头那条判据）；
// 这里判的是「前端交出去的就是后端那一行，一个字不加、不拼」与「每条路径问的是哪一形」。
import { describe, it, expect, vi, beforeEach } from "vitest";

const render = vi.hoisted(() => ({
  renderCli: vi.fn(),
  planLocalLaunch: vi.fn(),
}));
vi.mock("../../../src/frontend/ui/launch-render", () => ({
  renderCli: render.renderCli,
  planLocalLaunch: render.planLocalLaunch,
}));
const term = vi.hoisted(() => ({ openTerminal: vi.fn() }));
vi.mock("../../../src/frontend/ui/terminal-open", async (orig) => ({
  ...(await orig<typeof import("../../../src/frontend/ui/terminal-open")>()),
  openTerminal: term.openTerminal,
}));
const clip = vi.hoisted(() => ({ writeClipboard: vi.fn() }));
vi.mock("../../../src/frontend/ui/clipboard", () => clip);
const settingsWin = vi.hoisted(() => ({ openSettingsWindow: vi.fn() }));
vi.mock("../../../src/frontend/ui/settings/open-settings", () => settingsWin);
vi.mock("../../../src/frontend/ui/resync", () => ({
  offerResyncRetry: vi.fn(),
}));
vi.mock("../../../src/frontend/ui/kit/toast", () => ({ toast: vi.fn(), failToast: vi.fn() }));
vi.mock("../../../src/frontend/ui/launch-arrival", () => ({
  expectArrival: vi.fn(),
  awaitArrival: vi.fn().mockResolvedValue(true),
  arrivedBody: (o: string) => `报出了@${o}`,
}));

import { toast as showActionFailureToast } from "../../../src/frontend/ui/kit/toast";
import {
  runRemoteAttach,
} from "../../../src/frontend/ui/remote-launch-run";
import type { CliRenderRequest } from "../../../src/frontend/ui/launch-cli-wire";

const toastMock = showActionFailureToast as unknown as ReturnType<typeof vi.fn>;

/** 后端那一行的替身：中性、认得出（判的是「交出去的就是它」，不是它长得像不像一条真命令）。 */
const lineFor = (req: CliRenderRequest): string =>
  `ccm <rendered:${JSON.stringify(req)}>`;

/** 写剪贴板那一口（`clipboard.ts`，经壳）换成 `writeText`。 */
function stubClipboard(writeText: (t: string) => Promise<void>): void {
  clip.writeClipboard.mockImplementation(writeText);
}

function requests(): CliRenderRequest[] {
  return render.renderCli.mock.calls.map((c) => c[1] as CliRenderRequest);
}

beforeEach(() => {
  vi.clearAllMocks();
  render.renderCli.mockImplementation(
    async (_o: string, req: CliRenderRequest) => ({
      cmd: lineFor(req),
      account: null,
    }),
  );
  render.planLocalLaunch.mockResolvedValue({
    cmd: "ccm -- --attach n-cc",
    account: null,
  });
  term.openTerminal.mockResolvedValue(undefined);
  stubClipboard(vi.fn().mockResolvedValue(undefined));
});

describe("每条远端起会话路径：问那台要那一行，原样交给终端", () => {
  it("接回：动作 attach，交给终端的就是那一行", async () => {
    await runRemoteAttach("devbox", "claude", "cc-x");
    const [req] = requests();
    expect(req.action).toEqual({ kind: "attach", name: "cc-x" });
    expect(term.openTerminal).toHaveBeenCalledWith("devbox", lineFor(req));
  });
});

describe("失败怎么说", () => {
  it("接回开不了终端 ⇒ 剪贴板与提示里是那一整行，长 tmux 名不截（超过 15 个字也是全名）", async () => {
    term.openTerminal.mockRejectedValue(new Error("没开成"));
    const writeText = vi.fn().mockResolvedValue(undefined);
    stubClipboard(writeText);
    const name = "android-terminal-session";
    await runRemoteAttach("devbox", "claude", name);
    const line = lineFor(requests()[0]);
    expect(line).toContain(name);
    expect(writeText).toHaveBeenCalledWith(line);
    expect(String(toastMock.mock.calls[0][1]).split("\n")).toContain(line);
  });

});
