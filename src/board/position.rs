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

//! Board position representation — FEN parsing, make/unmake move, and draw detection.

use super::attacks;
use super::bitboard::*;
use super::moves::*;
use super::types::*;
use super::zobrist;

// ============================================================================
// Undo data
// ============================================================================

#[derive(Clone, Copy, Default)]
pub struct Undo {
    pub captured_type: PieceType,
    pub castling: CastlingRights,
    pub ep_sq: Square,
    pub rev_moves: i32,
    pub hash_key: u64,
    pub pawn_key: u64,
    pub mg_sc: [i32; 2],
    pub eg_sc: [i32; 2],
}

impl Undo {
    /// Create a zero-initialized Undo struct.
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }
}

// ============================================================================
// Position struct
// ============================================================================

#[derive(Clone)]
pub struct Position {
    // === Cache-hot fields (first ~144 bytes) ===
    // Bitboards — used in every movegen/attack call
    pub cl_bb: [Bitboard; 2], // color bitboards (16B)
    pub tp_bb: [Bitboard; 6], // piece-type bitboards (48B)

    // Hash key — used in TT probe, repetition, Zobrist at every node
    pub hash_key: u64, // (8B)

    // Mailbox — used in do_move/undo_move, eval, move scoring
    pub pc: [Piece; 64], // piece on each square (64B)

    // King locations — used in check detection, eval
    pub king_sq: [Square; 2], // (8B)

    // Side to move — used everywhere
    pub side: Color, // (1B + 3B padding)

    // === Warm fields ===
    pub castling: CastlingRights, // castling rights
    pub ep_sq: Square,            // en passant square
    pub phase: i32,               // game phase
    pub rev_moves: i32,           // reversible (half)move counter
    pub head: usize,              // repetition list index
    pub pawn_key: u64,            // pawn hash key

    // === Cold fields (eval-only) ===
    cnt: [[i32; 6]; 2],  // piece counts [color][piece_type] — use count() accessor
    pub mg_sc: [i32; 2], // midgame PST score per side
    pub eg_sc: [i32; 2], // endgame PST score per side

    // Repetition list — holds hash keys for draw detection.
    // 1024 entries is generous enough to cover the longest legal games
    // plus UCI "position ... moves" lists.
    pub rep_list: [u64; 1024],
}

impl Default for Position {
    fn default() -> Self {
        Self::new()
    }
}

impl Position {
    pub fn new() -> Self {
        Position {
            cl_bb: [Bitboard::EMPTY; 2],
            tp_bb: [Bitboard::EMPTY; 6],
            hash_key: 0,
            pc: [NO_PC; 64],
            king_sq: [NO_SQ; 2],
            side: WC,
            castling: 0,
            ep_sq: NO_SQ,
            phase: 0,
            rev_moves: 0,
            head: 0,
            pawn_key: 0,
            cnt: [[0; 6]; 2],
            mg_sc: [0; 2],
            eg_sc: [0; 2],
            rep_list: [0; 1024],
        }
    }

    // ========================================================================
    // Bitboard accessors
    // ========================================================================

    #[inline(always)]
    pub fn pawns(&self, sd: Color) -> Bitboard {
        self.cl_bb[sd.index()] & self.tp_bb[P.index()]
    }
    #[inline(always)]
    pub fn knights(&self, sd: Color) -> Bitboard {
        self.cl_bb[sd.index()] & self.tp_bb[N.index()]
    }
    #[inline(always)]
    pub fn bishops(&self, sd: Color) -> Bitboard {
        self.cl_bb[sd.index()] & self.tp_bb[B.index()]
    }
    #[inline(always)]
    pub fn rooks(&self, sd: Color) -> Bitboard {
        self.cl_bb[sd.index()] & self.tp_bb[R.index()]
    }
    #[inline(always)]
    pub fn queens(&self, sd: Color) -> Bitboard {
        self.cl_bb[sd.index()] & self.tp_bb[Q.index()]
    }
    #[inline(always)]
    pub fn kings(&self, sd: Color) -> Bitboard {
        self.cl_bb[sd.index()] & self.tp_bb[K.index()]
    }

