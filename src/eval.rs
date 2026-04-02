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

//! Evaluation module — static position evaluation with incremental PST updates.

pub mod endgame;
pub mod eval_data;
pub mod global_pst;
pub mod king_safety;
pub mod material;
pub mod params;
pub mod passers;
pub mod patterns;
pub mod pawn_hash;
pub mod pawns;
pub mod pieces;
pub mod pst;
pub mod threats;

use crate::board::position::Position;
use crate::board::types::*;
use eval_data::EvalData;
use params::EvalParams;

// MAX_EVAL is imported from board::types via glob.
pub const EVAL_HASH_SIZE: usize = 1 << 16; // 65536 entries
const EVAL_HASH_MASK: usize = EVAL_HASH_SIZE - 1;

#[derive(Clone)]
#[repr(C)]
pub struct EvalHashEntry {
    pub key: u64,
    pub score: i32,
}

/// Main evaluation function — computes the position score.
pub fn evaluate(
    p: &Position,
    par: &EvalParams,
    eval_tt: &mut [EvalHashEntry],
    pawn_tt: &mut pawn_hash::PawnHash,
    game_key: u64,
) -> i32 {
    // Try eval hash
    let addr = (p.hash_key as usize) & EVAL_HASH_MASK;
    let entry = &eval_tt[addr];
    if entry.key == p.hash_key {
        let sc = entry.score;
        return if p.side == WC { sc } else { -sc };
    }

    let mut e = EvalData::new();

    // Init from incremental PST scores
    e.mg[WC.index()] = p.mg_sc[WC.index()];
    e.mg[BC.index()] = p.mg_sc[BC.index()];
    e.eg[WC.index()] = p.eg_sc[WC.index()];
    e.eg[BC.index()] = p.eg_sc[BC.index()];

    // Init pawn helper bitboards
    e.init_pawn_data(p);

    // Run all evaluation subroutines
    material::evaluate_material(p, &mut e, par, WC);
    material::evaluate_material(p, &mut e, par, BC);
    pieces::evaluate_pieces(p, &mut e, par, WC);
    pieces::evaluate_pieces(p, &mut e, par, BC);
    pawns::evaluate_pawn_struct(p, &mut e, par, pawn_tt);
    passers::evaluate_passers(p, &mut e, par, WC);
    passers::evaluate_passers(p, &mut e, par, BC);
    passers::evaluate_unstoppable(&mut e, p);
    threats::evaluate_threats(p, &mut e, par, WC);
    threats::evaluate_threats(p, &mut e, par, BC);

    // Tempo bonus
    e.add(p.side, par.tempo_mg, par.tempo_eg);

    // Patterns
    patterns::evaluate_knight_patterns(p, &mut e, par);
    patterns::evaluate_bishop_patterns(p, &mut e, par);
    patterns::evaluate_king_patterns(p, &mut e, par);
    patterns::evaluate_central_patterns(p, &mut e, par);

    // King attack
    king_safety::evaluate_king_attack(p, &mut e, par, WC);
    king_safety::evaluate_king_attack(p, &mut e, par, BC);

    // Add pawn scores
    e.mg[WC.index()] += e.mg_pawns[WC.index()];
    e.mg[BC.index()] += e.mg_pawns[BC.index()];
    e.eg[WC.index()] += e.eg_pawns[WC.index()];
    e.eg[BC.index()] += e.eg_pawns[BC.index()];

    // Asymmetric piece-keeping bonus
    let ps = par.prog_side;
    let psi = ps.index();
    e.mg[psi] += par.keep_pc[Q.index()] * p.count(ps, Q);
    e.mg[psi] += par.keep_pc[R.index()] * p.count(ps, R);
    e.mg[psi] += par.keep_pc[B.index()] * p.count(ps, B);
    e.mg[psi] += par.keep_pc[N.index()] * p.count(ps, N);
    e.mg[psi] += par.keep_pc[P.index()] * p.count(ps, P);

    // Interpolate
    let mut score = interpolate(p, &e);

    // Material imbalance (Crafty-based)
    let minor_balance = p.count(WC, N) - p.count(BC, N) + p.count(WC, B) - p.count(BC, B);
    let major_balance = p.count(WC, R) - p.count(BC, R) + 2 * p.count(WC, Q) - 2 * p.count(BC, Q);

    let x = (major_balance + 4).clamp(0, 8) as usize;
    let y = (minor_balance + 4).clamp(0, 8) as usize;
    score += par.imbalance[x][y];

    // Weakening: add pseudo-random value to eval score for strength-limiting
    if par.eval_blur > 0 {
        let blur = par.eval_blur as u64;
        let rand_mod = (par.eval_blur / 2) - ((p.hash_key ^ game_key) % blur) as i32;
        score += rand_mod;
    }

    // KBN vs K helper
    score += endgame::checkmate_helper(p, par);

    // Draw factor — scale score toward zero in drawish endgames
    let draw_factor = match score.signum() {
        1 => endgame::get_draw_factor(p, WC),
        -1 => endgame::get_draw_factor(p, BC),
        _ => 64,
    };
    score = (score * draw_factor) / 64;

    // Clamp
    score = score.clamp(-MAX_EVAL, MAX_EVAL);

    // Save to eval hash
    let entry = &mut eval_tt[addr];
    entry.key = p.hash_key;
    entry.score = score;

    if p.side == WC { score } else { -score }
}

fn interpolate(p: &Position, e: &EvalData) -> i32 {
    let mg_tot = e.mg[WC.index()] - e.mg[BC.index()];
    let eg_tot = e.eg[WC.index()] - e.eg[BC.index()];
    let mg_phase = p.phase.min(24);
    let eg_phase = 24 - mg_phase;
    (mg_tot * mg_phase + eg_tot * eg_phase) / 24
}

/// Create a properly sized eval hash table.
pub fn new_eval_hash() -> Vec<EvalHashEntry> {
    vec![EvalHashEntry { key: 0, score: 0 }; EVAL_HASH_SIZE]
}
