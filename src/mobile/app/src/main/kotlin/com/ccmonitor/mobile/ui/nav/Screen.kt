package com.ccmonitor.mobile.ui.nav

import java.net.URLEncoder

/** 全局导航路由。 */
sealed class Screen(
    val route: String,
) {
    /**
     * 启动落地判定屏（`startDestination`）。
     *
     * 打开即进聊天屏要等一个异步事实：上次那台机器（核对过还在）；
     * 而 `startDestination` 建图时就得给出字符串。所以起点是这个不画东西的判定屏，
     * 读到答案后用 `popUpTo(inclusive = true)` 替换自己，返回键不会退回它。
     * 没有上次那台时去 [Hosts]。
     */
    data object Launch : Screen("launch")

    data object Hosts : Screen("hosts")

    data object HostEdit : Screen("host/{id}/edit")

    data object Identities : Screen("identities")

    // 按 tab key 路由，hostId/cd/launcherId 由 SessionTabManager.get(key) 提供；VM 经 per-key store 保活。
    data object Session : Screen("session/{key}?open={open}") {
        /** 文件 tab 进来时要打开的那个路径（可缺）。 */
        const val ARG_OPEN = "open"

        fun of(
            key: String,
            open: String,
        ): String = "session/$key?$ARG_OPEN=" + URLEncoder.encode(open, "UTF-8").replace("+", "%20")
    }

    /** 会话列表（抽屉「会话」进）：已固定 → 各组（段头 组名 ＋ 计数）→ 按天；顶栏「需手动 n」进需手动屏。 */
    data object Conversations : Screen("conversations/{hostId}") {
        fun of(hostId: String): String = "conversations/$hostId"
    }

    /**
     * 一个对话。由「主机 + 对话编号」共同确定，只用编号的话两台主机上同号的对话会撞（见 `chatKey`）。
     */
    data object Chat : Screen("chat/{hostId}/{sid}?new={new}&cwd={cwd}&at={at}&tool={tool}") {
        /**
         * [isNew] = 对话刚开，远端还不存在。`ChatRoute` 据此决定加不加 `--resume <编号>`：
         * 对远端没见过的编号 resume 会直接失败（`--resume` 还按目录划分作用域）。
         * 缺省 `false`：从列表点进来的对话确实已存在。
         */
        fun of(
            hostId: String,
            sid: String,
            isNew: Boolean = false,
            /** 新建会话从哪进来的那个目录（见 `ChatRoute.draftCwd`）。 */
            cwd: String? = null,
            /** 从搜内容的一处命中打开：命中那一行的字节起点（`SearchHit.offset`）；缺 ⇒ 读最后一屏。 */
            at: Long? = null,
            /** 那处命中落在哪次工具调用的卡里（`SearchHit.tool`）。 */
            tool: String? = null,
        ): String =
            "chat/$hostId/$sid?new=$isNew" + (cwd?.let { "&cwd=" + enc(it) } ?: "") + (at?.let { "&at=$it" } ?: "") +
                (tool?.let { "&tool=" + enc(it) } ?: "")

        private fun enc(s: String) = URLEncoder.encode(s, "UTF-8").replace("+", "%20")

        const val ARG_NEW = "new"
        const val ARG_CWD = "cwd"
        const val ARG_AT = "at"
        const val ARG_TOOL = "tool"
    }

    /** 需手动屏（会话列表 · 聊天屏顶栏「需手动 n」进）：跨会话、各台的需手动；[hostId] 是进来时的那台（排在最前）。 */
    data object Overview : Screen("overview/{hostId}") {
        fun of(hostId: String): String = "overview/$hostId"
    }

    data object Settings : Screen("settings")

    /** 编辑按键（终端屏 ⋯「编辑按键」· 绑了「编辑按键」的触发进来；只有这一页）。 */
    data object Keys : Screen("keys")

    /** 轮换屏：一条会话的（[sid]）· 这台的设置页那一屏（[sid] 缺；会话里［编辑策略］跳过来的带看哪份 [strategy] · 开哪个号 [account]）。 */
    data object Rotation : Screen("rotation/{hostId}?sid={sid}&strategy={strategy}&account={account}") {
        fun of(
            hostId: String,
            sid: String? = null,
        ): String = "rotation/$hostId" + (sid?.let { "?sid=$it" } ?: "")

        /** 设置页那一屏：看 [strategy] 那一份、开 [account] 的编辑小窗。 */
        fun edit(
            hostId: String,
            strategy: String,
            account: String?,
        ): String = "rotation/$hostId?strategy=" + enc(strategy) + (account?.let { "&account=" + enc(it) } ?: "")

        private fun enc(s: String) = URLEncoder.encode(s, "UTF-8").replace("+", "%20")

        const val ARG_SID = "sid"
        const val ARG_STRATEGY = "strategy"
        const val ARG_ACCOUNT = "account"
    }

    /** agent 列表：一条会话（[sid]）派出的子 agent（聊天屏 ⋯ → agent 进）。 */
    data object Agents : Screen("agents/{hostId}/{sid}") {
        fun of(
            hostId: String,
            sid: String,
        ): String = "agents/$hostId/$sid"
    }

    /** agent 屏：那条会话里的一个子运行（[run] 是运行表里那一格）；栈里同一个运行只出现一次（`AgentStack`）。 */
    data object Agent : Screen("agent/{hostId}/{sid}/{run}") {
        fun of(
            hostId: String,
            sid: String,
            run: String,
        ): String = "agent/$hostId/$sid/" + URLEncoder.encode(run, "UTF-8").replace("+", "%20")

        const val ARG_RUN = "run"
    }

    /** 计划 · 片：这台的 pb 工作区与片（聊天屏 ⋯ → 计划进；[sid] 是从哪条会话进来的，带到大纲认此会话接手的块）。 */
    data object Plan : Screen("plan/{hostId}?sid={sid}") {
        fun of(
            hostId: String,
            sid: String?,
        ): String = "plan/$hostId" + query("sid" to sid)
    }

    /** 计划 · 大纲：一个工作区里的一片。 */
    data object PlanSlice : Screen("plan-slice/{hostId}?ws={ws}&slice={slice}&sid={sid}") {
        fun of(
            hostId: String,
            ws: String,
            slice: String,
            sid: String?,
        ): String = "plan-slice/$hostId" + query("ws" to ws, "slice" to slice, "sid" to sid)
    }

    /** 计划 · 一格详情（点子格再推一屏）。 */
    data object PlanCell : Screen("plan-cell/{hostId}?ws={ws}&slice={slice}&id={id}") {
        fun of(
            hostId: String,
            ws: String,
            slice: String,
            id: String,
        ): String = "plan-cell/$hostId" + query("ws" to ws, "slice" to slice, "id" to id)
    }

    /** 设置 → 机器与账号 → 轮换：当前服务器（上次那台）新会话用的那份。 */
    data object RotationHere : Screen("settings/rotation")

    /** 设置 → 机器与账号 → 账号：当前那台（上次那台）账号库里的号。 */
    data object Accounts : Screen("settings/accounts")

    /** 设置 → 账号卡 · 账号一览号的抽屉 → 用量：当前那台每个号的额度。 */
    data object Usage : Screen("settings/usage")
}

/** 查询串：值是 `null` 的不写；值照 URL 编码（路径里的 `/` · 中文片名来回不丢）。 */
private fun query(vararg kv: Pair<String, String?>): String =
    kv
        .filter { it.second != null }
        .joinToString("&", prefix = "?") { (k, v) -> "$k=" + URLEncoder.encode(v, "UTF-8").replace("+", "%20") }
        .takeIf { it != "?" }
        .orEmpty()
