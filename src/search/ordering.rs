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

//! Move ordering — staged move picker with history, killer, and refutation heuristics.

use crate::board::attacks;
use crate::board::bitboard::Bitboard;
use crate::board::moves::*;
use crate::board::position::Position;
use crate::board::types::*;
use crate::movegen::generate;
use crate::movegen::movelist::MoveList;
use crate::movegen::see;
use crate::{eval, tt::TransTable};

use super::alphabeta::LmrTable;

/// Per-thread search context — bundles all shared mutable resources
/// that are threaded through the search tree unchanged.
///
/// The remaining per-call arguments (`pos`, `alpha`, `beta`, `depth`, `ply`, `pv`)
/// stay as explicit function parameters since they change at every recursive call.
pub struct SearchCtx<'a> {
    /// Per-thread search state (history, killers, refutation, timing).
    pub searcher: &'a mut Searcher,
    /// Transposition table (shared across threads in SMP via raw pointer).
    pub tt: &'a mut TransTable,
    /// Immutable evaluation parameters.
    pub par: &'a eval::params::EvalParams,
    /// Per-thread evaluation hash.
    pub eval_hash: &'a mut Vec<eval::EvalHashEntry>,
    /// Per-thread pawn structure hash.
    pub pawn_tt: &'a mut eval::pawn_hash::PawnHash,
    /// Late-move reduction table (immutable, computed once per search).
    pub lmr: &'a LmrTable,
}

/// Classification of a move returned by the staged picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MoveKind {
    Hash = 0,
    Capture = 1,
    Killer = 2,
    Normal = 3,
    BadCapt = 4,
    Refutation = 5,
}

// ============================================================================
// MovePicker — 9-phase staged move generation
// ============================================================================

pub struct MovePicker {
    phase: i32,
    trans_move: Move,
    ref_move: Move,
    ref_sq: i32,
    killer1: Move,
    killer2: Move,
    list: MoveList,
    next_idx: usize,
    bad: [Move; 32],
    bad_count: usize,
    bad_next: usize,
}

impl MovePicker {
    pub fn new(
        trans_move: Move,
        ref_move: Move,
        ref_sq: i32,
        killer1: Move,
        killer2: Move,
    ) -> Self {
        MovePicker {
            phase: 0,
            trans_move,
            ref_move,
            ref_sq,
            killer1,
            killer2,
            list: MoveList::new(),
            next_idx: 0,
            bad: [Move::NONE; 32],
            bad_count: 0,
            bad_next: 0,
        }
    }

