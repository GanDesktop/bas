# 安装与后端配置

## Linux / NixOS 安装

在仓库根目录执行，Nix 需启用 flakes：

```sh
make install-bas-service
bast agents
```

`make install` 或 `make install-bas` 安装 `bas` 和 `bast`，不启用服务。
`make install-bas-service` 同时启用或重启 systemd 用户服务。
安装脚本使用包含未提交源码的快照，不包含环境文件或构建产物。
可通过 `NIX_PROFILE_OPTIONS='--profile /path/to/profile -L'` 指定 profile。

源码已加入 Git 时，也可执行 `nix profile install .#bas`。
NixOS / Home Manager 可将 `inputs.bas.packages.${pkgs.stdenv.hostPlatform.system}.bas`
加入 `environment.systemPackages` / `home.packages`，服务另行声明。

### 安装独立 ACP 测试 Agent

```sh
make install-acp-test-agent
acp-test-agent --help
readlink -f "$(command -v acp-test-agent)"
```

该目标只安装独立测试 Agent，不修改服务或配置。将得到的绝对路径填入
`daemon.json` 的自定义 Agent `command.program`，参数为 `["--protocol", "v1"]`。
通过 `bast session create --agent acp-test-agent` 创建会话，再发送 `/help` 查看命令。
测试不需要模型或 API key。详见 [测试 Agent 文档](../acp-test-agent/README.md)。

## 从 ACP Registry 安装后端

```sh
bast agent catalog
bast agent catalog --refresh
bast agent install codex-acp
bast agent login codex-acp
bast agent install opencode
bast agent install pi-acp
bast agent check pi-acp
```

BasT 自己显示的表单和 URL 确认在 Windows 使用可中断的 Reedline/Emacs 行编辑，
不保存输入历史。Enter 提交一行，Ctrl-C 取消；空行 Ctrl-D 或 Ctrl-Z 表示 EOF，
Ctrl-Z 前已有文字时返回该行。服务取消输入或连接失败后，先等待读取退出并恢复
控制台模式，再交接下一次输入；进度消息仍实时显示。若读取无法退出或模式恢复失败，
本进程停止复用 stdin 并报错，不再启动另一个读取者。
这些键绑定不作用于 Agent 自己启动的登录程序；Unix 仍使用原有终端输入行为。

安装目录、版本记录和 Registry 缓存由 daemon 管理。优先选择当前平台 binary，
其次使用 npm；npm 安装隔离在后端版本目录，Node 优先复用 daemon 环境中的可用版本，
缺少时下载经过校验的私有 runtime。Pi 的 npm 安装包含它需要的 Pi 本体。
目前 uvx 分发仍会显示在目录中，并标记暂不支持安装。

