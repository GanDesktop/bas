# 破坏性改名与手动迁移

不支持旧命令、旧环境变量或旧 gRPC 名称，不提供兼容别名，也不会自动迁移用户数据。
仓库地址为 <https://github.com/4t145/bas>。

## 旧名 → 新名对照

本节保留旧名，仅供识别已有安装和手动迁移使用。

| 对象 | 旧名 | 新名 |
| --- | --- | --- |
| 项目 | BASR / Basic Agent Service in Rust | Bas / Basic Agent Service |
| daemon 命令 | `basrd` | `bas` |
| 终端命令 / 展示名 | `basrctl` | `bast` / BasT |
| Rust 包与库 | `basr` | `bas` |
| daemon / 共用环境变量前缀 | `BASRD_*`、`BASR_*` | `BAS_*` |
| 终端专用环境变量前缀 | `BASRCTL_*` | `BAST_*` |
| gRPC package / service | `basr.v1` / `Basr` | `bas.v1` / `Bas` |
| systemd 单元 | `basrd.service` | `bas.service` |
| 设置文件（配置根目录下） | `basr/settings.json` | `bas/daemon.json` |
| 服务环境文件（配置根目录下） | `basr/basrd.env` | `bas/bas.env` |
| daemon 数据库（状态根目录下） | `basr/sessions.sqlite3` | `bas/sessions.sqlite3` |
| 托管 ACP 数据（本地数据根目录下） | `basr/acp/` | `bas/acp/` |
| 托管安装清单文件名 | `basr-installation.json` | `bas-installation.json` |
| 补充信息敏感字段 metadata key | `basr/sensitive` | `bas/sensitive` |
| GitHub 仓库 | `https://github.com/4t145/basr` | `https://github.com/4t145/bas` |

例如 Linux 默认设置文件从 `~/.config/basr/settings.json` 移至
`~/.config/bas/daemon.json`；旧统一文件需提取 `daemon` 部分。默认数据库从
`~/.local/state/basr/sessions.sqlite3` 移至 `~/.local/state/bas/sessions.sqlite3`；
托管数据从 `~/.local/share/basr/acp/` 移至 `~/.local/share/bas/acp/`；

## 先确认实际路径

上表是相对默认根目录的路径，不能直接用于所有安装：

- 设置根目录由 [共享目录规则](../crates/protocol/src/config/paths.rs) 选择：绝对
  `XDG_CONFIG_HOME` 优先，否则使用系统用户配置目录。后端使用 `--daemon-settings-file` /
  `BAS_DAEMON_SETTINGS_FILE`。旧统一文件兼容规则见 [安装说明](install.md#配置-acp-后端)。
- 数据库使用同一模块的状态根目录：绝对 `XDG_STATE_HOME` 优先，否则使用系统状态目录，
  不可用时回退至本地数据目录。默认文件为 `bas/sessions.sqlite3`；
  `--localdb-path` / `BAS_LOCALDB_PATH` 可覆盖它，`memory` 模式不持久化。
  路径解析见 [daemon 配置](../src/bin/bas/config/daemon.rs)。
- 托管 ACP 数据使用 `dirs::data_local_dir()` 下的 `bas/acp`，或
  `BAS_ACP_DATA_DIR` 指定的绝对目录，见 [安装管理](../src/bin/bas/management.rs)。
  Linux 通常为 `$XDG_DATA_HOME` 或 `~/.local/share` 下的目录。
  移动目录时保留文件名；这不是会话数据库。

先核对旧进程的启动参数、服务环境文件、XDG 覆盖和运行用户；自定义目录可能不在默认位置，
也可能与其他应用共享父目录。不要移动整个共享父目录。

## 保守的手动升级顺序

1. **盘点并备份。** 记录当前版本、服务单元、启动参数和实际目录，备份设置、数据库和外部 Agent 配置。
2. **停止旧服务。** 确认旧 daemon 和 Agent 子进程已经退出。SQLite 的 `-wal`、`-shm` 伴随文件也要妥善保留。
3. **手动移动数据。** 按上表移动服务配置、数据库和托管 ACP 数据；新旧位置均有内容时先分别备份，不覆盖数据库。
4. **更新引用。** 核对命令、环境变量、Agent 程序路径和安装清单引用。旧统一配置可通过兼容读取导入，保存后端配置后生成 `daemon.json`。
5. **安装并验证。** 运行 `make install-bas-service`，检查 `systemctl --user status bas.service`、`bast agents`、会话历史和后端路径。
6. **清理旧安装。** 验证完成后按原安装方式移除旧程序和服务链接；保留备份与外部 Agent 凭据。

安装器不会替你完成以上数据移动或旧安装清理。不要让新旧服务同时使用同一数据库；
需要回退时先停止新版本，再从独立备份恢复旧安装与数据，不混用两版客户端。

## 托管 Agent 安装的额外检查

托管注册不仅记录程序路径，还绑定清单的绝对路径与 SHA-256。
新版本只接受 `bas-installation.json` 这个清单文件名，见
[安装引用校验](../crates/protocol/src/config/registration.rs)。仅移动外层数据目录并不足以完成迁移。

若保留安装目录的原绝对位置，可以在备份后只改清单文件名，并更新设置中的
`installation.path`；清单内容字节不变时，其 SHA-256 不变。
若重定位安装目录，清单内的启动命令和参数可能仍指向原位置。
这种情况下推荐在新版本中重新安装并注册对应 Agent：先备份设置，移除失效的托管注册，
保留有效的自定义配置，再由新安装生成清单及引用。不要只修改清单内容或绕过摘要校验，
也不要删除 Agent 独立管理的凭据。