    /// Main staged move picker for the interior search. Returns
    /// `(move, kind, is_sac)`.
    ///
    /// `is_sac` is the cached sacrifice classification:
    ///   - scored-list phases (good captures, quiet moves) read it from
    ///     `ScoredMove.is_sac` at zero cost;
    ///   - single-move phases (hash / killer / refutation) compute
    ///     `is_sacrificial` once for the one returned move;
    ///   - bad captures are always reported as `false` because
    ///     `bad_capture` only defers SEE-losing captures that do *not*
    ///     target the king zone — sacrificial captures stay in the main
    ///     pool with a `SAC_BONUS` boost from `score_captures`.
    pub fn next_move(
        &mut self,
        pos: &Position,
        history: &[[i32; 64]; 13],
    ) -> (Move, MoveKind, bool) {
        loop {
            match self.phase {
                0 => {
                    // Phase 0: hash move
                    let mv = self.trans_move;
                    self.phase = 1;
                    if mv.is_some() && pos.legal(mv) {
                        return (mv, MoveKind::Hash, is_sacrificial(pos, mv));
                    }
                }
                1 => {
                    // Phase 1: generate captures
                    self.list.clear();
                    generate::generate_captures(pos, &mut self.list);
                    score_captures(pos, &mut self.list);
                    self.next_idx = 0;
                    self.bad_count = 0;
                    self.phase = 2;
                }
                2 => {
                    // Phase 2: return good captures, defer bad ones
                    while self.next_idx < self.list.count {
                        let idx = self.next_idx;
                        let mv = self.list.best_move(idx);
                        self.next_idx += 1;
                        if mv == self.trans_move {
                            continue;
                        }
                        if bad_capture(pos, mv) {
                            if self.bad_count < self.bad.len() {
                                self.bad[self.bad_count] = mv;
                                self.bad_count += 1;
                            }
                            continue;
                        }
                        // `best_move` swapped the best-scoring entry to
                        // position `idx`; read the cached sac flag there.
                        let is_sac = self.list.moves[idx].is_sac;
                        return (mv, MoveKind::Capture, is_sac);
                    }
                    self.phase = 3;
                }
                3 => {
                    // Phase 3: killer 1
                    let mv = self.killer1;
                    self.phase = 4;
                    if mv.is_some()
                        && mv != self.trans_move
                        && pos.pc[mv.to_sq() as usize] == NO_PC
                        && pos.legal(mv)
                    {
                        return (mv, MoveKind::Killer, is_sacrificial(pos, mv));
                    }
                }
                4 => {
                    // Phase 4: killer 2
                    let mv = self.killer2;
                    self.phase = 5;
                    if mv.is_some()
                        && mv != self.trans_move
                        && pos.pc[mv.to_sq() as usize] == NO_PC
                        && pos.legal(mv)
                    {
                        return (mv, MoveKind::Killer, is_sacrificial(pos, mv));
                    }
                }
                5 => {
                    // Phase 5: refutation move
                    let mv = self.ref_move;
                    self.phase = 6;
                    if mv.is_some()
                        && mv != self.trans_move
                        && pos.pc[mv.to_sq() as usize] == NO_PC
                        && mv != self.killer1
                        && mv != self.killer2
                        && pos.legal(mv)
                    {
                        return (mv, MoveKind::Refutation, is_sacrificial(pos, mv));
                    }
                }
                6 => {
                    // Phase 6: generate quiet moves
                    self.list.clear();
                    generate::generate_quiet(pos, &mut self.list);
                    score_quiet(pos, &mut self.list, history, self.ref_sq);
                    self.next_idx = 0;
                    self.phase = 7;
                }
                7 => {
                    // Phase 7: return quiet moves
                    while self.next_idx < self.list.count {
                        let idx = self.next_idx;
                        let mv = self.list.best_move(idx);
                        self.next_idx += 1;
                        if mv == self.trans_move
                            || mv == self.killer1
                            || mv == self.killer2
                            || mv == self.ref_move
                        {
                            continue;
                        }
                        let is_sac = self.list.moves[idx].is_sac;
                        return (mv, MoveKind::Normal, is_sac);
                    }
                    self.bad_next = 0;
                    self.phase = 8;
                }
                8 => {
                    // Phase 8: return bad captures. `bad_capture` does
                    // not defer sacrificial-style captures (they stay in
                    // the main pool), so anything in `bad[]` is by
                    // construction non-sacrificial — hard-code `false`.
                    if self.bad_next < self.bad_count {
                        let mv = self.bad[self.bad_next];
                        self.bad_next += 1;
                        return (mv, MoveKind::BadCapt, false);
                    }
                    return (Move::NONE, MoveKind::Normal, false);
                }
                _ => return (Move::NONE, MoveKind::Normal, false),
            }
        }
    }
}

// ============================================================================
// NextSpecialMove — for QuiesceChecks (captures + killers + checking moves)
// ============================================================================

pub struct SpecialPicker {
    phase: i32,
    trans_move: Move,
    killer1: Move,
    killer2: Move,
    list: MoveList,
    next_idx: usize,
}

impl SpecialPicker {
    pub fn new(trans_move: Move, killer1: Move, killer2: Move) -> Self {
        SpecialPicker {
            phase: 0,
            trans_move,
            killer1,
            killer2,
            list: MoveList::new(),
            next_idx: 0,
        }
    }

