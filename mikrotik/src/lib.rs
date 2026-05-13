pub mod api;
pub mod deploy;

pub const PLUGIN_TYPE: &str = "mikrotik";

pub mod keys {
    pub const HOST: &str = "host";
    pub const PORT: &str = "port";
    pub const USER: &str = "user";
    pub const PASSWORD: &str = "password";
    pub const WG_NAME: &str = "wg_name";
    pub const WG_IP: &str = "wg_ip";
    pub const WG_PORT: &str = "wg_port";
    pub const SSL: &str = "ssl";
}
