# 安装与运行

## BAS

Linux / NixOS 上，在仓库根目录执行：

```sh
make install-bas-service
bast agents
```

`make install-bas` 只安装 `bas`、`bast`；`make install-bas-service` 同时启用 systemd 用户服务。
安装使用当前源码快照，包含未提交源码。NixOS / Home Manager 可将 flake 的 `bas` package 加入系统或用户软件列表，另行声明服务。

```sh
systemctl --user status bas
journalctl --user -u bas -f
systemctl --user restart bas
```

服务环境文件为用户配置目录中的 `bas/bas.env`；修改环境后重启服务。
卸载前运行 `systemctl --user disable --now bas`，再运行 `nix profile remove bas`。

## Agent

```sh
bast agent catalog
bast agent install codex-acp
bast agent login codex-acp
bast agent check codex-acp
bast session create --agent codex-acp --cwd "$PWD"
```

安装在 daemon 主机进行，优先选择平台 binary，其次 npm；缺少 Node 时可安装私有 runtime。
认证沿用 Agent 的原生流程。更新与移除注册不删除已有会话、原生凭据或已下载版本。
其他 Agent 从目录选择，例如 `opencode`、`pi-acp`。uvx 分发目前不能自动安装。

也可配置自行安装的 ACP 程序。用户配置目录下的 `bas/daemon.json` 示例：

```json
{
  "agents": [
    {
      "id": "my-agent",
      "kind": "acp",
      "command": {
        "program": "/absolute/path/to/agent",
        "args": ["acp"]
      }
    }
  ],
  "acp": {
    "idle_timeout_secs": 1800,
    "default_agent": "my-agent",
    "pool_size": 1,
    "pools": {}
  }
}
```

参数以所用 Agent 的 ACP 入口为准。daemon 使用自己的 PATH、文件系统和登录环境；
终端能找到程序不代表 systemd 服务也能找到，必要时使用绝对路径。
配置每秒检查更新，新会话使用新目录，现有会话保留已绑定后端。

`--daemon-settings-file` / `BAS_DAEMON_SETTINGS_FILE` 可指定配置文件。
`--agents` / `BAS_AGENTS` 固定覆盖 Agent 目录；`--agents-file` 支持独立注册数组。
固定目录模式下不能通过 RPC 编辑注册或托管安装。

## 会话与存储

```sh
bast session list
bast prompt <id> 帮我看看这个目录
bast attach <id>
bast history <id>
bast cancel <id>
bast session archive <id>
bast session restore <id>
```

`attach` 退出只断开订阅。归档停止会话并保留历史，恢复需要 Agent 支持加载原会话。
`bast session remove <id>` 删除 BAS 会话与历史。

默认使用 SQLite，位于用户状态目录的 `bas/sessions.sqlite3`；
`--localdb-path` 可修改位置，`--storage memory` 使用进程内有界历史。
数据库应定期备份；升级会自动执行 schema 迁移，回退程序时需恢复对应版本的备份。
BAS 历史不替代 Agent 自身的执行上下文。

## 后台运行选项

| 参数 | 默认值 | 作用 |
| --- | --- | --- |
| `--agent-management` | `true` | 在线目录、安装、移除与配置 RPC |
| `--acp-maintenance` | `true` | 后台预热与空闲回收 |
| `--acp-maintenance-interval` | `5` 秒 | 后台扫描间隔 |
| `--shutdown-grace` | `5` 秒 | 关闭宽限期 |
| `--permissions` | `deny` | 无交互客户端时的固定工具审批策略 |

ACP 配置中，`idle_timeout_secs: null` 关闭空闲回收；`pool_size: 0` 关闭默认预热，
`pools` 可单独指定 Agent 的备用连接数。预热只初始化连接，不创建会话或重发输入。
Agent 的 `cancellation_grace_ms` 默认 5000，取消后未在宽限期结束的实例会被回收。

Registry 与运行目录可通过 `BAS_ACP_REGISTRY_URL`、`BAS_ACP_DATA_DIR` 覆盖。
Nix 包提供托管 ELF 所需的 `BAS_ACP_PATCHELF`、`BAS_ACP_ELF_LOADER`、`BAS_ACP_ELF_LIBRARY_PATH`。
完整启动参数见 `bas --help`，连接方式见[服务连接](connections.md)。

## 独立 ACP 演示 Agent

```sh
make install-acp-test-agent
acp-test-agent --help
```

将其绝对路径注册为 ACP Agent，参数使用 `["--protocol", "v1"]`，发送 `/help` 查看演示。
它不调用模型，用于手工检查客户端的协议与界面，详见[命令说明](../bin/acp-test-agent/README.md)。
