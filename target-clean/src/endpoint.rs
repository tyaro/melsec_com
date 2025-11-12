use crate::mc_define::AccessRoute;

/// Represents a connection target (PLC endpoint) including address, access route and monitor timer.
#[derive(Clone, Debug)]
pub struct ConnectionTarget {
    /// TCP/UDP address as "host:port"
    pub ip: String,
    pub port: u16,
    pub addr: String,
    /// MC4E access route (5 bytes)
    pub access_route: AccessRoute,
}

const DEFAULT_IP: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 5000;

impl ConnectionTarget {
    /// Create a new `ConnectionTarget` with full options.
    #[must_use]
    pub fn new() -> Self {
        Self {
            ip: DEFAULT_IP.to_string(),
            port: DEFAULT_PORT,
            addr: format!("{DEFAULT_IP}:{DEFAULT_PORT}"),
            access_route: AccessRoute::default(),
        }
    }
    #[must_use]
    pub const fn with_access_route(mut self, route: AccessRoute) -> Self {
        self.access_route = route;
        self
    }
    #[must_use]
    pub fn with_ip(mut self, ip: impl Into<String>) -> Self {
        self.ip = ip.into();
        self.addr = format!(
            "{self_ip}:{self_port}",
            self_ip = self.ip,
            self_port = self.port
        );
        self
    }
    #[must_use]
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self.addr = format!(
            "{self_ip}:{self_port}",
            self_ip = self.ip,
            self_port = self.port
        );
        self
    }
    #[must_use]
    pub const fn build(self) -> Self {
        self
    }
    /// Create a direct-connection target using common access-route `[0x00, 0xFF, 0xFF, 0x03, 0x00]`
    #[must_use]
    pub fn direct(ip: impl Into<String>, port: u16) -> Self {
        Self::new()
            .with_ip(ip)
            .with_port(port)
            .with_access_route(AccessRoute::default())
    }
}
impl Default for ConnectionTarget {
    fn default() -> Self {
        Self::new()
    }
}
