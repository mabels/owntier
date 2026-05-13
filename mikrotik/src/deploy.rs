use crate::{api::RosSession, keys};
use anyhow::{bail, Context, Result};
use base64::Engine as _;
use owntier::network::NetworkRecord;
use std::collections::HashMap;

fn display_sentence(words: &[&str]) -> String {
    words
        .iter()
        .map(|w| {
            if let Some(rest) = w.strip_prefix('=') {
                if let Some((k, v)) = rest.split_once('=') {
                    return format!("{}=\"{}\"", k, v);
                }
            }
            w.to_string()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub struct DeployFlags {
    pub stopped: bool,
    pub verbose: bool,
    pub dry_run: bool,
}

pub fn deploy(
    record: &NetworkRecord,
    wg_private_key: [u8; 32],
    cfg: &HashMap<String, String>,
    flags: &DeployFlags,
) -> Result<()> {
    let host = cfg.get(keys::HOST).context("missing 'host' in mikrotik plugin config")?;
    let port: u16 = cfg
        .get(keys::PORT)
        .map(|v| v.parse())
        .transpose()
        .context("invalid 'port' in mikrotik plugin config")?
        .unwrap_or(8728);
    let user = cfg.get(keys::USER).map(|s| s.as_str()).unwrap_or("admin");
    let password = cfg.get(keys::PASSWORD).map(|s| s.as_str()).unwrap_or("");
    let wg_name_default = format!("wg-{}", record.name);
    let wg_name = cfg.get(keys::WG_NAME).map(|s| s.as_str()).unwrap_or(&wg_name_default);
    let wg_ip = cfg.get(keys::WG_IP).context("missing 'wg_ip' in mikrotik plugin config")?;
    let wg_port: u16 = cfg
        .get(keys::WG_PORT)
        .map(|v| v.parse())
        .transpose()
        .context("invalid 'wg_port' in mikrotik plugin config")?
        .unwrap_or(51820);

    let privkey_b64 = base64::engine::general_purpose::STANDARD.encode(wg_private_key);
    let disabled = if flags.stopped { "yes" } else { "no" };
    let bgp_template = format!("owntier-{}", record.name);

    let ssl = cfg.get(keys::SSL).map(|v| v == "true").unwrap_or(false);

    let mut api = if flags.dry_run {
        None
    } else {
        let mut s = RosSession::connect(host, port, ssl)?;
        s.login(user, password)?;
        Some(s)
    };

    // ── helpers ───────────────────────────────────────────────────────────────

    let print = flags.verbose || flags.dry_run;
    let prefix = if flags.dry_run { "[dry-run]" } else { "[sent]  " };

    let mut send = |api: &mut Option<RosSession>, words: &[&str]| -> Result<()> {
        if print {
            println!("  {} {}", prefix, display_sentence(words));
        }
        if let Some(ref mut s) = api {
            s.exec(words)?;
        }
        Ok(())
    };

    // In dry-run we skip existence checks (assume a fresh device).
    let mut query = |api: &mut Option<RosSession>, words: &[&str]| -> Result<Vec<HashMap<String, String>>> {
        if print {
            println!("  [query]  {}", display_sentence(words));
        }
        match api {
            Some(ref mut s) => {
                let rows = s.run(words)?;
                if print {
                    if rows.is_empty() {
                        println!("  [query]  → (none)");
                    } else {
                        for row in &rows {
                            let mut pairs: Vec<_> = row.iter().collect();
                            pairs.sort_by_key(|(k, _)| k.as_str());
                            let summary: Vec<String> = pairs.iter()
                                .map(|(k, v)| format!("{}={:?}", k, v))
                                .collect();
                            println!("  [query]  → {}", summary.join(" "));
                        }
                    }
                }
                Ok(rows)
            }
            None => Ok(vec![]),
        }
    };

    // ── WireGuard interface ───────────────────────────────────────────────────

    let existing_wg = query(&mut api, &[
        "/interface/wireguard/print",
        &format!("?name={}", wg_name),
    ])?;
    if let Some(entry) = existing_wg.first() {
        let id = entry.get(".id").context("WireGuard entry missing .id")?;
        println!("  updating WireGuard interface {}", wg_name);
        send(&mut api, &[
            "/interface/wireguard/set",
            &format!("=.id={}", id),
            &format!("=listen-port={}", wg_port),
            &format!("=private-key={}", privkey_b64),
            &format!("=disabled={}", disabled),
        ])?;
    } else {
        println!("  creating WireGuard interface {}", wg_name);
        send(&mut api, &[
            "/interface/wireguard/add",
            &format!("=name={}", wg_name),
            &format!("=listen-port={}", wg_port),
            &format!("=private-key={}", privkey_b64),
            &format!("=disabled={}", disabled),
        ])?;
    }

    // ── IP address ────────────────────────────────────────────────────────────

    let existing_addr = query(&mut api, &[
        "/ip/address/print",
        &format!("?interface={}", wg_name),
    ])?;
    if let Some(entry) = existing_addr.first() {
        let id = entry.get(".id").context("IP address entry missing .id")?;
        println!("  updating IP address on {} to {}", wg_name, wg_ip);
        send(&mut api, &[
            "/ip/address/set",
            &format!("=.id={}", id),
            &format!("=address={}", wg_ip),
        ])?;
    } else {
        println!("  assigning IP {} to {}", wg_ip, wg_name);
        send(&mut api, &[
            "/ip/address/add",
            &format!("=address={}", wg_ip),
            &format!("=interface={}", wg_name),
        ])?;
    }

    // ── BGP template ──────────────────────────────────────────────────────────

    let existing_bgp = query(&mut api, &[
        "/routing/bgp/template/print",
        &format!("?name={}", bgp_template),
    ])?;
    if let Some(entry) = existing_bgp.first() {
        let id = entry.get(".id").context("BGP template entry missing .id")?;
        println!("  updating BGP template {} (AS {})", bgp_template, record.as_number);
        send(&mut api, &[
            "/routing/bgp/template/set",
            &format!("=.id={}", id),
            &format!("=as={}", record.as_number),
        ])?;
    } else {
        println!("  creating BGP template {} (AS {})", bgp_template, record.as_number);
        send(&mut api, &[
            "/routing/bgp/template/add",
            &format!("=name={}", bgp_template),
            &format!("=as={}", record.as_number),
        ])?;
    }

    // ── summary ───────────────────────────────────────────────────────────────

    if flags.dry_run {
        println!("  (dry-run complete — nothing sent)");
    } else if flags.stopped {
        println!("  deployed (stopped — interface disabled)");
    } else {
        println!("  deployed and active");
    }
    Ok(())
}
