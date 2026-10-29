//! The serializable Split Card schema.

use serde::{Deserialize, Serialize};

/// A signed agreement describing how a team's prize will be split.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct Card {
    /// Schema version; this phase supports version `1`.
    pub version: u32,
    /// Stable identifier for this agreement.
    pub card_id: String,
    /// Hackathon or event name.
    pub event: String,
    /// Team name.
    pub team: String,
    /// Team members in tie-breaking order.
    pub members: Vec<Member>,
    /// Rules attached to this agreement.
    pub rules: Rules,
    /// Lowercase hexadecimal SHA-256 hash of the card this card supersedes.
    pub supersedes: Option<String>,
}

/// A person entitled to a portion of a prize.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct Member {
    /// Nostr x-only public key, encoded as lowercase hexadecimal.
    pub pubkey: String,
    /// Display name.
    pub name: String,
    /// Positive integer share weight.
    pub weight: u64,
    /// Destination for this member's payment.
    pub payout: Payout,
}

/// A payment destination.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum Payout {
    /// A Lightning Address payment destination.
    Lightning {
        /// Lightning Address in `local@domain.tld` form.
        address: String,
    },
}

/// Optional rules governing a card.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct Rules {
    /// Optional peer-review rule data.
    pub peer_review: Option<PeerReview>,
}

/// Parameters for a future peer-review process.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct PeerReview {
    /// Maximum permitted shift, expressed in points.
    pub max_shift_points: u32,
}
