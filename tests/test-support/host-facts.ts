/**
 * 「壳在那个平台上给的那一份本机能力」：读跨语言金样 `tests/__fixtures__/host-facts.golden.json`（壳那一侧按编它的平台对拍同一份）。
 * `unknown` ⇒ 界面读不到时的那一份（`UNKNOWN_HOST`）。
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { UNKNOWN_HOST, type HostFacts, type HostOs } from "../../src/frontend/ui/settings/host-os";
import { REPO_ROOT } from "./repo-root";

const GOLDEN = JSON.parse(readFileSync(resolve(REPO_ROOT, "tests/__fixtures__/host-facts.golden.json"), "utf8")) as Record<string, HostFacts>;

export function factsOn(os: HostOs): HostFacts {
  return os === "unknown" ? UNKNOWN_HOST : GOLDEN[os];
}
