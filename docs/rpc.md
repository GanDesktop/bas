# BAS 接口约定

[架构边界](design/bas.md)

接口定义以 [bas.proto](../crates/protocol/proto/bas/v1/bas.proto) 为准；Rust 客户端为 `bas-client`。
BAS 与外部 Agent 协商 ACP v1，复用 SDK schema；`AcpPayload.json` 保存原生 UTF-8 JSON，
完整 JSON-RPC 消息由 `AcpMessage.json` 承载。公开会话 ID 属于 BAS，Agent ID 由后端映射。

## 客户端连接

1. 通过 BAS 管理接口创建、选择或恢复会话。
2. `AcpTransport.Prepare` 指定会话或认证目标，取得有效期 60 秒的一次性连接 ID。
3. 携带 `bas-acp-connection` metadata 调用双向 `Open`。
4. 使用 ACP SDK 完成 `initialize`、`session/load`，随后发送请求。

会话连接只能访问准备时指定的会话。`session/load` 的 cwd 必须一致，MCP 配置在 BAS 侧完成。
网关支持 prompt、cancel、set_mode、set_config_option；会话创建、删除、归档和历史分页使用管理接口。

`interactive=true` 接收权限和 elicitation 回调。多个前端可同时连接，BAS 只转交第一个有效回答。
前端断开不取消已受理执行或待处理交互。文件操作位于 daemon 主机；网关不把文件或工具终端转交前端执行。

## 提交与观察

| 接口 | 语义 |
| --- | --- |
| ACP `session/prompt` | 等待回合结束，返回原生 `PromptResponse` 或 ACP 错误 |
| BAS `Prompt` | 提交带稳定 turn ID 的输入，返回接受凭据或明确拒绝 |
| `GetPromptAcceptance` | 查询该 turn 是否已受理；未知不等于失败 |
| `ObserveAcp` | 观察输入、Agent 更新及原生响应 |
| `Attach` | 观察 BAS 会话、工作区及运行状态 |

同一提交 ID 和相同内容可取回原凭据，不重复执行；内容冲突被拒绝。
接受凭据包含 turn、提交游标及发往 Agent 的 ACP request ID，不表示执行成功。
响应丢失时查询凭据，不自动生成新 ID 重发。

客户端从历史与 ACP 流构建对话，不从回执插入消息。两条观察流独立推进游标；
BAS 故障不是 ACP 响应，Agent 请求错误也不是服务故障。
使用 `ObserveAcp` 的前端可设置 `callbacks_only=true`，避免 SDK 连接重复回放聊天内容。

## 历史

| 查询 | 用途 |
| --- | --- |
| `LoadHistory(after, limit)` | 原始事件按游标向后分页 |
| `LoadHistoryWindow(before, limit)` | 从最近内容向前加载，limit 单位为用户回合 |
| `LoadHistoryOverview` | 回合起点、时间、有界提示词预览及快照高水位 |
| `LoadHistoryWindowAt(cursor)` | 定位包含某游标的回合，并返回下一逻辑页起点 |

窗口返回原始 events、窗口前必要的 context、向前翻页的 before，以及快照高水位 cursor。
消息、工具和终端依赖会扩大窗口，不能为了页面大小截断一条消息或跳过依赖。
`before` 必须为回合起点或边界值，回合中间的游标返回 `INVALID_ARGUMENT`。
向前加载后，以较旧窗口的 context 和拼接事件重建显示状态。

读取最新窗口后，从其高水位开始订阅两条实时流，避免丢失查询期间的新事件。
较旧页面的高水位不能替代现有实时游标；定位到历史中间时，也不能把实时后缀直接拼过未加载区间。
历史中的权限请求只用于观察，不能重新发起交互。

显式游标要求完整续传，缺失历史返回 `DATA_LOSS`，未来游标返回 `INVALID_ARGUMENT`。
无游标的 attach 可回放最近 N 条；带游标时 replay 不限制续传数量。
单条 gRPC 消息上限为 32 MiB。原始分页同时遵守条数与字节预算，窗口超限返回 `RESOURCE_EXHAUSTED`。
Notice 只实时广播，cursor 为 0，不推进持久游标或重复回放。

