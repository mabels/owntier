use clap::Subcommand;

#[derive(Subcommand)]
pub enum NetworkCmd {
    /// Create a new WireGuard + BGP mesh network.
    ///
    /// Generates (or reuses) a p43 device key whose X25519 public key becomes
    /// the WireGuard public key for this node.
    Create {
        /// Network name (also used as the device key label if --device-id is omitted).
        #[arg(long)]
        name: String,

        /// 32-bit BGP AS number for this hub.
        /// Generated randomly if omitted: upper 16 bits random (1–65535),
        /// lower 16 bits from the private range (65000–65535).
        #[arg(long)]
        r#as: Option<u32>,

        /// Reuse an existing p43 device key by its device-id.
        /// If omitted, a fresh key is generated and saved to the p43 bus store.
        #[arg(long, value_name = "DEVICE_ID")]
        device_id: Option<String>,
    },

    /// Show details of a network.
    Show {
        /// Network name.
        #[arg(value_name = "NAME")]
        name: String,
    },

    /// Delete a network record (the p43 device key is not removed).
    Delete {
        /// Network name.
        #[arg(value_name = "NAME")]
        name: String,

        /// Skip confirmation prompt.
        #[arg(long)]
        force: bool,
    },

    /// Deploy a network to its attached plugin target.
    ///
    /// Reads connection details from the encrypted plugin config attached with
    /// e.g. `owntier mikrotik attach`.
    Deploy {
        /// Network name.
        #[arg(value_name = "NAME")]
        name: String,

        /// Create config but leave the WireGuard interface disabled.
        #[arg(long)]
        stopped: bool,

        /// Print each RouterOS command as it is sent.
        #[arg(long)]
        verbose: bool,

        /// Print all RouterOS commands without connecting or sending anything.
        #[arg(long)]
        dry_run: bool,
    },

    /// List all networks.
    List,
}
