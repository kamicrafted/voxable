//! Comparing release versions.
//!
//! Only what a self-published app needs: dotted numeric versions, optionally with a
//! leading `v` from a git tag and optionally with a pre-release suffix. Anything it
//! cannot parse is treated as "not newer", so a malformed tag on the release page can
//! never prompt someone to downgrade.

/// A version as compared here: numeric parts plus whether it carries a pre-release
/// suffix (`0.5.0-beta.1`), which sorts *before* the same numbers without one.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Version {
    parts: Vec<u64>,
    prerelease: Option<String>,
}

fn parse(raw: &str) -> Option<Version> {
    let s = raw.trim().trim_start_matches(['v', 'V']);
    if s.is_empty() {
        return None;
    }
    // Split the pre-release suffix off first: 0.5.0-beta.1 → "0.5.0" + "beta.1".
    let (nums, prerelease) = match s.split_once('-') {
        Some((n, pre)) if !pre.is_empty() => (n, Some(pre.to_string())),
        Some(_) => return None,
        None => (s, None),
    };
    let parts: Vec<u64> = nums
        .split('.')
        .map(|p| p.parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;
    if parts.is_empty() {
        return None;
    }
    Some(Version { parts, prerelease })
}

/// Compare, padding the shorter version with zeros so `0.4` and `0.4.0` are equal.
fn cmp(a: &Version, b: &Version) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let len = a.parts.len().max(b.parts.len());
    for i in 0..len {
        let (x, y) = (
            a.parts.get(i).copied().unwrap_or(0),
            b.parts.get(i).copied().unwrap_or(0),
        );
        match x.cmp(&y) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
    // Same numbers: a release outranks a pre-release of itself.
    match (&a.prerelease, &b.prerelease) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => x.cmp(y),
    }
}

/// Is `remote` a newer version than `local`?
///
/// False when either side is unparseable, when they are equal, or when `remote` is
/// older — so the only way to prompt an update is a version that definitely leads.
pub fn is_newer(local: &str, remote: &str) -> bool {
    match (parse(local), parse(remote)) {
        (Some(l), Some(r)) => cmp(&r, &l) == std::cmp::Ordering::Greater,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_higher_patch_minor_or_major_is_newer() {
        assert!(is_newer("0.4.0", "0.4.1"));
        assert!(is_newer("0.4.0", "0.5.0"));
        assert!(is_newer("0.4.0", "1.0.0"));
    }

    #[test]
    fn the_same_version_is_not_newer() {
        assert!(!is_newer("0.4.0", "0.4.0"));
    }

    #[test]
    fn an_older_version_is_not_newer() {
        assert!(!is_newer("0.4.0", "0.3.9"));
        assert!(!is_newer("1.0.0", "0.9.9"));
    }

    #[test]
    fn a_leading_v_from_a_git_tag_is_ignored() {
        assert!(is_newer("0.4.0", "v0.5.0"));
        assert!(!is_newer("v0.5.0", "0.4.0"));
        assert!(!is_newer("v0.4.0", "v0.4.0"));
    }

    #[test]
    fn double_digit_parts_compare_numerically_not_alphabetically() {
        // The bug in every naive string compare: "0.10.0" < "0.9.0" as text.
        assert!(is_newer("0.9.0", "0.10.0"));
        assert!(!is_newer("0.10.0", "0.9.0"));
    }

    #[test]
    fn missing_parts_are_treated_as_zero() {
        assert!(!is_newer("0.4.0", "0.4"));
        assert!(!is_newer("0.4", "0.4.0"));
        assert!(is_newer("0.4", "0.4.1"));
    }

    #[test]
    fn a_prerelease_is_older_than_its_release() {
        assert!(is_newer("0.5.0-beta.1", "0.5.0"));
        assert!(!is_newer("0.5.0", "0.5.0-beta.1"));
        assert!(is_newer("0.4.0", "0.5.0-beta.1"));
    }

    #[test]
    fn anything_unparseable_is_never_newer() {
        // A malformed tag must not be able to prompt a downgrade.
        assert!(!is_newer("0.4.0", "latest"));
        assert!(!is_newer("0.4.0", ""));
        assert!(!is_newer("0.4.0", "v"));
        assert!(!is_newer("0.4.0", "0.4.x"));
        assert!(!is_newer("not-a-version", "0.5.0"));
    }
}
