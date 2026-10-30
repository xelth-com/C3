//! The retention / clearing / precedence policy of recorded endpoint health, in one place.
//!
//! [`crate::health`] and [`crate::quota`] record failures and usable replies and later judge
//! whether a recorded mark still keeps an endpoint out. The windows (how long a mark lives), what
//! clears a mark (a later usable reply, the mark's own reset time, a clearing anchor) and the order
//! marks are checked in used to live as inline constants and prose comments spread through those two
//! files (waves 9a-9i: cap evidence, clearing anchors, retention tiers). This module owns them as
//! data ([`RULES`], [`rule`]) plus small pure predicates that `health.rs` / `quota.rs` call instead
//! of their inline constants/conditions. The byte-for-byte behaviour is unchanged; this is the
//! documented source of truth for the rules.
//!
//! # The marks and their retention
//!
//! | kind | retention | cleared by | precedence |
//! |------|-----------|------------|------------|
//! | [`MarkKind::Auth`] | 24 h ([`FAILURE_RETENTION_MINUTES`]) from the failure | a later usable reply | 1 |
//! | [`MarkKind::QuotaKnownReset`] | until the failure's own reset (`retry_after`) | reset passing, or a later usable reply | 2 |
//! | [`MarkKind::QuotaWindowBlock`] | until the window's `resets_at` | the window dropping below [`QUOTA_BLOCK_PERCENT`] or its reset passing | 2 |
//! | [`MarkKind::QuotaResetless`] | [`QUOTA_OUT_MINUTES`] (60) from the hit | the out-window passing, or a later usable reply | 3 |
//! | [`MarkKind::Burst`] | [`BURST_OUT_MINUTES`] (10) from the hit | the out-window passing, or a later usable reply | 3 |
//! | [`MarkKind::ModelUnsupported`] | 24 h ([`MODEL_UNSUPPORTED_RETENTION_MINUTES`]) from the failure | a later usable reply **from the same model** | 4 |
//! | [`MarkKind::CapEvidence`] | 35 days ([`CAP_PAIR_RETENTION_MINUTES`]) from the hit | a clearing success (usable reply) no older than the evidence | 5 |
//! | [`MarkKind::ClearingAnchor`] | 35 days ([`CAP_PAIR_RETENTION_MINUTES`]) from the reply | nothing - it IS the clearing proof, evicted only by the cap | 5 |
//!
//! Precedence is the order [`crate::verdict`] checks the direct marks (credentials first, then auth,
//! then a quota with a known reset, then a reset-less quota, then the per-model mark); cap evidence
//! and the clearing anchor feed the plan-cap path ([`crate::plan::plan_cap_verdict`]) rather than a
//! verdict branch of their own, so they share the last rank.
//!
//! A reset-less quota failure's out-window is 60 minutes, or 10 for a burst 429 (a 429 /
//! too-many-requests / concurrency condition that names no usage window) - [`out_minutes`]. An
//! INSTANT 429 ([`is_instant_429`]) - one that came back in under [`INSTANT_429_SECS`] seconds and
//! carries a consult id - is the cap-pair evidence the monthly/weekly/daily cap detector counts.
//!
//! The newest record wins: `health.rs` sorts records newest-first, so a usable reply that is newer
//! than a failure is seen first and clears it. A usable reply is itself kept as `recent_usable` for
//! [`RECENT_USABLE_MINUTES`] minutes. On the machine-wide health file a plain row is pruned once it
//! is older than [`MACHINE_RECENT_HOURS`] hours ([`machine_row_recent`]) unless it is still out, cap
//! evidence within retention, or a clearing anchor within retention.

use chrono::{DateTime, Duration, Utc};

// --------------------------------------------------------------------------- the policy constants

