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

//! Stack-allocated scored move array for move generation and ordering.

use crate::board::moves::Move;

/// Maximum number of moves in any position (generous upper bound).
pub const MAX_MOVES: usize = 256;

/// A move paired with a score for move ordering.
#[derive(Clone, Copy, Default)]
pub struct ScoredMove {
    pub mv: Move,
    pub score: i32,
}

/// Stack-allocated list of scored moves.
/// Used by generators and the move picker.
#[derive(Clone)]
pub struct MoveList {
    pub moves: [ScoredMove; MAX_MOVES],
    pub count: usize,
}

impl Default for MoveList {
    fn default() -> Self {
        Self::new()
    }
}

impl MoveList {
    #[inline]
    pub fn new() -> Self {
        MoveList {
            moves: [ScoredMove::default(); MAX_MOVES],
            count: 0,
        }
    }

    /// Add a move (unscored, score=0).
    #[inline(always)]
    pub fn push(&mut self, mv: Move) {
        debug_assert!(self.count < MAX_MOVES);
        // SAFETY: count is always < MAX_MOVES (asserted in debug)
        unsafe { *self.moves.get_unchecked_mut(self.count) = ScoredMove { mv, score: 0 } };
        self.count += 1;
    }

    /// Clear the list.
    #[inline(always)]
    pub fn clear(&mut self) {
        self.count = 0;
    }

    /// Get move at index.
    #[inline(always)]
    pub fn get(&self, idx: usize) -> Move {
        debug_assert!(idx < self.count);
        // SAFETY: idx is always < count which is always < MAX_MOVES
        unsafe { self.moves.get_unchecked(idx) }.mv
    }

    /// Swap the highest-scored move in range [start..count) to position `start`.
    /// Returns the move (for the staged move picker).
    #[inline]
    pub fn best_move(&mut self, start: usize) -> Move {
        // SAFETY: start and all i values are bounded by self.count <= MAX_MOVES
        let mut best_idx = start;
        let mut best_score = unsafe { self.moves.get_unchecked(start) }.score;

        for i in (start + 1)..self.count {
            let score = unsafe { self.moves.get_unchecked(i) }.score;
            if score > best_score {
                best_score = score;
                best_idx = i;
            }
        }

        if best_idx != start {
            // SAFETY: both indices are bounded by self.count <= MAX_MOVES
            unsafe {
                let ptr = self.moves.as_mut_ptr();
                std::ptr::swap(ptr.add(start), ptr.add(best_idx));
            }
        }

        unsafe { self.moves.get_unchecked(start) }.mv
    }
}
