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

//! Internal (embedded) Polyglot opening book — reads the compiled-in `ph-tal2.bin`.
//!
//! The binary data is standard Polyglot format: entries are sorted by key,
//! each 16 bytes big-endian:
//! - `key`:    `u64` BE (Polyglot Zobrist hash)
//! - `move`:   `u16` BE (from/to/promotion encoding)
//! - `weight`: `u16` BE
//! - `n`:      `u16` BE (unused)
//! - `learn`:  `u16` BE (unused)
//!
//! Position hashing, move decoding, and weighted random selection live in
//! the shared [`polyglot`] module.

use crate::board::moves::Move;
use crate::board::position::Position;

use super::polyglot;

/// Embedded Polyglot book data.
const BOOK_DATA: &[u8] = include_bytes!("ph-tal2.bin");

/// Size of one Polyglot entry in bytes.
const ENTRY_SIZE: usize = 16;

// ============================================================================
// Polyglot book reader — operates on the embedded BOOK_DATA blob
// ============================================================================

/// Number of entries in the book.
fn entry_count() -> usize {
    BOOK_DATA.len() / ENTRY_SIZE
}

/// Read a big-endian u64 from the book data at a given byte offset.
#[inline]
fn read_u64_be(offset: usize) -> u64 {
    u64::from_be_bytes([
        BOOK_DATA[offset],
        BOOK_DATA[offset + 1],
        BOOK_DATA[offset + 2],
        BOOK_DATA[offset + 3],
        BOOK_DATA[offset + 4],
        BOOK_DATA[offset + 5],
        BOOK_DATA[offset + 6],
        BOOK_DATA[offset + 7],
    ])
}

/// Read a big-endian u16 from the book data at a given byte offset.
#[inline]
fn read_u16_be(offset: usize) -> u16 {
    u16::from_be_bytes([BOOK_DATA[offset], BOOK_DATA[offset + 1]])
}

/// Read the key of entry `n`.
#[inline]
fn entry_key(n: usize) -> u64 {
    read_u64_be(n * ENTRY_SIZE)
}

/// Read entry `n`: (key, raw_move, weight).
#[inline]
fn read_entry(n: usize) -> (u64, u16, u16) {
    let off = n * ENTRY_SIZE;
    let key = read_u64_be(off);
    let mv = read_u16_be(off + 8);
    let weight = read_u16_be(off + 10);
    (key, mv, weight)
}

/// Binary search — find the leftmost entry whose key >= `target`.
fn find_pos(target: u64) -> usize {
    let count = entry_count();
    if count == 0 {
        return 0;
    }
    let mut left: usize = 0;
    let mut right: usize = count - 1;

    while left < right {
        let mid = (left + right) / 2;
        if target <= entry_key(mid) {
            right = mid;
        } else {
            left = mid + 1;
        }
    }

    if entry_key(left) == target {
        left
    } else {
        count // not found
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Probe the embedded Polyglot book for the given position.
/// Returns a weighted-random legal move, or None.
pub fn probe(pos: &Position, verbose: bool, book_filter: i32) -> Option<Move> {
    let count = entry_count();
    if count == 0 {
        return None;
    }

    let key = polyglot::polyglot_key(pos);

    if verbose {
        println!("info string probing internal book (key={key:#018x})...");
    }

    let start = find_pos(key);
    if start >= count {
        return None;
    }

    // Phase 1: collect all matching moves with their weights, find max_weight
    let mut candidates: Vec<(Move, i32)> = Vec::new();
    let mut max_weight: i32 = 0;

    let mut i = start;
    while i < count {
        let (h, raw_mv, weight) = read_entry(i);
        if h != key {
            break;
        }

        let mv_str = polyglot::polyglot_move_to_string(raw_mv, pos);
        let mv = pos.str_to_move(&mv_str);
        let w = weight as i32;

        if mv != Move::NONE && pos.legal(mv) {
            if w > max_weight {
                max_weight = w;
            }
            candidates.push((mv, w));
        }

        i += 1;
    }

    if candidates.is_empty() {
        return None;
    }

    // Phase 2: weighted random selection, filtering infrequent moves
    let mut rng_state = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    // Ensure non-zero seed
    if rng_state == 0 {
        rng_state = 0xDEADBEEF_CAFEBABE;
    }

    let weight_sum: i32 = candidates
        .iter()
        .filter(|(_, w)| !polyglot::is_infrequent(*w, max_weight, book_filter))
        .map(|(_, w)| *w)
        .sum();

    let mut vals_acc: i32 = 0;
    let mut choice: Option<Move> = None;

    for (mv, w) in &candidates {
        if verbose {
            if polyglot::is_infrequent(*w, max_weight, book_filter) {
                println!("info string {mv}?! ({w})");
            } else if weight_sum > 0 {
                println!("info string {mv} {} %", (*w * 100) / weight_sum);
            }
        }

        if !polyglot::is_infrequent(*w, max_weight, book_filter) {
            vals_acc += *w;
            if polyglot::simple_random(&mut rng_state, vals_acc) < *w {
                choice = Some(*mv);
            }
        }
    }

    choice
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::types::*;

    #[test]
    fn book_loads() {
        let count = entry_count();
        assert!(count > 1000, "Expected >1000 book entries, got {count}");
        // Verify entries are 16 bytes each
        assert_eq!(
            BOOK_DATA.len() % ENTRY_SIZE,
            0,
            "Book data not aligned to 16-byte entries"
        );
    }

    #[test]
    fn book_first_entry_nonzero() {
        let (key, mv, weight) = read_entry(0);
        assert!(key != 0, "First entry key should be nonzero");
        assert!(mv != 0, "First entry move should be nonzero");
        assert!(weight > 0, "First entry weight should be positive");
    }

    #[test]
    fn startpos_has_book_move() {
        // Initialize engine tables so Position works
        crate::board::init();
        let par = crate::eval::params::EvalParams::new();
        crate::eval::global_pst::init(&par);

        let mut pos = Position::new();
        pos.set_position(START_POS);

        let mv = probe(&pos, false, 20);
        assert!(
            mv.is_some(),
            "Start position should have at least one book move"
        );
    }

    #[test]
    fn polyglot_key_startpos() {
        // Initialize engine tables
        crate::board::init();
        let par = crate::eval::params::EvalParams::new();
        crate::eval::global_pst::init(&par);

        let mut pos = Position::new();
        pos.set_position(START_POS);

        let key = polyglot::polyglot_key(&pos);
        // The canonical Polyglot starting position key
        assert_eq!(
            key, 0x463b96181691fc9c,
            "Polyglot key for startpos should be 0x463b96181691fc9c, got {key:#018x}"
        );
    }
}
