//! Pure domain: identity, peers, messages and the presence state machine.

pub mod attachment;
pub mod clock;
pub mod ids;
pub mod message;
pub mod nickname;
pub mod peer;
pub mod presence;

pub use attachment::{
    Attachment, AttachmentKind, AttachmentMeta, AttachmentState, FileName, Sha256,
};
pub use clock::{Clock, ManualClock, SystemClock, UnixMillis};
pub use ids::{AttachmentId, AvatarSeed, DeviceId, MessageId};
pub use message::{ChatMessage, Direction, MessageBody, MessagePreview, MessageStatus};
pub use nickname::Nickname;
pub use peer::{Handshake, PeerProfile, PeerView};
pub use presence::{PresenceMachine, PresencePhase, PresenceStatus};
