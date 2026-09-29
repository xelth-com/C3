//! The v1 smoothed score and its parity with the plugin's R15 rate.
//!
//! R15 (`panel::routing::routing_rate`): `w = (yes + 0.5*partly + m*pi)/(n + m)` with
//! `m = 2`, `pi = 0.5`, `score = 0.25 + 1.75*w`. Router v1 keeps the same arithmetic but
//! lets `m` and `pi` come from a global prior. With `m = 2`, `pi = 0.5` and no local
//! evidence the neutral case returns `0.25 + 1.75*pi` (= `ROUTING_NEUTRAL`), and with local
//! evidence [`smoothed_score`] reduces to `routing_rate` operation for operation — the same
//! `f64` bits, which the property test in `tests/router.rs` pins over random rating sets.

/// `w = (yes + 0.5*partly + m*pi)/(n + m)`, scaled into `[0.25, 2]`. With no local evidence
/// (`n == 0`) the weight is the prior mean itself (`score = 0.25 + 1.75*pi`, M9 §2.3), which
/// also avoids the `(m*pi)/m != pi` rounding a general division would introduce.
pub fn smoothed_score(yes: f64, partly: f64, n: f64, pi: f64, m: f64) -> f64 {
    let w = if n == 0.0 {
        pi
    } else {
        (yes + 0.5 * partly + m * pi) / (n + m)
    };
    0.25 + 1.75 * w
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panel::routing::routing_rate;

    #[test]
    fn smoothed_score_is_bit_identical_to_r15_rate() {
        // A tiny xorshift PRNG so the parity check needs no `rand` dependency.
        let mut s: u64 = 0x9e3779b97f4a7c15;
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s >> 11) as f64 / (1u64 << 53) as f64
        };
        for _ in 0..5000 {
            let n = (next() * 40.0).floor();
            let yes = (next() * (n + 1.0)).floor();
            let partly = (next() * (n - yes + 1.0)).floor();
            // v1 with the defaults: pi = 0.5, m = 2.
            let v1 = smoothed_score(yes, partly, n, 0.5, 2.0);
            let r15 = routing_rate(yes, partly, n);
            assert_eq!(
                v1.to_bits(),
                r15.to_bits(),
                "yes={yes} partly={partly} n={n}: v1={v1} r15={r15}"
            );
        }
    }
}
