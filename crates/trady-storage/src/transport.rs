//! Verified TLS for remote PostgreSQL; plaintext is restricted to explicit local hosts.

use native_tls::{Protocol, TlsConnector};
use postgres::{Client, Config, NoTls, config::Host, config::SslMode};
use postgres_native_tls::MakeTlsConnector;
use trady_domain::{Error, Result, require};

fn uses_tls(config: &Config) -> Result<bool> {
    require(
        !config.get_hosts().is_empty(),
        "explicit database host is required",
    )?;
    let local_hosts = config.get_hosts().iter().all(|host| match host {
        Host::Tcp(name) => matches!(name.as_str(), "127.0.0.1" | "::1"),
        #[cfg(unix)]
        Host::Unix(_) => true,
    });
    let local_addresses = config
        .get_hostaddrs()
        .iter()
        .all(std::net::IpAddr::is_loopback);
    require(
        (local_hosts && local_addresses) || config.get_ssl_mode() == SslMode::Require,
        "remote database host or hostaddr requires sslmode=require with verified TLS",
    )?;
    let tls = config.get_ssl_mode() == SslMode::Require;
    require(
        !tls || config
            .get_hosts()
            .iter()
            .all(|host| matches!(host, Host::Tcp(_))),
        "TLS database connections require TCP server names",
    )?;
    Ok(tls)
}

/// Remote DSNs must request TLS. The platform trust store and connector verify
/// both the certificate chain and the configured server name. No downgrade,
/// invalid-certificate or invalid-hostname option is exposed.
pub fn connect(dsn: &str) -> Result<Client> {
    let mut config: Config = dsn
        .parse()
        .map_err(|_| Error("invalid PostgreSQL connection configuration".into()))?;
    let tls = uses_tls(&config)?;
    if config.get_connect_timeout().is_none() {
        config.connect_timeout(std::time::Duration::from_secs(10));
    }
    if tls {
        let connector = TlsConnector::builder()
            .min_protocol_version(Some(Protocol::Tlsv12))
            .build()
            .map_err(|_| Error("cannot initialize verified PostgreSQL TLS".into()))?;
        config
            .connect(MakeTlsConnector::new(connector))
            .map_err(super::db)
    } else {
        config.connect(NoTls).map_err(super::db)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_explicit_local_endpoints_allow_plaintext() {
        for dsn in ["host=127.0.0.1", "host=::1", "host=/tmp"] {
            assert!(!uses_tls(&dsn.parse().unwrap()).unwrap());
        }
        for dsn in [
            "",
            "host=db.example.com",
            "host=localhost",
            "host=127.0.0.1 hostaddr=203.0.113.10",
            "host=db.example.com sslmode=disable",
        ] {
            assert!(uses_tls(&dsn.parse().unwrap()).is_err(), "{dsn}");
        }
    }

    #[test]
    fn remote_connections_require_tls_without_downgrade() {
        for dsn in [
            "host=db.example.com sslmode=require",
            "host=127.0.0.1 sslmode=require",
            "host=db.example.com hostaddr=203.0.113.10 sslmode=require",
        ] {
            assert!(uses_tls(&dsn.parse().unwrap()).unwrap());
        }
        assert!(uses_tls(&"host=/tmp sslmode=require".parse().unwrap()).is_err());
    }
}
