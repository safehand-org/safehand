//! Team slugs: unique across the server, and later each team's Lightning
//! Address username, so they follow LUD-16's username rules.

use std::collections::BTreeSet;

use splitcard_core::validate::is_lightning_address;

/// Shortest allowed slug.
pub const MIN_SLUG_LEN: usize = 3;
/// Longest allowed slug.
pub const MAX_SLUG_LEN: usize = 32;

/// Slugs no team may take, since they name the server's own addresses and
/// paths.
pub const RESERVED_SLUGS: [&str; 20] = [
    "admin",
    "administrator",
    "api",
    "www",
    "root",
    "system",
    "support",
    "help",
    "info",
    "mail",
    "postmaster",
    "abuse",
    "security",
    "lnurl",
    "lnurlp",
    "nostr",
    "splitcard",
    "server",
    "teams",
    "profiles",
];

/// Why a slug is not allowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlugError {
    /// Shorter than [`MIN_SLUG_LEN`] or longer than [`MAX_SLUG_LEN`].
    Length,
    /// Contains an uppercase letter; slugs are lowercase.
    Uppercase,
    /// Contains a character a LUD-16 username cannot.
    Characters,
    /// One of [`RESERVED_SLUGS`].
    Reserved,
}

impl SlugError {
    /// A short machine-readable reason.
    #[must_use]
    pub fn reason(self) -> &'static str {
        match self {
            Self::Length => "length",
            Self::Uppercase => "uppercase",
            Self::Characters => "characters",
            Self::Reserved => "reserved",
        }
    }
}

/// Checks `slug` against the slug rules: 3-32 characters, lowercase, a valid
/// LUD-16 username as `splitcard-core` checks it, and not reserved.
///
/// # Errors
///
/// The first rule `slug` breaks.
pub fn check_slug(slug: &str) -> Result<(), SlugError> {
    if !(MIN_SLUG_LEN..=MAX_SLUG_LEN).contains(&slug.chars().count()) {
        return Err(SlugError::Length);
    }
    if slug.chars().any(char::is_uppercase) {
        return Err(SlugError::Uppercase);
    }
    // The username part of a Lightning Address, checked by the same code as
    // the addresses on cards.
    if !is_lightning_address(&format!("{slug}@example.com")) {
        return Err(SlugError::Characters);
    }
    if RESERVED_SLUGS.contains(&slug) {
        return Err(SlugError::Reserved);
    }
    Ok(())
}

/// Derives a slug for each `(id, name)` team, in order: the name lowercased,
/// with runs of anything but `a-z` and `0-9` replaced by `-`, cut to length,
/// and suffixed `-team` if too short or reserved, then `-2`, `-3`, ... until
/// it is unused by an earlier team.
#[must_use]
pub fn derive_slugs(teams: &[(i64, String)]) -> Vec<(i64, String)> {
    let mut taken = BTreeSet::new();
    teams
        .iter()
        .map(|(id, name)| {
            let base = base_slug(name);
            let mut slug = base.clone();
            let mut number = 2_u32;
            while taken.contains(&slug) {
                let suffix = format!("-{number}");
                let keep = MAX_SLUG_LEN.saturating_sub(suffix.len()).min(base.len());
                slug = format!("{}{suffix}", base[..keep].trim_end_matches('-'));
                number = number.saturating_add(1);
            }
            taken.insert(slug.clone());
            (*id, slug)
        })
        .collect()
}

fn base_slug(name: &str) -> String {
    let mut slug = String::new();
    for character in name.to_lowercase().chars() {
        if character.is_ascii_lowercase() || character.is_ascii_digit() {
            slug.push(character);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    // Only ASCII is left, so byte lengths are character counts.
    slug.truncate(MAX_SLUG_LEN);
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "team".to_owned()
    } else if slug.len() < MIN_SLUG_LEN || RESERVED_SLUGS.contains(&slug) {
        format!("{slug}-team")
    } else {
        slug.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_SLUG_LEN, SlugError, check_slug, derive_slugs};

    fn derive(names: &[&str]) -> Vec<String> {
        let teams: Vec<(i64, String)> = names
            .iter()
            .zip(1..)
            .map(|(name, id)| (id, (*name).to_owned()))
            .collect();
        derive_slugs(&teams)
            .into_iter()
            .map(|(_, slug)| slug)
            .collect()
    }

    #[test]
    fn slugs_follow_the_rules() {
        for slug in ["llamas", "abc", "team_1.x+y", &"a".repeat(MAX_SLUG_LEN)] {
            assert_eq!(check_slug(slug), Ok(()), "{slug}");
        }
        assert_eq!(check_slug("ab"), Err(SlugError::Length));
        assert_eq!(
            check_slug(&"a".repeat(MAX_SLUG_LEN + 1)),
            Err(SlugError::Length)
        );
        assert_eq!(check_slug("Llamas"), Err(SlugError::Uppercase));
        for slug in ["lla mas", "llamas!", "lláma", "a@b", "a/b"] {
            assert_eq!(check_slug(slug), Err(SlugError::Characters), "{slug}");
        }
        for slug in ["admin", "api", "www"] {
            assert_eq!(check_slug(slug), Err(SlugError::Reserved), "{slug}");
        }
    }

    #[test]
    fn derived_slugs_are_valid_and_unique() {
        let names = [
            "Lightning Llamas",
            "Lightning  Llamas!",
            "lightning-llamas",
            "Ñandú",
            "AB",
            "!!!",
            "API",
            "A very long team name that goes past the limit",
            "A very long team name that goes past the limit",
        ];
        let slugs = derive(&names);
        assert_eq!(
            slugs,
            [
                "lightning-llamas",
                "lightning-llamas-2",
                "lightning-llamas-3",
                "and",
                "ab-team",
                "team",
                "api-team",
                "a-very-long-team-name-that-goes",
                "a-very-long-team-name-that-goe-2",
            ]
        );
        for slug in &slugs {
            assert_eq!(check_slug(slug), Ok(()), "{slug}");
        }
    }
}
