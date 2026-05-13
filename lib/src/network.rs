use anyhow::{Context, Result};
use p43::bus::{self, resolve_device_key, resolve_own_device_label, DeviceKey};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkRecord {
    pub name: String,
    pub as_number: u32,
    pub device_id: String,
    /// X25519 public key from the p43 DeviceKey — used directly as the WireGuard public key.
    pub wg_public_key: [u8; 32],
}

/// Generate a 32-bit BGP AS number.
/// Lower 16 bits: random in 65000–65535 (private 16-bit AS range).
/// Upper 16 bits: random in 1–65535.
fn random_as() -> u32 {
    let mut rng = rand::thread_rng();
    let high: u32 = rng.gen_range(1..=65535);
    let low: u32 = rng.gen_range(65000..=65535);
    (high << 16) | low
}

fn networks_dir(owntier_dir: &Path) -> PathBuf {
    owntier_dir.join("networks")
}

fn network_path(owntier_dir: &Path, name: &str) -> PathBuf {
    networks_dir(owntier_dir).join(format!("{}.cbor", name))
}

/// Create a new network, resolving or generating the device identity from the p43 bus store.
///
/// If `device_id` is given, the matching key must already exist in `bus_dir`.
/// Otherwise a fresh `DeviceKey` is generated and saved, labelled after `name`.
pub fn create(
    name: &str,
    as_number: Option<u32>,
    device_id: Option<&str>,
    bus_dir: &Path,
    owntier_dir: &Path,
) -> Result<NetworkRecord> {
    let net_path = network_path(owntier_dir, name);
    if net_path.exists() {
        anyhow::bail!("network {name:?} already exists at {}", net_path.display());
    }

    let key = match device_id {
        Some(id) => {
            let label = resolve_own_device_label(bus_dir, None, Some(id))
                .with_context(|| format!("device-id {id} not found in {}", bus_dir.display()))?;
            let (_label, _path, key) = resolve_device_key(bus_dir, Some(&label))?;
            key
        }
        None => {
            let mut key = DeviceKey::generate(name);
            let key_path = bus::device_key_path(bus_dir, &key.label);
            if key_path.exists() {
                key.label = key.device_id();
            }
            let key_path = bus::device_key_path(bus_dir, &key.label);
            key.save(&key_path)
                .with_context(|| format!("save device key to {}", key_path.display()))?;
            key
        }
    };

    let record = NetworkRecord {
        name: name.to_string(),
        as_number: as_number.unwrap_or_else(random_as),
        device_id: key.device_id(),
        wg_public_key: key.ecdh_pubkey(),
    };

    save(&record, owntier_dir)?;
    Ok(record)
}

pub fn save(record: &NetworkRecord, owntier_dir: &Path) -> Result<()> {
    let dir = networks_dir(owntier_dir);
    std::fs::create_dir_all(&dir)?;
    let path = network_path(owntier_dir, &record.name);
    let mut buf = Vec::new();
    ciborium::into_writer(record, &mut buf).context("CBOR encode network record")?;
    std::fs::write(&path, &buf)
        .with_context(|| format!("write network record to {}", path.display()))?;
    Ok(())
}

pub fn load(name: &str, owntier_dir: &Path) -> Result<NetworkRecord> {
    let path = network_path(owntier_dir, name);
    let buf = std::fs::read(&path)
        .with_context(|| format!("network {name:?} not found at {}", path.display()))?;
    ciborium::from_reader(buf.as_slice())
        .with_context(|| format!("CBOR decode network record {}", path.display()))
}

pub fn delete(name: &str, owntier_dir: &Path) -> Result<()> {
    let path = network_path(owntier_dir, name);
    std::fs::remove_file(&path)
        .with_context(|| format!("network {name:?} not found at {}", path.display()))
}

pub fn list(owntier_dir: &Path) -> Result<Vec<NetworkRecord>> {
    let dir = networks_dir(owntier_dir);
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut records = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("cbor") {
            continue;
        }
        let buf = std::fs::read(&path)
            .with_context(|| format!("read network record {}", path.display()))?;
        let record: NetworkRecord = ciborium::from_reader(buf.as_slice())
            .with_context(|| format!("CBOR decode network record {}", path.display()))?;
        records.push(record);
    }
    records.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(records)
}