    #[inline(always)]
    pub fn straight_movers(&self, sd: Color) -> Bitboard {
        self.cl_bb[sd.index()] & (self.tp_bb[R.index()] | self.tp_bb[Q.index()])
    }
    #[inline(always)]
    pub fn diag_movers(&self, sd: Color) -> Bitboard {
        self.cl_bb[sd.index()] & (self.tp_bb[B.index()] | self.tp_bb[Q.index()])
    }

    #[inline(always)]
    pub fn pc_bb(&self, sd: Color, tp: PieceType) -> Bitboard {
        self.cl_bb[sd.index()] & self.tp_bb[tp.index()]
    }
    #[inline(always)]
    pub fn occ_bb(&self) -> Bitboard {
        self.cl_bb[0] | self.cl_bb[1]
    }
    #[inline(always)]
    pub fn unocc_bb(&self) -> Bitboard {
        !self.occ_bb()
    }

    #[inline(always)]
    pub fn king_sq(&self, sd: Color) -> Square {
        self.king_sq[sd.index()]
    }
    /// Piece count for a given side and piece type.
    #[inline(always)]
    pub fn count(&self, sd: Color, pt: PieceType) -> i32 {
        self.cnt[sd.index()][pt.index()]
    }
    #[inline(always)]
    pub fn tp_on_sq(&self, sq: Square) -> PieceType {
        self.pc[sq as usize].piece_type()
    }

    /// Can we do a null move? (must have non-pawn, non-king pieces)
    #[inline(always)]
    pub fn may_null(&self) -> bool {
        (self.cl_bb[self.side.index()] & !(self.tp_bb[P.index()] | self.tp_bb[K.index()]))
            .is_not_empty()
    }

    #[inline(always)]
    pub fn is_on_sq(&self, sd: Color, tp: PieceType, sq: Square) -> bool {
        self.pc_bb(sd, tp).contains(sq)
    }

    #[inline(always)]
    pub fn in_check(&self) -> bool {
        attacks::is_attacked(
            self.king_sq[self.side.index()],
            !self.side,
            self.occ_bb(),
            &self.cl_bb,
            &self.tp_bb,
        )
    }

    /// Is the position illegal? (i.e., the side that just moved left their king in check)
    #[inline(always)]
    pub fn illegal(&self) -> bool {
        attacks::is_attacked(
            self.king_sq[(!self.side).index()],
            self.side,
            self.occ_bb(),
            &self.cl_bb,
            &self.tp_bb,
        )
    }

    // ========================================================================
    // FEN parsing
    // ========================================================================

