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

// Distance tables — bonus/tropism/chebyshev distance.
// Matches cDistance class.

use std::sync::OnceLock;

use super::types::*;

// ============================================================================
// Distance tables — initialized once, read forever
// ============================================================================

struct DistanceTables {
    /// Flattened [64][64] bonus table (heap-allocated)
    bonus: Vec<i32>,
    /// Flattened [64][64] metric table (heap-allocated)
    metric: Vec<i32>,
}

static TABLES: OnceLock<DistanceTables> = OnceLock::new();

fn tables() -> &'static DistanceTables {
    TABLES.get().expect("distance::init() not called")
}

/// Initialize distance tables. Called from board::init().
pub fn init() {
    TABLES
        .set({
            let mut bonus = vec![0i32; 64 * 64];
            let mut metric = vec![0i32; 64 * 64];
            for sq1 in 0..64i32 {
                for sq2 in 0..64i32 {
                    let file_dist = (file_of(sq1) - file_of(sq2)).abs();
                    let rank_dist = (rank_of(sq1) - rank_of(sq2)).abs();

                    metric[(sq1 as usize) * 64 + sq2 as usize] = file_dist.max(rank_dist);
                    bonus[(sq1 as usize) * 64 + sq2 as usize] = 14 - (file_dist + rank_dist);
                }
            }
            DistanceTables { bonus, metric }
        })
        .ok();
}

/// Bonus distance between two squares (14 - manhattan distance).
#[inline(always)]
pub fn bonus(sq1: Square, sq2: Square) -> i32 {
    debug_assert!((0..64).contains(&sq1) && (0..64).contains(&sq2));
    // SAFETY: sq1 and sq2 are valid squares (0-63), so index is always < 4096
    unsafe {
        *tables()
            .bonus
            .get_unchecked((sq1 as usize) * 64 + sq2 as usize)
    }
}

/// Chebyshev distance between two squares.
#[inline(always)]
pub fn metric(sq1: Square, sq2: Square) -> i32 {
    debug_assert!((0..64).contains(&sq1) && (0..64).contains(&sq2));
    // SAFETY: sq1 and sq2 are valid squares (0-63), so index is always < 4096
    unsafe {
        *tables()
            .metric
            .get_unchecked((sq1 as usize) * 64 + sq2 as usize)
    }
}
