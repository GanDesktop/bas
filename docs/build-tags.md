# 构建标识

版本输出包含 Cargo package version 和构建标签：

| 构建方式 | 标签含义 |
| --- | --- |
| Nix | `nix-source-<源码 store hash>`，标识过滤后的源码快照，包含纳入快照的未提交文件 |
| Cargo / Git | `git-<HEAD 前 12 位>-worktree`，仅标识基准提交，不证明工作区干净或内容相同 |
| 无 Git 信息 | `source-unknown` |

可用 `BAS_BUILD_TAG=release-0.1.0+ci.42 cargo build` 指定标签；Nix 对应包参数 `buildTag`。
标签限定为 1–80 个 ASCII 字母、数字或 `._-+`，不包含时间、路径或凭据。
