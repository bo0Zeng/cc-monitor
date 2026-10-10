package com.ccmonitor.mobile.ui.claude

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Scaffold
import androidx.compose.ui.Modifier
import com.ccmonitor.mobile.core.ui.theme.AppTheme

/**
 * debug 预览屏：直接渲染 [sampleRenderUnits]，看阅读面卡片的层次。
 * 启动：`adb shell am start -n com.ccmonitor.mobile/.ui.claude.ClaudePreviewActivity`
 */
class ClaudePreviewActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            AppTheme {
                Scaffold { inner ->
                    ClaudeReadingPane(
                        units = sampleRenderUnits(),
                        modifier = Modifier.fillMaxSize().padding(inner),
                    )
                }
            }
        }
    }
}
