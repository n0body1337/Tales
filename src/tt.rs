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

//! Transposition table — 4-bucket replacement with age-based eviction.
//!
//! Uses full 64-bit keys for correctness.

use crate::board::moves::Move;
use crate::board::types::MAX_EVAL;

// TT flag constants
pub const UPPER: u8 = 1; // alpha (fail-low)
pub const LOWER: u8 = 2; // beta (fail-high)
pub const EXACT: u8 = 3; // exact score

// ============================================================================
// TT Entry — full 64-bit key matching ENTRY struct.
// ============================================================================

#[derive(Clone, Copy, Default)]
#[repr(C)]
pub struct TtEntry {
    pub key: u64,       // 8 bytes — full position hash
    pub best_move: i16, // 2 bytes — best move (raw u16 as i16)
    pub score: i16,     // 2 bytes — score
    pub date: i16,      // 2 bytes — search generation
    pub flags: u8,      // 1 byte  — UPPER/LOWER/EXACT
    pub depth: u8,      // 1 byte  — search depth
}

// ============================================================================
// TransTable — 4-bucket hash table
// ============================================================================

pub struct TransTable {
    table: Vec<TtEntry>,
    tt_size: usize, // number of entries
    tt_mask: usize, // mask for indexing (size - 4)
    pub tt_date: i16,
}

// SAFETY: Shared TT access is standard in chess engines (Lazy SMP).
// Benign data races on entries are acceptable — corrupted entries
// are filtered out by key mismatch on retrieval.
unsafe impl Send for TransTable {}
unsafe impl Sync for TransTable {}

/// Result of a TT probe — returned by `TransTable::retrieve()`.
///
/// - `best_move`: always valid when a matching entry is found
/// - `score`, `flag`: only meaningful when `cutoff` is true (depth was sufficient)
/// - `cutoff`: true when the stored bound proves the position can be pruned
pub struct TtHit {
    pub best_move: Move,
    pub score: i32,
    pub flag: u8,
    pub cutoff: bool,
}

impl TransTable {
    pub fn new(mb_size: usize) -> Self {
        // Round down to power of 2
        let mut size = 2;
        while size * 2 <= mb_size {
            size *= 2;
        }

        let num_entries = size * 1024 * 1024 / std::mem::size_of::<TtEntry>();
        // Mask ensures idx+3 stays within bounds for 4-bucket probing.
        // Since num_entries is always a power of 2, (num_entries - 4) masks
        // to bucket-aligned indices: idx & mask gives the first of 4 entries.
        let mask = num_entries - 4;

        TransTable {
            table: vec![TtEntry::default(); num_entries],
            tt_size: num_entries,
            tt_mask: mask,
            tt_date: 0,
        }
    }

    pub fn clear(&mut self) {
        self.tt_date = 0;
        self.table.fill(TtEntry::default());
    }

    pub fn new_search(&mut self) {
        self.tt_date = self.tt_date.wrapping_add(1);
    }

    /// Issue a software prefetch hint for the TT bucket corresponding to `key`.
    /// Call this before doing work (e.g., in_check) that precedes the actual TT probe,
    /// so the cache line is loaded in parallel with that computation.
    #[inline(always)]
    pub fn prefetch(&self, key: u64) {
        let idx = (key as usize) & self.tt_mask;
        let ptr = unsafe { self.table.as_ptr().add(idx) as *const u8 };
        #[cfg(target_arch = "x86_64")]
        unsafe {
            core::arch::x86_64::_mm_prefetch(ptr as *const i8, core::arch::x86_64::_MM_HINT_T0);
        }
        #[cfg(not(target_arch = "x86_64"))]
        let _ = ptr; // no-op on non-x86_64
    }

