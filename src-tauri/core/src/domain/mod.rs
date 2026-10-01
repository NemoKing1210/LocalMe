//! Pure domain: identity, peers, messages and the presence state machine.
//!
//! Nothing here touches the network, the database, the clock or the UI. Every rule that
//! can be expressed as a function of its inputs is expressed here, which is what makes the
//! interesting parts of the system unit-testable without a socket or a filesystem.

pub mod clock;
pub mod ids;
pub mod message;
pub mod nickname;
pub mod peer;
pub mod presence;

pub use clock::{Clock, ManualClock, SystemClock, UnixMillis};
pub use ids::{AvatarSeed, DeviceId, MessageId};
pub use message::{ChatMessage, Direction, MessageBody, MessagePreview, MessageStatus};
pub use nickname::Nickname;
pub use peer::{Handshake, PeerProfile, PeerView};
pub use presence::{PresenceMachine, PresencePhase, PresenceStatus};
