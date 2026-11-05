# **Bas** is a Basic Agent Service

Bas 的目的是为linux系统（或者可能更多）在各种各样的环境下提供一个基础的agent服务能力。

从旧版本升级前请阅读 [破坏性改名与手动迁移](docs/rename-migration.md)。

可能有以下应用场景
1. 比如在系统的装机阶段，或者在用户缺少python/node环境或者相关依赖的时候，或者在纯终端环境下，能够获得一定的AI能力。
2. 有一个系统级别的ai唤起流程，可以通过终端调用配置的agent provider来为用户完成任务。

项目分为如下几个部分
1. 守护进程 Bas（`bas`），负责管理agent会话，提供服务供客户端使用
2. 终端客户端 BasT（`bast`），能够通过命令行来管理 Bas 服务

出于可用性的考虑，应当尽量减少动态库依赖。比如tls使用rustls, 使用musl静态链接编译等。

## 开发环境

开发环境由 Nix flake 提供，Rust 工具链版本固定在 `rust-toolchain.toml`，`flake.lock` 锁定 nixpkgs 与 rust-overlay 的版本。

```sh
nix develop        # 进入开发 shell
cargo check        # 在 shell 内构建
```


## 测试

```sh
nix develop -c cargo test --all-targets  # 日常 Rust 测试
python3 tests/install_bas.py -v
python3 tests/nix_source.py -v
```

安装器和 Nix source 测试使用临时目录与模拟环境，不修改真实 profile。

QEMU/NixOS 系统测试目前不在仓库中，后续统一补全。

操作步骤和新增测试的方法见 [测试环境说明](docs/testing.md)。

## 目录结构

```text
🦀Cargo.toml
  +--📄proto/bas/v1/bas.proto        gRPC 接口定义（bas.v1.Bas）
  +--🦀build.rs                      protoc-bin-vendored + tonic-prost-build 生成代码
  +--🦀src/lib.rs                    公共库：领域模型与客户端协议支持
  |   +--🦀model.rs                  会话生命周期、事件信封与 ACP 类型导出
  |   +--🦀service.rs                gRPC 信封、ACP JSON 编解码与客户端连接
  |       +--🦀defaults.rs           客户端与守护进程共用的地址、回放默认值
  +--🦀src/bin/bas/                  守护进程内部实现
  |   +--🦀main.rs                   组装与启动
  |   +--🦀agent.rs                  后端抽象与驱动
  |   |   +--🦀embedded.rs           进程内 agent（当前为 echo 占位）
  |   |   +--🦀acp.rs                外部 agent（Agent Client Protocol 客户端）
  |   +--🦀session.rs                会话生命周期
  |   |   +--🦀conversation.rs       单个会话：回放窗口 + 事件广播
  |   |   +--🦀attachment.rs         attach/detach 与游标续传
  |   |   +--🦀history.rs            有界历史环形缓冲
  |   |   +--🦀registry.rs           会话注册表与后端目录
  |   +--🦀storage.rs                memory/localdb 会话存储
  |   +--🦀service.rs                基于会话注册表的 gRPC 服务端实现
  |   +--🦀config.rs                 启动参数、环境变量与后端目录配置
  |   +--🦀system.rs                 进程生命周期与 systemd 集成
  |   +--🦀tracing.rs                日志后端
  +--🦀src/bin/bast/                 命令行管理工具
```

公共库导出 `bas::acp`（agent-client-protocol 3.3.0 的 v1 schema）、`bas::acp_v2`（展示模型）、`bas::model` 与 `bas::service`。后端驱动、会话运行时和
守护进程配置由 `bas` 的私有模块管理；客户端通过 gRPC 使用它们。

## 构建与运行

本机开发分别启动 daemon 和客户端；外部 Agent 通过 `bast agent install` 安装，认证使用 Agent 原生流程，见 [安装说明](docs/install.md)。

```console
$ cargo build --release
$ ./target/release/bas                         # 默认使用同用户本地 IPC，不监听 TCP
$ ./target/release/bast agents
$ ./target/release/bast session create --title demo
```

构建不需要系统安装 `protoc`：`build.rs` 使用 `protoc-bin-vendored` 提供的编译器。

## 安装 Bas

Linux / NixOS 在仓库根目录运行：

```sh
make install-bas-service
bast agents
```

只安装 `bas`、`bast` 并启用 systemd 用户服务；不创建或修改用户配置。
只安装可执行程序可使用 `make install-bas`。Codex、OpenCode、Pi 等外部后端可在 `bast agent install` 中按需安装，
也可沿用用户独立安装的程序和原生登录状态。
旧 `--with-codex` 已移除，已有捆绑包和配置不会被自动删除或改写。
官方安装链接、路径配置、迁移及服务管理见 [安装与后端配置](docs/install.md)。

## 配置

Agent 注册与 ACP 策略保存在用户配置目录的 `bas/daemon.json`。
服务监听、权限及容量等启动参数仍通过命令行或环境变量设置，命令行同名参数优先。

