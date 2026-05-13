# owntier — Project Context

## What is owntier

**owntier** is an open-source WireGuard + BGP mesh onboarding and management platform. It is a direct answer to ZeroTier's increasingly unclear licensing and the lack of any tool that combines WireGuard peer management, BGP neighbor automation, device-specific config generation, and federation-ready architecture in one place.

The name reflects the core philosophy: you own the network layer. Not a vendor, not a SaaS controller — you.

---

## Problem Statement

ZeroTier has been the backbone of many self-hosted multi-site networks. Its join-and-approve UX is excellent. But:

- BSL 1.1 → MPL 2.0 (core) + commercial source-available (controller) split in v1.16 (Sept 2025)
- The controller is the critical component — now under a restrictive license for commercial/government use
- Trajectory is clearly toward monetizing the controller further

The gap in the open-source ecosystem: no tool that provides ZeroTier-like onboarding UX on top of plain WireGuard + BGP, with MikroTik support, self-hosted controller, and a clean license.

---

## Design Decisions

### Transport: WireGuard
- Native in RouterOS 7 on all ARM MikroTik devices — no containers needed
- Linux kernel native since 5.6
- iOS and Android via official WireGuard app
- Pure L3, no overlay complexity

### Routing: BGP (FRRouting on Linux, native BGP on MikroTik)
- WireGuard handles encrypted transport
- BGP handles route distribution between sites
- Clean separation of concerns — WireGuard sees point-to-point links, BGP works exactly as expected
- Adding a new site = BGP peer comes up, routes propagate automatically

### UI/Onboarding: wg-portal v2 as baseline reference
- wg-portal supports wgctrl (Linux) AND MikroTik backend natively via RouterOS API
- Generates QR codes for mobile
- MIT licensed
- owntier goes further: adds BGP automation, device-specific script generation, federation

### Security Model: Leaf-initiated, controller-passive
**Critical invariant: the controller NEVER holds credentials to any leaf and NEVER initiates connections outbound.**

- Controller is a passive API — serves config, never connects outbound
- Agent/script on the leaf authenticates to controller with a token generated during onboarding
- One-time bootstrap token exchanged for long-lived leaf identity
- Bootstrap token expires immediately after exchange
- From that point: leaf polls controller, controller responds

This is non-negotiable. Controller-push is a backdoor by definition.

### Identity: Pluggable via IdentityAnchor interface
Designed to integrate with **project-43** (see below) but ships with a simpler default.

```typescript
interface IdentityAnchor {
  readonly verify: (leafId: string) => Promise<VerificationResult>;
  readonly onboard: (leaf: LeafConfig) => Promise<IdentityBinding>;
  readonly revoke: (leafId: string) => Promise<void>;
  readonly onStatusChange: (cb: (leafId: string, status: IdentityStatus) => void) => void;
}
```

---

## Architecture

### Components

```
┌─────────────────────────────────────────┐
│           owntier Controller            │
│  - Web UI (onboarding, peer management) │
│  - REST API (passive, leaf-polled)      │
│  - wg-portal v2 integration             │
│  - BGP neighbor automation (FRRouting)  │
│  - Device config generator              │
│  - SQLite/Postgres backend              │
└──────────────────┬──────────────────────┘
                   │ leaf-initiated poll
        ┌──────────┴──────────┐
        │                     │
┌───────▼──────┐    ┌─────────▼────────┐
│ Linux Agent  │    │  Sidecar Agent   │
│ (owntier-    │    │  (for MikroTik   │
│  agent)      │    │  without native  │
│              │    │  container)      │
│ - wgctrl     │    │ - RouterOS API   │
│ - vtysh/FRR  │    │ - librouteros    │
│ - p43 FFI    │    │ - SSH fallback   │
└──────────────┘    └────────┬─────────┘
                             │ RouterOS API (local only)
                    ┌────────▼─────────┐
                    │    MikroTik      │
                    │ (config target)  │
                    └──────────────────┘
```

### Leaf Device Support

| Device type | Agent | WireGuard | BGP |
|---|---|---|---|
| Linux (K8s node, VM) | owntier-agent binary | wgctrl | vtysh/FRRouting |
| MikroTik (ARM, with Docker) | owntier-agent in container | RouterOS native WG | RouterOS native BGP |
| MikroTik (ARM, no Docker) | Sidecar on nearby Linux | RouterOS native WG | RouterOS native BGP |
| Mobile (iOS/Android) | WireGuard app (no agent) | WireGuard app | N/A |

### MikroTik Limitations (documented honestly)
- RouterOS scripts run in main VRF only — `/tool/fetch` ignores VRF context
- No native crypto primitives for token handling in scripts
- No proper diff/apply logic in RouterOS scripting
- **Therefore**: MikroTik is a config target, not an agent participant
- For MikroTik without Docker: owntier generates a complete ready-to-paste RouterOS script during onboarding. One-time manual application. Sidecar handles ongoing sync optionally.