/// Minutes an endpoint stays out after a quota failure that named no reset time: a burst 429
/// recovers fast (`$script:BurstOutMinutes`, wave 24c); a real usage window keeps
/// [`QUOTA_OUT_MINUTES`].
pub const BURST_OUT_MINUTES: i64 = 10;
/// Minutes a reset-less usage-limit failure keeps the endpoint out (`$script:QuotaOutMinutes`).
pub const QUOTA_OUT_MINUTES: i64 = 60;
/// (wave 9, D4) A 429 that came back in less than this many seconds is an INSTANT 429 - the
/// evidence the plan-cap detector counts.
pub const INSTANT_429_SECS: f64 = 2.0;
/// (wave 9e, F76-1) Id-carrying cap-pair hits and each endpoint's clearing anchor are retained up to
/// this bound (35 days, the longest monthly cap period). The per-entry period rule - a hinted
/// entry's hit counts until its declared reset, an unhinted entry's only while it is younger than
/// 24 h - is applied at read time in [`crate::plan::plan_cap_verdict`], where the roster entry (hence
/// the hint) is known.
pub const CAP_PAIR_RETENTION_MINUTES: i64 = 35 * 24 * 60;
/// How long an auth failure, a last usage limit and a last failure stay live (24 h). The same window
/// backs the machine record's auth `until` (24 h) and the per-model mark below.
pub const FAILURE_RETENTION_MINUTES: i64 = 24 * 60;
/// (C3 model-health) How long a model-unsupported mark keeps ONE roster entry (provider + model)
/// out: the same 24 h as the per-provider auth/unavailable mark. The mark has no reset time of its
/// own - an account-tier model restriction is persistent, not a window - so the newest outcome of
/// the model decides: a later usable reply from that model clears it (see
/// [`crate::health::endpoint_health_set`]), otherwise it holds for a day and the entry is re-probed.
pub const MODEL_UNSUPPORTED_RETENTION_MINUTES: i64 = 24 * 60;
/// A quota window at or above this share whose reset lies ahead keeps its endpoint out until that
/// reset (`crate::health::endpoint_health_set`, like a usage limit with a known reset).
pub const QUOTA_BLOCK_PERCENT: f64 = 98.0;
/// A quota window at or above this share (not reset) adds `low quota: ...` to an available verdict.
pub const QUOTA_WARN_PERCENT: f64 = 80.0;
/// A usable reply is kept as `recent_usable` while it is younger than this many minutes.
pub const RECENT_USABLE_MINUTES: f64 = 60.0;
/// A machine-wide health row is "recent" (kept by the prune on its own) while it is this many hours
/// old or younger. Integer hours, to match the plugin's `num_hours() <= 24`.
pub const MACHINE_RECENT_HOURS: i64 = 24;

// --------------------------------------------------------------------------- the rule table

/// A recorded mark that can keep an endpoint out.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MarkKind {
    /// A per-provider auth failure.
    Auth,
    /// A quota failure that named its own reset time (`retry_after`).
    QuotaKnownReset,
    /// A quota WINDOW observed at [`QUOTA_BLOCK_PERCENT`] or more with its reset ahead.
    QuotaWindowBlock,
    /// A reset-less usage-limit failure.
    QuotaResetless,
    /// A reset-less burst 429 (names no usage window).
    Burst,
    /// A per-model "not supported on this login" failure.
    ModelUnsupported,
    /// An id-carrying instant-429 cap-pair hit (plan-cap evidence).
    CapEvidence,
    /// A clearing success (usable reply) that supersedes older cap evidence.
    ClearingAnchor,
}

/// How long a mark lives.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Retention {
    /// Live while it is this many minutes old or younger (counted from when it was recorded).
    Minutes(i64),
    /// Live for a fixed out-window of this many minutes counted from the hit.
    OutWindow(i64),
    /// Live until the mark's own reset time (`retry_after` / `resets_at`).
    UntilReset,
}