    /// Set position from a FEN/EPD string.
    pub fn set_position(&mut self, epd: &str) {
        *self = Position::new();

        let bytes = epd.as_bytes();
        let mut idx = 0;

        // Piece placement (rank 8 down to rank 1 in FEN, parsing from index 56)
        let mut i: i32 = 56; // starting at a8
        while i >= 0 {
            let mut j = 0;
            while j < 8 {
                if idx >= bytes.len() {
                    return;
                }
                let ch = bytes[idx] as char;
                if ('1'..='8').contains(&ch) {
                    let skip = (ch as i32) - ('0' as i32);
                    for _ in 0..skip {
                        self.pc[(i + j) as usize] = NO_PC;
                        j += 1;
                    }
                } else {
                    let pc_idx = match ch {
                        'P' => Some(Piece::WP),
                        'p' => Some(Piece::BP),
                        'N' => Some(Piece::WN),
                        'n' => Some(Piece::BN),
                        'B' => Some(Piece::WB),
                        'b' => Some(Piece::BB),
                        'R' => Some(Piece::WR),
                        'r' => Some(Piece::BR),
                        'Q' => Some(Piece::WQ),
                        'q' => Some(Piece::BQ),
                        'K' => Some(Piece::WK),
                        'k' => Some(Piece::BK),
                        _ => None,
                    };

                    if let Some(piece) = pc_idx {
                        let sq = (i + j) as usize;
                        let color = piece.color();
                        let tp = piece.piece_type();

                        self.pc[sq] = piece;
                        self.cl_bb[color.index()] ^= Bitboard::from_sq(sq as i32);
                        self.tp_bb[tp.index()] ^= Bitboard::from_sq(sq as i32);

                        if tp == K {
                            self.king_sq[color.index()] = sq as Square;
                        }

                        // PST incremental accumulation
                        self.phase += PH_VALUE[tp.index()];
                        self.cnt[color.index()][tp.index()] += 1;
                        self.mg_sc[color.index()] +=
                            crate::eval::global_pst::mg(color.index(), tp.index(), sq);
                        self.eg_sc[color.index()] +=
                            crate::eval::global_pst::eg(color.index(), tp.index(), sq);
                        j += 1;
                    } else {
                        // Parse error — reset to the starting position.
                        // This recursive call is safe: START_POS is a known-good FEN
                        // constant that always parses successfully, so no infinite loop.
                        println!(
                            "info string error: invalid FEN character '{}', falling back to startpos",
                            ch
                        );
                        self.set_position(START_POS);
                        return;
                    }
                }
                idx += 1;
            }
            idx += 1; // skip '/' or space
            i -= 8;
        }

        // Side to move
        if idx < bytes.len() {
            self.side = if bytes[idx] == b'w' { WC } else { BC };
            idx += 1;
        }
        idx += 1; // skip space

        // Castling rights
        if idx < bytes.len() && bytes[idx] == b'-' {
            idx += 1;
        } else {
            while idx < bytes.len() && bytes[idx] != b' ' {
                match bytes[idx] {
                    b'K' => self.castling |= W_KS,
                    b'Q' => self.castling |= W_QS,
                    b'k' => self.castling |= B_KS,
                    b'q' => self.castling |= B_QS,
                    _ => break,
                }
                idx += 1;
            }
        }
        idx += 1; // skip space

        // En passant
        if idx < bytes.len() && bytes[idx] == b'-' {
            self.ep_sq = NO_SQ;
        } else if idx + 1 < bytes.len() {
            let file = (bytes[idx] - b'a') as i32;
            let rank = (bytes[idx + 1] - b'1') as i32;
            let ep = sq(file, rank);

            // Only set EP if an enemy pawn can actually capture
            if (attacks::pawn_attacks(!self.side, ep) & self.pawns(self.side)).is_not_empty() {
                self.ep_sq = ep;
            } else {
                self.ep_sq = NO_SQ;
            }
        }

        self.init_hash_key();
        self.init_pawn_key();
    }

    /// Initialize main hash key from scratch.
    fn init_hash_key(&mut self) {
        self.hash_key = 0;
        for sq in 0..64 {
            if self.pc[sq] != NO_PC {
                self.hash_key ^= zobrist::piece_key(self.pc[sq], sq as Square);
            }
        }
        self.hash_key ^= zobrist::castle_key(self.castling);
        if self.ep_sq != NO_SQ {
            self.hash_key ^= zobrist::ep_key(self.ep_sq);
        }
        if self.side == BC {
            self.hash_key ^= zobrist::SIDE_KEY;
        }
    }

    /// Initialize pawn hash key (pawns + kings).
    fn init_pawn_key(&mut self) {
        self.pawn_key = 0;
        for sq in 0..64 {
            if self.pc[sq] != NO_PC {
                let tp = self.pc[sq].piece_type();
                if tp == P || tp == K {
                    self.pawn_key ^= zobrist::piece_key(self.pc[sq], sq as Square);
                }
            }
        }
    }

    // ========================================================================
    // Make / Unmake move
    // ========================================================================

