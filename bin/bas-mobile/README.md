# BAS Mobile

纯 Rust 的 NSH 手机客户端，以 Android 为主。没有 TSX 或 WebView。界面通过 gds-uim 的 `el!` / `m!` 组织，文案使用 Fluent，跟随系统选择中文或英文。

- 未连接时，首页只保留新建连接入口和右上角应用菜单；连接后才显示会话界面与侧栏。
- 左侧三横线按钮打开会话抽屉，顶部切换后台、工作区，新建会话时选择智能体；已有会话时，右上角三点菜单左侧也提供新建会话入口。列表支持搜索，长按会话打开重命名、刷新、关闭／重启、归档与删除菜单。
- 右上角应用菜单仅包含设置和关于。设置使用 NSH NavStack 独立页面，按前端和后端分区；次级页使用纵向 section，不使用 tab。
- 输入框平时为一行，只显示加号、输入和发送。聚焦／键盘弹出时在同一面板展开，模型选择外置，其余 ACP `configOptions` 收入会话配置菜单；无 configOptions 时使用 Agent 报告的模式。Android 返回一次收起键盘并恢复单行，保留草稿；展开只改变布局，不切换原生输入类型。
- 加号菜单统一选择照片／视频和文件。附件按 Agent 声明的 image、audio、embeddedContext 能力提交；发送结果不确定时禁止重复提交。
- 会话支持 Markdown、图片与资源文本、工具调用状态／输出、NSH DiffView 文件差异、权限审批，以及 Agent 发来的表单和 URL 交互。同一轮内连续思考和工具调用默认合并折叠，正文会打断分组；摘要保留运行／失败数量，长命令单行省略，展开查看完整内容。
- 前端设置保存语言、明暗模式、字号、字体、行高、透明度和对话偏好；服务连接支持命名、测试连接、保存和切换。
- Android 12 起使用系统壁纸／主题色板，启动、恢复前台及系统配置变化时刷新；明暗模式仍可跟随系统或手动选择。Android 9–11 和桌面使用内置色板。
- 后端设置中的智能体管理按注册目录、自定义和内置分组，详情页提供检查、安装／更新、移除、自定义编辑、登录与退出；支持从智能体行开始会话。
- 会话管理支持空闲回收时间、默认预热智能体、默认备用数和逐智能体覆盖。有效修改自动保存，保留后台版本检查，冲突不会覆盖服务端的新修改。

## 连接电脑上的真实 BAS

电脑本地预览可直接使用现有 BAS 服务的同用户 IPC：

```sh
BAS_MOBILE_ENDPOINT=local make dev-mobile
```

或在右上角菜单 → 设置 → 服务连接中填 `local` 并连接。无需另起测试后台，工作区、会话、智能体和管理配置均来自现有服务。也支持 `unix:///绝对路径`、`tcp://主机:端口` 和普通 `http://主机:端口`。环境变量仅覆盖本次启动选择；成功连接仍会保存到应用连接列表。

Android 上的 `local` 指手机自身，不是电脑；手机连接电脑仍需 TCP 地址或 USB 转发。`127.0.0.1:50051` 的旧 `target/phone-test` 实例是回显测试服务，与电脑正在使用的真实 BAS IPC 服务不同。

## 日常开发：电脑本地预览

Linux 开发机上，在仓库根目录运行：

```sh
make dev-mobile
```

该命令进入包含 GTK4、GStreamer 和 Rust 工具链的 Nix 环境，用 NSH CLI 启动默认 390 × 844 的原生窗口，运行同一套 Rust UI。保存 Rust 文件后自动重新编译并重启窗口，不需要连接手机、安装 APK 或启动模拟器。重启会重置内存中的界面状态。首次运行需要下载依赖并编译；NSH CLI 需按下方说明安装（本机已安装）。

平铺窗口管理器可能覆盖默认尺寸；可将 BAS 窗口切换为浮动并调整到手机比例。关闭窗口或在终端按 Ctrl-C 结束预览。

需要隔离开发数据时，也可以另开终端启动测试后台；日常连接现有服务使用上面的 `local`：

```sh
nix develop --command cargo run -p bas -- --listen 127.0.0.1:50051
```

