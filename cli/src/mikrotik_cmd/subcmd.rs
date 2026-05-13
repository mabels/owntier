use clap::Subcommand;

#[derive(Subcommand)]
pub enum MikrotikCmd {
    /// Attach MikroTik plugin config to a network (encrypted with the device key).
    ///
    /// Password is read interactively if --password is omitted.
    Attach {
        /// Network name.
        #[arg(long)]
        network: String,

        /// MikroTik hostname or IP address.
        #[arg(long)]
        host: String,

        /// Use TLS (SSL API on port 8729). Certificate is not verified — MikroTik
        /// uses self-signed certs by default.
        #[arg(long)]
        ssl: bool,

        /// RouterOS API port. Defaults to 8729 with --ssl, 8728 without.
        #[arg(long)]
        port: Option<u16>,

        /// RouterOS username (default: admin).
        #[arg(long, default_value = "admin")]
        user: String,

        /// RouterOS password (prompted interactively if omitted).
        #[arg(long)]
        password: Option<String>,

        /// WireGuard interface name on the device (default: wg-<network-name>).
        #[arg(long)]
        wg_name: Option<String>,

        /// WireGuard overlay IP with prefix length (e.g. 10.64.0.1/16).
        #[arg(long)]
        wg_ip: String,

        /// WireGuard listen port (default: 51820).
        #[arg(long, default_value_t = 51820)]
        wg_port: u16,
    },

    /// Show the decrypted MikroTik plugin config for a network.
    Show {
        /// Network name.
        #[arg(long)]
        network: String,
    },

    /// Remove the MikroTik plugin config from a network.
    Detach {
        /// Network name.
        #[arg(long)]
        network: String,
    },
}
