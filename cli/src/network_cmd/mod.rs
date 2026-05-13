pub mod subcmd;

use anyhow::{Context as _, Result};
use base64::Engine as _;
use p43::bus;
use std::path::Path;
use subcmd::NetworkCmd;

pub fn run(cmd: NetworkCmd, owntier_dir: &Path, store_dir: &Path) -> Result<()> {
    let bus_dir = bus::bus_dir(store_dir);
    std::fs::create_dir_all(&bus_dir)?;

    match cmd {
        NetworkCmd::Create {
            name,
            r#as,
            device_id,
        } => cmd_create(owntier_dir, &bus_dir, &name, r#as, device_id.as_deref()),
        NetworkCmd::Show { name } => cmd_show(owntier_dir, store_dir, &name),
        NetworkCmd::Delete { name, force } => cmd_delete(owntier_dir, &name, force),
        NetworkCmd::Deploy {
            name,
            stopped,
            verbose,
            dry_run,
        } => cmd_deploy(owntier_dir, store_dir, &name, stopped, verbose, dry_run),
        NetworkCmd::List => cmd_list(owntier_dir),
    }
}

fn cmd_create(
    owntier_dir: &Path,
    bus_dir: &Path,
    name: &str,
    as_number: Option<u32>,
    device_id: Option<&str>,
) -> Result<()> {
    let record = owntier::network::create(name, as_number, device_id, bus_dir, owntier_dir)?;
    print_record(&record);
    Ok(())
}

fn cmd_show(owntier_dir: &Path, store_dir: &Path, name: &str) -> Result<()> {
    let record = owntier::network::load(name, owntier_dir)?;
    print_record(&record);

    let plugins = owntier::plugin::list_attached(name, owntier_dir)?;
    if plugins.is_empty() {
        println!("plugins    : (none)");
        return Ok(());
    }

    let device_secret = crate::mikrotik_cmd::resolve_device_secret(owntier_dir, store_dir, name)?;

    for plugin_type in &plugins {
        println!("plugin     : {}", plugin_type);
        match owntier::plugin::load(plugin_type, name, &device_secret, owntier_dir) {
            Ok(values) => {
                let mut pairs: Vec<_> = values.iter().collect();
                pairs.sort_by_key(|(k, _)| k.as_str());
                for (k, v) in pairs {
                    if k == "password" {
                        println!("  {:<12}: (set)", k);
                    } else {
                        println!("  {:<12}: {}", k, v);
                    }
                }
            }
            Err(e) => println!("  (could not decrypt: {e})"),
        }
    }
    Ok(())
}

fn cmd_delete(owntier_dir: &Path, name: &str, force: bool) -> Result<()> {
    if !force {
        eprint!("Delete network {name:?}? [y/N] ");
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        if !matches!(answer.trim().to_lowercase().as_str(), "y" | "yes") {
            eprintln!("Aborted.");
            return Ok(());
        }
    }
    owntier::network::delete(name, owntier_dir)?;
    println!("Deleted network: {name}");
    Ok(())
}

fn cmd_deploy(
    owntier_dir: &Path,
    store_dir: &Path,
    name: &str,
    stopped: bool,
    verbose: bool,
    dry_run: bool,
) -> Result<()> {
    let device_secret = crate::mikrotik_cmd::resolve_device_secret(owntier_dir, store_dir, name)?;
    let cfg = owntier::plugin::load(
        owntier_mikrotik::PLUGIN_TYPE,
        name,
        &device_secret,
        owntier_dir,
    )
    .with_context(|| {
        format!("attach a mikrotik plugin first: owntier mikrotik attach --network {name} ...")
    })?;

    let record = owntier::network::load(name, owntier_dir)?;
    println!("Deploying network {:?} ...", name);
    owntier_mikrotik::deploy::deploy(
        &record,
        device_secret,
        &cfg,
        &owntier_mikrotik::deploy::DeployFlags {
            stopped,
            verbose,
            dry_run,
        },
    )
}

fn cmd_list(owntier_dir: &Path) -> Result<()> {
    let records = owntier::network::list(owntier_dir)?;
    if records.is_empty() {
        println!("No networks found.");
        return Ok(());
    }
    println!(
        "{:<20}  {:<12}  {:<16}  {}",
        "name", "as", "device_id", "wg_pubkey"
    );
    println!("{}", "-".repeat(84));
    for r in records {
        let wg_pubkey_b64 = base64::engine::general_purpose::STANDARD.encode(r.wg_public_key);
        println!(
            "{:<20}  {:<12}  {:<16}  {}",
            r.name, r.as_number, r.device_id, wg_pubkey_b64
        );
    }
    Ok(())
}

fn print_record(r: &owntier::network::NetworkRecord) {
    let wg_pubkey_b64 = base64::engine::general_purpose::STANDARD.encode(r.wg_public_key);
    println!("name       : {}", r.name);
    println!("as         : {} (0x{:08x})", r.as_number, r.as_number);
    println!("device_id  : {}", r.device_id);
    println!("wg_pubkey  : {}", wg_pubkey_b64);
}