    pub fn next_move(&mut self, pos: &Position, history: &[[i32; 64]; 13]) -> (Move, MoveKind) {
        loop {
            match self.phase {
                0 => {
                    let mv = self.trans_move;
                    self.phase = 1;
                    if mv.is_some() && pos.legal(mv) {
                        return (mv, MoveKind::Hash);
                    }
                }
                1 => {
                    self.list.clear();
                    generate::generate_captures(pos, &mut self.list);
                    score_captures(pos, &mut self.list);
                    self.next_idx = 0;
                    self.phase = 2;
                }
                2 => {
                    while self.next_idx < self.list.count {
                        let mv = self.list.best_move(self.next_idx);
                        self.next_idx += 1;
                        if mv == self.trans_move {
                            continue;
                        }
                        if bad_capture(pos, mv) {
                            continue;
                        }
                        return (mv, MoveKind::Capture);
                    }
                    self.phase = 3;
                }
                3 => {
                    let mv = self.killer1;
                    self.phase = 4;
                    if mv.is_some()
                        && mv != self.trans_move
                        && pos.pc[mv.to_sq() as usize] == NO_PC
                        && pos.legal(mv)
                    {
                        return (mv, MoveKind::Killer);
                    }
                }
                4 => {
                    let mv = self.killer2;
                    self.phase = 5;
                    if mv.is_some()
                        && mv != self.trans_move
                        && pos.pc[mv.to_sq() as usize] == NO_PC
                        && pos.legal(mv)
                    {
                        return (mv, MoveKind::Killer);
                    }
                }
                5 => {
                    // Generate checking moves
                    self.list.clear();
                    generate::generate_special(pos, &mut self.list);
                    score_quiet(pos, &mut self.list, history, -1);
                    self.next_idx = 0;
                    self.phase = 6;
                }
                6 => {
                    while self.next_idx < self.list.count {
                        let mv = self.list.best_move(self.next_idx);
                        self.next_idx += 1;
                        if mv == self.trans_move || mv == self.killer1 || mv == self.killer2 {
                            continue;
                        }
                        return (mv, MoveKind::Normal);
                    }
                    return (Move::NONE, MoveKind::Normal);
                }
                _ => return (Move::NONE, MoveKind::Normal),
            }
        }
    }
}

// ============================================================================
// CapturesPicker — simplified picker for Quiesce (captures only, MVV-LVA)
// ============================================================================

pub struct CapturesPicker {
    list: MoveList,
    next_idx: usize,
}

impl CapturesPicker {
    pub fn new(pos: &Position) -> Self {
        let mut list = MoveList::new();
        generate::generate_captures(pos, &mut list);
        score_captures(pos, &mut list);
        CapturesPicker { list, next_idx: 0 }
    }

    pub fn next(&mut self) -> Move {
        if self.next_idx < self.list.count {
            let mv = self.list.best_move(self.next_idx);
            self.next_idx += 1;
            mv
        } else {
            Move::NONE
        }
    }
}

// ============================================================================
// Scoring functions
// ============================================================================

#[inline]
fn score_captures(pos: &Position, list: &mut MoveList) {
    for i in 0..list.count {
        // SAFETY: i is bounded by list.count which is always <= MAX_MOVES
        let entry = unsafe { list.moves.get_unchecked_mut(i) };
        let mv = entry.mv;
        let mut sc = mvv_lva(pos, mv);
        let mut is_sac = false;
        // Sacrificial-capture promotion: SEE-losing captures that target
        // the enemy king zone or deliver check get bumped above the rest of
        // the bad captures so they sort with the good ones rather than
        // being deferred. Only spend the SEE call when the move could
        // possibly need it (cheap MVV-LVA filter first). The `is_sac` flag
        // is cached on the entry so the main search loop doesn't have to
        // re-run `is_sacrificial` on the returned move.
        let from = mv.from_sq();
        let to = mv.to_sq();
        if mv.move_type() != EP_CAP
            && TP_VALUE[pos.tp_on_sq(to).index()] < TP_VALUE[pos.tp_on_sq(from).index()]
            && see::see(pos, from, to) < -SAC_THRESHOLD
            && targets_king_or_checks(pos, mv)
        {
            sc += SAC_BONUS;
            is_sac = true;
        }
        entry.score = sc;
        entry.is_sac = is_sac;
    }
}