    /// Make a move, saving undo information.
    #[inline]
    pub fn do_move(&mut self, mv: Move, u: &mut Undo) {
        let sd = self.side;
        let op = !sd;
        let fsq = mv.from_sq();
        let tsq = mv.to_sq();
        // SAFETY: fsq and tsq are always valid squares (0-63)
        let ftp = unsafe { *self.pc.get_unchecked(fsq as usize) }.piece_type();
        let ttp = unsafe { *self.pc.get_unchecked(tsq as usize) }.piece_type();

        // Save undo data
        u.captured_type = ttp;
        u.castling = self.castling;
        u.ep_sq = self.ep_sq;
        u.rev_moves = self.rev_moves;
        u.hash_key = self.hash_key;
        u.pawn_key = self.pawn_key;
        u.mg_sc = self.mg_sc;
        u.eg_sc = self.eg_sc;

        // Update repetition list (wrapping within buffer bounds)
        self.rep_list[self.head % self.rep_list.len()] = self.hash_key;
        self.head += 1;
        if ftp == P || ttp != NO_TP {
            self.rev_moves = 0;
        } else {
            self.rev_moves += 1;
        }

        // Update pawn hash on pawn or king move
        if ftp == P || ftp == K {
            let pc = Piece::new(sd, ftp);
            self.pawn_key ^= zobrist::piece_key(pc, fsq) ^ zobrist::piece_key(pc, tsq);
        }

        // Update castling rights
        self.hash_key ^= zobrist::castle_key(self.castling);
        self.castling &= castle_mask(fsq) & castle_mask(tsq);
        self.hash_key ^= zobrist::castle_key(self.castling);

        // Clear en passant square
        if self.ep_sq != NO_SQ {
            self.hash_key ^= zobrist::ep_key(self.ep_sq);
            self.ep_sq = NO_SQ;
        }

        // Move own piece
        let pc_moving = Piece::new(sd, ftp);
        // SAFETY: fsq and tsq are valid squares (0-63) from move encoding
        unsafe {
            *self.pc.get_unchecked_mut(fsq as usize) = NO_PC;
            *self.pc.get_unchecked_mut(tsq as usize) = pc_moving;
        }
        self.hash_key ^= zobrist::piece_key(pc_moving, fsq) ^ zobrist::piece_key(pc_moving, tsq);
        let sq_mask = Bitboard::from_sq(fsq) | Bitboard::from_sq(tsq);
        self.cl_bb[sd.index()] ^= sq_mask;
        self.tp_bb[ftp.index()] ^= sq_mask;
        // PST incremental update
        self.mg_sc[sd.index()] +=
            crate::eval::global_pst::mg(sd.index(), ftp.index(), tsq as usize)
                - crate::eval::global_pst::mg(sd.index(), ftp.index(), fsq as usize);
        self.eg_sc[sd.index()] +=
            crate::eval::global_pst::eg(sd.index(), ftp.index(), tsq as usize)
                - crate::eval::global_pst::eg(sd.index(), ftp.index(), fsq as usize);

        // Update king location
        if ftp == K {
            self.king_sq[sd.index()] = tsq;
        }

        // Capture enemy piece
        if ttp != NO_TP {
            let cap_pc = Piece::new(op, ttp);
            self.hash_key ^= zobrist::piece_key(cap_pc, tsq);
            if ttp == P {
                self.pawn_key ^= zobrist::piece_key(cap_pc, tsq);
            }
            self.cl_bb[op.index()] ^= Bitboard::from_sq(tsq);
            self.tp_bb[ttp.index()] ^= Bitboard::from_sq(tsq);
            self.phase -= PH_VALUE[ttp.index()];
            self.cnt[op.index()][ttp.index()] -= 1;
            self.mg_sc[op.index()] -=
                crate::eval::global_pst::mg(op.index(), ttp.index(), tsq as usize);
            self.eg_sc[op.index()] -=
                crate::eval::global_pst::eg(op.index(), ttp.index(), tsq as usize);
        }

        // Handle special move types
        let mt = mv.move_type();
        match mt {
            NORMAL => {}

            CASTLE => {
                let (r_from, r_to) = match tsq {
                    C1 => (A1, D1),
                    G1 => (H1, F1),
                    C8 => (A8, D8),
                    G8 => (H8, F8),
                    _ => unreachable!(),
                };
                let rook_pc = Piece::new(sd, R);
                // SAFETY: r_from and r_to are valid squares from the fixed castling table
                unsafe {
                    *self.pc.get_unchecked_mut(r_from as usize) = NO_PC;
                    *self.pc.get_unchecked_mut(r_to as usize) = rook_pc;
                }
                self.hash_key ^=
                    zobrist::piece_key(rook_pc, r_from) ^ zobrist::piece_key(rook_pc, r_to);
                let rook_mask = Bitboard::from_sq(r_from) | Bitboard::from_sq(r_to);
                self.cl_bb[sd.index()] ^= rook_mask;
                self.tp_bb[R.index()] ^= rook_mask;
                self.mg_sc[sd.index()] +=
                    crate::eval::global_pst::mg(sd.index(), R.index(), r_to as usize)
                        - crate::eval::global_pst::mg(sd.index(), R.index(), r_from as usize);
                self.eg_sc[sd.index()] +=
                    crate::eval::global_pst::eg(sd.index(), R.index(), r_to as usize)
                        - crate::eval::global_pst::eg(sd.index(), R.index(), r_from as usize);
            }

            EP_CAP => {
                let cap_sq = tsq ^ 8;
                let cap_pawn = Piece::new(op, P);
                // SAFETY: cap_sq = tsq ^ 8, always a valid square
                unsafe { *self.pc.get_unchecked_mut(cap_sq as usize) = NO_PC };
                self.hash_key ^= zobrist::piece_key(cap_pawn, cap_sq);
                self.pawn_key ^= zobrist::piece_key(cap_pawn, cap_sq);
                self.cl_bb[op.index()] ^= Bitboard::from_sq(cap_sq);
                self.tp_bb[P.index()] ^= Bitboard::from_sq(cap_sq);
                // Note: no phase adjustment needed — PH_VALUE[Pawn] == 0.
                self.cnt[op.index()][P.index()] -= 1;
                self.mg_sc[op.index()] -=
                    crate::eval::global_pst::mg(op.index(), P.index(), cap_sq as usize);
                self.eg_sc[op.index()] -=
                    crate::eval::global_pst::eg(op.index(), P.index(), cap_sq as usize);
            }

            EP_SET => {
                let ep_target = tsq ^ 8;
                if (attacks::pawn_attacks(sd, ep_target) & self.pawns(op)).is_not_empty() {
                    self.ep_sq = ep_target;
                    self.hash_key ^= zobrist::ep_key(ep_target);
                }
            }

            N_PROM | B_PROM | R_PROM | Q_PROM => {
                let prom_tp = mv.prom_type();
                let prom_pc = Piece::new(sd, prom_tp);
                let pawn_pc = Piece::new(sd, P);
                // SAFETY: tsq is a valid square (0-63)
                unsafe { *self.pc.get_unchecked_mut(tsq as usize) = prom_pc };
                self.hash_key ^=
                    zobrist::piece_key(pawn_pc, tsq) ^ zobrist::piece_key(prom_pc, tsq);
                self.pawn_key ^= zobrist::piece_key(pawn_pc, tsq);
                self.tp_bb[P.index()] ^= Bitboard::from_sq(tsq);
                self.tp_bb[prom_tp.index()] ^= Bitboard::from_sq(tsq);
                self.phase += PH_VALUE[prom_tp.index()] - PH_VALUE[P.index()];
                self.cnt[sd.index()][P.index()] -= 1;
                self.cnt[sd.index()][prom_tp.index()] += 1;
                self.mg_sc[sd.index()] +=
                    crate::eval::global_pst::mg(sd.index(), prom_tp.index(), tsq as usize)
                        - crate::eval::global_pst::mg(sd.index(), P.index(), tsq as usize);
                self.eg_sc[sd.index()] +=
                    crate::eval::global_pst::eg(sd.index(), prom_tp.index(), tsq as usize)
                        - crate::eval::global_pst::eg(sd.index(), P.index(), tsq as usize);
            }

            _ => {}
        }

        // Change side to move
        self.side = !self.side;
        self.hash_key ^= zobrist::SIDE_KEY;
    }

