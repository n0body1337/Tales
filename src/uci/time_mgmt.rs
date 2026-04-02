// ============================================================================
// Tales - UCI chess engine written in Rust
// Copyright (C) 2025-2026 Andre MARTINS
//
// Tales is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// Tales is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
// details.
//
// You should have received a copy of the GNU General Public License along
// with this program. If not, see <https://www.gnu.org/licenses/>.
// ============================================================================

//! Time management — translates UCI clock/increment parameters into a move time budget.

/// Compute time for one move from base clock, increment, and moves-to-go.
///
/// Applies SlowMover scaling, overhead subtraction, and bullet correction.
pub fn compute_move_time(
    mut base: i64,
    inc: i64,
    movestogo: i64,
    overhead: i64,
    time_percentage: i32,
) -> u64 {
    if base < 0 {
        return 5000;
    } // fallback

    let mtg = movestogo.max(1);

    // Time control safety: deduct safety margin on last move of time control
    if mtg == 1 {
        base -= 1000_i64.min(base / 10);
    }

    let mut time = (base + inc * (mtg - 1)) / mtg;

    // Apply SlowMover percentage (only when safe)
    if 2 * time > base {
        time = (time * time_percentage as i64) / 100;
    }

    // Safety: don't use more than base allows
    if time > base {
        time = base;
    }

    // Subtract buffer for lag
    time -= overhead;
    if time < 0 {
        time = 0;
    }

    // Bullet correction
    time = bullet_correction(time);

    time as u64
}

/// Reduce allocated time for very fast time controls to avoid flagging.
fn bullet_correction(time: i64) -> i64 {
    if time < 200 {
        (time * 23) / 32
    } else if time < 400 {
        (time * 26) / 32
    } else if time < 1200 {
        (time * 29) / 32
    } else {
        time
    }
}
