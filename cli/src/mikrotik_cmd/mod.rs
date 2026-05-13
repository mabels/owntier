pub mod subcmd;

use anyhow::{Context as _, Result};
use owntier_mikrotik::keys;
use std::collections::HashMap;
use std::path::Path;
use subcmd::MikrotikCmd;

pub fn run(cmd: MikrotikCmd, owntier_dir: &Path, store_dir: &Path) -> Result<()> {
    match cmd {
        MikrotikCmd::Attach { network, host, ssl, port, user, password, wg_name, wg_ip, wg_port } => {
            let port = port.unwrap_or(if ssl { 8729 } else { 8728 });
            let wg_name = wg_name.unwrap_or_else(|| format!("wg-{}", network));
            cmd_attach(owntier_dir, store_dir, &network, &host, port, ssl, &user, password, &wg_name, &wg_ip, wg_port)
        }
        MikrotikCmd::Show { network } => cmd_show(owntier_dir, store_dir, &network),
        MikrotikCmd::Detach { network } => cmd_detach(owntier_dir, &network),
    }
}

#[allow(clippy::too_many_arguments)]
fn cmd_attach(
    owntier_dir: &Path,
    store_dir: &Path,
    network: &str,
    host: &str,
    port: u16,
    ssl: bool,
    user: &str,
    password: Option<String>,
    wg_name: &str,
    wg_ip: &str,
    wg_port: u16,
) -> Result<()> {
    let password = match password {
        Some(p) => p,
        None => rpassword::prompt_password("RouterOS password: ")
            .context("read RouterOS password")?,
    };

    let record = owntier::network::load(network, owntier_dir)?;
    let device_pubkey = record.wg_public_key;

    let mut values = HashMap::new();
    values.insert(keys::HOST.to_string(), host.to_string());
    values.insert(keys::PORT.to_string(), port.to_string());
    values.insert(keys::SSL.to_string(), ssl.to_string());
    values.insert(keys::USER.to_string(), user.to_string());
    values.insert(keys::PASSWORD.to_string(), password);
    values.insert(keys::WG_NAME.to_string(), wg_name.to_string());
    values.insert(keys::WG_IP.to_string(), wg_ip.to_string());
    values.insert(keys::WG_PORT.to_string(), wg_port.to_string());

    owntier::plugin::attach(
        owntier_mikrotik::PLUGIN_TYPE,
        network,
        &values,
        &device_pubkey,
        owntier_dir,
    )?;

    println!("MikroTik plugin attached to network {:?}", network);
    println!("  host     : {}", host);
    println!("  port     : {} ({})", port, if ssl { "TLS" } else { "plain" });
    println!("  user     : {}", user);
    println!("  wg_name  : {}", wg_name);
    println!("  wg_ip    : {}", wg_ip);
    println!("  wg_port  : {}", wg_port);
    Ok(())
}

fn cmd_show(owntier_dir: &Path, store_dir: &Path, network: &str) -> Result<()> {
    let device_secret = resolve_device_secret(owntier_dir, store_dir, network)?;
    let values = owntier::plugin::load(
        owntier_mikrotik::PLUGIN_TYPE,
        network,
        &device_secret,
        owntier_dir,
    )?;

    println!("MikroTik plugin config for {:?}:", network);
    let mut pairs: Vec<_> = values.iter().collect();
    pairs.sort_by_key(|(k, _)| k.as_str());
    for (k, v) in pairs {
        if k == keys::PASSWORD {
            println!("  {:<12}: (set)", k);
        } else {
            println!("  {:<12}: {}", k, v);
        }
    }
    Ok(())
}

fn cmd_detach(owntier_dir: &Path, network: &str) -> Result<()> {
    owntier::plugin::detach(owntier_mikrotik::PLUGIN_TYPE, network, owntier_dir)?;
    println!("MikroTik plugin detached from network {:?}", network);
    Ok(())
}

// ── helpers ───────────────────────────────────────────────────────────────────

pub(crate) fn resolve_device_secret(
    owntier_dir: &Path,
    store_dir: &Path,
    network: &str,
) -> Result<[u8; 32]> {
    let record = owntier::network::load(network, owntier_dir)?;
    let bus_dir = p43::bus::bus_dir(store_dir);
    let label = p43::bus::resolve_own_device_label(&bus_dir, None, Some(&record.device_id))
        .with_context(|| {
            format!(
                "device-id {} not found in p43 bus store",
                record.device_id
            )
        })?;
    let (_label, _path, key) = p43::bus::resolve_device_key(&bus_dir, Some(&label))?;
    Ok(key.ecdh_secret())
}
