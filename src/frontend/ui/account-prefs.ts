/**
 * **账号偏好**：本机 config.json 里 `accounts` 那一段 —— 「我这台 cc-monitor 起新会话默认用哪个号」
 * （`defaultName`）与「每个号默认用哪个模型」（`modelByAccount`）。本机、不跨机器同步。
 *
 * 从 `accounts.ts` 拆出来（审计 B §6 必须拆 4；守的要求）。
 * 写口改成按键补丁（`config.ts::patchConfig`）：每个写者只交 `accounts.<自己那一格>` 这一条路径，
 * 不再「读整份 → 改一个键 → 整份写回」（E §E1：两次读-改-写一交错，后写的整份盖掉先写的键）。
 */
import { loadConfig, patchConfig, removeAt, setAt } from "./config";
import { modelNameOk } from "./generated/judgment-rules";
import { copyText } from "./copy-table";

const CFG_KEY = "accounts";

/** 读本机默认账号名（config.json accounts.defaultName）。无则 null。 */
export async function getDefaultName(): Promise<string | null> {
  try {
    const cfg = (await loadConfig()) as Record<string, unknown>;
    const a = cfg[CFG_KEY];
    if (a && typeof a === "object") {
      const dn = (a as Record<string, unknown>).defaultName;
      if (typeof dn === "string" && dn) return dn;
    }
  } catch (e) {
    console.warn("getDefaultName failed:", e);
  }
  return null;
}

/** 写本机默认账号名。null = 清除（回退跟随 manifest）。只动 `accounts.defaultName` 这一条路径。 */
export async function setDefaultName(name: string | null): Promise<void> {
  await patchConfig([
    name === null ? removeAt([CFG_KEY, "defaultName"]) : setAt([CFG_KEY, "defaultName"], name),
  ]);
}

const MODEL_MAP_KEY = "modelByAccount";

/** F07：读某账号配置的默认模型偏好（config.json accounts.modelByAccount[name]）。无则 undefined。
 *  结构上是 `defaultName`（单值）的复数版——按账号名索引，同样是本机、不跨机器同步的偏好。 */
export async function getModelForAccount(name: string): Promise<string | undefined> {
  try {
    const cfg = (await loadConfig()) as Record<string, unknown>;
    const a = cfg[CFG_KEY];
    if (a && typeof a === "object") {
      const map = (a as Record<string, unknown>)[MODEL_MAP_KEY];
      if (map && typeof map === "object") {
        const v = (map as Record<string, unknown>)[name];
        if (typeof v === "string" && v) return v;
      }
    }
  } catch (e) {
    console.warn("getModelForAccount failed:", e);
  }
  return undefined;
}

/** 写某账号的模型偏好。`model === null` 清除该账号这一条（其余账号不受影响）。
 *
 *  Phase D 审计发现的阻塞项：校验必须在**写入点**做，不能只留给
 *  `MODEL_DIMENSION.apply()`（起会话时）——那样一个非法值一旦落盘，会让该账号**此后每一次**
 *  resume/新建/tmux resume 在 `buildLaunchPlan` 里统一 throw，用户只看到一堆"无法构造 resume
 *  命令"的 toast，且设置面板的输入框不会标出"当前值非法"，很难把两者联系起来。fail-closed：
 *  非法即 throw，调用方（UI）负责 catch 并提示，绝不静默落盘。 */
export async function setModelForAccount(name: string, model: string | null): Promise<void> {
  // 写入点先说一句（用户在设置里敲完就知道，不必等下次起会话）—— 但**不手抄规则**：
  // 读 monitor 从 `shell_quote_core::model_name_ok` 那组常量现生成的式子（`src/frontend/ui/generated/judgment-rules.ts`），
  // 两侧由共用金样逐条对。原先这里调 TS 自己那份 `isValidModelName`〔散文墓碑〕（会拒 `sonnet[1m]`、Bedrock / Vertex 名）。
  if (model && !modelNameOk(model)) {
    throw new Error(copyText("accounts.setModel.invalid", { model: JSON.stringify(model) }));
  }
  // 只动 `accounts.modelByAccount.<name>` 这一条路径：别的账号、`defaultName` 都不经这里。
  const path = [CFG_KEY, MODEL_MAP_KEY, name] as const;
  await patchConfig([model ? setAt(path, model) : removeAt(path)]);
}

// ------------------------------------------------------------ 带缓存的取数
