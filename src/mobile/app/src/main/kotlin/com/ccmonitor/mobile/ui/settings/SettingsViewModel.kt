package com.ccmonitor.mobile.ui.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.ccmonitor.mobile.core.data.repo.SettingsRepository
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/**
 * 应用级设置，存在 [SettingsRepository] 里。
 * 默认 claudeDir 由 SessionViewModel/SshKeepAliveService 经 `ClaudePaths.resolveClaudeDir(host.claudeDir, 此默认)` 消费。
 */
class SettingsViewModel(
    private val repo: SettingsRepository,
    private val io: CoroutineDispatcher = Dispatchers.IO, // 可注入便于 JVM 测
) : ViewModel() {
    val defaultClaudeDir: StateFlow<String?> =
        repo.defaultClaudeDir().stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    /** 保存应用级默认 claudeDir；空串由 repo 归一化为 null（=回退内置默认）。 */
    fun save(value: String) = viewModelScope.launch(io) { repo.setDefaultClaudeDir(value) }

    /**
     * 新界面总开关，默认关。
     *
     * 初值给 `false` 而不是 `null`：三态开关只会让人困惑，「还没读出来」与「关着」对用户意义相同。
     */
    val newUiEnabled: StateFlow<Boolean> =
        repo.newUiEnabled().stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), false)

    fun setNewUi(enabled: Boolean) = viewModelScope.launch(io) { repo.setNewUiEnabled(enabled) }

    /** 对话总览的来源路径。null = 没设过，由界面显示回退值。 */
    val overviewSourcePath: StateFlow<String?> =
        repo.overviewSourcePath().stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    fun saveOverviewSourcePath(value: String) = viewModelScope.launch(io) { repo.setOverviewSourcePath(value) }

    /** 新对话的权限模式。null = 用 Claude 自己的默认。 */
    val permissionMode: StateFlow<String?> =
        repo.permissionMode().stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), null)

    fun setPermissionMode(value: String?) = viewModelScope.launch(io) { repo.setPermissionMode(value) }
}
