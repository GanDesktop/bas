# ACP Test Agent

一个用于测试 ACP（Agent Client Protocol）客户端的脚本化 Agent。它同时实现 ACP
v1 与 v2，没有模型、网络或真实 AI 能力：prompt 文本被当作命令，执行固定的功能演示。
命令覆盖会话生命周期、流式消息、工具调用、计划、权限、Elicitation、文件、终端、配置、
Notice、压缩、扩展方法等，便于手动验证客户端 UI 与协议处理。

## 快速开始

```sh
cargo run -p acp-test-agent
```

进程通过 stdio 收发 JSON-RPC，stdout 只用于协议；诊断信息输出到 stderr。
`cargo run -p acp-test-agent -- --help` 查看启动参数。

把它配置成任意 ACP 客户端的 Agent 即可。在当前 Bas 检出中，`bas` 的 Agent 目录使用
如下 `AgentSpec` 形状（`program` 指向编译产物，建议用绝对路径）：

```json
{
  "id": "acp-test-agent",
  "kind": "acp",
  "command": {
    "program": "/absolute/path/to/bas/target/release/acp-test-agent",
    "args": []
  }
}
```

创建或恢复会话时会主动发布命令列表。发送 `/help` 可查看全部脚本命令。普通文本按 `/echo <text>` 处理。

### Linux / NixOS 安装与终端测试

在当前仓库根目录执行：

```sh
make install-acp-test-agent
acp-test-agent --help
readlink -f "$(command -v acp-test-agent)"
```

安装程序使用源码快照，包含未提交文件；只安装测试 Agent，不修改 Bas 配置或重启 daemon。
`nix/acp-test-agent.nix` 从 workspace 中只构建和安装此程序。
仓库 flake 提供 package、app 和 check `acp-test-agent`；源码已经加入 Git 时也可执行：

```sh
nix build .#acp-test-agent
nix run .#acp-test-agent -- --help
```

在 `daemon.json` 的 `agents` 数组中加入上面的配置，程序使用安装后的绝对路径，
参数为 `["--protocol", "v1"]`。使用 `bast session create --agent acp-test-agent` 创建会话，
再通过 `bast prompt <session-id> /help` 和 `bast attach <session-id>` 检查输出。
测试取消可发送 `/sleep 10000`，然后执行 `bast cancel <session-id>`。

`/plan` 按客户端能力选择旧式 `plan` 或新版 `plan_update`，不会重复发布两份。
停止正在运行的计划会结束当前回合并移除计划；完成后的计划可用 `/plan clear` 清除。
`/rich` 的 `sample.md` 引用指向实际临时文件，可在同一主机打开；文件在 Agent 连接结束后清理。
`/help` 输出标准 Markdown 命令列表。

