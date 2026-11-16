/// A qualifying LONG or SHORT contributes one fixed directional vote.
/// Keep this separate from the model's weighted setup-strength score.
pub const SIGNAL_DIRECTION_SCORE_POINTS: i64 = 50;

/// Returns the fixed signed direction vote for a valid signal direction.
pub fn signed_direction_score(direction: &str) -> Option<i64> {
    if direction.eq_ignore_ascii_case("LONG") {
        Some(SIGNAL_DIRECTION_SCORE_POINTS)
    } else if direction.eq_ignore_ascii_case("SHORT") {
        Some(-SIGNAL_DIRECTION_SCORE_POINTS)
    } else {
        None
    }
}