/// What clears a mark.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClearedBy {
    /// A later usable reply on the same endpoint.
    UsableReply,
    /// A later usable reply from the SAME model.
    UsableReplyOfModel,
    /// The window dropping below the block share, or its reset passing (state, not a later reply).
    WindowResetOrDrop,
    /// A clearing success (usable reply) no older than the evidence.
    ClearingSuccess,
    /// Nothing - the mark is itself the clearing proof (evicted only by the cap).
    Nothing,
}

/// One row of the retention policy.
#[derive(Clone, Copy, Debug)]
pub struct RetentionRule {
    pub kind: MarkKind,
    pub retention: Retention,
    pub cleared_by: ClearedBy,
    /// Verdict check order (lower first); the cap-pair inputs share the last rank.
    pub precedence: u8,
}

/// The whole policy, one row per mark kind.
pub const RULES: [RetentionRule; 8] = [
    RetentionRule {
        kind: MarkKind::Auth,
        retention: Retention::Minutes(FAILURE_RETENTION_MINUTES),
        cleared_by: ClearedBy::UsableReply,
        precedence: 1,
    },
    RetentionRule {
        kind: MarkKind::QuotaKnownReset,
        retention: Retention::UntilReset,
        cleared_by: ClearedBy::UsableReply,
        precedence: 2,
    },
    RetentionRule {
        kind: MarkKind::QuotaWindowBlock,
        retention: Retention::UntilReset,
        cleared_by: ClearedBy::WindowResetOrDrop,
        precedence: 2,
    },
    RetentionRule {
        kind: MarkKind::QuotaResetless,
        retention: Retention::OutWindow(QUOTA_OUT_MINUTES),
        cleared_by: ClearedBy::UsableReply,
        precedence: 3,
    },
    RetentionRule {
        kind: MarkKind::Burst,
        retention: Retention::OutWindow(BURST_OUT_MINUTES),
        cleared_by: ClearedBy::UsableReply,
        precedence: 3,
    },
    RetentionRule {
        kind: MarkKind::ModelUnsupported,
        retention: Retention::Minutes(MODEL_UNSUPPORTED_RETENTION_MINUTES),
        cleared_by: ClearedBy::UsableReplyOfModel,
        precedence: 4,
    },
    RetentionRule {
        kind: MarkKind::CapEvidence,
        retention: Retention::Minutes(CAP_PAIR_RETENTION_MINUTES),
        cleared_by: ClearedBy::ClearingSuccess,
        precedence: 5,
    },
    RetentionRule {
        kind: MarkKind::ClearingAnchor,
        retention: Retention::Minutes(CAP_PAIR_RETENTION_MINUTES),
        cleared_by: ClearedBy::Nothing,
        precedence: 5,
    },
];

/// The policy row of a mark kind.
pub fn rule(kind: MarkKind) -> &'static RetentionRule {
    // RULES is ordered to mirror MarkKind; find by kind so a reorder cannot desync the two.
    RULES
        .iter()
        .find(|r| r.kind == kind)
        .expect("every MarkKind has a RULES row")
}

// --------------------------------------------------------------------------- the pure predicates

/// Minutes a reset-less quota failure keeps the endpoint out: [`BURST_OUT_MINUTES`] for a burst 429,
/// else [`QUOTA_OUT_MINUTES`].
pub fn out_minutes(is_burst: bool) -> i64 {
    if is_burst {
        BURST_OUT_MINUTES
    } else {
        QUOTA_OUT_MINUTES
    }
}

/// (wave 9, D4) Did this run come back fast enough to be an INSTANT 429?
pub fn is_instant_429(wall_secs: f64) -> bool {
    wall_secs < INSTANT_429_SECS
}

/// Is a failure still live under the 24 h failure window? (`age_minutes` is floored minutes.)
pub fn within_failure_retention(age_minutes: i64) -> bool {
    age_minutes <= FAILURE_RETENTION_MINUTES
}

/// Is a per-model mark still live under its 24 h window?
pub fn model_unsupported_live(age_minutes: i64) -> bool {
    age_minutes <= MODEL_UNSUPPORTED_RETENTION_MINUTES
}