#[inline]
fn score_quiet(pos: &Position, list: &mut MoveList, history: &[[i32; 64]; 13], ref_sq: i32) {
    for i in 0..list.count {
        // SAFETY: i is bounded by list.count which is always <= MAX_MOVES
        let entry = unsafe { list.moves.get_unchecked_mut(i) };
        let mv = entry.mv;
        let fsq = mv.from_sq();
        let tsq = mv.to_sq();
        // SAFETY: pc index and square are always valid (piece 0-12, sq 0-63)
        let pc_idx = unsafe { *pos.pc.get_unchecked(fsq as usize) }.index();
        let mut sc = unsafe { *history.get_unchecked(pc_idx).get_unchecked(tsq as usize) };
        if fsq == ref_sq {
            sc += 2048;
        }
        // Sacrificial-quiet bonus — promotes a quiet move that loses
        // material by SEE but threatens the enemy king (e.g. a quiet
        // queen lift to the attack zone). `is_sacrificial` fast-fails
        // on the common case (move doesn't attack the king), so SEE
        // is only paid for moves that actually target the king.
        // The `is_sac` flag is cached on the entry so the main search
        // loop can use it without re-running the classifier.
        let is_sac = is_sacrificial(pos, mv);
        if is_sac {
            sc += SAC_QUIET_BONUS;
        }
        entry.score = sc;
        entry.is_sac = is_sac;
    }
}

/// MVV-LVA scoring
#[inline(always)]
pub fn mvv_lva(pos: &Position, mv: Move) -> i32 {
    let tsq = mv.to_sq();
    let fsq = mv.from_sq();

    // SAFETY: tsq and fsq are valid squares (0-63) derived from Move encoding
    if unsafe { *pos.pc.get_unchecked(tsq as usize) } != NO_PC {
        return pos.tp_on_sq(tsq).index() as i32 * 6 + 5 - pos.tp_on_sq(fsq).index() as i32;
    }

    if mv.is_prom() {
        return mv.prom_type().index() as i32 - 5;
    }

    5
}

/// Should this capture be deferred to the bad-capture pool?
///
/// Sacrificial-style captures (SEE < `-SAC_THRESHOLD` that target the
/// enemy king zone or deliver check) are NOT considered bad — they flow
/// through the main capture phase with a `SAC_BONUS` already applied in
/// `score_captures`, so the search tries them at this depth instead of
/// deferring them after every quiet move.
///
/// En-passant captures short-circuit to `false` (non-bad) before the sac
/// check runs, so an EP capture that would classify as sacrificial is
/// treated identically to any other EP and never visits the sac path. EP
/// sacs are too rare to justify extending this gate.
#[inline]
pub fn bad_capture(pos: &Position, mv: Move) -> bool {
    let fsq = mv.from_sq();
    let tsq = mv.to_sq();

    // Equal or winning captures are ok
    if TP_VALUE[pos.tp_on_sq(tsq).index()] >= TP_VALUE[pos.tp_on_sq(fsq).index()] {
        return false;
    }

    // En passant is always ok
    if mv.move_type() == EP_CAP {
        return false;
    }

    let see_val = see::see(pos, fsq, tsq);
    if see_val >= 0 {
        return false;
    }
    // Sacrificial-style losing captures stay in the main pool.
    if see_val < -SAC_THRESHOLD && targets_king_or_checks(pos, mv) {
        return false;
    }
    true
}

// ============================================================================
// Sacrifice classifier
// ============================================================================
//
// Tales is a Tal-style engine: it deliberately wants to find sacrificial
// moves the rest of the search would prune away. The classifier below is
// the shared predicate used by:
//   - move ordering (promote sacs out of the bad-capture pool)
//   - search extensions (extend follow-ups after a sacrificial parent)
//   - pruning relaxation (skip futility/LMP and reduce LMR for sacs)
//
// A move is "sacrificial-looking" when:
//   1) it loses material by SEE (worse than -SAC_THRESHOLD), and
//   2) it either delivers check, or its destination square belongs to the
//      enemy king's attack zone.
//
// The conjunction filters out random pawn pushes that happen to be SEE<0
// (e.g. losing a tempo on the queenside). Pure quiet file/diagonal openers
// like Rd5 that are SEE>=0 are NOT classified — those are caught by eval
// (king tropism, line opening) rather than by the search bias.

