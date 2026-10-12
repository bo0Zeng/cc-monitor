//! 命令表 · 账号：`accounts-*` · `apikey-*` · `quota-read` · `quota-probe` · `rotation-*`。

use crate::stream::inbound::spec::{arg, both, out, CommandSpec, Run};
use crate::stream::inbound::LocalFiles;

/// 改账号库那几条（`accounts-*`）递给执行器的 key 表几口：写 key · 删号清那一行 · 回滚放回去 · 表在哪。
/// 那份表（上游选择自己的状态）的写口只从这里递出去，写经分派那一层递进来的写句柄。
fn with_account_keys<R>(
    own: &crate::platform::Own,
    f: impl FnOnce(&crate::faces::accounts_face::KeyDoor<'_>) -> R,
) -> R {
    use crate::accounts::upstream_select::file_face;
    let set = |a: &serde_json::Value| file_face::answer_set(a, own);
    let drop = |a: &serde_json::Value| file_face::answer_drop(a, own);
    let restore = |a: &serde_json::Value| file_face::answer_restore(a, own);
    f(&crate::faces::accounts_face::KeyDoor {
        set: &set,
        drop: &drop,
        restore: &restore,
        path: &file_face::machine_path,
    })
}

pub(super) const SPECS: &[CommandSpec] = &[
    CommandSpec {
        name: "quota-read",
        summary: "这台的额度账",
        codes: &[],
        fields: &[out("accounts", "每个号一条，按 `(agent, account)` 排：`agent` 路由第 1 段（哪一家）· `account` 路由第 2 段（哪个号"), out("candidates", "换号 · 加号那几处的候选 `{agent, account, kind}`：出过数的在前、没出过的在后，起会话时没说是哪个号的 `_` 不在里面（判断只住 `name_words::is_candidate`）；界面照它画，不另拼、不另滤"), out("detail", "只在 `unreadable` 时有：复制详情（时刻 · 机器 · 命令 · 码 · 原话；排法同失败应答），`reason` 那一句不带原话"), out("earliestReturn", "被拒 / 超额在兜的号里最早回来的那个 `{account, at}`；没有、或都说不出时刻 ⇒ `null`"), out("fiveHour", "「5h 那一格」写好的字：顶上那一格只在 `unreadable` 时有（`5h 读不到`）、否则 `null`；`accounts[]` 每个出过数的订阅号一格（`5h 41%` · 卡着的照语义位的字），按量号 ⇒ `null`；`unseen[]` 恒 `null`"), out("names", "号名与语义位名的字（核心一处写，出口照它画、不认码）：`{accounts: {码: 字}, slots: {码: 字}}`；`accounts` 只列与原名不同的号（起会话时没说是哪个号的 `_`），不在表里的号就叫它自己的名字；`slots` 列 `5h` · `7d`"), out("now", "这台此刻的 unix 秒（界面算「几分钟前看到的」「还有多久重置」都按这台的钟）；回包里每个时刻（`at` · `seenAt` · `resetsAt` · `fromResetsAt` · `since`）旁边有一格 `…Text`：出口按看的那一台的时区（请求的 `tz`）写好的字（当天 `HH:MM` · 当年 `MM-DD HH:MM` · 别的年带年），还没到的再有一格 `…RelText`（距今 `+1h50m` · `+3d`），界面照抄、不换算；`slots[]` 每格带写好的 `text` 与 `tone`（plain · fail · warn）"), out("path", "那份文件的绝对路径（家推不出来时 `null`）"), out("reason", "只在 `unreadable` 时有：为什么读不出来；其余 `null`"), out("state", "`\"present\"`（读得懂）· `\"absent\"`（还没看到过任何回包）· `\"unreadable\"`（文件读不出来 / 家推不出来）"), out("unseen", "账号库里有、额度账上从没出过数的号：`{agent, account, kind, login, subId?, rows, warm}`（几格同下）。出过数的与没出过的每号都带 `rows`：那一段写好的几行 `[[{text, tone}]]`（首行 名 · 类型 · 标签；其余每行 键 · 值 · ↻ · 距今；采样那一格只写几点，哪台采的由出口写）· `warm`：开窗那一判 `{act: send|wait, at?, text}`（quota-warm 照它发一句 / 睡到 `at`）"), out("text", "读不出（`unreadable`）· 一个号都没有 ⇒ 那一句（`--text` 拼字时放最前）；否则 `null`"), out("usableNow", "此刻发得出去的号（路由第 2 段）：登录拿得到、不是被拒 / 超额在兜（快满 · 数旧 · 没采样 · 上一窗已过都算）")],
        takes_input: false,
        run: Run::BlockingData(|r| crate::faces::rotation_face::answer_quota_read(&r.tz).map(Some)),
    },
    // 用某个号查一次额度（帧面宿主 `faces/quota_probe_face.rs`）：起官方客户端、等它 ⇒ 阻塞档，总期限登记在 `caps.rs`。
    CommandSpec {
        name: "quota-probe",
        summary: "用某个号查一次额度",
        codes: &[
            "bad_args",
            "child_timed_out",
            "failed",
            "io_failed",
            "not_found",
            "unsupported",
        ],
        fields: &[both("account", "账号库里的号（配置目录末段；账号 0 是 `0`）"), both("agent", "路由第 1 段：哪一家"), out("from", "恒 `\"usage\"`"), out("now", "这台此刻的 unix 秒"), out("path", "额度账那份文件的绝对路径"), out("reason", "只在 `unreadable` 时有：哪一处读不懂（英文诊断）"), out("state", "`\"read\"`（读得懂、已记进额度账）· `\"unreadable\"`（输出对不上：**不猜、不写账**）"), out("windows", "读到的窗口（形状同 `reading.windows`）；`unreadable` ⇒ `[]`")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            crate::faces::quota_probe_face::answer_probe(&r.args, own).map(Some)
        }),
    },
    // 换号那一族（帧面宿主 `faces/rotation_face.rs`，`轮换-积木.md` §5）：配置表读 / 存 / 改名 / 删 / 设为本机默认 · 策略读 / 装 / 删 / 试跑 ·
    //   会话读 / 打开配置 / 改副本 / 存 / 还原 / 手动调整 · 推演 · 现在就换。
    //   同步文件 I/O（含沙箱里跑策略，一判至多 50 ms）⇒ 阻塞档；「现在就换」里重启换那一半要等 `session-restart` ⇒ 异步、失败可带码。
    CommandSpec {
        name: "rotation-configs-read",
        summary: "这台的轮换配置表",
        codes: &[],
        fields: &[out("configs", "每份一项（本机默认在最前、其余按名字）：`{id, name, strategy, accounts, params, acctInit, parked, tz?, rev, updatedAt, savedBy, text, strategyName, inUse, isDefault}`；`text` 是后端写好的描述（策略的 `describe` 片段写成字），`inUse` ＝ 开着它的会话"), out("defaultConfig", "本机默认那份的 id"), arg("lang", "可缺：策略自己的字写哪种语言（`zh` 缺省 · `en`）"), out("detail", "只在 `unreadable` 时有：复制详情"), out("path", "那份文件的绝对路径（家推不出 ⇒ `null`）"), out("reason", "只在 `unreadable` 时有"), out("state", "`\"present\"` · `\"absent\"`（没动过：内置三份）· `\"unreadable\"`")],
        takes_input: true,
        run: Run::BlockingData(|r| crate::faces::rotation_face::answer_configs_read(&r.args, &r.tz).map(Some)),
    },
    CommandSpec {
        name: "rotation-config-save",
        summary: "存一份轮换配置（新建或覆盖）",
        codes: &["bad_args", "io_failed", "no_such_config"],
        fields: &[arg("config", "整份内容 `{name, strategy, accounts: [{start: true} | {rest: true} | {id}, params], params, acctInit, parked?}`；参数照那份策略的声明逐格校验，缺的格补齐"), out("errors", "只在 `refused` 时有：`[{cell, code, said}]`（`name` · `strategy` · `accounts[i]` · `params.<k>` · `accounts[i].<k>` · `acctInit.<k>`，时段细到 `[j].at` / `[j].line`）"), arg("force", "可缺：`true` ⇒ `ifRev` 对不上也覆盖"), both("id", "入参：改哪份；缺 ＝ 新建。应答：写成的那份"), arg("ifRev", "可缺：改之前读到的 `rev`；对不上 ⇒ `conflict`"), both("rev", "写成的版本 · `conflict` 时此刻的版本"), out("savedAtText", "只在 `saved` 时有：写成那一刻的钟面 `HH:MM`"), out("state", "`\"saved\"` · `\"refused\"` · `\"conflict\"`"), out("theirs", "只在 `conflict` 时有：别处那一版跟这一份差哪几格 `[{acct?, k, from, to, text}]`")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            crate::faces::rotation_face::answer_config_save(&r.args, &r.tz, own).map(Some)
        }),
    },
    CommandSpec {
        name: "rotation-config-rename",
        summary: "给一份轮换配置改名",
        codes: &["bad_args", "io_failed", "no_such_config"],
        fields: &[out("errors", "同 `rotation-config-save`"), both("id", "哪份"), arg("ifRev", "同 `rotation-config-save`"), arg("name", "新名字（1–24 字，本机不重名）"), out("rev", "同 `rotation-config-save`"), out("savedAtText", "同 `rotation-config-save`"), out("state", "同 `rotation-config-save`"), out("theirs", "同 `rotation-config-save`（改名撞版本时为空）")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            crate::faces::rotation_face::answer_config_rename(&r.args, &r.tz, own).map(Some)
        }),
    },
    CommandSpec {
        name: "rotation-config-delete",
        summary: "删一份轮换配置",
        codes: &["bad_args", "io_failed", "no_such_config"],
        fields: &[arg("id", "哪份"), out("inUse", "只在 `refused` 时有：开着它的会话"), out("isDefault", "只在 `refused` 时有：是本机默认"), out("said", "只在 `refused` 时有：那一句"), out("state", "`\"done\"` · `\"refused\"`（有会话开着 · 是本机默认）")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            crate::faces::rotation_face::answer_config_delete(&r.args, own).map(Some)
        }),
    },
    CommandSpec {
        name: "rotation-default-set",
        summary: "设这台的本机默认配置",
        codes: &["bad_args", "io_failed", "no_such_config"],
        fields: &[arg("id", "设为本机默认的那份（已开着的会话不跟着换）"), out("state", "`\"done\"`")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            crate::faces::rotation_face::answer_default_set(&r.args, own).map(Some)
        }),
    },
    CommandSpec {
        name: "rotation-strategies-read",
        summary: "这台加载的轮换策略",
        codes: &[],
        fields: &[arg("lang", "可缺：参数名 · 选项 · 灰字写哪种语言（`zh` 缺省 · `en`）"), out("lib", "共用小函数库 `{source}`（随引擎走，只读）"), out("strategies", "每份一项（内置在前、本机按文件名）：`{id, name, builtin, path?, ordered, text, params: [Decl＋字], acctParams: [Decl＋字], source, error?: {kind, line?, col?, msg, text}, usedBy}`；`Decl＋字` ＝ `{k, type, init, of?, min?, max?, label, hint?, options?: [{v, label, hint?}]}`，界面只照它画格")],
        takes_input: true,
        run: Run::BlockingData(|r| crate::faces::rotation_face::answer_strategies_read(&r.args, &r.tz).map(Some)),
    },
    CommandSpec {
        name: "rotation-strategy-add",
        summary: "装一份轮换策略进本机策略目录",
        codes: &["bad_args", "io_failed"],
        fields: &[out("error", "只在 `refused` 时有：读不懂那一处 `{kind, line?, col?, msg, text}`"), both("id", "文件名去 `.rhai`（路由段闸那一套字符，至多 32 字，不以 `@` 开头；与内置同 id ⇒ 不收）"), out("said", "只在 `refused` 时有：那一句"), arg("source", "策略文件的全文（至多 64 KiB）"), out("state", "`\"done\"` · `\"refused\"`"), out("usedBy", "删不了时：用它的配置")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            crate::faces::rotation_face::answer_strategy_add(&r.args, own).map(Some)
        }),
    },
    CommandSpec {
        name: "rotation-strategy-delete",
        summary: "删本机的一份轮换策略",
        codes: &["bad_args", "io_failed", "no_such_strategy"],
        fields: &[out("error", "同 `rotation-strategy-add`"), both("id", "哪份（内置的删不了）"), out("said", "同 `rotation-strategy-add`"), out("state", "`\"done\"` · `\"refused\"`（内置的 · 有配置在用）"), out("usedBy", "只在有配置在用时有：那几份的 id")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            crate::faces::rotation_face::answer_strategy_delete(&r.args, own).map(Some)
        }),
    },
    CommandSpec {
        name: "rotation-strategy-try",
        summary: "按此刻的事实试跑一份策略（不写盘不换号）",
        codes: &["bad_args", "no_such_config", "no_such_session"],
        fields: &[arg("config", "跑哪份配置（与 `draft` · `sid` 三选一）"), arg("draft", "草稿：一份完整配置内容"), arg("id", "可缺：用哪份已加载的策略（缺 ⇒ 配置写的那份）"), arg("lang", "可缺：字写哪种语言"), out("maxOps", "一判的操作数上限"), out("memo", "这一判回的 `@memo`（没写 ⇒ 缺）"), out("ops", "这一判用了几次操作"), out("pick", "会挑谁（停发 ⇒ 缺）"), out("rows", "有效优先级的先后 `[{account, p, why, back?, backText?}]`：`p` 为 `null` ＝ 此刻不挑"), arg("sid", "这个会话此刻那一份（从它此刻的号起，手动调整 · 备忘照叠）"), arg("source", "可缺：现编的一份策略全文（不装，只跑）"), out("strategyError", "出错 / 超时 / 形状不对时：为什么（这一判照内置策略 1 排）")],
        takes_input: true,
        run: Run::BlockingData(|r| crate::faces::rotation_face::answer_strategy_try(&r.args, &r.tz).map(Some)),
    },
    CommandSpec {
        name: "rotation-plan",
        summary: "推演：一份配置接下来会怎么换（都不写）",
        codes: &["bad_args", "no_such_config", "no_such_session"],
        fields: &[arg("config", "跑哪份配置（与 `draft` · `sid` 三选一；没有会话 ⇒ 从起会话的号起）"), arg("draft", "草稿：一份完整配置内容（带 `sid` 时 ＝ 这个会话的草稿）"), out("names", "号名与语义位名的字"), out("from", "视窗起（此刻之前那一截的起点）"), out("grid", "刻度 `[{at, atText, label?}]`，按看的那一台的时区对齐"), arg("lang", "可缺：字写哪种语言"), out("now", "这台此刻的 unix 秒"), out("nowText", "`now` 写好的字"), out("plan", "`[{from, fromText, to, toText, account, why?, est?}]`：`account` 为 `null` ＝ 停发；`why` ＝ 那一段开头为什么换过来；此刻之后的都是估的（`est`）"), out("rows", "合图每个号一行 `{account, bars: [{from, to, k, est?}], resets: [{w, at, atText}]}`：`k` ＝ `use` · `line`（策略声明了 `line` 且到线才出）· `off` · `refused` · `full` · `overage` · `login` · `skip`"), arg("sid", "一个会话：此刻那一份，从它此刻的号起；此刻之前那一截照换号记录"), arg("source", "可缺：现编的一份策略全文（不装，只跑）"), out("strategyError", "此刻这一判出错时：为什么"), out("until", "视窗止"), both("view", "`12h`（缺省：前 3h · 后 9h）· `24h`（前 6h · 后 18h）")],
        takes_input: true,
        run: Run::BlockingData(|r| crate::faces::rotation_face::answer_plan(&r.args, &r.tz).map(Some)),
    },
    CommandSpec {
        name: "rotation-session-read",
        summary: "一批会话的轮换",
        codes: &["bad_args", "failed"],
        fields: &[out("detail", "同 `rotation-configs-read`"), arg("lang", "可缺：字写哪种语言"), out("names", "号名与语义位名的字：`{accounts: {码: 字}, slots: {码: 字}}`"), out("now", "这台此刻的 unix 秒；回包里每个时刻旁有 `…Text`"), out("reason", "同 `rotation-configs-read`"), out("sessions", "每个 sid 一份：`{state: \"present\", agent, start, current, since, history: [{at, from, to, leave, pick?, text}], inPlace, config: {id, name, rev, dirty, newer?: {rev, at, by, text}, diff?}, text, strategy: {id, name, ordered}, values: {params, accounts: [{who, params}], acctInit}, rows: [{account, why, skip?, back?, backText?, alsoUsed, facts, fromRest?}], hot?, blocked?: {earliest?, text}, strategyError?, quota}` · 中转没见过 ⇒ `{state: \"absent\", inPlace}`；`rows` 是有效优先级的先后，第一行在用"), arg("sids", "会话 id 的数组"), out("state", "那份文件的三态（同 `rotation-configs-read`）")],
        takes_input: true,
        run: Run::BlockingData(|r| crate::faces::rotation_face::answer_session_read(&r.args, &r.tz).map(Some)),
    },
    CommandSpec {
        name: "rotation-session-open",
        summary: "会话打开一份配置",
        codes: &["bad_args", "io_failed", "no_such_config"],
        fields: &[arg("config", "打开哪份"), out("diff", "只在 `dirty` 时有：本会话副本改了哪几格"), arg("discard", "可缺：`true` ⇒ 丢掉本会话没存的改动再打开"), arg("sid", "哪个会话"), out("state", "`\"opened\"`（手动调整与备忘清掉）· `\"dirty\"`（副本有没存的改动，先问）")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            crate::faces::rotation_face::answer_session_open(&r.args, own).map(Some)
        }),
    },
    CommandSpec {
        name: "rotation-session-edit",
        summary: "改会话打开的那份配置的副本",
        codes: &["bad_args", "io_failed", "no_such_session"],
        fields: &[arg("add", "可缺：加进来的号（新的一行抄 `acctInit`）"), out("diff", "副本跟原样差哪几格 `[{acct?, k, from, to, text}]`"), out("dirty", "副本改没改过（改回原样 ⇒ `false`，副本丢掉）"), arg("drop", "可缺：不参与的号（删那一行）"), out("errors", "收不下的格 `[{cell, code, said}]`（那几格不改）"), arg("lang", "可缺：字写哪种语言"), arg("set", "`[{acct?, k, v}]`：`acct` 缺 ＝ 整份参数 · `@start` · `@rest` · `@init`（新加的号缺省）· 号 id（`rest` 底下的号 ⇒ 给它单写一行）"), arg("sid", "哪个会话"), arg("strategy", "可缺：换一份策略（旧参数收进 `parked`，同名同类型的照搬）")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            crate::faces::rotation_face::answer_session_edit(&r.args, own).map(Some)
        }),
    },
    CommandSpec {
        name: "rotation-session-save",
        summary: "存会话的副本（写回或另存为）",
        codes: &["bad_args", "io_failed"],
        fields: &[arg("as", "可缺：`{name?}` ⇒ 另存为（新 id，本会话改开它；不起名 ⇒ `配置 N`）"), out("errors", "只在 `refused` 时有"), arg("force", "可缺：`true` ⇒ 撞版本也覆盖"), out("id", "只在 `saved` 时有：写成的那份"), arg("ifRev", "打开时的 `rev`；对不上且没 `force` ⇒ `conflict`"), out("mine", "只在 `conflict` 时有：本会话的改动"), out("rev", "写成的版本 · `conflict` 时此刻的版本"), arg("sid", "哪个会话"), out("state", "`\"saved\"` · `\"conflict\"` · `\"refused\"`"), out("theirs", "只在 `conflict` 时有：别处那一版跟本会话的差")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            crate::faces::rotation_face::answer_session_save(&r.args, &r.tz, own).map(Some)
        }),
    },
    CommandSpec {
        name: "rotation-session-revert",
        summary: "丢掉会话副本的改动",
        codes: &["bad_args", "io_failed", "no_such_session"],
        fields: &[arg("sid", "哪个会话"), out("state", "`\"done\"`"), arg("toLatest", "可缺：载入新版本（同一件事：副本丢掉、版本跟上）")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            crate::faces::rotation_face::answer_session_revert(&r.args, own).map(Some)
        }),
    },
    CommandSpec {
        name: "rotation-session-set",
        summary: "会话的手动调整（拖一行 · 刷新策略）",
        codes: &["bad_args", "io_failed", "no_such_session"],
        fields: &[arg("place", "`{account, at}`：把那个号放到第 `at` 位（0 起；`0` ＝ 现在就换过去，同 `rotation-switch` 热换，不出任何提示）"), arg("refresh", "`true` ⇒ 清掉全部手动调整与备忘，照策略重算"), arg("sid", "哪个会话"), out("state", "`\"done\"` · `\"skipped\"` · `\"failed\"`"), out("why", "没换成时的码（同 `rotation-switch` 的 `hot` 那一格）")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            crate::faces::rotation_face::answer_session_set(&r.args, own).map(Some)
        }),
    },
    CommandSpec {
        name: "rotation-switch",
        summary: "现在就换",
        codes: &["bad_args", "failed"],
        fields: &[arg("mode", "`hot`（不重启，下一发起就走它）· `restart`（换号重启）"), both("sessions", "`hot`：会话 id；`restart`：每项是 `session-restart` 的入参（不带 `account`）；应答里是逐个结果"), arg("target", "换到哪个号")],
        takes_input: true,
        run: Run::AsyncOwn(|r, own| {
            Box::pin(async move {
                crate::faces::rotation_switch_face::answer_switch(r.args, r.until, &LocalFiles, own)
                    .await
                    .map(Some)
                    .map_err(|(c, m)| (c.to_string(), m))
            })
        }),
    },
    // **上游选择**那份凭据文件在**这台机器上**的读写口 —— 上游选择自己的状态，
    //   不是用户文件（判清全文）⇒ 写经 `Own`（启动时造、经分派递进来），
    //   **只从这里一扇门进来**。远端账号页配的 key 从此落在会话跑的那台机器上。
    //   ⚠ 明文只在 `apikey-key-set` 的 `args.key` 里（帧面：长连接入方向；派生 CLI 面：stdin），
    //     **不进 argv / env / 日志**；两条的应答都只有掩码。
    //   ⚠ 两条都在阻塞档：同步文件 I/O，开跑之后打不断。
    //   ⚠ 它们**不起中转**、中转那几条也**不碰凭据**（「账号就账号, 中转就中转」）。
    CommandSpec {
        name: "apikey-key-set",
        summary: "给一个账号写 key，写完读回",
        codes: &["bad_args", "bad_file", "io_failed"],
        // 入 `configDir`（账号 id 由后端推）· 出 `account`（推出来的那个）。
        fields: &[out("account", "推出来的账号 id"), both("baseUrl", "入（可选）：这个账号的第三方端点"), arg("configDir", "这个号的账号目录"), arg("key", "明文"), out("masked", "写完**再读一遍**、这一行 key 的掩码（盘上的事实）"), out("path", "那份文件的绝对路径")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            crate::accounts::upstream_select::file_face::answer_set(&r.args, own).map(Some)
        }),
    },
    // 出成品：`{meta, accounts, notice}`，并上这台机器自己那份 apikey 表；`agent` 随请求带（必填）。
    CommandSpec {
        name: "accounts-list",
        summary: "账号清单",
        codes: &["bad_args", "too_large"],
        fields: &[out("accounts", "每账号一个对象 `{name, email, configDir, isDefault, mode, exists, loggedIn, authKind, authReady, selectable, keyMasked, baseUrl, badge, color}`；`selectable` ＝ 能拿来起会话 / 选为默认（`acct_core::account_selectable`）；`color` ＝ 号色（`--acct-c0..c7` 第几格，账号库里的事实：加号时分第一个空格、存在清单里，`Manifest::with_colors`），界面只画它"), arg("agent", "必填：这次起会话的是哪一家（适配器 id；空串 ⇒ 默认那一家，注册表里没有 ⇒ `bad_args`）"), out("meta", "`{enabled, acctsDir, manifestPath, updatedAt, sharedStore, count, error, unsupported, nextDefault, effectiveDefault, home}`；`effectiveDefault` ＝ 这台的默认号（标了的第一个，没标 ⇒ 第一个；`acct_core::effective_default`）"), out("notice", "「能用但有缺」的那一句（几件都有 ⇒ 连成一句）：启用了却一个账号 0 都没有（写清单的那一侧旧到不认账号 0）· 这台的 API key 凭据文件不是只给本人（宽在哪 ＋ 怎么修；查不出 ⇒ 为什么）；都没有 ⇒ `null`")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args, &r.tz)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 停 / 重启 / 更新 / 卸载这台的 cc-monitor 之前会打断什么（界面问那台 ＋ 问本机数通往那台的转发，并排画）。只读。
    CommandSpec {
        name: "machine-interrupts",
        summary: "停 / 重启 / 更新 / 卸载这台的 cc-monitor 之前，会打断什么",
        codes: &["bad_args"],
        fields: &[arg("appExit", "可缺；`true` ⇒ 问的是「重启 / 退出 cc-monitor」：这台后端选了随它退出一起停才数会话与账上全部转发，否则全零"), out("forwards", "见 `machine`"), out("liveStreams", "这台活着的会话数（停的那几秒 cc-monitor 里它们不更新）"), arg("machine", "可缺"), out("relayedMaybe", "活着、说不清走不走中转的几个（环境这一刻读不出 / agent 自己的设置可能压过它）"), out("relayedSessions", "这台的活会话里经本机中转走请求的几个（停了就断，直到再启动）")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args, &r.tz)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    CommandSpec {
        name: "accounts-sessions",
        summary: "正在跑的会话各属哪个账号",
        codes: &["too_large"],
        fields: &[out("lines", "每条运行中会话一行 `{pid, sessionId, cwd, configDir, account, bare, alive, viaRelay}`；`account:null` ＝ 查不到（不猜）")],
        takes_input: false,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args, &r.tz)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 换号前的信任预检：`configDir`（缺席 / null = 账号 0）＋ `cwd` → `{trusted, known}`。
    //   同族同档（读一份 manifest ＋ 一份 `.claude.json` ⇒ 阻塞档）、同一个只读宿主。
    CommandSpec {
        name: "accounts-trust",
        summary: "换号前的信任预检",
        codes: &[
            "bad_args",
            "failed",
            "manifest_unavailable",
            "no_home",
            "unknown_config_dir",
            "unsafe_config_dir",
        ],
        fields: &[arg("configDir", "目标账号的 config dir（必须逐字 ∈ manifest，否则 `unknown_config_dir`）；**缺席或 `null` = 账号 0**（读 agent 自己那份用户级配置，不收路径）"), arg("cwd", "要预检的工作目录（必填）"), out("known", "这个账号的用户级配置里有该目录的记录（`false` ⇒ 首次进入，大概率会弹确认）"), out("trusted", "这个账号接受过该目录的信任对话框")],
        takes_input: true,
        run: Run::Blocking(|r| {
            crate::faces::read_face::answer(&r.cmd, &r.args, &r.tz)
                .map(Some)
                .map_err(|(c, m)| (c.to_string(), m))
        }),
    },
    // 改账号库的那几条：本体 `accounts/manage/`，帧面宿主 `faces/accounts_face.rs`（建 API 号写 key · 账号表变了改配置文件、重生成别名）。
    //   写经这台的文件管理面（[`LocalFiles`]）；同步文件 I/O ⇒ 阻塞档。`accounts-verify` / `accounts-login-cmd` 只读。
    CommandSpec {
        name: "accounts-init",
        summary: "建账号库",
        codes: &[
            "bad_args",
            "io_failed",
            "not_enabled",
            "refused",
            "unsupported",
        ],
        fields: &[out("aliasNames", "这个号会拿到的别名名字（`accounts-init` / `accounts-add`，预演时也给；别的命令 ⇒ `[]`）"), out("aliases", "改配置文件（`profiles.toml`）的结局，恰一条 `{path, changed, added, removed, skipped, note}`：加了（基于 `cc` / `cct`、只写自己的号）/ 删了（合下来用这个号的全部段）/ 名字被占跳过的那几段；配置文件有写错的地方 ⇒ 不动、`note` 说一句"), out("applied", "这一趟真改了盘没有"), out("backup", "这一趟留的备份（`~/.cc-monitor/accounts/.backup-<这一段>`，回滚用它）；没改动 ⇒ `null`"), arg("dryRun", "可缺席的布尔：真 ⇒ 只算不做，`steps` 是将要做的那几步"), arg("name", "默认号的名字：过 `shell_quote_core::account_name_ok`（与 `ccm … --account` 同一条），`0` 是保留名"), out("notes", "提示（不挡这一趟），比如共享库里还没有可共享的项"), out("steps", "做了（预演时：将要做）的每一步，一句一行")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            with_account_keys(own, |keys| {
                crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, keys)
            })
            .map(Some)
        }),
    },
    CommandSpec {
        name: "accounts-add",
        summary: "新建一个号",
        codes: &[
            "bad_args",
            "io_failed",
            "not_enabled",
            "refused",
            "unsupported",
        ],
        fields: &[out("account", "建出来的那个号：`{name, configDir}`"), out("aliasNames", "同 `accounts-init`"), out("aliases", "同 `accounts-init`"), out("applied", "同 `accounts-init`"), out("backup", "同 `accounts-init`"), arg("baseUrl", "只 API 号：上游地址（缺席 = 默认上游）与 key 明文"), out("configDir", "那个号的配置目录（`account` 里）"), arg("credFile", "只订阅号：导入哪一份凭据（家目录底下的绝对路径或 `~/…`；是链接 / 空文件 / 不在 ⇒ `refused`）"), arg("dryRun", "可缺席的布尔：真 ⇒ 只算不做，`steps` 是将要做的那几步"), arg("isDefault", "可缺席的布尔：真 ⇒ 建好后它是默认号（`ccm` 不带 `--account` 时用它）"), arg("key", "只 API 号：上游地址（缺席 = 默认上游）与 key 明文"), out("keyMasked", "API 号：写进 apikey 表之后的掩码；号建好了 key 却没写进去时那一句（界面据此让人在那一行重填）"), out("keyProblem", "API 号：写进 apikey 表之后的掩码；号建好了 key 却没写进去时那一句（界面据此让人在那一行重填）"), arg("kind", "`\"subscription\"`（订阅号）或 `\"api-key\"`（API 号，清单里写 `authKind: \"api-key\"`）"), out("loginCmd", "订阅号没导入凭据时：在终端里跑这一行登录（`'<家>/.cc-monitor/bin/ccm' -- --account '<名>'`，agent 自己的登录界面）；否则 `null`"), arg("name", "同 `accounts-init`；已有同名号 / 同名目录 ⇒ `refused`"), out("notes", "同 `accounts-init`"), out("steps", "同 `accounts-init`")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            with_account_keys(own, |keys| {
                crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, keys)
            })
            .map(Some)
        }),
    },
    CommandSpec {
        name: "accounts-remove",
        summary: "删一个号",
        codes: &[
            "bad_args",
            "io_failed",
            "not_enabled",
            "refused",
            "unsupported",
        ],
        fields: &[out("aliases", "同 `accounts-init`；参数指向它的别名随之删掉（`removed`）"), out("applied", "同 `accounts-init`；参数指向它的别名随之删掉（`removed`）"), out("backup", "同 `accounts-init`；参数指向它的别名随之删掉（`removed`）"), arg("dryRun", "可缺席的布尔：真 ⇒ 只算不做，`steps` 是将要做的那几步"), arg("force", "可缺席的布尔：删的是默认号时必须给真（剩下的第一个号接着当默认）"), arg("name", "要删的号；`0` · 不认识的号 ⇒ `refused`"), out("notes", "同 `accounts-init`；参数指向它的别名随之删掉（`removed`）"), out("steps", "同 `accounts-init`；参数指向它的别名随之删掉（`removed`）")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            with_account_keys(own, |keys| {
                crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, keys)
            })
            .map(Some)
        }),
    },
    CommandSpec {
        name: "accounts-set-default",
        summary: "设默认号",
        codes: &[
            "bad_args",
            "io_failed",
            "not_enabled",
            "refused",
            "unsupported",
        ],
        fields: &[out("aliases", "同 `accounts-init`；已经是默认 ⇒ `applied: false`、一个字节不写"), out("applied", "同 `accounts-init`；已经是默认 ⇒ `applied: false`、一个字节不写"), out("backup", "同 `accounts-init`；已经是默认 ⇒ `applied: false`、一个字节不写"), arg("dryRun", "可缺席的布尔：真 ⇒ 只算不做，`steps` 是将要做的那几步"), arg("name", "要当默认的号（不认识 ⇒ `refused`）"), out("notes", "同 `accounts-init`；已经是默认 ⇒ `applied: false`、一个字节不写"), out("steps", "同 `accounts-init`；已经是默认 ⇒ `applied: false`、一个字节不写")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            with_account_keys(own, |keys| {
                crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, keys)
            })
            .map(Some)
        }),
    },
    CommandSpec {
        name: "accounts-repair",
        summary: "修复账号库（幂等）",
        codes: &[
            "bad_args",
            "io_failed",
            "not_enabled",
            "refused",
            "unsupported",
        ],
        fields: &[out("aliases", "同 `accounts-init`；再跑一次 ⇒ `applied: false`、`backup: null`"), out("applied", "同 `accounts-init`；再跑一次 ⇒ `applied: false`、`backup: null`"), out("backup", "同 `accounts-init`；再跑一次 ⇒ `applied: false`、`backup: null`"), arg("dryRun", "可缺席的布尔：真 ⇒ 只算不做，`steps` 是将要做的那几步"), out("notes", "同 `accounts-init`；再跑一次 ⇒ `applied: false`、`backup: null`"), out("steps", "同 `accounts-init`；再跑一次 ⇒ `applied: false`、`backup: null`")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            with_account_keys(own, |keys| {
                crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, keys)
            })
            .map(Some)
        }),
    },
    CommandSpec {
        name: "accounts-rollback",
        summary: "按一份备份还原",
        codes: &[
            "bad_args",
            "io_failed",
            "not_enabled",
            "refused",
            "unsupported",
        ],
        fields: &[out("aliases", "同 `accounts-init`；`notes` 里是撤销清单里认不出、跳过了的行；回滚前后账号表里消失 / 重新出现的号照删号 / 建号改别名"), out("applied", "同 `accounts-init`；`notes` 里是撤销清单里认不出、跳过了的行；回滚前后账号表里消失 / 重新出现的号照删号 / 建号改别名"), both("backup", "入：用哪一份（`.backup-` 后面那一段，只许 `[0-9A-Za-z._-]`、不含 `..`）；缺席 ⇒ 最近一份还没还原过的"), arg("dryRun", "可缺席的布尔：真 ⇒ 只算不做，`steps` 是将要做的那几步"), out("notes", "同 `accounts-init`；`notes` 里是撤销清单里认不出、跳过了的行；回滚前后账号表里消失 / 重新出现的号照删号 / 建号改别名"), out("steps", "同 `accounts-init`；`notes` 里是撤销清单里认不出、跳过了的行；回滚前后账号表里消失 / 重新出现的号照删号 / 建号改别名")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            with_account_keys(own, |keys| {
                crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, keys)
            })
            .map(Some)
        }),
    },
    CommandSpec {
        name: "accounts-verify",
        summary: "核对账号库",
        codes: &["bad_args", "io_failed", "unsupported"],
        fields: &[out("account", "说的是哪个号；全局那几条 ⇒ `null`"), out("checks", "每条 `{level, account, text}`"), out("fails", "`fail`"), out("level", "`ok` · `warn` · `fail` · `skip`"), out("pass", "没有一条 `fail`"), out("text", "给人看的那一句"), out("warns", "`warn` 各几条")],
        takes_input: false,
        run: Run::BlockingOwn(|r, own| {
            with_account_keys(own, |keys| {
                crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, keys)
            })
            .map(Some)
        }),
    },
    CommandSpec {
        name: "accounts-login-cmd",
        summary: "在终端里登录一个号的那一行",
        codes: &["bad_args", "io_failed", "refused", "unsupported"],
        fields: &[out("cmd", "那一行：这台的 `ccm` 带 `--account` 起 agent（它自己的登录界面）；值一律经唯一的 quote（`shell_quote_core::posix_quote`）"), arg("name", "清单里的一个号（不认识 ⇒ `refused`）")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            with_account_keys(own, |keys| {
                crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, keys)
            })
            .map(Some)
        }),
    },
    // 各号共用的用户级 MCP（本体 `accounts/manage/mcp_share_exec.rs`）：读各号的配置文件、只改那一个键、写回共享集合 ⇒ 阻塞档。
    //   成品只有名字与号名，不带定义里的任何值。
    CommandSpec {
        name: "accounts-mcp-read",
        summary: "这台各账号共用的用户级 MCP 此刻的样子",
        codes: &["bad_args", "io_failed", "refused"],
        fields: &[out("changed", "这一趟改写了哪几个号（这一条恒空）"), out("choices", "每一版 `{from, holders, gone}`"), out("conflicts", "两边都改了、等用户挑的那几条：每条 `{name, choices}`"), out("enabled", "这台有没有账号库（没有 ⇒ 不做同步，其余几格为空）"), out("from", "挑这一版时交回 `accounts-mcp-pick` 的 `from`：`null` = 共享的那一版；号名 = 那个号里的那一版"), out("gone", "这一版是「没有这一条」（在 cc-monitor 里删过）"), out("holders", "此刻是这一版的那几个号"), out("name", "那一条的名字"), out("notes", "提示：某个号的配置读不出来（这一趟不同步它）· 没写进去"), out("servers", "共享集合里的名字（排好序）"), out("sync", "各号之间在不在同步（用户停了 ⇒ `false`：各号各管各的、`conflicts` 恒空；见 `accounts-mcp-sync`）")],
        takes_input: false,
        run: Run::BlockingOwn(|r, own| {
            with_account_keys(own, |keys| {
                crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, keys)
            })
            .map(Some)
        }),
    },
    CommandSpec {
        name: "accounts-mcp-remove",
        summary: "从各账号共用的用户级 MCP 里删一条",
        codes: &["bad_args", "io_failed", "not_found", "refused"],
        fields: &[out("changed", "同 `accounts-mcp-read`；`changed` = 这一趟撤掉它的那几个号"), out("choices", "同 `accounts-mcp-read`"), out("conflicts", "同 `accounts-mcp-read`；`changed` = 这一趟撤掉它的那几个号"), out("enabled", "同 `accounts-mcp-read`；`changed` = 这一趟撤掉它的那几个号"), out("from", "同 `accounts-mcp-read`"), out("gone", "同 `accounts-mcp-read`"), out("holders", "同 `accounts-mcp-read`"), arg("name", "要删的那一条（共享集合里与哪个号里都没有 ⇒ `not_found`）"), out("notes", "同 `accounts-mcp-read`；`changed` = 这一趟撤掉它的那几个号"), out("servers", "同 `accounts-mcp-read`；`changed` = 这一趟撤掉它的那几个号"), out("sync", "同 `accounts-mcp-read`；`changed` = 这一趟撤掉它的那几个号")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            with_account_keys(own, |keys| {
                crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, keys)
            })
            .map(Some)
        }),
    },
    CommandSpec {
        name: "accounts-mcp-pick",
        summary: "两边都改了的那一条用哪一版",
        codes: &["bad_args", "io_failed", "not_found", "refused"],
        fields: &[out("changed", "同 `accounts-mcp-read`；停着同步时拒（`refused`）"), out("choices", "同 `accounts-mcp-read`；停着同步时拒（`refused`）"), out("conflicts", "同 `accounts-mcp-read`；停着同步时拒（`refused`）"), out("enabled", "同 `accounts-mcp-read`；停着同步时拒（`refused`）"), arg("from", "→ 那个号里此刻的那一版；缺席 / `null` = 共享的那一版（共享集合里已经删了 ⇒ 删）"), out("gone", "同 `accounts-mcp-read`；停着同步时拒（`refused`）"), out("holders", "同 `accounts-mcp-read`；停着同步时拒（`refused`）"), arg("name", "那一条"), out("notes", "同 `accounts-mcp-read`；停着同步时拒（`refused`）"), out("servers", "同 `accounts-mcp-read`；停着同步时拒（`refused`）"), out("sync", "同 `accounts-mcp-read`；停着同步时拒（`refused`）")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            with_account_keys(own, |keys| {
                crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, keys)
            })
            .map(Some)
        }),
    },
    // 停 / 开各号之间的同步（只改共享集合那份文件里那一格；开回来那一刻同步一趟）。
    CommandSpec {
        name: "accounts-mcp-sync",
        summary: "停 / 开各账号之间同步用户级 MCP",
        codes: &["bad_args", "io_failed", "refused"],
        fields: &[out("changed", "同 `accounts-mcp-read`；开回来那一趟 `changed` = 同步改写了的那几个号"), out("choices", "同 `accounts-mcp-read`"), out("conflicts", "同 `accounts-mcp-read`；开回来那一趟 `changed` = 同步改写了的那几个号"), out("enabled", "同 `accounts-mcp-read`；开回来那一趟 `changed` = 同步改写了的那几个号"), out("from", "同 `accounts-mcp-read`"), out("gone", "同 `accounts-mcp-read`"), out("holders", "同 `accounts-mcp-read`"), out("name", "同 `accounts-mcp-read`"), out("notes", "同 `accounts-mcp-read`；开回来那一趟 `changed` = 同步改写了的那几个号"), arg("on", "`false` = 停；`true` = 开回来"), out("servers", "同 `accounts-mcp-read`；开回来那一趟 `changed` = 同步改写了的那几个号"), out("sync", "同 `accounts-mcp-read`；开回来那一趟 `changed` = 同步改写了的那几个号")],
        takes_input: true,
        run: Run::BlockingOwn(|r, own| {
            with_account_keys(own, |keys| {
                crate::faces::accounts_face::answer(&LocalFiles, &r.cmd, &r.args, keys)
            })
            .map(Some)
        }),
    },
];
