# 服务连接类型

Bas 和 BasT 共用连接目标格式。Bas 和 BasT 默认使用同一系统用户的本地 IPC，
不需要配对，也不默认监听 TCP。
远程 mTLS 支持手动提供证书和密钥文件，见 [mTLS 配置说明](mtls.md)。
扫码配对仍在[远程接入规划](design/remote-access.md)中，尚未实现。

## 默认本地连接

```sh
bas
bast agents
```

首次使用时，两者都默认使用 `local`：

- Unix：使用用户私有目录中的 Unix socket，验证文件权限和双方的系统用户身份。
- Windows：使用当前用户 SID 对应的本地 Named Pipe，通过用户 ACL 和双方身份检查建立连接。
  不允许远程 Named Pipe，也不把同名管道直接视为可信服务。

本地连接失败时不回退到 TCP。服务端以哪个系统用户运行，就由该用户的本地客户端连接；
系统服务以独立账号运行时，不能因为客户端也在这台机器上就自动获得访问权。

Unix 默认使用 `$XDG_RUNTIME_DIR/bas-<uid>/service.sock`；没有运行时目录时使用系统临时目录下的
`bas-<uid>/service.sock`。服务端与客户端需要使用同一运行时目录，否则应显式指定 socket。
仓库的 systemd 系统服务模板使用 `/run/bas/service.sock`，同用户客户端需指定
`--server unix:///run/bas/service.sock`；用户服务仍使用默认 `local`。

所有获准客户端访问同一个服务实例的共享执行环境。本地免配对不引入额外的 Bas 用户体系。

## 显式选择连接类型

| 目标格式 | 含义 |
| --- | --- |
| `local` | 当前用户在当前平台的默认本地 IPC |
| `unix:///absolute/path/service.sock` | 指定 Unix socket；只在 Unix 平台可用 |
| `pipe://bas-work` | 指定本地 Windows Named Pipe 名称；只在 Windows 可用 |
| `tcp://127.0.0.1` | 显式使用明文、无认证 TCP |
| `mtls://workstation:7338` | 双向证书认证 TLS，需显式提供证书文件 |
| `127.0.0.1` / `[::1]` | 不带协议前缀的 TCP 地址 |
| `tcp://workstation` / `workstation.tailnet.ts.net` | 通过系统解析器连接主机名 |

TCP 支持系统 DNS、hosts 文件和已配置的 Tailscale MagicDNS；不依赖 Tailscale。
配置解析和加载时不查询 DNS，连接（含重连）或绑定监听地址时才由系统解析。
TCP 和 mTLS 的端口可省略，统一使用 [`DEFAULT_PORT`](../src/service/defaults.rs) 定义的默认端口，例如 `workstation`、
`tcp://192.0.2.10` 或 `mtls://workstation`。显式端口必须为 `0` 到 `65535` 的数字；
监听端口 `0` 会报告实际分配的 IP 和端口，不会被替换为默认值。
IPv6 必须使用方括号，例如 `tcp://[::1]` 或 `tcp://[::1]:7443`。
省略端口与显式填写默认端口使用相同连接身份和状态文件；配置和传输保留完整端口，
界面可省略默认端口，但不会省略非默认端口或 `0`。
地址不能包含凭据、路径、查询、片段或空白。
主机名（包括 `localhost`）不代表同用户身份或本地文件权限，也不会自动启用 TLS。

`pipe://` 后面是本地名称，不是远程主机或 UNC 路径。Unix socket 所在目录必须属于当前用户、
仅该用户可访问；不会自动放宽已有目录的权限。

客户端使用 `--server` 或 `BAS_SERVER` 选择一个目标，命令行优先：

```sh
bast --server local agents
bast --server pipe://bas-work
bast --server tcp://192.0.2.10 agents
bast --server tcp://workstation.tailnet.ts.net agents
```

## 启用 TCP 或多个监听入口

TCP 是保留的可选传输，显式选择即可使用，不需要额外的确认开关。
TCP 不提供加密或客户端认证；回环 TCP 也不等于已认证的本地用户。
连接者可使用同一服务的配置、安装、登录和会话接口；明文 TCP 也不提供用户间隔离。

只监听 TCP：

```sh
bas --listen tcp://127.0.0.1
bast --server tcp://127.0.0.1 agents
```

同时提供本地 IPC 和 TCP：

```sh
bas --listen local --listen tcp://127.0.0.1
```

每个入口访问同一个会话注册表和执行环境。`--listen` 可以重复指定；
`BAS_LISTEN` 使用分号分隔多个目标，例如 `local;tcp://127.0.0.1`。
显式提供监听配置会替换默认的 `local`，不会隐式再打开其他入口。
任一指定入口绑定失败都会使启动失败，不静默忽略配置。

## 从原有 TCP 默认值升级

- daemon 和 CLI 的默认值一起改为 `local`；更新双方后，同一用户无需配对即可使用。
- 已有 `--listen`、`--server`、`BAS_LISTEN`、`BAS_SERVER` 的显式 TCP 设置继续有效，
  不会被自动改写或禁用。
- 新客户端连接仍使用 TCP 的旧 daemon 时，需显式设置 `--server 127.0.0.1`；
  旧客户端连接新版 daemon 时，可以显式开启对应 TCP 监听。
- 不会在本地 IPC 失败时自动连接旧 TCP 地址。