/// SEE threshold below which a move is considered to "lose material".
/// 90 cp ≈ slightly less than a minor piece, so capturing a defended pawn
/// with a piece (SEE ≈ −225) qualifies, while a routine SEE = −10 wood
/// shuffle does not.
pub const SAC_THRESHOLD: i32 = 90;

/// Score bonus applied to sacrificial captures in `score_captures` so they
/// sort right after good captures and ahead of all quiet moves. Bigger than
/// any MVV-LVA delta (~35) but smaller than a clean +200 SEE winning
/// capture, so a winning trade still wins the slot.
pub const SAC_BONUS: i32 = 400;

/// Score bonus applied to sacrificial quiet moves in `score_quiet`. Sits
/// below the 2048 refutation bonus and above typical history scores so that
/// a sac quiet outranks ordinary quiets but does not displace a refutation
/// continuation already known to refute the previous move.
pub const SAC_QUIET_BONUS: i32 = 1500;

/// Cheap pre-move "does this move deliver direct check?" test.
///
/// # Scope
///
/// Direct checks only — the move places its piece on a square that attacks
/// the enemy king. This covers the dominant sacrificial family
/// (Bxh7+ / Nxf7+ / Rxg7+ / Qh5+).
///
/// # Known limitations (accepted)
///
/// - **Discovered checks** (a slider behind the moving piece is uncovered)
///   are not detected. Detecting them would require scanning all friendly
///   sliders on rays through `from`, which doubles the classifier cost on
///   the hot path; they are rare in the EPD suite and do not justify it.
/// - **En-passant captures**: the computed `occ_after` removes the from-
///   square and sets the to-square but does NOT remove the captured pawn
///   from its rank (for ep the captured pawn sits on `to ± 8`). A slider
///   check that only appears because the EP pawn has vanished is therefore
///   a false negative. EP sacs are vanishingly rare, so this is accepted.
#[inline]
fn gives_direct_check(pos: &Position, mv: Move) -> bool {
    let from = mv.from_sq();
    let to = mv.to_sq();
    let mover = pos.tp_on_sq(from);
    if mover == NO_TP {
        return false;
    }

    let enemy = !pos.side;
    let king_sq = pos.king_sq(enemy);

    // Approximate post-move occupancy: clear `from`, set `to`. This is
    // correct for ordinary captures and quiet moves. See the function
    // docstring above for the ep-capture corner case we deliberately skip.
    let occ_after = (pos.occ_bb() ^ Bitboard::from_sq(from)) | Bitboard::from_sq(to);

    // The piece that lands on `to` after the move (account for promotions).
    let landing = if mv.is_prom() { mv.prom_type() } else { mover };

    let attack_bb = match landing {
        PieceType::Pawn => attacks::pawn_attacks(pos.side, to),
        PieceType::Knight => attacks::knight_attacks(to),
        PieceType::Bishop => attacks::bishop_attacks(occ_after, to),
        PieceType::Rook => attacks::rook_attacks(occ_after, to),
        PieceType::Queen => attacks::queen_attacks(occ_after, to),
        PieceType::King => attacks::king_attacks(to),
        PieceType::None => return false,
    };
    attack_bb.contains(king_sq)
}

/// "Does this move target the enemy king?" — the (check OR king-zone) leg of
/// the sacrifice classifier, factored out so `bad_capture` and
/// `score_captures` can reuse the cheap predicate without paying a second
/// SEE call once they already know the move loses material.
#[inline]
fn targets_king_or_checks(pos: &Position, mv: Move) -> bool {
    if gives_direct_check(pos, mv) {
        return true;
    }
    let enemy = !pos.side;
    attacks::king_attack_zone(pos.king_sq(enemy), enemy).contains(mv.to_sq())
}

