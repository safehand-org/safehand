//! The serializable Split Card schema.
//!
//! Two versions exist. A [`CardV1`] names every member's payout address in
//! the card itself. A [`CardV2`] lists members by public key only; each
//! member's payout address is in their own signature event (a `lud16` tag),
//! so the card does not change as members sign.

use serde::{Deserialize, Serialize};

/// A card of any supported version, parsed from JSON with [`Card::from_json`].
///
/// Serializes as the inner card, so a version 1 card's canonical JSON and
/// hash are exactly what they were before version 2 existed.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum Card {
    /// A version 1 card, or any version other than 2 (rejected by validation).
    V1(CardV1),
    /// A version 2 card.
    V2(CardV2),
}

impl Card {
    /// Parses card JSON, choosing the schema from its `version` field.
    ///
    /// `"version": 2` parses as [`CardV2`]. Anything else parses as
    /// [`CardV1`] exactly as before version 2 existed, with the same errors,
    /// and an unsupported version is left for validation to reject.
    ///
    /// # Errors
    ///
    /// Returns the JSON error for the chosen schema.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        #[derive(Deserialize)]
        struct Version {
            version: u32,
        }
        match serde_json::from_str::<Version>(json) {
            Ok(Version { version: 2 }) => serde_json::from_str(json).map(Self::V2),
            _ => serde_json::from_str(json).map(Self::V1),
        }
    }

    /// The `version` field.
    #[must_use]
    pub fn version(&self) -> u32 {
        match self {
            Self::V1(card) => card.version,
            Self::V2(card) => card.version,
        }
    }

    /// The `card_id` field.
    #[must_use]
    pub fn card_id(&self) -> &str {
        match self {
            Self::V1(card) => &card.card_id,
            Self::V2(card) => &card.card_id,
        }
    }

    /// The event name.
    #[must_use]
    pub fn event(&self) -> &str {
        match self {
            Self::V1(card) => &card.event,
            Self::V2(card) => &card.event,
        }
    }

    /// The team name.
    #[must_use]
    pub fn team(&self) -> &str {
        match self {
            Self::V1(card) => &card.team,
            Self::V2(card) => &card.team,
        }
    }

    /// The hash of the card this one supersedes, if any.
    #[must_use]
    pub fn supersedes(&self) -> Option<&str> {
        match self {
            Self::V1(card) => card.supersedes.as_deref(),
            Self::V2(card) => card.supersedes.as_deref(),
        }
    }

    /// The number of members.
    #[must_use]
    pub fn member_count(&self) -> usize {
        match self {
            Self::V1(card) => card.members.len(),
            Self::V2(card) => card.members.len(),
        }
    }

    /// Member public keys in card order.
    #[must_use]
    pub fn pubkeys(&self) -> Vec<&str> {
        match self {
            Self::V1(card) => card.members.iter().map(|m| m.pubkey.as_str()).collect(),
            Self::V2(card) => card.members.iter().map(|m| m.pubkey.as_str()).collect(),
        }
    }

    /// Member weights in card order.
    #[must_use]
    pub fn weights(&self) -> Vec<u64> {
        match self {
            Self::V1(card) => card.members.iter().map(|m| m.weight).collect(),
            Self::V2(card) => card.members.iter().map(|m| m.weight).collect(),
        }
    }

    /// Whether `pubkey` is one of the card's members.
    #[must_use]
    pub fn is_member(&self, pubkey: &str) -> bool {
        self.pubkeys().contains(&pubkey)
    }

    /// The name of the member with `pubkey`: see [`Card::member_names`].
    #[must_use]
    pub fn member_name(&self, pubkey: &str) -> Option<&str> {
        let index = self.pubkeys().iter().position(|key| *key == pubkey)?;
        self.member_names().into_iter().nth(index).flatten()
    }

    /// Each member's name in card order: the `name` of a version 1 member,
    /// or the optional `label` of a version 2 member.
    #[must_use]
    pub fn member_names(&self) -> Vec<Option<&str>> {
        match self {
            Self::V1(card) => card.members.iter().map(|m| Some(m.name.as_str())).collect(),
            Self::V2(card) => card.members.iter().map(|m| m.label.as_deref()).collect(),
        }
    }
}

impl From<CardV1> for Card {
    fn from(card: CardV1) -> Self {
        Self::V1(card)
    }
}

impl From<CardV2> for Card {
    fn from(card: CardV2) -> Self {
        Self::V2(card)
    }
}

pub use v1::Card as CardV1;

mod v1 {
    use serde::{Deserialize, Serialize};

    use super::{Member, Rules};

    /// A version 1 card: an agreement naming each member's payout address.
    ///
    /// Exported as [`CardV1`](super::CardV1). The type itself is named
    /// `Card` because serde puts the name in parse errors, which must read
    /// exactly as they did before version 2.
    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    #[serde(deny_unknown_fields, rename_all = "snake_case")]
    pub struct Card {
        /// Schema version; `1`.
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

/// A version 2 card: members by public key, share and leave rule. Payout
/// addresses are not in the card; each member signs theirs.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct CardV2 {
    /// Schema version; `2`.
    pub version: u32,
    /// Stable identifier for this agreement.
    pub card_id: String,
    /// Hackathon or event name.
    pub event: String,
    /// Team name.
    pub team: String,
    /// Team members in tie-breaking order.
    pub members: Vec<MemberV2>,
    /// Rules attached to this agreement.
    pub rules: RulesV2,
    /// Lowercase hexadecimal SHA-256 hash of the card this card supersedes.
    pub supersedes: Option<String>,
}

/// A version 2 member: who, and what share.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct MemberV2 {
    /// Nostr x-only public key, encoded as lowercase hexadecimal.
    pub pubkey: String,
    /// Positive integer share weight.
    pub weight: u64,
    /// Optional display name typed by the card's creator: at most
    /// [`MAX_LABEL_CHARS`](crate::validate::MAX_LABEL_CHARS) characters, no
    /// control characters. Left out of the JSON when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Rules attached to a version 2 card.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub struct RulesV2 {
    /// Optional peer-review rule data.
    pub peer_review: Option<PeerReview>,
    /// What a member who leaves before the team submits keeps.
    pub leave: LeaveRule,
}

/// What a member who leaves before the team submits keeps.
///
/// Recorded in the signed card only: nothing enforces it yet.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LeaveRule {
    /// They leave with nothing. A struct variant, so extra fields are
    /// rejected; unit variants of an internally tagged enum ignore them.
    Nothing {},
    /// They keep a fixed percentage of the prize.
    KeepPercent {
        /// Whole percent kept, 1 to 99.
        keep_percent: u32,
    },
}
