use anyhow::{bail, Context, Result};
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng as AeadOsRng},
    Aes256Gcm,
};
use hkdf::Hkdf;
use sha2::Sha256;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use x25519_dalek::{EphemeralSecret, PublicKey};

// ── paths ─────────────────────────────────────────────────────────────────────

fn plugins_dir(owntier_dir: &Path) -> PathBuf {
    owntier_dir.join("plugins")
}

fn plugin_path(owntier_dir: &Path, network: &str, plugin_type: &str) -> PathBuf {
    plugins_dir(owntier_dir).join(format!("{network}.{plugin_type}.bin"))
}

// ── ECIES: ephemeral X25519 + HKDF-SHA256 + ChaCha20-Poly1305 ────────────────
//
// Blob layout: eph_pubkey(32) | nonce(12) | ciphertext+tag

fn seal(plaintext: &[u8], device_pubkey: &[u8; 32]) -> Result<Vec<u8>> {
    let eph_secret = EphemeralSecret::random_from_rng(AeadOsRng);
    let eph_public = PublicKey::from(&eph_secret);
    let shared = eph_secret.diffie_hellman(&PublicKey::from(*device_pubkey));

    let mut key = [0u8; 32];
    Hkdf::<Sha256>::new(None, shared.as_bytes())
        .expand(b"owntier-plugin-config", &mut key)
        .expect("32-byte key always fits");

    let cipher = Aes256Gcm::new_from_slice(&key).expect("32-byte key is valid");
    let nonce = Aes256Gcm::generate_nonce(AeadOsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("encrypt plugin config: {e}"))?;

    let mut blob = Vec::with_capacity(32 + 12 + ciphertext.len());
    blob.extend_from_slice(eph_public.as_bytes());
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ciphertext);
    Ok(blob)
}

fn open(blob: &[u8], device_secret: &[u8; 32]) -> Result<Vec<u8>> {
    if blob.len() < 32 + 12 + 16 {
        bail!("plugin blob too short");
    }
    let eph_pubkey: [u8; 32] = blob[..32].try_into().unwrap();
    let nonce: [u8; 12] = blob[32..44].try_into().unwrap();
    let ciphertext = &blob[44..];

    let device_priv = x25519_dalek::StaticSecret::from(*device_secret);
    let shared = device_priv.diffie_hellman(&PublicKey::from(eph_pubkey));

    let mut key = [0u8; 32];
    Hkdf::<Sha256>::new(None, shared.as_bytes())
        .expand(b"owntier-plugin-config", &mut key)
        .expect("32-byte key always fits");

    let cipher = Aes256Gcm::new_from_slice(&key).expect("32-byte key is valid");
    cipher
        .decrypt(nonce.as_ref().into(), ciphertext)
        .map_err(|e| anyhow::anyhow!("decrypt plugin config: {e}"))
}

// ── public API ────────────────────────────────────────────────────────────────

/// Encrypt and store a plugin config for a network.
/// Overwrites any existing config for the same plugin type.
pub fn attach(
    plugin_type: &str,
    network: &str,
    values: &HashMap<String, String>,
    device_pubkey: &[u8; 32],
    owntier_dir: &Path,
) -> Result<()> {
    let mut buf = Vec::new();
    ciborium::into_writer(values, &mut buf).context("CBOR encode plugin config")?;
    let blob = seal(&buf, device_pubkey)?;

    let dir = plugins_dir(owntier_dir);
    std::fs::create_dir_all(&dir)?;
    let path = plugin_path(owntier_dir, network, plugin_type);
    std::fs::write(&path, &blob)
        .with_context(|| format!("write plugin config to {}", path.display()))?;
    Ok(())
}

/// Decrypt and return a plugin config for a network.
pub fn load(
    plugin_type: &str,
    network: &str,
    device_secret: &[u8; 32],
    owntier_dir: &Path,
) -> Result<HashMap<String, String>> {
    let path = plugin_path(owntier_dir, network, plugin_type);
    let blob = std::fs::read(&path).with_context(|| {
        format!(
            "no {plugin_type} plugin attached to network {network:?} ({})",
            path.display()
        )
    })?;
    let plaintext = open(&blob, device_secret)?;
    ciborium::from_reader(plaintext.as_slice()).context("CBOR decode plugin config")
}

/// Return the plugin types attached to a network (e.g. `["mikrotik"]`).
pub fn list_attached(network: &str, owntier_dir: &Path) -> Result<Vec<String>> {
    let dir = plugins_dir(owntier_dir);
    if !dir.exists() {
        return Ok(vec![]);
    }
    let prefix = format!("{network}.");
    let suffix = ".bin";
    let mut types = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let name = entry?.file_name();
        let name = name.to_string_lossy();
        if let Some(rest) = name.strip_prefix(&prefix) {
            if let Some(plugin_type) = rest.strip_suffix(suffix) {
                types.push(plugin_type.to_string());
            }
        }
    }
    types.sort();
    Ok(types)
}

/// Remove a plugin config for a network.
pub fn detach(plugin_type: &str, network: &str, owntier_dir: &Path) -> Result<()> {
    let path = plugin_path(owntier_dir, network, plugin_type);
    std::fs::remove_file(&path).with_context(|| {
        format!("no {plugin_type} plugin attached to network {network:?}")
    })
}