/// Sacrifice classifier — see the section comment above for the
/// motivation and the (SEE-loss) ∧ (target-king) definition.
///
/// Pre-make-move predicate. Cheap by design: the (cheap) king-zone /
/// direct-check test runs first; the SEE call is only paid for the small
/// fraction of moves that actually target the enemy king, so most quiet
/// moves cost only a few bitboard ops.
///
/// Skip list:
///   - Castling: never classified as a sacrifice.
///   - En-passant: rarely sacrificial in practice; we don't fast-path it.
#[inline]
pub fn is_sacrificial(pos: &Position, mv: Move) -> bool {
    if mv.is_none() || mv.move_type() == CASTLE {
        return false;
    }

    // Fast fail: if the move doesn't plausibly attack the enemy king
    // (neither lands in the king zone nor delivers a direct check),
    // it is not a sacrifice. This fast-path keeps the SEE call off the
    // common case in score_quiet where most moves don't target the king.
    if !targets_king_or_checks(pos, mv) {
        return false;
    }

    // Material loss check (SEE-based). A move that wins or breaks even
    // on material isn't a sacrifice even if it attacks the king.
    see::see(pos, mv.from_sq(), mv.to_sq()) < -SAC_THRESHOLD
}

// ============================================================================
// Searcher — per-thread search state (history, killers, refutation, etc.)
// ============================================================================

pub struct Searcher {
    pub history: [[i32; 64]; 13],     // [Piece][Square]
    pub killer: [[Move; 2]; MAX_PLY], // [ply][0/1]
    pub refutation: [[Move; 64]; 64], // [from][to] → refutation move
    pub root_depth: i32,
    pub dp_completed: i32,
    pub seldepth: usize, // max ply reached (selective depth)
    pub has_root_choice: bool,
    pub pv_eng: [Move; 2], // engine's best/ponder moves
    pub nodes: u64,
    pub abort_search: bool,
    pub start_time: std::time::Instant,
    pub time_limit_ms: u64,
    pub move_overhead_ms: u64,   // lag safety buffer
    pub game_key: u64,           // random key per game for eval_blur
    pub nodes_limit: u64,        // node limit from "go nodes" (0 = unlimited)
    pub nps_limit: i32,          // NPS limit for strength clamping
    pub avoid_moves: [Move; 65], // moves to skip in SearchRoot (for MultiPV)
    pub avoid_count: usize,      // number of avoid moves
    pub multi_pv: usize,         // number of PV lines to search
    pub is_pondering: bool,      // true when searching in ponder mode
    pub ponder_time_ms: u64,     // real time limit to apply on ponderhit
    pub ponder_enabled: bool,    // UCI Ponder option — controls bestmove ponder output
    pub silent: bool,            // suppress per-iteration UCI info output (suite/test runner)
}

impl Searcher {
    pub fn new() -> Self {
        Searcher {
            history: [[0; 64]; 13],
            killer: [[Move::NONE; 2]; MAX_PLY],
            refutation: [[Move::NONE; 64]; 64],
            root_depth: 0,
            dp_completed: 0,
            seldepth: 0,
            has_root_choice: false,
            pv_eng: [Move::NONE; 2],
            nodes: 0,
            abort_search: false,
            start_time: std::time::Instant::now(),
            time_limit_ms: u64::MAX,
            move_overhead_ms: 50,
            game_key: 0,
            nodes_limit: 0,
            nps_limit: 0,
            avoid_moves: [Move::NONE; 65],
            avoid_count: 0,
            multi_pv: 1,
            is_pondering: false,
            ponder_time_ms: 0,
            ponder_enabled: false,
            silent: false,
        }
    }

    pub fn clear_all(&mut self) {
        self.history.iter_mut().for_each(|row| row.fill(0));
        self.refutation
            .iter_mut()
            .for_each(|row| row.fill(Move::NONE));
        self.killer.fill([Move::NONE; 2]);
        self.clear_avoid_list();
    }

    pub fn clear_avoid_list(&mut self) {
        self.avoid_moves.fill(Move::NONE);
        self.avoid_count = 0;
    }

    pub fn set_avoid_move(&mut self, mv: Move) {
        if self.avoid_count < 65 {
            self.avoid_moves[self.avoid_count] = mv;
            self.avoid_count += 1;
        }
    }

    pub fn is_avoid_move(&self, mv: Move) -> bool {
        self.avoid_moves[..self.avoid_count].contains(&mv)
    }

    pub fn age_hist(&mut self) {
        self.history.iter_mut().flatten().for_each(|v| *v /= 8);
        self.killer = [[Move::NONE; 2]; MAX_PLY];
    }

