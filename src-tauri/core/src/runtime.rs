//! Wiring: builds the whole object graph and hands out a running core.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::discovery::{CompositeDiscovery, MdnsDiscovery, OwnAnnouncement, UdpBeacon};
use crate::domain::clock::SystemClock;
use crate::domain::ids::DeviceId;
use crate::domain::nickname::Nickname;
use crate::domain::peer::Handshake;
use crate::domain::peer::PeerProfile;
use crate::error::CoreError;
use crate::ports::discovery::{Discovery, DiscoveryEvent};
use crate::ports::store::{META_DEVICE_ID, META_NICKNAME, Store};
use crate::protocol::limits::{DEFAULT_BEACON_PORT, DEFAULT_TCP_PORT, PROTOCOL_VERSION};
use crate::services::session::{SessionConfig, SessionHandle, spawn as spawn_session};
use crate::services::settings::{SettingsHandle, spawn as spawn_settings};
use crate::storage::SqliteStore;
use crate::transport::connection;
use crate::transport::listener::Listener;

#[derive(Debug, Clone)]
pub struct CoreConfig {
    pub data_dir: PathBuf,
    /// Listen port; falls back to an ephemeral one if it is taken.
    pub preferred_port: u16,
    pub beacon_port: u16,
    /// Nickname for the very first launch only.
    pub default_nickname: Nickname,
    pub enable_discovery: bool,
}

impl CoreConfig {
    #[must_use]
    pub fn new(data_dir: PathBuf, default_nickname: Nickname) -> Self {
        Self {
            data_dir,
            preferred_port: DEFAULT_TCP_PORT,
            beacon_port: DEFAULT_BEACON_PORT,
            default_nickname,
            enable_discovery: true,
        }
    }

    #[must_use]
    pub fn without_discovery(
        data_dir: PathBuf,
        nickname: Nickname,
        port: u16,
        beacon_port: u16,
    ) -> Self {
        Self {
            data_dir,
            preferred_port: port,
            beacon_port,
            default_nickname: nickname,
            enable_discovery: false,
        }
    }
}

pub struct Core {
    pub session: SessionHandle,
    pub settings: SettingsHandle,
    /// The port actually bound, which may differ from the preferred one.
    pub port: u16,
    /// Path the unusable database was preserved under, if recovery happened.
    pub storage_recovered: Option<String>,
    /// Set when discovery could not start; the application still runs.
    pub discovery_problem: Option<String>,
    pub device_id: DeviceId,
    pub profile: PeerProfile,
    discovery: Option<Arc<CompositeDiscovery>>,
    discovery_feed: mpsc::Sender<DiscoveryEvent>,
    listener_task: JoinHandle<()>,
    session_task: JoinHandle<()>,
}

impl Core {
    /// # Errors
    ///
    /// [`CoreError::Storage`] if the database cannot be opened or created, and
    /// [`CoreError::Transport`] if no socket can be bound at all.
    pub async fn start(config: CoreConfig) -> Result<Self, CoreError> {
        let mut config = config;

        tokio::fs::create_dir_all(&config.data_dir)
            .await
            .map_err(|error| {
                CoreError::Task(format!(
                    "could not create the data directory {:?}: {error}",
                    config.data_dir
                ))
            })?;

        let (store, storage_recovered) =
            SqliteStore::open_with_report(&config.data_dir.join("localme.db"))?;

        let (device_id, nickname) = resolve_identity(&store, &config.default_nickname).await?;
        config.default_nickname = nickname.clone();
        let profile = PeerProfile::new(device_id, nickname);

        let listener = Listener::bind(config.preferred_port).await?;
        let port = listener.port();

        let (settings_handle, settings_problem) =
            spawn_settings(config.data_dir.join("settings.json")).await;
        if let Some(problem) = &settings_problem {
            tracing::warn!(%problem, "settings could not be loaded; using defaults");
        }

        let session_config = SessionConfig::new(profile.clone(), port);
        let runtime = spawn_session(session_config, SystemClock, store).await?;

        let (discovery, discovery_problem) = if config.enable_discovery {
            let (discovery, problem) =
                start_discovery(&config, &profile, port, runtime.discovery.clone());
            (Some(discovery), problem)
        } else {
            (None, None)
        };

        let listener_task = spawn_accept_loop(
            listener,
            runtime.handle.clone(),
            runtime.connection.clone(),
            port,
        );

        Ok(Self {
            session: runtime.handle,
            settings: settings_handle,
            port,
            storage_recovered,
            discovery_problem,
            device_id,
            profile,
            discovery,
            discovery_feed: runtime.discovery,
            listener_task,
            session_task: runtime.task,
        })
    }