在右上角菜单 → 设置 → 服务连接中填 `http://127.0.0.1:50051`。若该端口已有开发后台，直接连接即可。工作区和会话使用该后台的数据。

```sh
make test-mobile
```

自动测试在无窗口的 Android 环境下驱动界面并连接隔离的真实 BAS 后台。日常布局、会话流程和业务逻辑先在本机开发；系统键盘、触摸手势、权限和安装行为再用 Android 模拟器或真机验证。Linux 预览使用桌面平台控件，不能完全代表 Android 外观。参见 [NSH 本地开发文档](https://nsh.dev/docs/dev-server-and-debugging/) 和 [Linux 支持](https://nsh.dev/docs/linux/)。

## Android 运行

使用仓库的 Rust 1.98.1 工具链（`nix develop`），并安装 [NSH CLI](https://nsh.dev/docs/installation)。NSH Git 依赖已固定到 `f81b965f69096f871ca21907f6fab6be32fd2d40`；CLI 会从 Cargo 解析的同一 NSH 源码构建 Rust runner。

仓库提供独立的 Android Nix 开发环境，包含 SDK Platform 35、Build Tools 35.0.1、Platform Tools / adb 37.0.0、Command-line Tools 20.0、NDK 29.0.14206865、JDK 17，以及 ARM64、ARMv7、x86_64 三个 Rust Android target。`ANDROID_HOME`、`ANDROID_SDK_ROOT`、`ANDROID_NDK_HOME`、`JAVA_HOME` 和交叉编译器路径由 shell 设置；首次进入时自动下载并按配置接受 Android SDK 许可。

```sh
# 仓库根目录；默认 nix develop 仍用于服务端开发。
nix develop .#android
adb version
adb devices -l
sdkmanager --list_installed
cargo check -p bas-mobile --target aarch64-linux-android
```

SDK 由 Nix 管理、只读；增减组件请修改 `flake.nix` 的 `android` shell 后重新进入，不要对该 SDK 运行 `sdkmanager --install`。本环境不包含模拟器或系统镜像。真机需开启 USB 调试，并在手机上确认调试授权；设备需 Android 9 / API 28 或更高。参见 [NSH Android 文档](https://nsh.dev/docs/android)。

NSH CLI 按上面的官方安装说明单独安装（本机已安装到 `~/.local/bin/nsh`）；Nixpkgs 的 `nsh` 是另一个同名 shell 项目，不能代替 Native Shell。

```sh
# 在仓库根目录、Android shell 内运行，且已安装 NSH CLI：
./scripts/mobile-android run
# 生成带系统动态配色的 APK：
./scripts/mobile-android package --abi arm64-v8a --out target/phone-test/apk
# 也可在 shell 外运行：make package-mobile
```

Android 构建入口会从 Cargo 锁定的 NSH 版本编译 Java 宿主，并通过 `NSH_RUNTIME` 交给 NSH CLI 打包。直接运行原始 `nsh package` 会使用默认宿主，不包含 BAS 的动态配色适配。界面仍由 Rust 绘制，Java 只负责系统生命周期和色板注入。实现与真机检查步骤见 [Android 宿主](android/README.md)。

在开发机上启动 TCP 后台（地址、端口按自己的配置调整）：

```sh
cargo run -p bas -- --listen 127.0.0.1:50051
```

Android 模拟器中填 `http://10.0.2.2:50051`；USB 真机可先运行 `adb reverse tcp:50051 tcp:50051`，然后填 `http://127.0.0.1:50051`。工作区路径属于后台主机；当前手机界面只选择已有工作区，新增工作区请在 GDS Agents 或后台工具中完成。后台地址列表与 Agent 列表是两个不同概念。

当前连接表单尚未提供 mTLS 证书选择。需要在主机终端执行的登录方法会明确提示在后台主机完成，再检查连接；手机端不执行后台返回的主机命令。音频和二进制资源目前显示类型/资源信息，未提供播放器与下载管理；客户端托管终端的交互输出尚未嵌入。未知发送结果会禁用再次发送，应先刷新历史确认，重新选择会话后再继续。

## 通过 USB 在真机测试

在仓库根目录进入 Android shell 后：

```sh
nix develop .#android
./scripts/mobile-android package --abi arm64-v8a --out target/phone-test/apk
adb install -r target/phone-test/apk/BAS.apk
adb shell am start -n io.gandesktop.bas/com.longbridge.nsh.AppActivity
# 电脑上的 BAS 监听 50051；让手机自动分配一个可用端口。
adb reverse tcp:0 tcp:50051
```

最后一条命令输出手机端口，例如 `38017`。在 App 的“设置 → 服务连接”中填 `http://127.0.0.1:38017`，点“连接”，再选工作区与会话发送消息。端口转发在重连、重启 adb 或手机后可能需要重新设置；以命令实际输出为准。iQOO 安装 APK 时可能要求在手机上确认“允许／继续安装”，请保持解锁等待提示。

熄屏通常不影响 USB 调试。若需要持续观察界面，可在手机开发者选项打开“保持唤醒状态／充电时不休眠”，测试后关闭。

本次测试后台数据位于 `target/phone-test/`，监听 `127.0.0.1:50051`，使用 `embedded` 回显 Agent，预置会话为 `Android test`。它用于确认连接和界面，不调用真实模型。后台 PID 在 `target/phone-test/daemon.pid`，完成后可核对 PID 并停止该测试进程。

## 开发与验证

```sh
nix develop --command cargo check -p bas-mobile
nix develop --command cargo build -p bas -p acp-test-agent -p bas-browser-bridge
nix develop --command cargo test -p bas-mobile
```

端到端测试启动隔离的真实 BAS 后台和 acp-test-agent，以 NSH headless 的 Android 手机环境执行会话、后台切换、ACP 模型/布尔配置、权限与表单交互、工具结果展开、附件选择、自定义注册和会话策略保存。包括键盘环境变化、草稿保留、长按重命名、应用菜单边界和纵向设置导航。`/activity` 场景验证思考／工具合并、正文分界、失败状态及详情展开。后台日志、UI mount trace、会话文本和结果保存在 `target/mobile-e2e/`。可用 `BAS_TEST_DAEMON` 指定后台可执行文件。Headless 验证不替代 Android 真机键盘、手势与 APK 安装验收。

ACP 登录使用独立页面：创建会话或实时对话返回结构化 `AuthRequired` 时自动打开，取消或返回后，对话保留「登录」入口。智能体详情也提供直接的「登录」按钮；「检查连接」结果只显示摘要和入口，不再内嵌授权表单。登录页展示智能体提供的登录方式、说明、表单／浏览器授权请求、进度和错误；取消会关闭本次鉴权流，重试使用新的请求。成功后回到原页面，必要时重试创建会话，但不自动重发提示。终端登录方式仍提示在后台主机完成，不在手机执行服务端命令。

`authentication` 端到端测试使用真实 BAS 和 `acp-test-agent --require-auth --interactive-auth --auth-state <临时路径>`，覆盖自动跳转、取消／拒绝／重试、表单校验、浏览器授权、登录过期、会话恢复及草稿保留。登录页文本和 UI trace 保存在 `target/mobile-e2e/authentication-*/`，不需要连接手机。

代码按 `app → pages → widgets → features → shared` 分层，会话、智能体管理和会话策略各自组织业务；可复用布局 `ContentPage` 声明 `header / content / footer` 插槽，`ControlRow` 声明 `leading / content / trailing` 插槽，通过 `Default::default()` 创建。

配色使用暖灰／陶土色的明暗主题，沿用 NSH 的 `sidebar / card / accent / primary` 等语义 token，交互使用内置 hover、active、selected、disabled 状态。当前 NSH 没有公开的通用图标库；工具栏使用 Lucide SVG，经 NSH `Image` 加载并随主题着色。图标来源和许可证随资源保存在 `assets/icons/`。

## 组件调研依据

实现前核对了 NSH 的 [完整组件列表](https://nsh.dev/elements/)、[Rust 应用指南](https://nsh.dev/docs/rust) 和以下组件文档、Rust builder 源码：

| 需求 | 复用组件 | 原因 |
| --- | --- | --- |
| 侧边栏 | [Sheet](https://nsh.dev/elements/sheet) | 已有左侧展开、遮罩、动画、焦点与 Android 返回处理 |
| 工作区、后台、模型选择 | [Select](https://nsh.dev/elements/select) | 手机自动使用底部面板，内置选择状态、可访问性、列表回收 |
| 会话和消息列表 | [List](https://nsh.dev/elements/list) | 原生滚动、回收单元格，消息列表支持跟随底部 |
| 消息内容 | [TextView](https://nsh.dev/elements/text-view) | 原生 Markdown、代码块、文本选择 |
| 输入与操作 | [Input](https://nsh.dev/elements/input)、[Button](https://nsh.dev/elements/button) | 原生输入与禁用状态、交互语义 |
| ACP 布尔配置 | [Switch](https://nsh.dev/elements/switch) | 复用开关行为与无障碍状态 |
| 工具调用详情 | [Collapsible](https://nsh.dev/elements/collapsible)、[DiffView](https://nsh.dev/elements/diff-view) | 折叠输出、统一差异视图与代码滚动 |
| 移除与退出登录确认 | [AlertDialog](https://nsh.dev/elements/alert-dialog) | 复用系统确认交互 |
| 附件 | NSH `pick_files` / `pick_images` | 使用系统选择器，按 ACP 能力构造提交内容 |
| 系统适配 | [Safe area](https://nsh.dev/elements/safe-area)、[Keyboard](https://nsh.dev/elements/keyboard) | 复用系统 Insets 和键盘动画 |

也核对了 Scroll、NavStack、Dialog、Menu、DockArea 的职责。会话抽屉复用 Sheet，设置层级使用 NavStack / NavPage；应用、附件、会话配置与长按菜单复用 Popover，保持锚定位置。设置行复用 gds-uim 解剖组件 SettingsSection / SettingRow，滑块使用 NSH Slider。

本轮功能参照同机 `gds/bin/agents` 的 composer、agent options、agents settings 和 ACP policy 实现，复用 BAS 的配置类型、注册校验、认证描述解析和 elicitation 校验；没有硬编码模型、思考级别或审批选项。

本轮间距与消息组织对照 GDS Agents 的 `ui/composer/view.rs`、`ui/content_header.rs`、`ui/thought.rs`、`ui/chat/content.rs` 和 `entities/session/model/conversation.rs`。输入区使用统一网格（44px 控件、18px 图标、32px 发送底色），下拉项为居中的 44px 行；空的审批区域不再额外占用间距。

## 手机本机 BAS 服务（PRoot）

本机服务使用独立的 APK 构建入口，保留现有远程客户端构建方式：

```sh
make package-mobile-local
# ARM64 APK：target/mobile-local/arm64-v8a/BAS.apk
# x86_64 模拟器：
./scripts/package-mobile-local x86_64
```

构建在主机交叉编译 Linux 版 BAS 和 `acp-test-agent`，下载并校验固定版本的 Ubuntu Base 与 Termux PRoot，打包到 APK。运行不需要 Root 或另外安装 Termux。首次启动服务在手机私有目录解压运行环境；操作入口为「设置 → 服务连接 → 手机本机服务」，依次点击「启动」「连接」。连接时初始化 `/home/bas/workspace` 工作区，可以使用内置 `acp-test-agent` 验证会话，它不调用模型。安装 npm Agent 时，BAS 按需下载 Node；运行环境额外包含官方 Node 25 及以上所需的 `libatomic1`，两个架构的包版本、来源和校验值固定在 `inputs.json`。Git 与原生模块编译工具链尚未打包。

Android 前台 Service 持有 BAS 进程与通知，界面关闭不主动停止服务。点击「停止」会中断任务，先向 BAS 发送 SIGTERM，再回收 PRoot；历史、配置和工作区保留。系统强制停止应用或杀进程后需要重新启动，不能保证后台永不被系统回收。

数据在应用私有 `files/bas-runtime/`：

- `home/`：持久工作区、`daemon.json`、`sessions.sqlite3` 和 Agent 数据。
- `versions/<sha256>/`：可替换的 Linux 用户态，不存用户工作区。
- `run/service.sock`：本机连接端点，沿用 BAS 同 UID 的 Unix socket 认证；PRoot 不使用伪 root。
- `service.log`、`install.log`：服务与解压日志。

本机构建使用单独的 [AndroidManifest.xml](android/AndroidManifest.xml)，`targetSdk=28`，用于私有目录执行 Linux 程序，面向 APK 侧载；现有远程客户端仍使用 NSH 默认 targetSdk。包目前使用 Android 开发签名。打包脚本复用 NSH 编译结果，加入运行环境、服务声明后重新对齐并签名，不修改 NSH 依赖源码。固定下载输入见 `scripts/mobile-runtime/inputs.json`。APK 中附带运行时来源、校验信息与许可证。

构建与 Rust 检查不替代真机验证。验收需要覆盖首次安装、连接并发送消息、切到桌面、停止/重启后的历史，以及强制停止后重新启动；不要用 USB 转发到电脑的 BAS 代替本机服务验收。

本次已在 Android 15 / API 35 的 x86_64 模拟器验证：APK 首次解压、应用 UID 下运行 BAS/PRoot、客户端通过私有 socket 连接、ACP 测试 Agent 流式回显、退到桌面后 RPC 可用、SIGTERM 停止、重启后恢复同一会话历史。验证截图、服务日志和重启前后会话 ID 保存在 `target/mobile-emulator/`。Android 16 的 V2536A ARM64 真机已验证安装、启动、连接及测试 Agent 进程创建。补齐 `libatomic1` 后，使用 QEMU 用户态执行官方 ARM64 Node 26.13.0 和 npm 11.20.0 的版本命令通过，记录在 `target/mobile-runtime/verification/`；这不替代真机 Agent 安装与模型调用验收。各厂商长期后台行为尚未验证。

模拟器环境可用 `nix develop .#android-emulator`，内含 API 35 的 Google APIs x86_64 镜像和 emulator。验证请创建独立 AVD 并指定独立端口，再使用 `adb -s <serial>` 安装 `target/mobile-local/x86_64/BAS.apk`，避免覆盖其他测试设备。按上述入口启动并连接，新建 `acp-test-agent` 会话、发送任意文字，然后测试退到桌面和停止/重启后的历史恢复。此流程不需要转发电脑服务端口。

### Linux 程序打开手机浏览器

本机运行环境包含 `bas-open-url`，设置 `BROWSER=/opt/bas/bin/bas-open-url`，并将 `/usr/local/bin/xdg-open` 指向它。支持继承 `BROWSER` 的程序和通过 `PATH` 调用 `xdg-open` 的程序；无需在 Ubuntu 中安装桌面浏览器。

助手将 HTTP(S) URL 原样写入应用私有的 `run/browser/`，app 显示目标域名和「打开浏览器／取消请求」。本机鉴权页内显示请求；其他页面使用独立底部面板。点击后使用 NSH 的 `open_url` 调用 Android `ACTION_VIEW`，成功交给系统才通知助手退出 0；取消退出 4，未配置桥接目录退出 3。打开失败会保留请求供重试。等待打开的时限为五分钟；退出 Agent、服务重启或请求过期后不再展示旧请求。服务后台运行时不会自行拉起浏览器，回到 app 后处理待办请求。

运行环境通过绑定 `/run/bas` 共享目录。请求与响应使用随机 ID 和原子写入，私有目录为 0700；只接受 HTTP(S) URL，界面不显示 URL 中的 OAuth 参数，日志不记录完整链接。此桥接只负责交接网址，不承担 OAuth 回调或 token 交换。当前 PRoot 没有独立网络命名空间，使用本机回环地址的 Agent 回调仍由其监听进程处理；自定义回调 scheme、依赖桌面浏览器扩展或内置终端的登录方式需要另行适配。

`make test-mobile` 会构建真实助手，验证 `$BROWSER`、`xdg-open` → 私有队列 → 手机界面 → 系统打开链接请求，以及取消、返回、进程结束和 URL 参数保留。工件在 `target/mobile-e2e/browser-bridge-*/`；无窗口测试不包含 Android 系统浏览器或真实第三方 OAuth 服务。
