# BAS crate boundaries

`bas-protocol` owns the shared model and wire contract. Both `bas-client` and
`bas-server` depend on it; neither depends on the other.

| Package | Responsibilities |
| --- | --- |
| `crates/protocol` (`bas-protocol`) | Models, configuration and authentication payloads, ACP schema types, protobuf definitions and conversions, validated history views |
| `crates/client` (`bas-client`) | RPC queries, request deadlines, ACP transport and SDK connection handles |
| `crates/server` (`bas-server`) | ACP gateway over the BAS service interface, connection routing and authentication callbacks |
| Root `bas` application | Agent processes, session lifecycle, persistence, installation, daemon configuration and the `bas` / `bast` binaries |

The protocol package uses ACP schema types directly. Its default features are
empty and do not include the ACP SDK, Agent processes or application storage.
`client` and `server` enable their respective generated gRPC stubs and the shared
`connection` module. That module owns IPC identity validation, connection targets
and TLS primitives, keeping security checks common to both endpoints. Server-only
listeners and installation-reference validation remain gated by `server`.

The client reexports shared types for its callers, but owns no model or protobuf
source. Tests that construct an ACP gateway depend explicitly on `bas-server`.
Pure history decoding tests live beside the protocol views; RPC deadline and
transport-error tests live beside the client queries. Workspace tests exercise
the gateway with the daemon implementation. CI also tests each library on its own
so workspace feature unification cannot hide a missing dependency.

Input admission uses `Prompt` and `GetPromptAcceptance`. There is no separate
connection-local binding ledger or SDK request-ID reservation API. The admitted
ACP request ID is shared by the receipt and the observed Agent request, as defined
in [frontend multiplexing](frontend-multiplexing.md). BAS management events and ACP
observations retain the [separate stream contract](event-streams.md).