    /// Exposes the session's discovery input so a test can inject a peer without multicast.
    #[must_use]
    pub fn discovery_feed(&self) -> mpsc::Sender<DiscoveryEvent> {
        self.discovery_feed.clone()
    }

    /// Stops everything in order: peers are told goodbye, then discovery, then the socket.
    pub async fn shutdown(mut self) {
        if let Err(error) = self.session.shutdown().await {
            tracing::debug!(%error, "the session had already stopped");
        }
        if let Some(discovery) = self.discovery.take() {
            discovery.stop();
        }
        self.listener_task.abort();
        // The session task ends on its own once the command channel closes; give it a moment
        // and then stop waiting, because a stuck storage write must not hang exit.
        let _ = tokio::time::timeout(Duration::from_secs(2), &mut self.session_task).await;
    }
}

async fn resolve_identity(
    store: &SqliteStore,
    fallback_nickname: &Nickname,
) -> Result<(DeviceId, Nickname), CoreError> {
    let device_id = match store.meta_get(META_DEVICE_ID).await? {
        Some(raw) => raw.parse::<DeviceId>().map_err(|error| {
            CoreError::Storage(crate::error::StorageError::InvalidRow(format!(
                "stored device id is not a UUID: {error}"
            )))
        })?,
        None => {
            let generated = DeviceId::generate();
            store
                .meta_set(META_DEVICE_ID, &generated.to_string())
                .await?;
            tracing::info!(%generated, "generated a device id for this installation");
            generated
        }
    };

    let nickname = match store.meta_get(META_NICKNAME).await? {
        Some(raw) => Nickname::parse(&raw).unwrap_or_else(|error| {
            tracing::warn!(%error, "stored nickname is not usable; falling back");
            fallback_nickname.clone()
        }),
        None => {
            // Persist the first-launch nickname immediately: leaving it in memory would let a
            // later hostname or locale change rename the user.
            store
                .meta_set(META_NICKNAME, fallback_nickname.as_str())
                .await?;
            fallback_nickname.clone()
        }
    };

    Ok((device_id, nickname))
}

/// A source that fails on its own is logged and skipped by the composite; only a failure of
/// the composite itself is reported here.
fn start_discovery(
    config: &CoreConfig,
    profile: &PeerProfile,
    port: u16,
    sink: mpsc::Sender<crate::ports::discovery::DiscoveryEvent>,
) -> (Arc<CompositeDiscovery>, Option<String>) {
    let announcement = OwnAnnouncement {
        device_id: profile.device_id,
        nickname: profile.nickname.clone(),
        avatar_seed: profile.avatar_seed.clone(),
        port,
    };

    let sources: Vec<Box<dyn Discovery>> = vec![
        Box::new(MdnsDiscovery::new(announcement.clone())),
        Box::new(UdpBeacon::new(announcement, config.beacon_port)),
    ];
    let composite = Arc::new(CompositeDiscovery::new(sources, profile.device_id));

    let problem = match composite.start(sink) {
        Ok(()) => {
            tracing::info!(port, beacon_port = config.beacon_port, "discovery started");
            None
        }
        Err(error) => {
            // Peers announced before the failure remain reachable, messages keep flowing.
            let message = format!("discovery could not be started: {error}");
            tracing::warn!(%message);
            Some(message)
        }
    };

    (composite, problem)
}

/// Accepts inbound connections forever.
///
/// Each accepted socket gets its own task, so a peer that connects and then says nothing
/// cannot hold up the listeners for everyone else. The handshake is built from the session's
/// *current* identity rather than a value captured at startup, which is what keeps the
/// announced nickname correct after a rename.
fn spawn_accept_loop(
    listener: Listener,
    session: SessionHandle,
    context: Arc<connection::ConnectionContext>,
    port: u16,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            let (stream, address) = match listener.accept().await {
                Ok(accepted) => accepted,
                Err(error) => {
                    tracing::warn!(%error, "accept failed");
                    // Usually resource exhaustion: back off rather than spin.
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    continue;
                }
            };

            let session = session.clone();
            let context = context.clone();
            tokio::spawn(async move {
                let profile = match session.own_profile().await {
                    Ok(profile) => profile,
                    Err(error) => {
                        tracing::debug!(%error, "dropping an inbound connection: session is gone");
                        return;
                    }
                };
                let handshake = Handshake::new(PROTOCOL_VERSION, &profile, port);
                match connection::accept(stream, &handshake, &context).await {
                    Ok(handshaken) => connection::serve(handshaken, context).await,
                    Err(error) => {
                        tracing::debug!(%address, %error, "inbound connection was refused");
                    }
                }
            });
        }
    })
}
