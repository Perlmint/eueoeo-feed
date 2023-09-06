pub struct Config {
    pub port: u16,
    pub listen_host: String,
    pub host_name: String,
    pub sqlite_db: String,
    pub subscription_endpoint: String,
    pub service_did: String,
    pub publisher_did: String,
    pub subscription_reconnect_delay: chrono::Duration,
}

impl<'de> serde::Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = ConfigRaw::deserialize(deserializer)?;

        let port = raw.port.unwrap_or(3000);
        let listen_host = raw.listen_host.unwrap_or_else(|| "localhost".to_string());
        let host_name = raw.host_name.unwrap_or_else(|| "localhost".to_string());
        let sqlite_db = raw.sqlite_db.unwrap_or_else(|| ":memory:".to_string());
        let subscription_endpoint = raw
            .subscription_endpoint
            .unwrap_or_else(|| "wss://bsky.social".to_string());
        let service_did = raw
            .service_did
            .unwrap_or_else(|| format!("did:web:{host_name}"));
        let publisher_did = raw
            .publisher_did
            .unwrap_or_else(|| "did:exapmle:alice".to_string());
        let subscription_reconnect_delay =
            chrono::Duration::milliseconds(raw.subscription_reconnect_delay.unwrap_or(3000) as _);

        Ok(Self {
            port,
            listen_host,
            host_name,
            sqlite_db,
            subscription_endpoint,
            service_did,
            publisher_did,
            subscription_reconnect_delay,
        })
    }
}

#[derive(serde::Deserialize)]
struct ConfigRaw {
    port: Option<u16>,
    listen_host: Option<String>,
    host_name: Option<String>,
    sqlite_db: Option<String>,
    subscription_endpoint: Option<String>,
    service_did: Option<String>,
    publisher_did: Option<String>,
    subscription_reconnect_delay: Option<u32>,
}