    /// Unmake a move, restoring state from undo data.
    #[inline]
    pub fn undo_move(&mut self, mv: Move, u: &Undo) {
        debug_assert!(self.head > 0, "undo_move: head underflow");
        let sd = !self.side; // the side that made the move
        let op = !sd;
        let fsq = mv.from_sq();
        let mut tsq = mv.to_sq();
        // SAFETY: tsq is always a valid square (0-63), derived from the original move
        let ftp = unsafe { *self.pc.get_unchecked(tsq as usize) }.piece_type();
        let ttp = u.captured_type;

        // Restore saved state
        self.castling = u.castling;
        self.ep_sq = u.ep_sq;
        self.rev_moves = u.rev_moves;
        self.hash_key = u.hash_key;
        self.pawn_key = u.pawn_key;
        self.mg_sc = u.mg_sc;
        self.eg_sc = u.eg_sc;
        self.head -= 1;

        // Move piece back
        // SAFETY: fsq and tsq are valid squares (0-63) from original move encoding
        unsafe {
            *self.pc.get_unchecked_mut(fsq as usize) = Piece::new(sd, ftp);
            *self.pc.get_unchecked_mut(tsq as usize) = NO_PC;
        }
        let sq_mask = Bitboard::from_sq(fsq) | Bitboard::from_sq(tsq);
        self.cl_bb[sd.index()] ^= sq_mask;
        self.tp_bb[ftp.index()] ^= sq_mask;

        // King location
        if ftp == K {
            self.king_sq[sd.index()] = fsq;
        }

        // Uncapture enemy piece
        if ttp != NO_TP {
            // SAFETY: tsq is valid square (0-63)
            unsafe { *self.pc.get_unchecked_mut(tsq as usize) = Piece::new(op, ttp) };
            self.cl_bb[op.index()] ^= Bitboard::from_sq(tsq);
            self.tp_bb[ttp.index()] ^= Bitboard::from_sq(tsq);
            self.phase += PH_VALUE[ttp.index()];
            self.cnt[op.index()][ttp.index()] += 1;
        }

        let mt = mv.move_type();
        match mt {
            NORMAL => {}

            CASTLE => {
                let (r_from, r_to) = match tsq {
                    C1 => (A1, D1),
                    G1 => (H1, F1),
                    C8 => (A8, D8),
                    G8 => (H8, F8),
                    _ => unreachable!(),
                };
                let rook_pc = Piece::new(sd, R);
                // SAFETY: r_to, r_from are valid from fixed castling table
                unsafe {
                    *self.pc.get_unchecked_mut(r_to as usize) = NO_PC;
                    *self.pc.get_unchecked_mut(r_from as usize) = rook_pc;
                }
                let rook_mask = Bitboard::from_sq(r_from) | Bitboard::from_sq(r_to);
                self.cl_bb[sd.index()] ^= rook_mask;
                self.tp_bb[R.index()] ^= rook_mask;
            }

            EP_CAP => {
                tsq ^= 8;
                // SAFETY: tsq is valid square — derived from move target ^ 8
                unsafe { *self.pc.get_unchecked_mut(tsq as usize) = Piece::new(op, P) };
                self.cl_bb[op.index()] ^= Bitboard::from_sq(tsq);
                self.tp_bb[P.index()] ^= Bitboard::from_sq(tsq);
                // Note: no phase adjustment needed — PH_VALUE[Pawn] == 0.
                self.cnt[op.index()][P.index()] += 1;
            }

            EP_SET => {}

            N_PROM | B_PROM | R_PROM | Q_PROM => {
                // SAFETY: fsq is valid square (0-63) from original move
                unsafe { *self.pc.get_unchecked_mut(fsq as usize) = Piece::new(sd, P) };
                self.tp_bb[P.index()] ^= Bitboard::from_sq(fsq);
                self.tp_bb[ftp.index()] ^= Bitboard::from_sq(fsq);
                self.phase += PH_VALUE[P.index()] - PH_VALUE[ftp.index()];
                self.cnt[sd.index()][P.index()] += 1;
                self.cnt[sd.index()][ftp.index()] -= 1;
            }

            _ => {}
        }

        self.side = !self.side;
    }