/// Is a cap-pair hit still within the 35-day retention? (`age_minutes` is floored minutes.)
pub fn within_cap_retention(age_minutes: i64) -> bool {
    age_minutes <= CAP_PAIR_RETENTION_MINUTES
}

/// Is a row still within the 35-day cap-pair retention, measured as an age [`Duration`]?
pub fn within_cap_retention_dur(age: Duration) -> bool {
    age.num_minutes() <= CAP_PAIR_RETENTION_MINUTES
}

/// Is a usable reply recent enough to count as `recent_usable`? (`age` in minutes.)
pub fn is_recent_usable(age: f64) -> bool {
    age <= RECENT_USABLE_MINUTES
}

/// Does a quota window at `used_percent` block its endpoint (share only - the caller checks reset)?
pub fn window_blocks(used_percent: f64) -> bool {
    used_percent >= QUOTA_BLOCK_PERCENT
}

/// Is a quota window at `used_percent` low enough to warn about (share only)?
pub fn window_low(used_percent: f64) -> bool {
    used_percent >= QUOTA_WARN_PERCENT
}

/// Is a machine-wide health row "recent" (kept by the prune on its own)? Integer hours, to match the
/// plugin's `num_hours() <= 24`.
pub fn machine_row_recent(age: Duration) -> bool {
    age.num_hours() <= MACHINE_RECENT_HOURS
}

// --------------------------------------------------------------------------- the policy functions

/// Is a mark of `kind`, recorded at `recorded_at`, still live at `now`? `reset` is the mark's own
/// reset time (`retry_after` / `resets_at`) when it declared one. This mirrors how `health.rs` and
/// `quota.rs` judge each window:
/// - [`Retention::Minutes`]: live while its floored age is at most the window.
/// - [`Retention::OutWindow`]: live while `now` is before `recorded_at + window`.
/// - [`Retention::UntilReset`]: live while the reset is still ahead of `now`.
pub fn is_live(
    kind: MarkKind,
    recorded_at: DateTime<Utc>,
    now: DateTime<Utc>,
    reset: Option<DateTime<Utc>>,
) -> bool {
    match rule(kind).retention {
        Retention::Minutes(m) => (now - recorded_at).num_minutes() <= m,
        Retention::OutWindow(m) => now < recorded_at + Duration::minutes(m),
        Retention::UntilReset => reset.is_some_and(|r| r > now),
    }
}

/// A later event on the same endpoint, as [`clears`] reads it.
#[derive(Clone, Copy, Debug, Default)]
pub struct LaterEvent {
    /// A usable reply occurred later on the same endpoint.
    pub usable_reply: bool,
    /// That usable reply was from the SAME model as the mark.
    pub same_model: bool,
    /// That usable reply is no older than the mark (its order is at least the mark's).
    pub no_older_than_mark: bool,
}

