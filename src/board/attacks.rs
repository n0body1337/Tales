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

//! Attack generation — leaper attack tables and slider attack wrappers.

use std::sync::atomic::{AtomicPtr, Ordering};

use super::bitboard::*;
use super::magic;
use super::types::*;

// ============================================================================
// Static attack tables — initialized once, accessed via raw pointer (no atomic)
// ============================================================================

struct LeaperTables {
    pawn_attacks: [[Bitboard; 64]; 2],
    knight_attacks: [Bitboard; 64],
    king_attacks: [Bitboard; 64],
}

// Raw pointer — initialized once in init(), then accessed with zero overhead.
static LEAPER_PTR: AtomicPtr<LeaperTables> = AtomicPtr::new(std::ptr::null_mut());

#[inline(always)]
fn tables() -> &'static LeaperTables {
    // SAFETY: init() is always called before any attack lookup.
    // After init(), LEAPER_PTR is a valid, aligned, non-null pointer
    // to a leaked Box that lives for the entire program.
    unsafe { &*LEAPER_PTR.load(Ordering::Relaxed) }
}

/// Initialize leaper attack tables. Called from board::init().
pub fn init() {
    let t = Box::new({
        let mut t = LeaperTables {
            pawn_attacks: [[Bitboard(0); 64]; 2],
            knight_attacks: [Bitboard(0); 64],
            king_attacks: [Bitboard(0); 64],
        };
        for sq in 0..64i32 {
            let bb = Bitboard::from_sq(sq);

            // Pawn attacks
            t.pawn_attacks[WC.index()][sq as usize] = shift_ne(bb) | shift_nw(bb);
            t.pawn_attacks[BC.index()][sq as usize] = shift_se(bb) | shift_sw(bb);

            // Knight attacks
            let bb_west = shift_west(bb);
            let bb_east = shift_east(bb);
            let mut n_att =
                Bitboard(((bb_east.0 | bb_west.0) << 16) | ((bb_east.0 | bb_west.0) >> 16));
            let bb_west2 = shift_west(bb_west);
            let bb_east2 = shift_east(bb_east);
            n_att.0 |= (bb_east2.0 | bb_west2.0) << 8;
            n_att.0 |= (bb_east2.0 | bb_west2.0) >> 8;
            t.knight_attacks[sq as usize] = n_att;

            // King attacks
            let mut k_att = bb;
            k_att = k_att | shift_sideways(k_att);
            k_att = k_att | shift_north(k_att) | shift_south(k_att);
            // Remove the king square itself
            t.king_attacks[sq as usize] = k_att ^ bb;
        }
        t
    });
    let t = Box::leak(t);
    LEAPER_PTR.store(t as *mut LeaperTables, Ordering::Release);
}

// ============================================================================
// Public leaper attack accessors
// ============================================================================

#[inline(always)]
pub fn pawn_attacks(color: Color, sq: i32) -> Bitboard {
    let t = tables();
    unsafe {
        *t.pawn_attacks
            .get_unchecked(color.index())
            .get_unchecked(sq as usize)
    }
}

#[inline(always)]
pub fn knight_attacks(sq: i32) -> Bitboard {
    unsafe { *tables().knight_attacks.get_unchecked(sq as usize) }
}

#[inline(always)]
pub fn king_attacks(sq: i32) -> Bitboard {
    unsafe { *tables().king_attacks.get_unchecked(sq as usize) }
}

// ============================================================================
// Slider wrappers (delegate to magic module)
// ============================================================================

#[inline(always)]
pub fn bishop_attacks(occ: Bitboard, sq: i32) -> Bitboard {
    magic::bishop_attacks(occ, sq)
}

#[inline(always)]
pub fn rook_attacks(occ: Bitboard, sq: i32) -> Bitboard {
    magic::rook_attacks(occ, sq)
}

#[inline(always)]
pub fn queen_attacks(occ: Bitboard, sq: i32) -> Bitboard {
    magic::queen_attacks(occ, sq)
}

// ============================================================================
// Composite attack functions (match POS methods)
// ============================================================================

/// All attacks from a given square (given the piece on that square).
/// Generates attack bitboard for the piece at the given square.
#[inline]
pub fn attacks_from(sq: i32, piece_type: PieceType, color: Color, occ: Bitboard) -> Bitboard {
    match piece_type {
        PieceType::Pawn => pawn_attacks(color, sq),
        PieceType::Knight => knight_attacks(sq),
        PieceType::Bishop => bishop_attacks(occ, sq),
        PieceType::Rook => rook_attacks(occ, sq),
        PieceType::Queen => queen_attacks(occ, sq),
        PieceType::King => king_attacks(sq),
        PieceType::None => Bitboard::EMPTY,
    }
}

/// Is square `sq` attacked by any piece of `color`?
/// Returns true if the given square is attacked by the given side.
#[inline]
pub fn is_attacked(
    sq: i32,
    by_color: Color,
    occ: Bitboard,
    cl_bb: &[Bitboard; 2],
    tp_bb: &[Bitboard; 6],
) -> bool {
    let them = cl_bb[by_color.index()];

    // Pawn attacks: a square is attacked by a pawn if the square's pawn-attack
    // (from the OPPOSITE color's perspective) intersects with their pawns.
    if (pawn_attacks(!by_color, sq) & them & tp_bb[P.index()]).is_not_empty() {
        return true;
    }
    if (knight_attacks(sq) & them & tp_bb[N.index()]).is_not_empty() {
        return true;
    }
    if (king_attacks(sq) & them & tp_bb[K.index()]).is_not_empty() {
        return true;
    }
    if (bishop_attacks(occ, sq) & them & (tp_bb[B.index()] | tp_bb[Q.index()])).is_not_empty() {
        return true;
    }
    if (rook_attacks(occ, sq) & them & (tp_bb[R.index()] | tp_bb[Q.index()])).is_not_empty() {
        return true;
    }
    false
}

/// Squares from which an enemy piece is considered to be "attacking the
/// king" — the king's immediate ring (the 8 surrounding squares) plus the
/// row immediately in front of the ring (relative to `king_color`).
///
/// Used by `eval::pieces::evaluate_pieces` to count attackers against the
/// enemy king, and by `search::ordering::is_sacrificial` to decide whether
/// a move's destination square targets the king area.
#[inline]
pub fn king_attack_zone(king_sq: i32, king_color: Color) -> Bitboard {
    let ring = king_attacks(king_sq);
    ring | shift_fwd(ring, king_color)
}

/// All pieces of all colors attacking a square.
/// Returns a bitboard of all pieces that attack the given square.
#[inline]
pub fn attacks_to(
    sq: i32,
    occ: Bitboard,
    cl_bb: &[Bitboard; 2],
    tp_bb: &[Bitboard; 6],
) -> Bitboard {
    (pawn_attacks(WC, sq) & cl_bb[BC.index()] & tp_bb[P.index()])
        | (pawn_attacks(BC, sq) & cl_bb[WC.index()] & tp_bb[P.index()])
        | (knight_attacks(sq) & tp_bb[N.index()])
        | (king_attacks(sq) & tp_bb[K.index()])
        | (bishop_attacks(occ, sq) & (tp_bb[B.index()] | tp_bb[Q.index()]))
        | (rook_attacks(occ, sq) & (tp_bb[R.index()] | tp_bb[Q.index()]))
}