### Generated RouterOS onboarding script (example shape)
```routeros
/interface/wireguard/add name=wg-owntier listen-port=51820
/interface/wireguard/peers/add \
    interface=wg-owntier \
    public-key="<hub-pubkey>" \
    endpoint-address=<hub-ip> \
    endpoint-port=51820 \
    allowed-address=10.0.0.0/8,192.168.128.0/22 \
    persistent-keepalive=25
/ip/address/add address=<assigned-wg-ip>/24 interface=wg-owntier
/routing/bgp/connection/add \
    name=owntier-hub \
    remote-address=<hub-wg-ip> \
    remote-as=65000 \
    local-as=<assigned-as>
```

---

## Onboarding Flow

### Site/Router leaf (Linux or MikroTik)
1. Admin opens controller UI, fills form: site name, local subnets (IPv4 + IPv6), device type
2. Controller calls wg-portal API → creates WireGuard peer → gets assigned WG IP + keypair
3. Controller calls FRRouting vtysh → adds BGP neighbor for that WG IP
4. Controller generates device-specific script/config (RouterOS script, Linux wg-quick config, or agent install command)
5. **One-time bootstrap token** generated and shown once
6. Admin applies script/token on leaf
7. Leaf exchanges bootstrap token for long-lived identity
8. Leaf begins polling controller for config updates

### Mobile/road warrior
1. Admin creates peer in UI
2. QR code generated instantly
3. User scans with WireGuard app on iOS/Android
4. AllowedIPs include all site subnets — traffic routes via hub

---

## Networking Model

### Address Space
- WireGuard overlay: configurable CIDR (e.g. `10.64.0.0/16`)
- Each controller instance owns a slice of the overlay space (for federation)
- Peer IPs auto-assigned from pool by wg-portal

### Routes
- Hub peer: AllowedIPs = all site subnets
- Site peers: AllowedIPs = their local LAN subnet(s), both IPv4 and IPv6
- Mobile peers: AllowedIPs = all site subnets pushed via wg-portal config

### BGP
- Hub runs FRRouting, acts as route reflector
- Each site MikroTik peers with hub over WireGuard interface IP
- AS numbers: configurable, from private range 64512–65534
- New site onboarded → BGP peer comes up → routes propagate to all sites automatically

---

## Federation (future, architecture must support from day one)

### Key principles
- Each controller is an autonomous system with its own AS number
- Each controller owns a non-overlapping address prefix
- Federation is leaf-federated in the data plane (leaf-to-leaf WireGuard tunnels)
- Controllers federate for control plane only (config exchange, trust)
- A compromised controller cannot forge leaf identities

### Address space coordination
- Controller instances claim a prefix from a published registry (GitHub-hosted YAML)
- No runtime coordination needed, no conflicts possible

### Trust model between federated controllers
- Same leaf-initiated model applies between controllers
- Controller B's agent initiates toward Controller A, never reverse
- No controller ever holds credentials to another controller's infrastructure

---

## Integration: project-43

**project-43** is a separate project (Rust + Flutter) building a decentralised identity platform where personal mobile devices act as the security anchor for cryptographic identities.

### Architecture
- Rust core lib handles all crypto, identity state machine, key ceremonies
- Flutter app provides the mobile anchor UI (iOS + Android)
- CLI uses the same Rust core via FFI
- Transport is pluggable (currently Matrix, but exchangeable — Matrix is just a message queue carrying signed opaque blobs)
- Multiple devices and people can co-own a leaf's identity (M-of-N threshold)

### Integration point with owntier
The owntier agent links p43 Rust core as a native library (same as the p43 CLI does today):

```toml
[dependencies]
p43-core = { git = "..." }
```

The `IdentityAnchor` interface is implemented by a p43 adapter. Everything else — key ceremonies, device threshold, revocation — is p43's problem, not owntier's.

### What this enables
- Leaf WireGuard identity is cryptographically bound to operator's physical mobile device(s)
- Compromised leaf config alone is not enough — attacker also needs physical mobile
- Revocation by any co-owner of the p43 room, without touching the controller
- Works across federated controllers — any controller can verify the mobile-bound credential independently
- MikroTik sidecar links p43 core natively — RouterOS device is just a config target, identity anchor lives in the Linux sidecar

### Current status
- p43 is in testing phase, features added incrementally
- owntier must design the `IdentityAnchor` interface as a clean boundary today
- Ship owntier v1 with a simpler default identity (token-based), p43 adapter added later

---

## Tech Stack

### Controller
- **Runtime**: TypeScript / Node.js (Hono backend, familiar from other projects)
- **Frontend**: React + TypeScript
- **DB**: SQLite default, Postgres for production
- **WireGuard management**: wg-portal v2 REST API (or direct wgctrl via node FFI)
- **BGP management**: FRRouting via vtysh subprocess or HTTP API
- **Deployment**: Single Docker Compose file, Helm chart for Kubernetes