    fn trim_hist(&mut self) {
        self.history.iter_mut().flatten().for_each(|v| *v /= 2);
    }

    #[inline]
    pub fn update_history(
        &mut self,
        pos: &Position,
        last_move: Move,
        mv: Move,
        depth: i32,
        ply: usize,
    ) {
        let tsq = mv.to_sq();
        let fsq = mv.from_sq();

        // No update on captures/promotions
        if pos.pc[tsq as usize] != NO_PC || mv.is_prom() || mv.move_type() == EP_CAP {
            return;
        }

        let pc_idx = pos.pc[fsq as usize].index();
        self.history[pc_idx][tsq as usize] += 2 * depth * depth;
        if self.history[pc_idx][tsq as usize] > MAX_HIST {
            self.trim_hist();
        }

        // Update refutation table.
        // Skip when last_move is NONE (null move / root) or SENTINEL (quiescence).
        // Only update counter-moves when a valid previous move exists (0 = null move, -1 = skip).
        if last_move.is_some() && last_move != Move::SENTINEL {
            let lf = last_move.from_sq() as usize;
            let lt = last_move.to_sq() as usize;
            self.refutation[lf][lt] = mv;
        }

        // Update killer moves
        if ply < MAX_PLY && mv != self.killer[ply][0] {
            self.killer[ply][1] = self.killer[ply][0];
            self.killer[ply][0] = mv;
        }
    }

    #[inline]
    pub fn decrease_history(&mut self, pos: &Position, mv: Move, depth: i32) {
        let tsq = mv.to_sq();
        let fsq = mv.from_sq();

        if pos.pc[tsq as usize] != NO_PC || mv.is_prom() || mv.move_type() == EP_CAP {
            return;
        }

        let pc_idx = pos.pc[fsq as usize].index();
        self.history[pc_idx][tsq as usize] -= depth * depth;
        if self.history[pc_idx][tsq as usize] < -MAX_HIST {
            self.trim_hist();
        }
    }

    #[inline(always)]
    pub fn get_refutation(&self, mv: Move) -> Move {
        if mv.is_none() {
            return Move::NONE;
        }
        self.refutation[mv.from_sq() as usize][mv.to_sq() as usize]
    }

    /// Interval (in nodes) between timeout checks.
    const TIMEOUT_CHECK_INTERVAL: u64 = 16383;

    /// Check if we should abort due to time, node limit, or NPS limit.
    #[inline]
    pub fn check_timeout(&mut self) {
        if self.nodes & Self::TIMEOUT_CHECK_INTERVAL == 0 {
            let elapsed = self.start_time.elapsed().as_millis() as u64;

            // Time limit (not enforced while pondering)
            if !self.is_pondering && elapsed >= self.time_limit_ms {
                self.abort_search = true;
                return; // short-circuit: skip remaining checks once we're stopping
            }

            // Poll stdin for "stop"/"quit"/"ponderhit"
            if input_available() {
                let mut cmd = String::new();
                if std::io::stdin().read_line(&mut cmd).is_ok() {
                    let cmd = cmd.trim();
                    if cmd == "stop" || cmd == "quit" {
                        self.abort_search = true;
                    } else if cmd == "ponderhit" {
                        // Transition from ponder to normal search:
                        // apply real time limit, reset start_time
                        self.is_pondering = false;
                        self.start_time = std::time::Instant::now();
                        self.time_limit_ms = self.ponder_time_ms;
                    }
                }
            }

            // Node limit (from "go nodes")
            if self.nodes_limit > 0 && self.nodes >= self.nodes_limit {
                self.abort_search = true;
            }

            // NPS slowdown
            if self.nps_limit > 0 && elapsed > 0 {
                let actual_nps = (self.nodes * 1000) / elapsed;
                if actual_nps > self.nps_limit as u64 {
                    let target_ms = (self.nodes * 1000) / self.nps_limit as u64;
                    let sleep_ms = target_ms.saturating_sub(elapsed);
                    if sleep_ms > 0 {
                        std::thread::sleep(std::time::Duration::from_millis(sleep_ms.min(50)));
                    }
                }
            }
        }
    }
}

