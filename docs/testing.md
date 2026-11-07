# 验证

```sh
cargo fmt --all -- --check
cargo build --workspace --bins --locked
cargo test --workspace --all-targets --locked -- --nocapture
cargo clippy --workspace --all-targets --locked -- -D warnings
```

主端到端测试位于 `bin/bast/tests/e2e/`，启动真实 `bas` 和 `bast`，使用内置 echo 后端、真实 IPC/TCP/mTLS 和 SQLite。
先构建 workspace 二进制；测试从 `bast` 同一产物目录启动 `bas`。
每个场景使用独立配置、工作区和数据库；按就绪日志、协议事件或进程退出同步，超时只用于失败截止。

覆盖会话断线与恢复、提交去重、配置冲突、工作区生命周期、ACP 网关作用域、传输认证，以及 Unix 上的进程崩溃和存储故障恢复。
外部模型、Agent 原生登录、下载安装及系统服务安装不在这些自动场景中。

每次运行保留 `target/e2e/<场景>-<随机标识>/`，包含启动命令、日志、CLI 输出、请求结果、配置和 SQLite 数据库。
失败和成功都保留工件，测试输出打印路径；CI 上传这些目录。`build.txt` 的含义见[构建标识](build-tags.md)。可用相同测试命令和场景过滤器复现，例如：

```sh
cargo test -p bast --test e2e sessions:: --locked -- --nocapture
```

演示 Agent 的协议测试位于 `bin/acp-test-agent/tests/`，工件同样写入 `target/e2e/`。
单元测试仅覆盖历史依赖计算、表单约束、分页字节边界和压缩块异常，位于对应模块的 `tests.rs`。
新增测试遵循 [agents.md](../agents.md#组织测试)。
