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

// External Polyglot opening book — reads a `.bin` file from disk.
//
// This module provides disk-based polyglot book support. It reuses the
// shared polyglot helpers from `internal.rs` (Zobrist hashing, move
// decoding, weighted random selection) to avoid any code duplication.
//
// When the user sets `UseBook true` and provides a valid `MainBookFile`
// path, this module loads the file into memory and probes it instead of
// the internal embedded book.

use crate::board::moves::Move;
use crate::board::position::Position;

use super::polyglot;

/// Size of one Polyglot entry in bytes.
const ENTRY_SIZE: usize = 16;

/// An external polyglot book loaded from disk.
pub struct ExternalBook {
    /// Raw book data (Polyglot `.bin` format).
    data: Vec<u8>,
    /// Number of 16-byte entries.
    count: usize,
}

impl ExternalBook {
    /// Load a Polyglot `.bin` file from `path`.
    ///
    /// Returns `Ok(ExternalBook)` on success, or `Err(message)` if the file
    /// cannot be read or has an invalid size.
    pub fn load(path: &str) -> Result<Self, String> {
        let data = std::fs::read(path).map_err(|e| format!("cannot read '{path}': {e}"))?;

        if data.is_empty() {
            return Err(format!("book file '{path}' is empty"));
        }
        if data.len() % ENTRY_SIZE != 0 {
            return Err(format!(
                "book file '{path}' size ({} bytes) is not a multiple of {ENTRY_SIZE}",
                data.len()
            ));
        }

        let count = data.len() / ENTRY_SIZE;
        Ok(ExternalBook { data, count })
    }

    /// Number of entries in this book.
    #[inline]
    pub fn entry_count(&self) -> usize {
        self.count
    }

    // ========================================================================
    // Private helpers — operate on `self.data` instead of the embedded blob
    // ========================================================================

    /// Read a big-endian u64 at `offset`.
    #[inline]
    fn read_u64_be(&self, offset: usize) -> u64 {
        u64::from_be_bytes([
            self.data[offset],
            self.data[offset + 1],
            self.data[offset + 2],
            self.data[offset + 3],
            self.data[offset + 4],
            self.data[offset + 5],
            self.data[offset + 6],
            self.data[offset + 7],
        ])
    }

    /// Read a big-endian u16 at `offset`.
    #[inline]
    fn read_u16_be(&self, offset: usize) -> u16 {
        u16::from_be_bytes([self.data[offset], self.data[offset + 1]])
    }

    /// Read the key of entry `n`.
    #[inline]
    fn entry_key(&self, n: usize) -> u64 {
        self.read_u64_be(n * ENTRY_SIZE)
    }

    /// Read entry `n`: (key, raw_move, weight).
    #[inline]
    fn read_entry(&self, n: usize) -> (u64, u16, u16) {
        let off = n * ENTRY_SIZE;
        let key = self.read_u64_be(off);
        let mv = self.read_u16_be(off + 8);
        let weight = self.read_u16_be(off + 10);
        (key, mv, weight)
    }

    /// Binary search — find the leftmost entry whose key >= `target`.
    fn find_pos(&self, target: u64) -> usize {
        if self.count == 0 {
            return 0;
        }
        let mut left: usize = 0;
        let mut right: usize = self.count - 1;

        while left < right {
            let mid = (left + right) / 2;
            if target <= self.entry_key(mid) {
                right = mid;
            } else {
                left = mid + 1;
            }
        }

        if self.entry_key(left) == target {
            left
        } else {
            self.count // not found
        }
    }

    // ========================================================================
    // Public API
    // ========================================================================

    /// Probe this external book for the given position.
    /// Returns a weighted-random legal move, or `None`.
    ///
    /// `book_filter` (0–100): minimum weight as a percentage of the best move.
    /// 0 = consider all moves, 100 = only the best move.
    pub fn probe(&self, pos: &Position, verbose: bool, book_filter: i32) -> Option<Move> {
        if self.count == 0 {
            return None;
        }

        let key = polyglot::polyglot_key(pos);

        if verbose {
            println!("info string probing external book (key={key:#018x})...");
        }

        let start = self.find_pos(key);
        if start >= self.count {
            return None;
        }

        // Phase 1: collect all matching moves with their weights, find max_weight
        let mut candidates: Vec<(Move, i32)> = Vec::new();
        let mut max_weight: i32 = 0;

        let mut i = start;
        while i < self.count {
            let (h, raw_mv, weight) = self.read_entry(i);
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
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_nonexistent_file_returns_error() {
        let result = ExternalBook::load("nonexistent_book_ZZZZZ.bin");
        assert!(result.is_err(), "Loading a missing file should return Err");
    }

    #[test]
    fn load_invalid_size_returns_error() {
        // Write a 17-byte file (not a multiple of 16)
        let path = std::env::temp_dir().join("tales_test_bad_book.bin");
        std::fs::write(&path, vec![0u8; 17]).expect("failed to write test file");

        let result = ExternalBook::load(path.to_str().unwrap());
        assert!(
            result.is_err(),
            "Loading a file with invalid size should return Err"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_empty_file_returns_error() {
        let path = std::env::temp_dir().join("tales_test_empty_book.bin");
        std::fs::write(&path, vec![0u8; 0]).expect("failed to write test file");

        let result = ExternalBook::load(path.to_str().unwrap());
        assert!(result.is_err(), "Loading an empty file should return Err");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_valid_book_succeeds() {
        // Create a 32-byte file (2 entries) — content is meaningless but valid size
        let path = std::env::temp_dir().join("tales_test_valid_book.bin");
        std::fs::write(&path, vec![0u8; 32]).expect("failed to write test file");

        let result = ExternalBook::load(path.to_str().unwrap());
        assert!(result.is_ok(), "Loading a valid-sized file should succeed");
        assert_eq!(result.unwrap().entry_count(), 2);

        let _ = std::fs::remove_file(&path);
    }
}