    /// Null move — just switch sides.
    #[inline]
    pub fn do_null(&mut self, u: &mut Undo) {
        u.ep_sq = self.ep_sq;
        u.hash_key = self.hash_key;

        self.rep_list[self.head % self.rep_list.len()] = self.hash_key;
        self.head += 1;
        self.rev_moves += 1;

        if self.ep_sq != NO_SQ {
            self.hash_key ^= zobrist::ep_key(self.ep_sq);
            self.ep_sq = NO_SQ;
        }
        self.side = !self.side;
        self.hash_key ^= zobrist::SIDE_KEY;
    }

    /// Undo null move.
    #[inline]
    pub fn undo_null(&mut self, u: &Undo) {
        debug_assert!(self.head > 0, "undo_null: head underflow");
        self.ep_sq = u.ep_sq;
        self.hash_key = u.hash_key;
        self.head -= 1;
        self.rev_moves -= 1;
        self.side = !self.side;
    }

    // ========================================================================
    // Draw detection
    // ========================================================================

    /// Check for draw (50-move rule, repetition, insufficient material, KPK).
    #[inline]
    pub fn is_draw(&self) -> bool {
        // 50 move rule
        if self.rev_moves > 100 {
            return true;
        }

        // Repetition detection — walk back through the rep_list in steps of 2.
        if self.rev_moves >= 4 {
            let mut i = 4i32;
            while i <= self.rev_moves {
                let idx = (self.head - i as usize) % self.rep_list.len();
                if self.hash_key == self.rep_list[idx] {
                    return true;
                }
                i += 2;
            }
        }

        // Insufficient material (no major pieces)
        if self.count(WC, Q) + self.count(BC, Q) + self.count(WC, R) + self.count(BC, R) == 0 {
            // Guard against detecting draw in illegal positions
            if !self.illegal() && self.count(WC, P) + self.count(BC, P) == 0 {
                // KK or KmK
                if self.count(WC, N) + self.count(BC, N) + self.count(WC, B) + self.count(BC, B)
                    <= 1
                {
                    return true;
                }
            }

            // KPK draws
            if self.count(WC, B) + self.count(BC, B) + self.count(WC, N) + self.count(BC, N) == 0
                && self.count(WC, P) + self.count(BC, P) == 1
            {
                if self.count(WC, P) == 1 {
                    return self.kpk_draw(WC);
                }
                if self.count(BC, P) == 1 {
                    return self.kpk_draw(BC);
                }
            }
        }

        false
    }

