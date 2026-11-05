# 测试环境

Nix 依赖由 `flake.lock` 固定，Rust 依赖由 `Cargo.lock` 固定。

## Rust 测试

```sh
nix develop -c cargo fmt --all -- --check
nix develop -c cargo test -p bas-protocol --no-default-features --locked
nix develop -c cargo test -p bas-client --locked
nix develop -c cargo test -p bas-server --locked
nix develop -c cargo test --workspace --all-targets --locked
nix develop -c cargo clippy --workspace --all-targets --locked -- -D warnings
```

覆盖共享模型、配置、会话存储、ACP 后端、终端输出、gRPC、进程生命周期和本地传输。
测试使用 embedded echo 与受控 ACP fixture，不读取凭据或调用真实模型。
CI 也在 macOS 和 Windows 检查原生服务、传输和后端安装生命周期。

## 安装与源码快照

```sh
python3 tests/install_bas.py -v
python3 tests/nix_source.py -v
```

安装脚本测试使用临时目录与模拟 Nix；源码快照测试需要真实 Nix，检查锁定 URI 往返和过滤规则。
这些测试不安装软件或操作真实服务。

## 系统测试

QEMU/NixOS 系统测试目前不在仓库中。系统级覆盖由 Rust 进程集成测试和真实 gRPC 测试承担。

Windows CI 额外运行 `cargo test --bin bast command::agents::terminal --locked`，
通过隔离 console 子进程检查取消后再次输入、Unicode、进度输出与控制台模式恢复。