SQLite 的 `history_chunks` 保存正文，`history_segments` 与 `history_state` 是可重建的导航与依赖索引。
完成回合可无损归并和压缩；每次提交成功后才确认与广播。内存模式无法恢复已淘汰的依赖。

## 会话与工作区

`Session.revision` 是快照包含的最后事件位置；按版本合并快照和更新，不能用墙上时间判断新旧。
`last_reply` 用于判断新回复，标题或归档变更不表示新回复。
`runtime` 单独描述当前运行实例的健康状态，存储失败不改写最后已提交的状态。

| 操作 | 行为 |
| --- | --- |
| `CloseSession` | 停止实例，保留记录和历史 |
| `RestartSession` | 启动已停止会话，或从最后提交快照恢复存储失败的实例；不重发输入 |
| `SetArchived(true)` | 停止并归档；默认列表隐藏，历史仍可读 |
| `SetArchived(false)` | 加载原 Agent 会话，成功后取消归档；失败保持原记录 |
| `DeleteSession` | 停止并删除 BAS 会话与历史，不删除 Agent 原生历史 |
| `ListAgentSessions` / `ImportAgentSession` | 发现并导入原 Agent 会话；同一原始身份去重，不创建替代会话 |
| `DeleteAgentSession` | 删除 Agent 历史；活跃的已导入会话须先关闭 |
| `SetSessionDirectories` | 空闲时替换副目录并重载原会话；失败保持旧值，结果未知则使实例失败 |

恢复需要 Agent 支持 load/resume；已有会话的 load 回放不重复入库。
重启时未完成回合记为中断，不能自动重放工具操作。

工作区独立于 Agent 存在。归档工作区会停止并归档关联会话；恢复工作区不自动恢复这些会话。
删除非空工作区需要 `force=true`，级联删除 BAS 会话而不删除用户文件。
级联操作失败时保留工作区及已完成的子操作，调用方刷新后再决定是否重试。

`OpenWorkspace` 使用 provider ID、schema version 和 JSON 参数；旧版本返回 `FAILED_PRECONDITION`。
provider 的 JSON Schema 使用本地 `$defs`，不读取远程引用；默认值由 provider 应用。
`x-bas.type = ServerDirectoryPath` 表示服务端路径，前端不能按本机路径规则改写。
`BrowseServerDirectories` 在创建工作区前即可使用；`QueryWorkspaceChoices` 查询 provider 的动态选项。

## 配置、安装与认证

配置读接口返回 profile 和 revision；保存必须携带它们。profile 不符返回 `FAILED_PRECONDITION`，
过期 revision 返回 `ABORTED`。注册、ACP 策略分别保存，不覆盖前端配置。

安装进度的 `AgentOperation.Phase` 是公共 protobuf 枚举：Resolving、Downloading、Extracting、
Runtime、Installing、Registering、Complete；展示文案由客户端生成，未知值按 Unspecified 处理。
`complete=true` 且 `spec_json` 非空表示注册完成。取消流可中止未完成安装，但最终注册可能已经提交，需刷新目录确认。

`CheckAgent`、认证和登出核对 `expected_spec_json`。程序存在、ACP 握手成功、会话可用是不同状态。
`AuthenticationRequired` 保留初始化能力和原始认证错误，客户端据此展示 Agent 广告的方法。
ACP 错误通过 gRPC status details 保留，可用共享 `acp_error` 解码。

认证连接支持 ACP authenticate 与 elicitation；终端执行计划通过 `WatchAuxiliary` 下发，
退出码通过 `TerminalResult` 回传，应先订阅辅助流再认证。
终端启动或 URL 打开不表示登录成功，需等待 Agent 认证响应和会话检测。
凭据输入和保存由 Agent 原生流程负责。

## 交互

`WatchPermissions` / `WatchElicitations` 返回当前待处理快照，Respond 接口按 BAS 生成的 request ID 回答。
选项和表单必须符合原请求；无效回答保留请求，已失效请求返回 `FAILED_PRECONDITION`。
URL 接受后仍等待对应完成通知。前端重连读取新快照，不重放旧回答或自动打开 URL。

回合取消、Agent 连接替换或会话关闭清理待处理交互；旧连接的迟到结果不能影响新连接。
会话曾进入交互审批后，无订阅期间继续等待，不能因前端断开而退回自动允许。
子会话先由已知父会话公告，拥有独立内容和状态；子消息不合并到主回答。
