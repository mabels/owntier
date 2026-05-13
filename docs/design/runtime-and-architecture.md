# Runtime & Architecture Design

**Status:** Decision  
**Date:** 2026-05-13

---

## Single Binary, Mode-Based CLI

owntier ships as one Rust binary. All modes are subcommands:

```
owntier hub start            # start hub daemon
owntier leaf start           # start leaf daemon (post-v1)
owntier peer add/remove      # management (talk to local daemon via Unix socket)
owntier token issue/revoke
owntier config generate --device mikrotik
```

No separate controller service. The hub daemon IS the controller — it runs on a Linux
box (e.g. Minisforum MS-A2), owns WireGuard and BGP, stores state locally in SQLite.

---

## Hub Daemon

Started via `owntier hub start`. Responsibilities:

- WebSocket server (leaf agents connect in, future frontend connects in)
- SQLite state (peers, tokens, onboarding records)
- Token / onboarding lifecycle
- RouterOS / VyOS / wg-quick config and script generation
- Plugin orchestration (see below)

The hub never initiates outbound connections to leaves. Security invariant: always
leaf-initiated, controller-passive. This applies equally to the hub WebSocket server —
leaves connect out, hub accepts.

---

## Leaf Daemon (post-v1)

Same binary, `owntier leaf start`. Connects out to hub WebSocket. Applies config
received from hub using its own plugin stack. Pushes status events back.

---

## Frontend (post-v1)

Separate standalone web app (React). Connects to hub WebSocket. Not embedded in the
binary. Not served by the hub.

---

## Device Identity: p43 Integration

**Decision:** owntier reuses the p43 `DeviceKey` as the node identity anchor.

`DeviceKey` (from `p43::bus`) holds two keys:
- Ed25519 signing key → `device_id()` (first 8 bytes hex) — stable node identifier
- X25519 ECDH key → **reused directly as the WireGuard keypair**

WireGuard uses Curve25519 (X25519) — the same primitive. No separate WireGuard keypair
is generated or stored. The device's ECDH public key IS the WireGuard public key.

This means: one key to back up (the p43 device key), deterministic WireGuard identity,
and owntier node identity is cryptographically bound to the operator's p43 device.

### Cargo reference

```toml
# Cargo.toml
[dependencies]
p43 = { path = "../project-43/lib", features = ["bus"] }
```

Path dependency for now — keeps both projects in lockstep during development, allows
extending p43 as needed. Git reference and potential crates.io publish are future
concerns.

Assumes sibling layout: `~/Software/project-43/` and `~/Software/owntier/`.

---

## Plugin Architecture

WireGuard and BGP are a single combined plugin per device type — they always move
together (adding a peer means WireGuard peer + BGP neighbor atomically).

```
NetworkPlugin implementations: linux | mikrotik | vyos | ...
```

A Linux hub uses the `linux` plugin. A MikroTik sidecar uses `mikrotik`. Only `linux`
is built for v1.

### NetworkPlugin trait

```rust
trait NetworkPlugin: Send + Sync {
    async fn check_prerequisites(&self) -> Result<Vec<PrereqCheck>>;
    async fn create_network(&self, config: &NetworkConfig) -> Result<()>;
    async fn ensure_services(&self) -> Result<ServiceStatus>;
    async fn add_peer(&self, peer: &PeerConfig) -> Result<()>;   // WG + BGP together
    async fn remove_peer(&self, id: &str) -> Result<()>;
    async fn status(&self) -> Result<NetworkStatus>;
}
```

`NetworkConfig` carries a `role: Hub | Leaf` field — the only behavioural difference
between hub and leaf at the plugin level (hub configures FRRouting as route reflector).

### Runtime dispatch (decided)

Trait objects resolved from config at startup. One binary handles any combination
without recompile. Feature flags can be layered on later for constrained targets.

---

## Communication Channels

| Channel | Protocol | Initiator |
|---|---|---|
| CLI → local daemon | Unix domain socket | CLI |
| Leaf agent → hub | WebSocket | leaf |
| Frontend → hub | WebSocket | frontend |

Agent-initiated everywhere. Hub never dials out.

---

## State

SQLite via `sqlx` or `rusqlite`. Stored at a configurable path (default
`/var/lib/owntier/owntier.db` for daemon, `~/.owntier/owntier.db` for dev).

Schema migrations: `sqlx migrate` or manual versioned SQL files.

---

## What This Stack Is NOT

- No separate TypeScript controller service
- No Cloudflare Workers (may revisit for a cloud-hosted relay variant, not v1)
- No React embedded in the hub binary
- No Node.js / Deno anywhere in the agent/hub path
- No pnpm workspace for agent code — Rust workspace (`Cargo.toml`) only

Frontend (when it arrives) will be a separate repo or `ui/` directory with its own
toolchain.
