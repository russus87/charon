//! Tunnel SSH puro Rust (crate `russh`): apre un port-forward locale verso il
//! database passando da un server SSH (bastion). Non richiede alcun binario
//! `ssh` di sistema, quindi funziona anche nell'app impacchettata su Windows.
//!
//! Il [`TunnelGuard`] tiene vivo il forward finché non viene rilasciato: la
//! connessione al DB (nativa o puro Rust) punta a `127.0.0.1:<porta locale>`.

/// Segnaposto quando il supporto SSH non è compilato (feature `ssh-tunnel` off),
/// così le firme che ne parlano restano valide.
#[cfg(not(feature = "ssh-tunnel"))]
pub struct TunnelGuard;

#[cfg(feature = "ssh-tunnel")]
pub use imp::{open, TunnelGuard};

#[cfg(feature = "ssh-tunnel")]
mod imp {
    use crate::model::{Connection, SshAuth, SshTunnel};
    use crate::{Error, Result};
    use std::sync::Arc;
    use tokio::net::TcpListener;

    /// Handler SSH minimale. NB: accetta qualunque chiave host (nessuna verifica
    /// known_hosts) — accettabile per un tunnel avviato dall'utente verso un
    /// server che conosce; da irrobustire con la verifica known_hosts.
    struct Handler;

    impl russh::client::Handler for Handler {
        type Error = russh::Error;
        async fn check_server_key(
            &mut self,
            _key: &russh::keys::PublicKey,
        ) -> std::result::Result<bool, Self::Error> {
            Ok(true)
        }
    }

    type Handle = russh::client::Handle<Handler>;

    /// Mantiene aperto il tunnel: al drop, il runtime viene chiuso e il forward
    /// terminato.
    pub struct TunnelGuard {
        pub local_host: String,
        pub local_port: u16,
        // L'ordine conta: il runtime va rilasciato per ultimo.
        _runtime: tokio::runtime::Runtime,
    }

    fn ssherr(e: russh::Error) -> Error {
        Error::Conn(format!("SSH: {e}"))
    }

    async fn connect_and_auth(ssh: &SshTunnel) -> Result<Handle> {
        let config = Arc::new(russh::client::Config::default());
        let mut handle = russh::client::connect(config, (ssh.host.as_str(), ssh.port), Handler)
            .await
            .map_err(|e| Error::Conn(format!("connessione SSH a {}:{}: {e}", ssh.host, ssh.port)))?;

        let ok = match &ssh.auth {
            SshAuth::Password { password } => handle
                .authenticate_password(&ssh.user, password)
                .await
                .map_err(ssherr)?
                .success(),
            SshAuth::Key { path, passphrase } => {
                let pass = if passphrase.is_empty() {
                    None
                } else {
                    Some(passphrase.as_str())
                };
                let key = russh::keys::load_secret_key(path, pass)
                    .map_err(|e| Error::Conn(format!("chiave SSH '{path}': {e}")))?;
                let key = russh::keys::PrivateKeyWithHashAlg::new(Arc::new(key), None);
                handle
                    .authenticate_publickey(&ssh.user, key)
                    .await
                    .map_err(ssherr)?
                    .success()
            }
            SshAuth::Agent => {
                return Err(Error::Unsupported(
                    "autenticazione via ssh-agent non ancora supportata: usa chiave o password".into(),
                ))
            }
        };
        if !ok {
            return Err(Error::Conn("autenticazione SSH rifiutata".into()));
        }
        Ok(handle)
    }

    /// Apre il tunnel verso `conn.host:conn.port` (risolti dal lato del server
    /// SSH) e restituisce la guardia con l'indirizzo locale da usare.
    pub fn open(conn: &Connection) -> Result<TunnelGuard> {
        let ssh = conn
            .ssh
            .clone()
            .ok_or_else(|| Error::Msg("tunnel::open senza configurazione SSH".into()))?;
        let target_host = conn.host.clone();
        let target_port = conn.port as u32;

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| Error::Msg(format!("runtime del tunnel: {e}")))?;

        let local_port = runtime.block_on(async move {
            let handle = Arc::new(connect_and_auth(&ssh).await?);
            let listener = TcpListener::bind(("127.0.0.1", 0u16))
                .await
                .map_err(|e| Error::Msg(format!("bind locale del tunnel: {e}")))?;
            let local_port = listener
                .local_addr()
                .map_err(|e| Error::Msg(e.to_string()))?
                .port();

            // Loop di accettazione: ogni connessione locale apre un canale
            // direct-tcpip verso il DB e travasa i byte nei due sensi.
            tokio::spawn(async move {
                loop {
                    let (mut socket, _) = match listener.accept().await {
                        Ok(pair) => pair,
                        Err(_) => break,
                    };
                    let handle = handle.clone();
                    let host = target_host.clone();
                    tokio::spawn(async move {
                        let channel = match handle
                            .channel_open_direct_tcpip(host, target_port, "127.0.0.1", 0)
                            .await
                        {
                            Ok(ch) => ch,
                            Err(_) => return,
                        };
                        let mut stream = channel.into_stream();
                        let _ = tokio::io::copy_bidirectional(&mut socket, &mut stream).await;
                    });
                }
            });

            Ok::<u16, Error>(local_port)
        })?;

        Ok(TunnelGuard {
            local_host: "127.0.0.1".into(),
            local_port,
            _runtime: runtime,
        })
    }
}
