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

//! Zobrist hashing.
//!
//! Uses a deterministic LCG: `next = next * 1103515245 + 12345` (seed = 1).

use std::sync::OnceLock;

use super::types::*;

// ============================================================================
// Static Zobrist key tables — initialized once via OnceLock
// ============================================================================

struct ZobristTables {
    piece: [[u64; 64]; 12],
    castle: [u64; 16],
    ep: [u64; 8],
}

static TABLES: OnceLock<Box<ZobristTables>> = OnceLock::new();

fn tables() -> &'static ZobristTables {
    TABLES.get().expect("zobrist::init() not called")
}

/// Zobrist PRNG (LCG with seed=1, matches POS::Random64)
fn random64(state: &mut u64) -> u64 {
    *state = state.wrapping_mul(1103515245).wrapping_add(12345);
    *state
}

/// Initialize Zobrist tables. Must be called once at startup.
/// The order and count of random64() calls must match the Polyglot standard exactly.
pub fn init() {
    TABLES
        .set(Box::new({
            let mut t = ZobristTables {
                piece: [[0; 64]; 12],
                castle: [0; 16],
                ep: [0; 8],
            };
            let mut rng: u64 = 1;

            // 12 piece types × 64 squares (for i in 0..12, j in 0..64)
            for piece_keys in &mut t.piece {
                for sq_key in piece_keys.iter_mut() {
                    *sq_key = random64(&mut rng);
                }
            }

            // 16 castling configurations
            for castle_key in &mut t.castle {
                *castle_key = random64(&mut rng);
            }

            // 8 en passant files
            for ep_key in &mut t.ep {
                *ep_key = random64(&mut rng);
            }

            t
        }))
        .ok();
}

// ============================================================================
// Accessors
// ============================================================================

/// Zobrist key for a piece on a square.
#[inline(always)]
pub fn piece_key(piece: Piece, sq: Square) -> u64 {
    debug_assert!(piece != NO_PC);
    debug_assert!((0..64).contains(&sq));
    tables().piece[piece.index()][sq as usize]
}

/// Zobrist key for castling rights.
#[inline(always)]
pub fn castle_key(rights: CastlingRights) -> u64 {
    debug_assert!((0..16).contains(&rights));
    tables().castle[rights as usize]
}

/// Zobrist key for en passant file.
#[inline(always)]
pub fn ep_key(ep_sq: Square) -> u64 {
    debug_assert!(ep_sq != NO_SQ);
    let file = file_of(ep_sq);
    debug_assert!((0..8).contains(&file));
    tables().ep[file as usize]
}

/// Side-to-move XOR constant
pub const SIDE_KEY: u64 = SIDE_RANDOM;
