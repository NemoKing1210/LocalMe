//! Wiring: builds the whole object graph and hands out a running core.
//!
//! This is the only module that knows which concrete adapters exist. Everything above it —
//! services, ports, domain — is expressed in terms of traits, and everything below it is an
//! implementation detail. A test that wants two cores on loopback calls
//! [`Core::start_for_test`] twice, which is exactly what the integration suite does.

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

/// Everything the core needs to start.
#[derive(Debug, Clone)]
pub struct CoreConfig {
    /// Directory holding the database and the settings file.
    pub data_dir: PathBuf,
    /// Port to listen on, falling back to an ephemeral one if it is taken.
    pub preferred_port: u16,
    /// UDP port for the discovery beacon.
    pub beacon_port: u16,
    /// Nickname to use on the very first launch.
    pub default_nickname: Nickname,
    /// Whether discovery should run at all.
    ///
    /// Used by tests that want to control peer discovery precisely; production always runs it.
    pub enable_discovery: bool,
}

impl CoreConfig {
    /// A configuration for a normal launch.
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

    /// A configuration with discovery disabled, for tests that drive it themselves.
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

/// A running core: the handles the host application uses, plus what is needed to stop it.
pub struct Core {
    /// Commands and events for the session.
    pub session: SessionHandle,
    /// The settings document.
    pub settings: SettingsHandle,
    /// The port this instance actually listens on.
    pub port: u16,
    /// Set when the database was unusable and has been preserved under this path.
    pub storage_recovered: Option<String>,
    /// Set when discovery could not be started; the application still runs.
    pub discovery_problem: Option<String>,
    /// This device's identifier.
    pub device_id: DeviceId,
    /// Our own identity as announced.
    pub profile: PeerProfile,
    discovery: Option<Arc<CompositeDiscovery>>,
    discovery_feed: mpsc::Sender<DiscoveryEvent>,
    listener_task: JoinHandle<()>,
    session_task: JoinHandle<()>,
}

impl Core {
    /// Opens the database, binds the listener, starts discovery and the session.
    ///
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

    /// A channel that feeds the session's discovery input directly.
    ///
    /// Discovery sources normally own this sender. Exposing it is what lets the loopback
    /// integration test hand the session a peer without depending on multicast, while still
    /// exercising the production path end to end: the session cannot tell an injected
    /// `Found` from one a discovery adapter produced.
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

/// Reads the durable identity, creating it on first launch.
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
            // Persist the first-launch nickname immediately. Leaving it only in memory would
            // make the identity depend on whatever default the *next* launch happens to
            // compute — a hostname change or a different locale would rename the user.
            store
                .meta_set(META_NICKNAME, fallback_nickname.as_str())
                .await?;
            fallback_nickname.clone()
        }
    };

    Ok((device_id, nickname))
}

/// Starts mDNS and the UDP beacon, reporting a failure instead of aborting the launch.
///
/// A mechanism that fails on its own is logged and skipped by the composite, so one blocked
/// port or an unavailable mDNS stack degrades discovery instead of disabling it. Only a
/// failure of the composite itself — no runtime, no channel — is reported here.
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
            // The application still works: peers announced before the failure remain
            // reachable, and messages to them keep flowing.
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
                    // A failing accept is usually resource exhaustion: back off rather than
                    // spin on the error.
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