官方目录与格式见 [ACP Registry](https://github.com/agentclientprotocol/registry)。
Registry URL、数据目录和 ELF 修补环境变量见下文；这些选项适合测试、镜像或系统打包，
日常使用无需设置。

## 配置 ACP 后端

```json
{
  "agents": [
    { "id": "embedded", "kind": "embedded" },
    {
      "id": "codex",
      "kind": "acp",
      "description": "用于代码审查与实现",
      "command": { "program": "codex-acp", "args": [] }
    },
    {
      "id": "opencode",
      "kind": "acp",
      "command": { "program": "opencode", "args": ["acp"] }
    },
    {
      "id": "pi",
      "kind": "acp",
      "command": { "program": "pi-acp", "args": [] }
    }
  ],
  "acp": {
    "idle_timeout_secs": 1800,
    "default_agent": "codex",
    "pool_size": 1,
    "pools": { "opencode": 2, "pi": 0 }
  }
}
```

ACP 后端可设置 `cancellation_grace_ms`，表示发送取消后等待本轮结束的宽限期，
单位为毫秒，必须大于零，默认 `5000`。例如需要较长工具清理时间的后端可以设置
`"cancellation_grace_ms": 15000`。超过期限仍会判定后端不可继续并回收进程；
该选项不会关闭强制清理机制。

选择目录的优先级为：`--agents` / `BAS_AGENTS` 简写目录、显式
`--agents-file` / `BAS_AGENTS_FILE` 旧格式数组文件、后端 `daemon.json`。
同一个参数的命令行值优先于环境变量。
显式指定的文件不存在、JSON 无效、重复 ID 或 ACP 缺少启动程序都会使启动失败；
默认 `daemon.json` 不存在时只读导入同目录旧 `settings.json` 的 `daemon`，
保留其中的 Agent 与 ACP 策略；旧统一文件也不存在时才导入 `agents.json`，都不存在才使用内置后端。
首次成功保存 Agent 配置生成独立 `daemon.json`，不删除或改写旧文件。
新文件一旦被运行中的服务观察到，格式错误或消失都不会退回旧配置源。
显式路径不查找相邻旧文件；显式指向旧统一文件时只更新其中的 `daemon`，
保留其他字段。前端保存旧统一文件也会保留其不再解析的 `daemon` 字段。

显式配置优先于默认值：`"idle_timeout_secs": null` 关闭空闲回收；
`"pool_size": 0` 关闭自动预热，但不会关闭 `pools` 中单独配置的备用连接。
要完全关闭预热，还需将 `pools` 设为 `{}`。已有文件中显式保存的 `null`／`0` 不会被新默认值覆盖。

这里的“上次使用”是成功创建并注册 ACP 会话或接受用户回合时记录的 Agent，
不包括查询历史、改标题、连接检测、预热和后台通知。默认 `localdb` 在创建注册成功后记录选择，
prompt 则将选择与接受事件原子提交。尚未注册就被取消的创建不改变选择。
记录写入不依赖 RPC 响应是否送达；并发操作按选择记录的存储提交顺序排列，
不按客户端收到响应的顺序排列。重启保留最后一次已提交的选择。
创建后的选择写入失败只告警，不撤销已注册的会话；预热继续使用上次有效选择。
它属于服务端数据库，不写回客户端设置；`memory` 模式则只保留到本次 daemon 退出。

### daemon 重启时的 ACP 启动范围

重启先恢复会话元数据，不按历史会话数重新启动进程。后台预热只按以下规则计算备用数量：

1. `pools` 中明确指定且数量大于零的 ACP Agent。
2. 自动目标优先选 `default_agent`；没有配置时读取数据库里的最近使用 Agent，
   但它必须仍在当前目录中且仍是 ACP 后端。
3. 自动目标使用 `pool_size`；同 ID 的 `pools` 条目优先（包括 `0`），两者不相加。
4. 没有有效最近使用记录时不猜测替代 Agent；显式的默认目标和独立池仍可预热。

例如默认配置下，上次使用的是 A，数据库中有 A 的十个会话和 B 的五个会话：
重启只预热 **一个尚未绑定会话的 A 连接**，不拉起十五个旧会话进程。
首次访问需要后端的操作才按原 ID load／resume；查看列表、阅读历史不触发恢复。
停止、归档和失败的会话不会自动启动；重启前未完成的回合沿用中断记录，不自动重发 prompt，
避免重复执行工具或外部操作。预热在后台进行，不等待它完成才开始服务。

首次从旧数据库升级时没有精确的最近使用记录，不从会话更新时间或历史正文猜测；
下一次实际使用后会开始记录。若希望此时立即预热，请显式配置 `default_agent` 或 `pools`。

### 空闲回收与备用连接

空闲回收每 5 秒检查一次，只卸载支持 `session/load` 或 `session/resume` 的 ACP 进程，
保留 Bas 会话 ID、历史及实时订阅。下次发送请求时恢复原 ACP 会话，不创建替代对话。
前台回合、进行中的 ACP 请求、待审批／用户交互，以及未确认空闲的子会话都会阻止回收。
普通客户端订阅和历史阅读不会阻止回收。回收先停止接纳新操作、提交已排队的事件，
再释放进程，不发送终止会话的 `session/close`。Agent 的上下文恢复仍依赖其声明的恢复能力；
协议未报告的后台任务无法由 Bas 推断；此类后端应显式将 `idle_timeout_secs` 设为 `null`。

池大小表示**已初始化、尚未绑定业务会话的备用连接数量**，不是运行中会话数上限。
创建／恢复会话会独占取走一个备用连接，并在后台补齐；用过的连接不回池，避免上下文串用。
预热提前完成进程启动和 `initialize`，仍需正常执行 `session/new` 或 load／resume。
备用进程本身也占内存，建议从每个常用 Agent 一个开始；缩小目标、改动 Agent 启动配置
或关闭 daemon 会释放相应备用连接。预热失败会退避重试，不阻止普通冷启动。

代码中使用同一 `bas::config::AcpLifecycle` 类型配置 daemon 的 `Config.acp`，
或经 `Registry::set_acp_policy` 校验后更新；`Default` 与配置文件省略字段的行为一致。

`ListAgents` 只返回当前可找到可执行文件的工具条目；缺失、目录或无执行权限的 ACP 程序会被隐藏，
但配置文件仍保留，安装工具或修正路径后下次刷新即可重新出现。
查找与实际启动共用 daemon 的 PATH（含该 Agent 的 `env.PATH` 覆盖），不使用调用方终端的 PATH。
`bast agent check <id>` 验证 ACP 握手和临时会话。
检测结束不会保持连接，也不验证模型请求。登录是单独的认证操作，检测命令不会启动登录。
使用此检测功能需要同时升级 `bast` 与 `bas`，旧版 daemon 会提示不支持。

修改 Agent 目录不需要重启。升级 daemon 可执行文件仍需重启；新版默认使用 `localdb`，
重启后可读取新建会话的历史并按 Agent 能力恢复对话。**旧版只存于内存的会话不能迁移**，
升级时重启仍会停止这些会话；不要以重启作为设置保存动作。

当前 SQLite 版本为 schema 8。部署前备份数据库并预留迁移所需的磁盘空间；
仅支持 schema 5／6／7 的旧 bas 不能直接打开升级后的文件，回滚程序时应同时恢复对应备份。
schema 7 在事务内将旧事件及已有压缩块转入统一的 `history_chunks`，删除旧历史表；
保留原始正文、游标和去重回执，不重新压缩旧块。首次启动可能因数据量较大而耗时，
首次访问会话时会重建段目录。失败整体回滚，不丢弃旧历史。
schema 8 为段目录增加有界提示词预览与时间字段，仅清除并重建派生索引，不改写正文 chunks。
完整回合导航与直接定位接口需要更新服务器与调用该接口的客户端；旧服务端不会回退为不完整的标记列表。
详见 [完成回合的物理存储](rpc.md#完成回合的物理存储)。

## Codex

按 [Codex ACP 官方安装说明](https://github.com/agentclientprotocol/codex-acp#installation)
独立安装 `@agentclientprotocol/codex-acp`。例如在满足上游运行环境要求后执行：

```sh
npm install -g @agentclientprotocol/codex-acp
command -v codex-acp
codex-acp cli login
```

上游适配器自身附带兼容的 Codex CLI；其版本、升级和认证由上游工具及用户管理，
Bas 不锁定、注入或覆盖 CLI 路径。需要自定义 CLI 时遵循上游的 `CODEX_PATH` 说明。
已有 Codex 登录可继续使用；登录命令应在运行 bas 的同一用户下执行。

把 `command -v codex-acp` 得到的、独立安装的可执行文件绝对路径填入
`command.program`，`command.args` 为 `[]`；确认 daemon 的 PATH 能找到它时也可写 `codex-acp`。
全局 npm 的安装目录不一定在 systemd 用户服务 PATH 内；不要把 shell 中能启动等同于服务能启动。
保存后可执行 `bast session create --agent codex --cwd "$PWD"`，
或在 终端创建 Codex 会话。模型等配置选项与 `/` 命令由后端提供。
模型与认证继续使用现有 Codex 配置，bas 不生成另一套凭据或修改 Codex 权限设置。
适配器实现见 [codex-acp](https://github.com/agentclientprotocol/codex-acp)，
底层接口见 [Codex App Server 文档](https://learn.chatgpt.com/docs/app-server)。

`codex-acp 2.1.1` 的权限 **Mode** 与 **Collaboration mode**（Default／Plan）独立。
其默认权限预设为 **Auto review**，每次发起回合都会显式传入审批与沙箱设置，
覆盖 Codex 全局配置中的同名值。若要由用户审批，选择 **Workspace access**
（允许工作区编辑，越界写入与联网请求审批）或 **Read-only**；
**Full access** 使用 `approval_policy=never`，不会弹出工具审批。
配置字段含义见 [Codex 配置参考](https://learn.chatgpt.com/docs/config-file/config-reference)。
权限预设及环境变量读取见
[`codex-acp 2.1.1` 的 AgentMode 定义](https://github.com/agentclientprotocol/codex-acp/blob/68d7d2d5ddfc0ed5746f9f6130892dda685e65dd/src/AgentMode.ts)。

适配器在新建及恢复会话时重新读取 `INITIAL_AGENT_MODE`，默认值为 `agent`。
可在 `bas.env` 中加入以下非秘密环境变量，使权限初始值为人工审批的工作区模式：

```sh
INITIAL_AGENT_MODE=workspace-write
```

修改服务环境文件后需重启 bas；已有 Agent 进程保留启动时的环境。
其他可选值为 `read-only`、`agent`、`agent-full-access`。
自定义 Agent 也可在 `daemon.json` 的该 Agent `env` 中设置；
修改目录安装记录以外的 Agent 配置会使它成为自定义注册，后续目录更新将报告注册冲突，
因此目录管理的安装宜使用服务环境设置默认值。

### 从旧版捆绑安装迁移

`--with-codex` 已移除，使用它会在任何安装动作前报错并指向本文；改用
`make install-bas-service`。更新 Bas 不会卸载旧适配器，也不会修改已有配置。
以前安装的 `~/.nix-profile/bin/codex-acp` 可能仍然存在，已有配置也可能继续指向它。
请按官方文档独立安装适配器，检查 `command -v codex-acp` 的结果和服务实际使用的路径，
再自行把后端命令改为所选安装路径。旧 profile 条目由用户决定是否保留；
本次迁移不自动删除任何包、凭据或配置。已有 `CODEX_PATH` / `env` 同样保留，请自行确认是否仍需要。

## OpenCode 与 Pi

按 [OpenCode 官方安装说明](https://opencode.ai/docs/) 独立安装 OpenCode；
它原生支持 ACP，使用 `opencode acp`，见[官方 ACP 文档](https://opencode.ai/docs/acp/)。
配置程序为 `opencode`（或其绝对路径），参数为 `["acp"]`。

Pi 通过 ACP Registry 目录安装并登录，或使用：

```sh
bast agent install pi-acp
bast agent login pi-acp
bast agent check pi-acp
```

Registry 安装负责适配器及其所需的 Pi 本体，成功后自动注册，不需要全局 npm 安装、
仓库开发 shell 或手写模型配置。认证沿用 Agent 原生流程。
已有独立安装的 ACP 程序仍可通过上文的通用自定义配置接入；Pi 原生的
`pi --mode rpc` 不是 ACP，需使用 [pi-acp 适配器](https://github.com/svkozak/pi-acp)。
`pi-acp 0.0.34` 仅在 Pi 扩展主动调用 `ctx.ui.select()`／`confirm()` 时转发
ACP 权限请求；普通助手文字中的选项和内置工具执行不会自动弹卡。
如需测试这种请求，可参考 [Pi 官方审批扩展示例](https://github.com/earendil-works/pi/blob/v1.1.0/packages/coding-agent/examples/extensions/permission-gate.ts)。
该适配器尚未将扩展的 `input`／`editor` 转为 ACP 表单。
配置命令不执行安装、下载、登录或模型请求。
如果服务找不到程序，可填写绝对路径；终端的 PATH 不代表 systemd 服务的 PATH。

## Registry 与运行环境覆盖

| 环境变量 | 默认 / 用途 |
| --- | --- |
| `BAS_ACP_REGISTRY_URL` | 官方 Registry CDN；允许 HTTPS，HTTP 仅限 loopback 测试服务 |
| `BAS_ACP_DATA_DIR` | 用户数据目录下 `bas/acp`；覆盖时必须为绝对路径 |
| `BAS_ACP_PATCHELF` | 可选的 patchelf 程序路径，Nix daemon 安装包自动提供 |
| `BAS_ACP_ELF_LOADER` | 托管 ELF 的动态解释器，Nix 安装包自动提供 |
| `BAS_ACP_ELF_LIBRARY_PATH` | 托管 ELF 的运行库路径，Nix 安装包自动提供 |

这些 ELF 选项只作用于 Bas 下载的托管程序，不改动用户独立安装的程序。
从源码运行 NixOS daemon 时可使用系统 Node 和已安装的后端；需要验证下载的 ELF 时，
设置与 Nix 安装包相同的修补环境。

## 服务管理

```sh
systemctl --user status bas.service
journalctl --user -u bas.service -f
systemctl --user stop bas.service
systemctl --user start bas.service
```

用户服务默认随用户会话启动，支持就绪通知、watchdog 与异常退出后重启。
可在 `~/.config/bas/bas.env`（或 `$XDG_CONFIG_HOME/bas/bas.env`）中设置服务环境变量，例如：

```ini
BAS_LISTEN=local
BAS_HISTORY=8192
```

服务的 PATH 包含 `~/.local/bin`、`~/.nix-profile/bin` 和系统软件路径；
其他位置的 ACP 程序应在 `daemon.json` 的 `agents` 中使用绝对路径。此运行时环境文件不进入 Nix 安装包。

同名版本更新时重新运行安装目标即可；跨改名升级先按 [手动迁移说明](rename-migration.md) 处理。
脚本会替换同名的旧 profile 条目，`install-bas-service` 还会重启
用户服务，让新可执行文件立即生效；后端配置保留。

```sh
make install-bas-service
```
卸载时先执行 `systemctl --user disable --now bas`，再执行 `nix profile remove bas`。
外部工具由用户单独管理。
