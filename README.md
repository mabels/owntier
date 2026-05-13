# owntier

Self-hosted WireGuard + BGP mesh network platform — a ZeroTier replacement you run yourself.

## What it does

owntier provisions and manages a hub-and-spoke WireGuard overlay network where BGP is used to distribute routes between nodes. Each node gets a unique 32-bit AS number. The control plane is a single Rust binary (`owntier`) that acts as CLI, hub agent, and leaf agent.

Plugin configs (credentials, interface names, IP assignments) are encrypted with the node's own X25519 device key so they are never stored in plaintext.

## Current status

Early development. The MikroTik plugin is the first device type supported — it configures WireGuard interfaces and BGP templates on RouterOS 7 via the RouterOS API (plain TCP or TLS).

## Prerequisites

- [p43](https://github.com/mabels/project-43) — device key management (path dependency during development)
- Rust toolchain (stable)

## Usage

```sh
# Create a network
owntier network create --name mymesh

# Attach MikroTik plugin config (encrypted with device key)
owntier mikrotik attach --network mymesh --host 192.168.1.1 --wg-ip 10.10.0.1/24

# Deploy to the device
owntier network deploy --network mymesh

# Dry-run to preview commands
owntier network deploy --network mymesh --dry-run --verbose
```

## License

Apache License 2.0 — see [LICENSE](LICENSE).
