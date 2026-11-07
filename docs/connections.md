# 服务连接

`bas --listen` 选择监听入口，`bast --server` 选择目标；对应环境变量为 `BAS_LISTEN`、`BAS_SERVER`，命令行优先。
默认都是 `local`，不监听 TCP。

| 目标 | 传输与身份 |
| --- | --- |
| `local` | 当前用户的默认 Unix socket 或 Windows Named Pipe |
| `unix:///absolute/path/service.sock` | 指定 Unix socket，验证目录权限与双方系统用户 |
| `pipe://bas-work` | 指定本地 Named Pipe，验证用户 ACL 和双方身份 |
| `tcp://127.0.0.1:7337` | 明文 TCP，无客户端认证 |
| `mtls://workstation:7337` | 双向证书认证，见 [mTLS 配置](mtls.md) |

TCP 和 mTLS 支持系统 DNS；省略端口时使用 7337，监听端口 `0` 由系统分配。
IPv6 使用方括号，例如 `tcp://[::1]:7337`。地址不能携带凭据、路径或查询参数。
主机名或回环地址不代表同用户身份，也不会自动启用 TLS。

Unix 默认路径为 `$XDG_RUNTIME_DIR/bas-<uid>/service.sock`，未设置时使用系统临时目录。
本地连接失败不会回退到 TCP。系统服务模板使用 `/run/bas/service.sock`，客户端需显式指定该路径。

```sh
bas --listen local --listen tcp://127.0.0.1:7337
bast --server local agents
bast --server tcp://127.0.0.1:7337 agents
```

`--listen` 可重复指定；环境变量使用分号分隔。显式配置替换默认入口，所有入口共享执行环境，
任一入口绑定失败都会使启动失败。一个实例内不隔离获准客户端的数据或执行权限。

连接使用 HTTP/2 心跳：空闲 15 秒后探测，5 秒没有 ACK 则关闭失效连接。
Channel 可重连，但订阅方须按各自游标重新订阅；不会自动重发 prompt。
Rust 客户端使用 `bas_client::service::Client`；注入自建 Channel 时应配置 `heartbeat::client`。