    /// Trivial KPK draw detection.
    fn kpk_draw(&self, sd: Color) -> bool {
        let op = !sd;
        let pawn_bb = self.pawns(sd);
        let strong_king = self.kings(sd);
        let weak_king = self.kings(op);

        // Opposition through the pawn
        if self.side == sd
            && (weak_king & shift_fwd(pawn_bb, sd)).is_not_empty()
            && (strong_king & shift_fwd(pawn_bb, op)).is_not_empty()
        {
            return true;
        }

        // Weaker side can create opposition in one move
        if self.side == op
            && (attacks::king_attacks(self.king_sq[op.index()]) & shift_fwd(pawn_bb, sd))
                .is_not_empty()
            && (strong_king & shift_fwd(pawn_bb, op)).is_not_empty()
            && !self.illegal()
        {
            return true;
        }

        // Opposition next to the pawn
        if self.side == sd
            && (strong_king & shift_sideways(pawn_bb)).is_not_empty()
            && (weak_king & shift_fwd(shift_fwd(strong_king, sd), sd)).is_not_empty()
        {
            return true;
        }

        false
    }

    /// Contempt-adjusted draw score .
    /// When it's the engine's turn (`self.side == prog_side`), returns -contempt
    /// (engine dislikes draws). Otherwise returns +contempt.
    pub fn draw_score(&self, contempt: i32, prog_side: Color) -> i32 {
        if self.side == prog_side {
            -contempt
        } else {
            contempt
        }
    }

    // ========================================================================
    // Utility
    // ========================================================================

    /// Parse a move string (e.g., "e2e4") relative to this position.
    pub fn str_to_move(&self, s: &str) -> Move {
        if s.len() < 4 {
            return Move::NONE;
        }
        let bytes = s.as_bytes();
        let from = sq((bytes[0] - b'a') as i32, (bytes[1] - b'1') as i32);
        let to = sq((bytes[2] - b'a') as i32, (bytes[3] - b'1') as i32);

        // Determine move type
        let ftp = self.tp_on_sq(from);

        // Promotion
        if s.len() >= 5 {
            let mt = match bytes[4] {
                b'n' => N_PROM,
                b'b' => B_PROM,
                b'r' => R_PROM,
                b'q' => Q_PROM,
                _ => NORMAL,
            };
            if mt != NORMAL {
                return Move::new(from, to, mt);
            }
        }

        // Castling
        if ftp == K && (from - to).abs() == 2 {
            return Move::new(from, to, CASTLE);
        }

        // En passant capture
        if ftp == P && to == self.ep_sq && self.ep_sq != NO_SQ {
            return Move::new(from, to, EP_CAP);
        }

        // Double pawn push
        if ftp == P && (from - to).abs() == 16 {
            return Move::new(from, to, EP_SET);
        }

        Move::normal(from, to)
    }

    /// Print the board (for debugging).
    pub fn print_board(&self) {
        let pc_chars = [
            'P', 'p', 'N', 'n', 'B', 'b', 'R', 'r', 'Q', 'q', 'K', 'k', '.',
        ];
        for rank in (0..8).rev() {
            print!("  {} ", rank + 1);
            for file in 0..8 {
                let s = sq(file, rank);
                let ch = pc_chars[self.pc[s as usize].index()];
                print!("{ch} ");
            }
            println!();
        }
        println!("    a b c d e f g h");
        println!(
            "  Side: {}, EP: {}, Castle: {:04b}, Phase: {}",
            self.side,
            sq_to_string(self.ep_sq),
            self.castling,
            self.phase
        );
    }