/// Does `later` clear a mark of `kind`? A window block and the clearing anchor are not cleared by a
/// later reply (a window clears by its own state, an anchor is itself the proof), so they return
/// `false` here.
pub fn clears(kind: MarkKind, later: &LaterEvent) -> bool {
    match rule(kind).cleared_by {
        ClearedBy::UsableReply => later.usable_reply,
        ClearedBy::UsableReplyOfModel => later.usable_reply && later.same_model,
        ClearedBy::ClearingSuccess => later.usable_reply && later.no_older_than_mark,
        ClearedBy::WindowResetOrDrop | ClearedBy::Nothing => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t(min: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_700_000_000, 0).unwrap() + Duration::minutes(min)
    }
    fn at_secs(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_700_000_000, 0).unwrap() + Duration::seconds(secs)
    }

    #[test]
    fn every_mark_kind_has_exactly_one_rule() {
        for k in [
            MarkKind::Auth,
            MarkKind::QuotaKnownReset,
            MarkKind::QuotaWindowBlock,
            MarkKind::QuotaResetless,
            MarkKind::Burst,
            MarkKind::ModelUnsupported,
            MarkKind::CapEvidence,
            MarkKind::ClearingAnchor,
        ] {
            assert_eq!(
                RULES.iter().filter(|r| r.kind == k).count(),
                1,
                "{k:?} must have exactly one rule row"
            );
        }
    }

    #[test]
    fn precedence_follows_the_verdict_order() {
        assert!(rule(MarkKind::Auth).precedence < rule(MarkKind::QuotaKnownReset).precedence);
        assert!(
            rule(MarkKind::QuotaKnownReset).precedence < rule(MarkKind::QuotaResetless).precedence
        );
        assert!(
            rule(MarkKind::QuotaResetless).precedence < rule(MarkKind::ModelUnsupported).precedence
        );
        assert!(
            rule(MarkKind::ModelUnsupported).precedence < rule(MarkKind::CapEvidence).precedence
        );
        assert_eq!(
            rule(MarkKind::QuotaKnownReset).precedence,
            rule(MarkKind::QuotaWindowBlock).precedence
        );
        assert_eq!(
            rule(MarkKind::QuotaResetless).precedence,
            rule(MarkKind::Burst).precedence
        );
    }

    // is_live, one row per rule, with the TTL boundary and one minute either side.
    #[test]
    fn is_live_minutes_boundary_auth() {
        let rec = t(0);
        // 24 h = 1440 min: at the TTL it is still live; a minute past it is not.
        assert!(is_live(MarkKind::Auth, rec, t(1439), None));
        assert!(is_live(MarkKind::Auth, rec, t(1440), None));
        assert!(!is_live(MarkKind::Auth, rec, t(1441), None));
    }

    #[test]
    fn is_live_minutes_boundary_model_unsupported() {
        let rec = t(0);
        assert!(is_live(MarkKind::ModelUnsupported, rec, t(1440), None));
        assert!(!is_live(MarkKind::ModelUnsupported, rec, t(1441), None));
    }

    #[test]
    fn is_live_minutes_boundary_cap_evidence_and_anchor() {
        let rec = t(0);
        let ttl = CAP_PAIR_RETENTION_MINUTES; // 35 days
        for k in [MarkKind::CapEvidence, MarkKind::ClearingAnchor] {
            assert!(is_live(k, rec, t(ttl - 1), None));
            assert!(is_live(k, rec, t(ttl), None));
            assert!(!is_live(k, rec, t(ttl + 1), None));
        }
    }

    #[test]
    fn is_live_out_window_resetless_quota() {
        let hit = at_secs(0);
        // 60 min out-window, boundary exclusive: live up to the last second before it.
        assert!(is_live(
            MarkKind::QuotaResetless,
            hit,
            at_secs(59 * 60),
            None
        ));
        assert!(is_live(
            MarkKind::QuotaResetless,
            hit,
            at_secs(60 * 60 - 1),
            None
        ));
        assert!(!is_live(
            MarkKind::QuotaResetless,
            hit,
            at_secs(60 * 60),
            None
        ));
    }

    #[test]
    fn is_live_out_window_burst() {
        let hit = at_secs(0);
        // 10 min out-window.
        assert!(is_live(MarkKind::Burst, hit, at_secs(10 * 60 - 1), None));
        assert!(!is_live(MarkKind::Burst, hit, at_secs(10 * 60), None));
    }

    #[test]
    fn is_live_until_reset() {
        let rec = t(0);
        let now = t(1000);
        // No reset -> not live; reset ahead -> live; reset at/behind now -> not live.
        assert!(!is_live(MarkKind::QuotaKnownReset, rec, now, None));
        assert!(is_live(
            MarkKind::QuotaKnownReset,
            rec,
            now,
            Some(now + Duration::seconds(1))
        ));
        assert!(!is_live(MarkKind::QuotaKnownReset, rec, now, Some(now)));
        assert!(is_live(
            MarkKind::QuotaWindowBlock,
            rec,
            now,
            Some(now + Duration::seconds(1))
        ));
        assert!(!is_live(MarkKind::QuotaWindowBlock, rec, now, Some(now)));
    }

    // clears, one row per rule.
    #[test]
    fn clears_per_rule() {
        let reply = LaterEvent {
            usable_reply: true,
            same_model: false,
            no_older_than_mark: false,
        };
        let no_event = LaterEvent::default();

        // auth / known-reset / reset-less / burst: any later usable reply clears.
        for k in [
            MarkKind::Auth,
            MarkKind::QuotaKnownReset,
            MarkKind::QuotaResetless,
            MarkKind::Burst,
        ] {
            assert!(clears(k, &reply), "{k:?} should clear on a usable reply");
            assert!(
                !clears(k, &no_event),
                "{k:?} should not clear with no event"
            );
        }

        // model-unsupported needs the SAME model.
        assert!(!clears(MarkKind::ModelUnsupported, &reply));
        assert!(clears(
            MarkKind::ModelUnsupported,
            &LaterEvent {
                usable_reply: true,
                same_model: true,
                no_older_than_mark: false,
            }
        ));

        // cap evidence is superseded by a clearing success no older than it.
        assert!(!clears(MarkKind::CapEvidence, &reply));
        assert!(clears(
            MarkKind::CapEvidence,
            &LaterEvent {
                usable_reply: true,
                same_model: false,
                no_older_than_mark: true,
            }
        ));

        // a window block and the anchor are never cleared by a later reply.
        assert!(!clears(MarkKind::QuotaWindowBlock, &reply));
        assert!(!clears(MarkKind::ClearingAnchor, &reply));
    }

    #[test]
    fn out_minutes_and_instant_429_and_percent_helpers() {
        assert_eq!(out_minutes(true), BURST_OUT_MINUTES);
        assert_eq!(out_minutes(false), QUOTA_OUT_MINUTES);

        assert!(is_instant_429(1.99));
        assert!(!is_instant_429(INSTANT_429_SECS));
        assert!(!is_instant_429(2.01));

        assert!(window_blocks(98.0));
        assert!(!window_blocks(97.999));
        assert!(window_low(80.0));
        assert!(!window_low(79.999));
    }

    #[test]
    fn age_window_helpers_at_the_boundaries() {
        assert!(within_failure_retention(FAILURE_RETENTION_MINUTES));
        assert!(!within_failure_retention(FAILURE_RETENTION_MINUTES + 1));
        assert!(model_unsupported_live(MODEL_UNSUPPORTED_RETENTION_MINUTES));
        assert!(!model_unsupported_live(
            MODEL_UNSUPPORTED_RETENTION_MINUTES + 1
        ));
        assert!(within_cap_retention(CAP_PAIR_RETENTION_MINUTES));
        assert!(!within_cap_retention(CAP_PAIR_RETENTION_MINUTES + 1));
        assert!(within_cap_retention_dur(Duration::minutes(
            CAP_PAIR_RETENTION_MINUTES
        )));
        // num_minutes() floors, so an extra second is still within; a full minute past is not.
        assert!(within_cap_retention_dur(
            Duration::minutes(CAP_PAIR_RETENTION_MINUTES) + Duration::seconds(59)
        ));
        assert!(!within_cap_retention_dur(Duration::minutes(
            CAP_PAIR_RETENTION_MINUTES + 1
        )));

        assert!(is_recent_usable(60.0));
        assert!(!is_recent_usable(60.001));

        assert!(machine_row_recent(Duration::hours(24)));
        // integer hours: 24 h 59 m still counts, 25 h does not.
        assert!(machine_row_recent(
            Duration::hours(24) + Duration::minutes(59)
        ));
        assert!(!machine_row_recent(Duration::hours(25)));
    }
}
