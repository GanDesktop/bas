# Android 宿主

`com/longbridge/nsh/AppActivity.java` 是 BAS 的 Rust 应用宿主，基于 Cargo 锁定版本的 NSH 入口适配。它在加载 Rust 根组件前注入动态色板，在 `onResume` 和 `onConfigurationChanged` 时重新读取系统资源；色板未变时不会重复提交主题。

`DynamicTheme.java` 将 Android 12 起提供的 `system_accent1_*`、`system_accent2_*`、`system_neutral1_*`、`system_neutral2_*` 映射到 NSH 的语义颜色。明暗两套颜色一起交给 `NshView.setThemeOverrides()`，宿主颜色优先于 Rust 内置色板，字体、圆角等其他主题配置保持由 Rust 管理。危险操作等语义状态色保留 NSH 默认值。Android 9–11 不注入覆盖，使用应用内置主题。

数据来自系统提供的调色板，不读取壁纸文件，不申请壁纸或照片权限。配色跟随系统实际提供的颜色，包括系统设置中选择的固定主题色；各厂商是否让它随壁纸变化，由系统实现决定。参见 [Android 动态配色文档](https://developer.android.com/develop/ui/views/theming/dynamic-colors)。

## 构建

仓库根目录运行 `make package-mobile`，或在 `nix develop .#android` 中运行：

```sh
./scripts/mobile-android package --abi arm64-v8a --out target/phone-test/apk
./scripts/mobile-android run
```

脚本通过 Cargo metadata 找到锁定的 NSH Java 源码，使用本目录的应用入口替换默认 `AppActivity`，编译到 `target/mobile-android/runtime`。NSH CLI 通过 `NSH_RUNTIME` 使用该宿主，并正常编译应用自身的 Rust 原生库；不修改 Cargo 缓存或全局安装的 NSH。升级 NSH 时需核对其 `AppActivity` 的生命周期变化。原始 NSH 许可证见 [licenses/NSH.txt](licenses/NSH.txt)。

## 真机验证

1. 安装生成的 APK，设置明暗模式为“跟随系统”。观察会话页、侧栏、输入框及设置页是否使用一致的系统色。
2. 在系统设置中更换明显不同的壁纸／主题色，返回 BAS；颜色应更新，当前会话、输入草稿不应因应用自行重启而丢失。
3. 切换系统深色模式，再在 BAS 中手动选择浅色和深色；两套颜色都应来自当前系统色板，手动明暗选择应保留。
4. 修改字号或字体；动态色板应继续生效。重启应用后应在首帧应用当前色板。
5. 在 Android 9–11 上确认默认色板和明暗切换仍正常。

宿主 Java 编译、DEX 和 APK 签名检查不等于真机壁纸切换验证。无设备时应明确保留这项验证限制。