| 环境变量 | 默认值 | 说明 |
| --- | --- | --- |
| `BAS_LISTEN` | `local` | 监听目标；支持本地 IPC 或显式 TCP，多目标以 `;` 分隔，见 [连接类型](docs/connections.md) |
| `BAS_AGENTS` | 未设置 | 简写后端目录，设置后覆盖 JSON 配置，见下 |
| `BAS_DAEMON_SETTINGS_FILE` | 用户配置目录下的 `bas/daemon.json` | 后端 Agent 注册与 ACP 策略，用于服务器配置 |
| `BAS_AGENTS_FILE` | 未设置 | 显式兼容旧版独立 agent 数组；设置后覆盖后端文件的 agent 目录 |
| `BAS_HISTORY` | `4096` | 每个会话的内存历史窗口容量；localdb 的磁盘历史不受此容量限制 |
| `BAS_STORAGE` | `localdb` | 会话存储：`localdb`（默认 SQLite） / `memory`（显式关闭持久化） |
| `BAS_LOCALDB_PATH` | 用户状态目录下的 `bas/sessions.sqlite3` | `localdb` 的数据库文件路径，`memory` 模式不能设置 |
| `BAS_BUFFER` | `1024` | 后端更新队列与直播广播容量，必须大于 0 |
| `BAS_STARTUP_TIMEOUT` | `30` | 后端启动超时（秒），必须大于 0 |
| `BAS_PERMISSIONS` | `deny` | 工具权限申请的固定审批策略：`deny` / `allow` |
| `BAS_LOG_BACKEND` | `auto` | 日志后端：`auto` / `stdout` / `journald` |
| `RUST_LOG` | `info` | 日志过滤表达式 |

默认启动会将新会话存入用户状态目录；可显式覆盖数据库位置：

```console
$ bas --storage localdb --localdb-path "$HOME/.local/state/bas/sessions.sqlite3"
```

仅需临时会话时使用 `--storage memory`。从旧的仅内存版升级不会迁移已有进程内会话；
重启旧 daemon 前请先处理仍在运行的会话。

数据库需要长期保留会话内容，应限制文件及目录访问权限并安排备份。持久化记录不等于所有外部
Agent 都能恢复执行上下文；只有后端支持 ACP `session/load` 或 `session/resume` 且成功完成恢复，
才允许在旧会话继续发送 prompt。重启时未完成的 turn 会记录为中断，不会自动重发工具操作。
当前内嵌 `embedded` 后端仅是无状态 echo，不提供真正的模型对话上下文。