### Agent (Linux)
- **Language**: TypeScript (Node.js) or Rust TBD
- **WireGuard**: wgctrl (Linux kernel WireGuard)
- **BGP**: vtysh subprocess calls to local FRRouting
- **p43**: Rust FFI (when p43 integration is added)
- **Comm**: HTTP polling to controller REST API

### Sidecar (MikroTik proxy)
- Same as Linux agent but uses RouterOS API (librouteros Python or SSH) instead of wgctrl/vtysh
- Runs on any nearby Linux box (K8s pod, Pi, Minisforum)

---

## Repository Structure (proposed)

```
owntier/
├── packages/
│   ├── controller/          # Hono backend + React UI
│   │   ├── src/
│   │   │   ├── api/         # REST endpoints
│   │   │   ├── adapters/    # wg-portal, FRRouting, MikroTik
│   │   │   ├── identity/    # IdentityAnchor interface + implementations
│   │   │   └── ui/          # React frontend
│   │   └── ...
│   ├── agent/               # Linux leaf agent
│   ├── sidecar/             # MikroTik proxy agent
│   └── shared/              # Shared types, interfaces, protocol
├── charts/                  # Helm chart
├── compose/                 # Docker Compose examples
├── scripts/                 # RouterOS script templates
└── docs/
    ├── architecture.md
    ├── onboarding.md
    ├── mikrotik.md
    └── federation.md
```

---

## Key Interfaces (TypeScript)

```typescript
// Leaf identity — pluggable, p43 adapter ships separately
interface IdentityAnchor {
  readonly verify: (leafId: string) => Promise<VerificationResult>;
  readonly onboard: (leaf: LeafConfig) => Promise<IdentityBinding>;
  readonly revoke: (leafId: string) => Promise<void>;
  readonly onStatusChange: (cb: (leafId: string, status: IdentityStatus) => void) => void;
}

// Device adapter — per hardware type
interface LeafAdapter {
  readonly applyWireGuardPeer: (peer: WGPeer) => Promise<void>;
  readonly removeWireGuardPeer: (publicKey: string) => Promise<void>;
  readonly applyBGPNeighbor: (neighbor: BGPNeighbor) => Promise<void>;
  readonly removeBGPNeighbor: (address: string) => Promise<void>;
  readonly getStatus: () => Promise<LeafStatus>;
}

// Implementations: LinuxAdapter, MikroTikAPIAdapter, MikroTikSSHAdapter

// Message bus — p43 transport abstraction
interface MessageBus {
  readonly send: (recipient: DeviceId, payload: SignedBlob) => Promise<void>;
  readonly receive: () => AsyncIterable<SignedBlob>;
  readonly acknowledge: (msgId: MessageId) => Promise<void>;
}
```

---

## What owntier is NOT

- Not a ZeroTier fork or wrapper
- Not a full mesh P2P overlay (no STUN/TURN, no NAT hole-punching) — hub-and-spoke with BGP is the model
- Not zero-touch for MikroTik without a sidecar — documented honestly in README
- Not a VPN-as-a-service — fully self-hosted, no cloud dependency

---

## Operator Notes (Meno's setup — for reference/testing)

- MikroTik routers: ARM, RouterOS 7, native WireGuard, native BGP, **no Docker** on most
- Network segments: 192.168.128-131.0/24 (four VLANs), IPv6 dual-stack
- Uplinks: Vodafone + Starlink, multi-VRF
- Kubernetes clusters: K3s, running Kea DHCP, Knot Resolver, cert-manager, external-dns
- Current overlay: ZeroTier + ZTNet self-hosted controller + FRRouting BGP
- Planned hub: Minisforum MS-A2 (Ryzen 7 7745HX) — Intel X710 SR-IOV, sufficient RAM for VMs
- Related project: project-43 (Rust + Flutter, identity platform, p43-core as embeddable Rust lib)

---

## Starting Point for Development

The logical first milestone:

1. **Monorepo scaffold** — pnpm workspace, packages/controller, packages/shared, packages/agent
2. **Shared types** — LeafConfig, WGPeer, BGPNeighbor, IdentityAnchor interface, LeafAdapter interface
3. **Controller skeleton** — Hono app, SQLite via Drizzle, basic REST API shape
4. **wg-portal adapter** — wrap wg-portal v2 REST API for peer creation/deletion
5. **FRRouting adapter** — vtysh subprocess wrapper for BGP neighbor management
6. **Onboarding UI** — form → generates RouterOS script + bootstrap token
7. **Linux agent** — polls controller, applies WireGuard + BGP config via wgctrl + vtysh

MikroTik sidecar, federation, and p43 integration are explicitly post-v1.