/// Non-blocking check if stdin has data available.
/// Check for pending stdin input using platform-specific APIs.
#[cfg(windows)]
fn input_available() -> bool {
    use std::os::windows::io::AsRawHandle;

    #[allow(non_snake_case)]
    unsafe extern "system" {
        fn PeekNamedPipe(
            hNamedPipe: *mut std::ffi::c_void,
            lpBuffer: *mut std::ffi::c_void,
            nBufferSize: u32,
            lpBytesRead: *mut u32,
            lpTotalBytesAvail: *mut u32,
            lpBytesLeftThisMessage: *mut u32,
        ) -> i32;
        fn GetNumberOfConsoleInputEvents(
            hConsoleInput: *mut std::ffi::c_void,
            lpNumberOfEvents: *mut u32,
        ) -> i32;
    }

    let handle = std::io::stdin().as_raw_handle();
    let mut bytes_available: u32 = 0;
    unsafe {
        if PeekNamedPipe(
            handle as *mut _,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut bytes_available,
            std::ptr::null_mut(),
        ) == 0
        {
            // PeekNamedPipe failed — might be a console, not a pipe.
            let mut events: u32 = 0;
            if GetNumberOfConsoleInputEvents(handle as *mut _, &mut events) != 0 {
                return events > 1;
            }
            return false;
        }
    }
    bytes_available > 0
}

/// Non-blocking check if stdin has data available (Unix version).
/// Uses `poll(2)` with a zero timeout on stdin (fd 0).
#[cfg(not(windows))]
fn input_available() -> bool {
    // Inline FFI to avoid adding `libc` as a crate dependency.
    #[repr(C)]
    struct PollFd {
        fd: i32,
        events: i16,
        revents: i16,
    }
    const POLLIN: i16 = 0x0001;

    extern "C" {
        fn poll(fds: *mut PollFd, nfds: u64, timeout: i32) -> i32;
    }

    let mut pfd = PollFd {
        fd: 0, // stdin
        events: POLLIN,
        revents: 0,
    };
    // SAFETY: pfd is a valid stack-allocated struct; nfds=1; timeout=0 is non-blocking.
    let ret = unsafe { poll(&mut pfd, 1, 0) };
    ret > 0 && (pfd.revents & POLLIN) != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::position::Position;

    fn setup() -> Position {
        // Initialize attack tables and PST tables once.
        crate::board::init();
        let par = eval::params::EvalParams::new();
        eval::global_pst::init(&par);
        Position::new()
    }

    #[test]
    fn classic_bxh7_sac_classifies() {
        let mut pos = setup();
        // Standard "Greek gift" position — Bxh7+ wins king for nothing,
        // but SEE is negative because the bishop is uncovered.
        pos.set_position("rnbqk2r/ppp2ppp/3p1n2/4p3/1bB1P3/3P1N2/PPP2PPP/RNBQK2R w KQkq -");
        let mv = pos.str_to_move("c4f7");
        assert!(pos.legal(mv));
        // Bxf7+ is the canonical Italian-game sac; it gives check and SEE is
        // negative (bishop for pawn). Should classify true.
        assert!(
            is_sacrificial(&pos, mv),
            "Bxf7+ should be classified as sacrificial"
        );
    }

    #[test]
    fn quiet_centralizing_knight_does_not_classify() {
        let mut pos = setup();
        pos.set_position("rnbqkbnr/pppppppp/8/8/8/2N5/PPPPPPPP/R1BQKBNR w KQkq -");
        // Nd5 — centralizing, SEE=0, not in king zone — should not classify.
        let mv = pos.str_to_move("c3d5");
        assert!(pos.legal(mv));
        assert!(!is_sacrificial(&pos, mv));
    }

    #[test]
    fn equal_capture_does_not_classify() {
        let mut pos = setup();
        pos.set_position("rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq -");
        // Nxe5 — wins a pawn cleanly; not sacrificial.
        let mv = pos.str_to_move("f3e5");
        assert!(pos.legal(mv));
        assert!(!is_sacrificial(&pos, mv));
    }
}