会话仅在此进程内存中保存；客户端重启 Agent 进程后不能恢复之前的测试会话。
本工具尚未实现 subagent 演示，不应用普通工具调用验收子 Agent 事件。
声明式 NixOS／Home Manager 安装和卸载方法见 [安装文档](../../docs/install.md#独立-acp-演示-agent)。

### 启动参数

| 参数 | 默认值 | 作用 |
| --- | --- | --- |
| `--page-size <N>` | `2` | `session/list` 每页返回的会话数；用于验证分页 UI |
| `--require-auth` | 关闭 | `session/new` 前必须先 `authenticate`（v1）或 `auth/login`（v2） |
| `--protocol <both\|v1\|v2>` | `both` | 限制 Agent 提供的协议版本，用于验证客户端的降级/回退逻辑 |

认证方法 id 为 `demo-login`。`--require-auth` 只拦截新建会话，不影响 `session/load`、
`session/resume` 等已有会话操作。`--protocol v1` 时 v2-only 客户端会在 initialize 阶段
失败；`--protocol v2` 则拒绝 v1-only 客户端，便于测试客户端的协议选择与回退路径。

## 工作方式

- 启动时使用 SDK 的 `Agent::protocol_router()` 同时注册 v1 和 v2 实现；
  首个 `initialize` 请求声明的版本决定由哪个实现接管连接。
- v1 与 v2 分别实现各自的 RPC、会话更新和作用域语义。v2 的 prompt 会先返回
  `messageId` 表示已接收，再通过 `state_update` 报告 `running` / `idle`；v1 的
  `session/prompt` 在回合结束后返回 `stopReason`。
- 会话保存在进程内存中，关闭进程即丢失；同一会话的 update 会按顺序记录，供
  `session/load`（v1）或带 `replayFrom: start` 的 `session/resume`（v2）回放。
- 每个 prompt 都在连接内单独 spawn，因此 `session/cancel`、权限请求和 Elicitation
  可以在回合执行期间正常到达。

## Prompt 命令

| 命令 | 说明 | v1 | v2 |
| --- | --- | --- | --- |
| `/help` | 列出命令，并发送 available commands 更新 | ✅ | ✅ |
| `/echo <text>` | 用多个 message chunk 流式回显 | ✅ | ✅ |
| `/rich` | text、image、audio、resource link、embedded resource | ✅ | ✅（额外发送 message upsert） |
| `/thought` | 先发送 thought chunk 再回答 | ✅ | ✅（额外发送 thought upsert） |
| `/tool` | tool call 生命周期、内容、diff、raw output | ✅ | ✅（upsert + content chunk + 多文件结构 diff） |
| `/plan [milliseconds]` | 推进三步计划（默认每步间隔 5000 ms），可用客户端停止按钮取消 | ✅ | ✅ |
| `/plan clear` | 清除演示计划 | ✅ | ✅ |
| `/permission` | 向客户端请求权限并回报结果 | ✅ | ✅ |
| `/elicit` | 发送表单 Elicitation | ✅ | ✅ |
| `/elicit-url` | 发送 URL Elicitation，并发送 `elicitation/complete` | ✅ | ✅ |
| `/fs` | 调用客户端 `fs/read_text_file` 与 `fs/write_text_file` | ✅ | 报告 v2 已移除文件回调 |
| `/terminal` | 终端演示 | ✅ 客户端托管 `terminal/*` | ✅ Agent 自有 terminal 更新 |
| `/sleep <ms>` | 等待指定毫秒数，便于测试取消 | ✅ | ✅ |
| `/config [model=fast\|deep\|on\|off\|toggle]` | 修改配置并发送 `config_option_update` | ✅ | ✅ |
| `/commands` | 重新发送 available commands 更新 | ✅ | ✅ |
| `/usage` | 发送 usage/cost 更新 | ✅ 回合响应也携带 usage | ✅ |
| `/notice` | 发送用户可见 Notice | 需要客户端声明能力 | ✅ |
| `/compact` | 发送上下文压缩 summary 与完成状态 | 需要客户端声明能力 | ✅ |
| `/info` | 更新会话标题与时间 | ✅ | ✅ |
| `/extension` | 发送自定义 `_demo/echo` 请求与 `_demo/notice` 通知 | ✅ | ✅ |
| `/fail` | 让回合以错误结束 | JSON-RPC error | `idle` 中的 error stop reason |
| `/stop <reason>` | 指定 stop reason | ✅（`error` 映射为 JSON-RPC error） | ✅（`error` 映射为 idle error） |

能力相关命令在客户端未声明对应能力时会返回说明文本，而不是伪造成功。

## 边界与验证

会话只保存在进程内存中。文件、终端和模型相关命令用于演示协议，不提供真实模型能力。
不支持 MCP 连接、subagent 或会话持久化。

```sh
cargo test -p acp-test-agent --all-targets --locked -- --nocapture
```

测试启动真实二进制，验证 v1 的认证、prompt 与回放，以及 v2 的认证和会话生命周期。
每个场景使用独立目录，结果保留在 `target/e2e/`。
