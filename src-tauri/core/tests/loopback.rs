#![allow(clippy::unwrap_used, clippy::expect_used)]
//! End-to-end tests: two independent cores on the loopback interface.
//!
//! These are the tests that prove the acceptance criteria rather than the units: two
//! instances discover each other, exchange messages, acknowledge them, go offline with a
//! correct "last seen", and forget each other on request. Nothing here talks to the Tauri
//! host, which is the point — the core is a complete application on its own.
//!
//! Discovery is injected rather than multicast in the deterministic tests, so the suite runs
//! the same way on a developer machine and in a container without a working multicast route.
//! [`two_instances_find_each_other_over_mdns`] covers the real discovery path separately.

use std::net::SocketAddr;
use std::path::Path;
use std::time::{Duration, Instant};

use localme_core::domain::ids::DeviceId;
use localme_core::domain::message::{MessageBody, MessageStatus};
use localme_core::domain::nickname::Nickname;
use localme_core::domain::peer::PeerView;
use localme_core::ports::discovery::{DiscoveredPeer, DiscoveryEvent};
use localme_core::runtime::{Core, CoreConfig};
use tempfile::TempDir;

/// A core running out of a temporary directory.
struct Instance {
    core: Core,
    _data_dir: TempDir,
}

impl Instance {
    async fn start(nickname: &str) -> Self {
        let data_dir = tempfile::tempdir().expect("temp dir");
        let config = CoreConfig::without_discovery(
            data_dir.path().to_path_buf(),
            Nickname::parse(nickname).expect("valid nickname"),
            0,
            0,
        );
        let core = Core::start(config).await.expect("core starts");
        Self {
            core,
            _data_dir: data_dir,
        }
    }

    fn port(&self) -> u16 {
        self.core.port
    }

    fn device_id(&self) -> DeviceId {
        self.core.device_id
    }

    fn loopback_address(&self) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], self.port()))
    }

    /// Announces another instance to this one, exactly as a discovery adapter would.
    async fn discover(&self, peer: &Instance, nickname: &str) {
        let announcement = DiscoveredPeer {
            device_id: peer.device_id(),
            nickname: Nickname::parse(nickname).expect("valid nickname"),
            avatar_seed: peer.core.profile.avatar_seed.clone(),
            addresses: vec![peer.loopback_address()],
        };
        self.core
            .discovery_feed()
            .send(DiscoveryEvent::Found(announcement))
            .await
            .expect("the session accepts the announcement");
    }

    async fn peers(&self) -> Vec<PeerView> {
        self.core.session.list_peers().await.expect("peers")
    }

    async fn peer(&self, device_id: DeviceId) -> Option<PeerView> {
        self.peers()
            .await
            .into_iter()
            .find(|peer| peer.device_id == device_id)
    }
}