    // ========================================================================
    // Single-move legality test (for hash/killer/refutation moves)
    // ========================================================================

    #[inline]
    pub fn legal(&self, mv: Move) -> bool {
        if mv.is_none() {
            return false;
        }

        let sd = self.side;
        let fsq = mv.from_sq();
        let tsq = mv.to_sq();
        let ftp = self.tp_on_sq(fsq);
        let ttp = self.tp_on_sq(tsq);

        // From-square must have our piece
        if ftp == NO_TP || self.pc[fsq as usize].color() != sd {
            return false;
        }
        // To-square must not have our piece
        if ttp != NO_TP && self.pc[tsq as usize].color() == sd {
            return false;
        }

        let mt = mv.move_type();
        match mt {
            NORMAL => {}
            CASTLE => {
                let op = !sd;
                if sd == WC {
                    if fsq != E1 {
                        return false;
                    }
                    if tsq > fsq {
                        return (self.castling & W_KS != 0)
                            && (self.occ_bb() & Bitboard(0x60)).is_empty()
                            && !attacks::is_attacked(
                                E1,
                                op,
                                self.occ_bb(),
                                &self.cl_bb,
                                &self.tp_bb,
                            )
                            && !attacks::is_attacked(
                                F1,
                                op,
                                self.occ_bb(),
                                &self.cl_bb,
                                &self.tp_bb,
                            );
                    }
                    return (self.castling & W_QS != 0)
                        && (self.occ_bb() & Bitboard(0x0E)).is_empty()
                        && !attacks::is_attacked(E1, op, self.occ_bb(), &self.cl_bb, &self.tp_bb)
                        && !attacks::is_attacked(D1, op, self.occ_bb(), &self.cl_bb, &self.tp_bb);
                }
                if fsq != E8 {
                    return false;
                }
                if tsq > fsq {
                    return (self.castling & B_KS != 0)
                        && (self.occ_bb() & Bitboard(0x6000000000000000)).is_empty()
                        && !attacks::is_attacked(E8, WC, self.occ_bb(), &self.cl_bb, &self.tp_bb)
                        && !attacks::is_attacked(F8, WC, self.occ_bb(), &self.cl_bb, &self.tp_bb);
                }
                return (self.castling & B_QS != 0)
                    && (self.occ_bb() & Bitboard(0x0E00000000000000)).is_empty()
                    && !attacks::is_attacked(E8, WC, self.occ_bb(), &self.cl_bb, &self.tp_bb)
                    && !attacks::is_attacked(D8, WC, self.occ_bb(), &self.cl_bb, &self.tp_bb);
            }
            EP_CAP => {
                return ftp == P && tsq == self.ep_sq && self.ep_sq != NO_SQ;
            }
            EP_SET => {
                return ftp == P
                    && ttp == NO_TP
                    && self.pc[(tsq ^ 8) as usize] == NO_PC
                    && ((tsq > fsq && sd == WC) || (tsq < fsq && sd == BC));
            }
            N_PROM | B_PROM | R_PROM | Q_PROM => {
                if ftp != P {
                    return false;
                }
            }
            _ => {}
        }

        // Pawn validation
        if ftp == P {
            if sd == WC {
                if rank_of(fsq) == 6 && !mv.is_prom() {
                    return false;
                }
                if tsq - fsq == 8 {
                    return ttp == NO_TP;
                }
                if (tsq - fsq == 7 && file_of(fsq) != 0) || (tsq - fsq == 9 && file_of(fsq) != 7) {
                    return ttp != NO_TP;
                }
            } else {
                if rank_of(fsq) == 1 && !mv.is_prom() {
                    return false;
                }
                if fsq - tsq == 8 {
                    return ttp == NO_TP;
                }
                if (fsq - tsq == 9 && file_of(fsq) != 0) || (fsq - tsq == 7 && file_of(fsq) != 7) {
                    return ttp != NO_TP;
                }
            }
            return false;
        }

        // Non-pawn promotions illegal
        if mv.is_prom() {
            return false;
        }

        // For pieces, check if the piece can reach the target square
        (attacks::attacks_from(fsq, ftp, sd, self.occ_bb()) & Bitboard::from_sq(tsq)).is_not_empty()
    }
}