    /// Probe the TT for a matching entry.
    ///
    /// Returns `Some(TtHit)` if an entry with the matching key is found.
    /// The `cutoff` field indicates whether the stored bound allows pruning.
    /// Returns `None` if no matching entry exists.
    ///
    /// Refreshes the entry's date on hit using an interior mutation.
    /// This is safe because benign data races on TT entries are standard
    /// in chess engines (Lazy SMP).
    #[inline]
    pub fn retrieve(&self, key: u64, alpha: i32, beta: i32, depth: i32, ply: i32) -> Option<TtHit> {
        let idx = (key as usize) & self.tt_mask;

        for i in 0..4 {
            // SAFETY: idx is masked with tt_mask (size-4), and i < 4,
            // so idx+i is always within the table
            let entry = unsafe { self.table.get_unchecked(idx + i) };
            if entry.key == key {
                // Refresh entry date to prevent premature eviction.
                //
                // This is technically UB (casting &T → *mut T), but is the standard
                // pattern in chess engines for Lazy SMP. Using UnsafeCell<i16> for the
                // date field would be formally correct but would break the #[repr(C)]
                // guarantee and prevent the entry from being Copy/Default.
                // The worst case is a stale date (harmless — entry just gets evicted
                // earlier), and LLVM/x86 makes this a plain aligned store.
                unsafe {
                    let entry_ptr = std::ptr::from_ref::<TtEntry>(entry).cast_mut();
                    (*entry_ptr).date = self.tt_date;
                }

                let best_move = Move(entry.best_move as u16);

                if entry.depth as i32 >= depth {
                    let mut score = entry.score as i32;
                    let flag = entry.flags;

                    // Adjust mate scores for ply
                    if score < -MAX_EVAL {
                        score += ply;
                    } else if score > MAX_EVAL {
                        score -= ply;
                    }

                    // Check for cutoff
                    let cutoff = (flag & UPPER != 0 && score <= alpha)
                        || (flag & LOWER != 0 && score >= beta);

                    return Some(TtHit {
                        best_move,
                        score,
                        flag,
                        cutoff,
                    });
                }

                // Entry found but depth insufficient — return move only
                return Some(TtHit {
                    best_move,
                    score: 0,
                    flag: 0,
                    cutoff: false,
                });
            }
        }

        None
    }

    /// Retrieve only the best move (no score/cutoff check).
    /// Also refreshes entry date on hit .
    #[inline]
    pub fn retrieve_move(&self, key: u64) -> Move {
        let idx = (key as usize) & self.tt_mask;

        for i in 0..4 {
            // SAFETY: idx is masked with tt_mask (size-4), and i < 4,
            // so idx+i is always within the table
            let entry = unsafe { self.table.get_unchecked(idx + i) };
            if entry.key == key {
                // Refresh date (see retrieve() for safety justification)
                unsafe {
                    let entry_ptr = std::ptr::from_ref::<TtEntry>(entry).cast_mut();
                    (*entry_ptr).date = self.tt_date;
                }
                return Move(entry.best_move as u16);
            }
        }

        Move(0)
    }

    /// Store a result in the TT.
    #[inline]
    pub fn store(&mut self, key: u64, mv: Move, mut score: i32, flags: u8, depth: i32, ply: i32) {
        // Adjust mate scores for storage
        if score < -MAX_EVAL {
            score -= ply;
        } else if score > MAX_EVAL {
            score += ply;
        }

        let idx = (key as usize) & self.tt_mask;
        let mut replace_idx = idx;
        let mut oldest = -1i32;
        let mut mv_raw = mv.0 as i16;

        for i in 0..4 {
            // SAFETY: idx is masked with tt_mask (size-4), and i < 4,
            // so idx+i is always within the table
            let entry = unsafe { self.table.get_unchecked(idx + i) };

            // Exact key match — always replace
            if entry.key == key {
                if mv_raw == 0 {
                    mv_raw = entry.best_move;
                }
                replace_idx = idx + i;
                break;
            }

            // Age-based replacement: prefer oldest/shallowest
            let age = ((self.tt_date.wrapping_sub(entry.date)) & 255) as i32 * 256 + 255
                - entry.depth as i32;
            if age > oldest {
                oldest = age;
                replace_idx = idx + i;
            }
        }

        // SAFETY: replace_idx was set within the [idx..idx+4] range
        // which is guaranteed by the tt_mask
        let replace = unsafe { self.table.get_unchecked_mut(replace_idx) };
        replace.key = key;
        replace.date = self.tt_date;
        replace.best_move = mv_raw;
        replace.score = score as i16;
        replace.flags = flags;
        replace.depth = depth as u8;
    }

    /// Hash-full per-mille (0-1000) — sample first 1000 entries
    pub fn hashfull(&self) -> i32 {
        let sample = self.tt_size.min(1000);
        let mut used = 0;
        for i in 0..sample {
            if self.table[i].flags != 0 && self.table[i].date == self.tt_date {
                used += 1;
            }
        }
        (used * 1000 / sample.max(1)) as i32
    }
}