/// Waits until `condition` holds, or fails with `what` after `timeout`.
async fn eventually<F, Fut>(what: &str, timeout: Duration, mut condition: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    let deadline = Instant::now() + timeout;
    loop {
        if condition().await {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out after {timeout:?} waiting for: {what}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

/// Installs a log subscriber when `RUST_LOG` is set, so a failing discovery test can be
/// diagnosed with `RUST_LOG=localme_core=debug cargo test -- --nocapture`.
fn init_logging() {
    if std::env::var_os("RUST_LOG").is_none() {
        return;
    }
    let filter = tracing_subscriber::EnvFilter::from_default_env();
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_test_writer()
        .try_init();
}

/// The message the tests exchange.
fn body(text: &str) -> MessageBody {
    MessageBody::parse(text).expect("valid body")
}

#[tokio::test]
async fn two_instances_connect_and_exchange_messages() {
    let anna = Instance::start("anna").await;
    let boris = Instance::start("boris").await;

    anna.discover(&boris, "boris").await;

    eventually(
        "boris to appear online for anna",
        Duration::from_secs(10),
        || async {
            anna.peer(boris.device_id())
                .await
                .is_some_and(|peer| peer.online)
        },
    )
    .await;

    // The connection is symmetric: boris is told who dialled him only through the handshake.
    eventually(
        "anna to appear online for boris",
        Duration::from_secs(10),
        || async {
            boris
                .peer(anna.device_id())
                .await
                .is_some_and(|peer| peer.online)
        },
    )
    .await;

    let view = anna.peer(boris.device_id()).await.expect("boris is known");
    assert_eq!(view.nickname.as_str(), "boris");
    assert_eq!(view.unread, 0);

    // anna → boris
    let sent = anna
        .core
        .session
        .send_message(boris.device_id(), body("привет, Борис"))
        .await
        .expect("message is accepted");
    assert!(
        matches!(
            sent.status,
            MessageStatus::Sending | MessageStatus::Sent | MessageStatus::Delivered
        ),
        "unexpected status {:?}",
        sent.status
    );

    let delivered = sent.id;
    eventually(
        "the message to be acknowledged",
        Duration::from_secs(10),
        || async {
            anna.core
                .session
                .history(boris.device_id(), None, 10)
                .await
                .expect("history")
                .first()
                .is_some_and(|message| {
                    message.id == delivered && message.status == MessageStatus::Delivered
                })
        },
    )
    .await;

    // boris has it, unread.
    let received = boris
        .core
        .session
        .history(anna.device_id(), None, 10)
        .await
        .expect("history");
    assert_eq!(received.len(), 1);
    assert_eq!(
        received.first().map(|m| m.body.as_str()),
        Some("привет, Борис")
    );
    assert_eq!(
        boris.peer(anna.device_id()).await.map(|peer| peer.unread),
        Some(1)
    );

    // boris → anna, so both directions are covered.
    boris
        .core
        .session
        .send_message(anna.device_id(), body("привет, Аня"))
        .await
        .expect("message is accepted");

    eventually(
        "anna to receive the reply",
        Duration::from_secs(10),
        || async {
            anna.core
                .session
                .history(boris.device_id(), None, 10)
                .await
                .expect("history")
                .len()
                == 2
        },
    )
    .await;

    // Reading clears the badge and is reported.
    let changed = boris
        .core
        .session
        .mark_read(anna.device_id())
        .await
        .expect("mark read");
    assert_eq!(changed, 1);
    assert_eq!(
        boris.peer(anna.device_id()).await.map(|peer| peer.unread),
        Some(0)
    );

    anna.core.shutdown().await;
    boris.core.shutdown().await;
}

#[tokio::test]
async fn a_peer_that_quits_goes_offline_with_a_last_seen_time() {
    let anna = Instance::start("anna").await;
    let boris = Instance::start("boris").await;
    anna.discover(&boris, "boris").await;

    eventually("boris to come online", Duration::from_secs(10), || async {
        anna.peer(boris.device_id())
            .await
            .is_some_and(|peer| peer.online)
    })
    .await;

    // A graceful shutdown sends `goodbye`, so the transition is immediate rather than waiting
    // for the heartbeat timeout.
    let boris_id = boris.device_id();
    boris.core.shutdown().await;

    eventually("boris to go offline", Duration::from_secs(5), || async {
        anna.peer(boris_id).await.is_some_and(|peer| !peer.online)
    })
    .await;

    let view = anna.peer(boris_id).await.expect("still listed");
    assert!(!view.online, "an offline peer stays in the list");
    assert!(
        view.last_seen_ms.is_some(),
        "the offline row must carry a last-seen time for the interface to render"
    );

    // Writing to an offline peer is refused, which is what disables the composer.
    let refused = anna
        .core
        .session
        .send_message(boris_id, body("are you there?"))
        .await;
    assert!(
        refused.is_err(),
        "a message to an offline peer must not be accepted"
    );

    anna.core.shutdown().await;
}

#[tokio::test]
async fn forgetting_a_peer_hides_it_and_can_delete_its_history() {
    let anna = Instance::start("anna").await;
    let boris = Instance::start("boris").await;
    anna.discover(&boris, "boris").await;

    eventually(
        "a conversation to exist",
        Duration::from_secs(10),
        || async {
            anna.peer(boris.device_id())
                .await
                .is_some_and(|peer| peer.online)
        },
    )
    .await;

    anna.core
        .session
        .send_message(boris.device_id(), body("goodbye"))
        .await
        .expect("message is accepted");

    anna.core
        .session
        .forget(boris.device_id(), true)
        .await
        .expect("forget");

    assert!(
        anna.peer(boris.device_id()).await.is_none(),
        "a forgotten peer must leave the user list"
    );
    assert!(
        anna.core
            .session
            .history(boris.device_id(), None, 10)
            .await
            .expect("history")
            .is_empty(),
        "the conversation was deleted with it"
    );

    // The device is still known, so the settings screen can show what was forgotten.
    let known = anna.core.session.known_devices().await.expect("devices");
    let forgotten = known
        .iter()
        .find(|device| device.device_id == boris.device_id())
        .expect("the device is still known");
    assert!(forgotten.forgotten, "and it is marked as forgotten");

    // "If their computer appears on the network again, it will be added as a new person":
    // a fresh announcement brings it back, and because the history was deleted with the
    // forget, it comes back empty — that is the whole point of the option.
    anna.discover(&boris, "boris").await;
    eventually(
        "a forgotten device that reappears to be added again",
        Duration::from_secs(10),
        || async {
            anna.peer(boris.device_id())
                .await
                .is_some_and(|peer| peer.online)
        },
    )
    .await;

    assert!(
        anna.core
            .session
            .history(boris.device_id(), None, 10)
            .await
            .expect("history")
            .is_empty(),
        "it comes back as a new person, without the deleted history"
    );
    assert_eq!(
        anna.peer(boris.device_id()).await.map(|peer| peer.unread),
        Some(0),
        "and with no stale unread count"
    );

    anna.core.shutdown().await;
    boris.core.shutdown().await;
}

#[tokio::test]
async fn a_forgotten_device_can_be_restored_without_reappearing() {
    let anna = Instance::start("anna").await;
    let boris = Instance::start("boris").await;
    anna.discover(&boris, "boris").await;

    eventually("a connection", Duration::from_secs(10), || async {
        anna.peer(boris.device_id())
            .await
            .is_some_and(|peer| peer.online)
    })
    .await;

    // Keeping the history is what makes restore meaningful: the conversation is still there
    // when the device comes back into the list.
    anna.core
        .session
        .send_message(boris.device_id(), body("remember me"))
        .await
        .expect("message is accepted");
    anna.core
        .session
        .forget(boris.device_id(), false)
        .await
        .expect("forget");

    let boris_id = boris.device_id();
    boris.core.shutdown().await;

    anna.core.session.restore(boris_id).await.expect("restore");

    let restored = anna.peer(boris_id).await.expect("back in the list");
    assert!(!restored.online, "restoring is not the same as connecting");
    assert_eq!(
        anna.core
            .session
            .history(boris_id, None, 10)
            .await
            .expect("history")
            .len(),
        1,
        "the kept conversation is still there"
    );
    assert!(
        anna.core
            .session
            .known_devices()
            .await
            .expect("devices")
            .iter()
            .find(|device| device.device_id == boris_id)
            .is_some_and(|device| !device.forgotten),
        "and the device is no longer marked as forgotten"
    );

    anna.core.shutdown().await;
}

#[tokio::test]
async fn forgetting_without_deleting_keeps_the_history() {
    let anna = Instance::start("anna").await;
    let boris = Instance::start("boris").await;
    anna.discover(&boris, "boris").await;

    eventually("a connection", Duration::from_secs(10), || async {
        anna.peer(boris.device_id())
            .await
            .is_some_and(|peer| peer.online)
    })
    .await;

    anna.core
        .session
        .send_message(boris.device_id(), body("keep me"))
        .await
        .expect("message is accepted");

    anna.core
        .session
        .forget(boris.device_id(), false)
        .await
        .expect("forget");

    let history = anna
        .core
        .session
        .history(boris.device_id(), None, 10)
        .await
        .expect("history");
    assert_eq!(
        history.len(),
        1,
        "the option to keep history must be honoured"
    );
    assert!(
        anna.peer(boris.device_id()).await.is_none(),
        "but the peer still leaves the list"
    );

    anna.core.shutdown().await;
    boris.core.shutdown().await;
}

#[tokio::test]
async fn a_renamed_instance_tells_its_peers() {
    let anna = Instance::start("anna").await;
    let boris = Instance::start("boris").await;
    anna.discover(&boris, "boris").await;

    eventually("a connection", Duration::from_secs(10), || async {
        boris
            .peer(anna.device_id())
            .await
            .is_some_and(|peer| peer.online)
    })
    .await;

    let before = boris
        .peer(anna.device_id())
        .await
        .expect("anna is known")
        .avatar_seed;
    assert_eq!(before.as_str(), format!("{}:anna", anna.device_id()));

    anna.core
        .session
        .set_nickname(Nickname::parse("Аня").expect("valid"))
        .await
        .expect("rename");

    eventually(
        "the new nickname to arrive",
        Duration::from_secs(10),
        || async {
            boris
                .peer(anna.device_id())
                .await
                .is_some_and(|peer| peer.nickname.as_str() == "Аня")
        },
    )
    .await;

    let after = boris.peer(anna.device_id()).await.expect("anna is known");
    assert_eq!(
        after.avatar_seed.as_str(),
        format!("{}:Аня", anna.device_id()),
        "the avatar seed travels with the nickname, so the picture changes for everyone"
    );

    anna.core.shutdown().await;
    boris.core.shutdown().await;
}

#[tokio::test]
async fn messages_survive_a_restart() {
    let anna = Instance::start("anna").await;
    let boris = Instance::start("boris").await;
    anna.discover(&boris, "boris").await;

    eventually("a connection", Duration::from_secs(10), || async {
        anna.peer(boris.device_id())
            .await
            .is_some_and(|peer| peer.online)
    })
    .await;

    anna.core
        .session
        .send_message(boris.device_id(), body("see you tomorrow"))
        .await
        .expect("message is accepted");

    // Wait for the acknowledgement so the row is committed on the receiving side too.
    eventually("delivery", Duration::from_secs(10), || async {
        boris
            .core
            .session
            .history(anna.device_id(), None, 10)
            .await
            .expect("history")
            .len()
            == 1
    })
    .await;

    let anna_id = anna.device_id();
    let boris_id = boris.device_id();
    let data_dir = Path::to_path_buf(anna._data_dir.path());

    anna.core.shutdown().await;
    boris.core.shutdown().await;

    // Reopen boris over the same data directory: the identity and the conversation are intact.
    let config =
        CoreConfig::without_discovery(data_dir, Nickname::parse("ignored").expect("valid"), 0, 0);
    let reopened = Core::start(config).await.expect("core restarts");
    assert_eq!(
        reopened.device_id, anna_id,
        "the device id must survive a restart, or peers would see a new person"
    );
    assert_eq!(
        reopened.profile.nickname.as_str(),
        "anna",
        "the stored nickname wins over the fallback"
    );

    let history = reopened
        .session
        .history(boris_id, None, 10)
        .await
        .expect("history");
    assert_eq!(history.len(), 1);
    assert_eq!(
        history.first().map(|m| m.body.as_str()),
        Some("see you tomorrow")
    );

    reopened.shutdown().await;
}

#[tokio::test]
async fn a_peer_with_no_addresses_is_listed_but_not_dialled() {
    let anna = Instance::start("anna").await;

    // An announcement with no addresses is legal — a peer behind a firewall can announce
    // itself and still be unreachable — and must not be retried in a tight loop.
    anna.core
        .discovery_feed()
        .send(DiscoveryEvent::Found(DiscoveredPeer {
            device_id: DeviceId::generate(),
            nickname: Nickname::parse("ghost").expect("valid"),
            avatar_seed: localme_core::domain::ids::AvatarSeed::parse("ghost").expect("valid"),
            addresses: Vec::new(),
        }))
        .await
        .expect("announced");

    eventually("the ghost to be listed", Duration::from_secs(5), || async {
        anna.peers()
            .await
            .iter()
            .any(|peer| peer.nickname.as_str() == "ghost")
    })
    .await;

    let ghost = anna
        .peers()
        .await
        .into_iter()
        .find(|peer| peer.nickname.as_str() == "ghost")
        .expect("listed");
    assert!(!ghost.online, "an unreachable peer is never online");

    anna.core.shutdown().await;
}

/// Real mDNS discovery between two instances, in one process.
///
/// This is acceptance criterion 1 exercised through the whole stack — a `ServiceDaemon` per
/// instance, real announcements on the machine's interfaces, the session dialling what it hears
/// — and it is the slowest and least reliable test in the suite, which is why it is not part of
/// the default run:
///
/// * it needs multicast to work between two daemons inside one process on whichever interfaces
///   the machine has, and a machine with several virtual adapters (Hyper-V, container, VPN) makes
///   that a race rather than a certainty;
/// * it takes seconds when it passes and a timeout when it does not.
///
/// A suite that fails one run in five teaches people to ignore failures, so this one is opt-in:
///
/// ```text
/// cargo test -p localme-core --test loopback -- --ignored
/// ```
///
/// The stronger check is the documented manual one — two instances of the real application
/// (README, "Two instances on one computer") — which is how it was verified: the second instance
/// logged `peer is online` 20 ms after starting discovery.
#[tokio::test]
#[ignore = "needs multicast between two daemons; run explicitly with --ignored"]
async fn two_instances_find_each_other_over_mdns() {
    init_logging();

    let anna_dir = tempfile::tempdir().expect("temp dir");
    let boris_dir = tempfile::tempdir().expect("temp dir");

    let anna = Core::start(CoreConfig {
        data_dir: anna_dir.path().to_path_buf(),
        preferred_port: 0,
        beacon_port: 0,
        default_nickname: Nickname::parse("mdns-anna").expect("valid"),
        enable_discovery: true,
    })
    .await
    .expect("anna starts");

    let boris = Core::start(CoreConfig {
        data_dir: boris_dir.path().to_path_buf(),
        preferred_port: 0,
        beacon_port: 0,
        default_nickname: Nickname::parse("mdns-boris").expect("valid"),
        enable_discovery: true,
    })
    .await
    .expect("boris starts");

    let anna_id = anna.device_id;
    let boris_id = boris.device_id;

    eventually(
        "mDNS discovery to make the two instances connect",
        Duration::from_secs(40),
        || async {
            let seen = anna.session.list_peers().await.unwrap_or_default();
            seen.iter()
                .any(|peer| peer.device_id == boris_id && peer.online)
        },
    )
    .await;

    let seen = boris.session.list_peers().await.expect("peers");
    assert!(
        seen.iter()
            .any(|peer| peer.device_id == anna_id && peer.online),
        "both ends must see the connection: {seen:?}"
    );

    anna.shutdown().await;
    boris.shutdown().await;
}
