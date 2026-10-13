plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

android {
    namespace = "com.ccmonitor.mobile.core.ui"
    // compileSdk / minSdk / compileOptions / jvmTarget 在根 subprojects{} 里统一配置。
    buildFeatures {
        compose = true
    }
}

dependencies {
    // 文案取文口住 core-copy（纯 JVM，上屏有字的模块依赖它）；core-ui 自己的默认文案（如「撤销」）也从那里取。
    api(project(":core-copy"))
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.ui)
    implementation(libs.androidx.ui.graphics)
    implementation(libs.androidx.material3)
    implementation(libs.androidx.ui.tooling.preview)
    debugImplementation(libs.androidx.ui.tooling)
}
