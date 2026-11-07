package com.ccmonitor.mobile

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import com.ccmonitor.mobile.core.ui.theme.AppTheme
import com.ccmonitor.mobile.ui.common.TooOldScreen
import com.ccmonitor.mobile.ui.nav.AppNavHost

class MainActivity : ComponentActivity() {
    // 通知权限申请（API33+）。结果不用处理：拒绝时通知不显示，别的照常。
    private val notifPermLauncher =
        registerForActivityResult(ActivityResultContracts.RequestPermission()) { }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val refused = (application as CcMonitorApp).refused
        if (refused == null) requestNotificationPermissionIfNeeded()
        // 固定深色（`AppTheme`）：系统栏图标也固定浅色，不跟系统深浅走。
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
        )
        setContent {
            AppTheme {
                // 状态库太老：什么都没起（[CcMonitorApp.refused]），整屏只说这一句。
                if (refused != null) TooOldScreen() else AppNavHost()
            }
        }
    }

    /** API33+ 未授予 POST_NOTIFICATIONS 时申请；保活通知要它。 */
    private fun requestNotificationPermissionIfNeeded() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            notifPermLauncher.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
    }
}