SQLite 以段目录、统一 chunks 和增量索引状态三张表管理历史；封口的回合无损归并，
并按收益选择压缩，活动回合仍保留增量日志。schema 7 会一次性迁移旧存储布局，
原始事件内容、游标和消息接受凭据不变；升级前需备份并预留磁盘空间。
存储边界和升级说明见 [回合存储](docs/rpc.md#完成回合的物理存储)。

`BAS_AGENTS` 用 `;` 分隔条目，条目形如 `id`、`id=kind` 或 `id=kind:命令行`：

```console
$ BAS_AGENTS='embedded;opencode=acp:opencode acp;gemini=acp:gemini --acp' bas
```

`kind` 取 `embedded` 或 `acp`；外部后端必须给出命令行（按空白切分，不支持引号）。
未设置简写目录时，读取显式 `--agents-file` 兼容文件，否则读取 `--daemon-settings-file` /
`BAS_DAEMON_SETTINGS_FILE` 或默认 `daemon.json` 的 `agents`。
JSON 支持完整参数数组，示例见 [contrib/daemon.json](contrib/daemon.json)。
文件目录每秒热更新，新建会话采用新配置，已有会话保持原后端，不需要重启。
默认后端文件不存在时，只读导入旧 `settings.json` 的 `daemon`；该文件也不存在才导入
`agents.json`。首次成功保存后端配置时生成 `daemon.json` 并保留旧文件。
显式旧 `--settings-file` 仍兼容；旧服务环境变量须改为 `BAS_DAEMON_SETTINGS_FILE`。

## BasT

```console
$ bast agents                                  # 列出可启动的已配置后端
$ bast agent catalog                           # 浏览可安装和已配置后端
$ bast agent install codex-acp                  # 在 daemon 主机按需安装并注册
$ bast agent login codex-acp                    # 使用后端原生认证
$ bast agent check codex-acp                    # 检查握手与会话创建
$ bast session create --agent embedded --title demo
$ bast session list
$ bast session show <id>
$ bast prompt <id> 帮我看看这个目录
$ bast attach <id> --replay 20                 # 回放最近 20 条并持续跟随；Ctrl-C 只是 detach
$ bast attach <id> --cursor 42 --replay 50     # 按游标续传
$ bast history <id> --after 42 --limit 50      # 翻页读取历史
$ bast cancel <id>
$ bast session rename <id> 新标题
$ bast session archive <id>                   # 隐藏但保留会话与历史
$ bast session list --all                     # 包括已归档会话
$ bast session restore <id>                   # 恢复到普通列表
$ bast session remove <id>                    # 永久删除：停止 Agent 并遗忘会话
```

`--server` 或 `BAS_SERVER` 可指定连接目标，默认 `local`。本地 IPC 不需要配对；
TCP 保留但需要显式开启，格式与升级方式见 [服务连接类型](docs/connections.md)。
归档停止后端并保留会话与历史；恢复时按 Agent 能力加载原会话。默认 `localdb` 持久化
会话与归档状态，`memory` 模式在重启后丢失。

## 架构要点

### 复用 ACP 定义

会话交互直接使用 `bas::acp` 的请求、内容、工具、配置选项与更新类型。
gRPC 用 `AcpPayload` 承载 SDK 的 JSON；Bas 只定义会话管理、回合接收确认和游标信封。
外部 ACP 驱动直接转发更新，内置 echo 后端也产生同样的 ACP 更新。
类型映射、客户端示例和兼容性说明见 [RPC 接口](docs/rpc.md)。

### 会话、attach 与游标

- 会话 id 为 UUID，元数据包含后端 id、发起用户与主机、工作目录、标题、状态。
- `detach` 只丢弃 `Attachment`；agent loop 继续运行，事件继续进入会话的回放窗口。
- 无游标的 `attach` 先回放最近 N 条**可回放事件**，随后转入直播；指定游标时回放其后
  全部保留事件，`--replay` 不限制续传数量。历史快照和直播订阅在同一锁内建立。
- 游标从 1 开始单调递增且不复用；客户端把最后看到的游标传回 `Attach` / `LoadHistory`
  即可续传。`Cursor::START`（0）表示“第一条之前”。
- ACP 内容块、工具局部更新和配置更新都写入历史；直播与回放消费相同的事件。
  客户端按游标去重、按 ACP 语义累积内容，不再依赖独立的 Delta / Message 协议。
- 标准 ACP `Notice` 仅实时广播，游标为 0；不写历史，也不改变最后可回放游标。
- 客户端落后超过广播容量时收到 `DATA_LOSS`，可以按最后游标重新 attach。该游标之后
  的历史若已淘汰，续传仍返回 `DATA_LOSS`；省略游标可重新读取当前保留窗口。
- 同一会话只允许一个活跃回合；忙碌时提交返回 `FAILED_PRECONDITION`。后端故障产生
  可回放的 `Failed` 事件，关闭流，已停止或失败的会话不会被迟到事件重新激活。

### 持久化

守护进程默认使用 SQLite 保存会话元数据与可回放事件；Agent 自身负责执行上下文的恢复。
显式 `memory` 模式只保留有界回放窗口（`BAS_HISTORY`）。

### systemd

- 日志：`BAS_LOG_BACKEND=journald` 时直接写 journald（`tracing-journald`）。
- 就绪与看门狗：`sd_notify` 发送 `READY=1` / `STOPPING=1`，`WatchdogSec=` 时按半个
  周期 ping。收到退出信号后关闭会话、后端和订阅，在 5 秒清理期限内退出。参考单元文件
  用户服务模板见 `contrib/bas-user.service`，安装与管理见 [安装说明](docs/install.md)。

## 当前状态

已可端到端运行（`bas` + `bast`），并有白箱测试覆盖模型、后端目录、会话回放、
游标分页与 protobuf 往返。

会话顺序、回合互斥、任务回收、ACP 故障和配置覆盖已补充回归测试，设计与接口变化见
[Bas 框架修订](docs/design/bas-review.md)。可运行 `cargo test --all-targets` 验证，其中
进程集成测试需要绑定本机回环端口，ACP 进程测试在 Unix 上使用一个受控的 shell 协议端。

仍在演进的部分：

- **嵌入式 agent**：`src/bin/bas/agent/embedded.rs` 目前是 echo 占位实现，用于在真实 agent 落地前
  打通全链路；替换 `Echo` 即可，会话层无需改动。
- **ACP 后端**：已实现初始化、创建会话、prompt、cancel、配置选项和模式设置，
  文件读写及权限回调直接复用 ACP 类型。内容、工具、plan、commands、mode、usage、
  compaction 等更新通过 RPC 与历史传输，Notice 仅实时广播。
  客户端托管的 `terminal/*` 执行回调尚未实现。
  协议保留富内容和标准 ACP 交互审批接口。
- **权限**：默认拒绝工具权限申请；`--permissions allow` 可开启自动批准。该策略不隔离
  后端进程的文件访问。审批客户端可订阅权限请求并选择允许／拒绝选项。
- **传输**：默认同用户本地 IPC（Unix socket / Windows Named Pipe），TCP 可显式开启；
  支持多个监听入口，见 [服务连接类型](docs/connections.md)。
- **远程接入**：[手动证书 mTLS](docs/mtls.md) 已实现；配对、账号服务和 Relay 仍按分期推进；
  一个服务实例提供共享执行环境，不在服务端内部做多用户隔离。范围、认证方向及验收场景见
  [远程接入、账号服务与 Relay 规划](docs/design/remote-access.md)。
