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

// Threat evaluation — evaluates threats from minor/major pieces.

use super::eval_data::{self, EvalData};
use super::params::EvalParams;
use super::pst;

use crate::board::position::Position;
use crate::board::types::*;

pub fn evaluate_threats(p: &Position, e: &mut EvalData, par: &EvalParams, sd: Color) {
    let op = !sd;
    let si = sd.index();
    let oi = op.index();
    let mut mg = 0;
    let mut eg = 0;

    let mut bb_undefended = p.cl_bb[oi];
    let bb_threatened = bb_undefended & e.p_takes[si];
    let bb_defended = bb_undefended & e.all_att[oi];
    let mut bb_hanging = bb_undefended & !e.p_takes[oi];

    bb_undefended &= !e.all_att[si];
    bb_undefended &= !e.all_att[oi];

    bb_hanging |= bb_threatened;
    bb_hanging &= e.all_att[si];

    let mut bb_defended_att = bb_defended & e.ev_att[si];
    bb_defended_att &= !e.p_takes[si];

    // Hanging pieces
    let mut bb = bb_hanging;
    while bb.is_not_empty() {
        let sq = bb.pop_lsb();
        let pc = p.tp_on_sq(sq).index();
        mg += pst::ATT_ON_HANG_MG[pc];
        eg += pst::ATT_ON_HANG_EG[pc];
    }

    // Defended pieces under attack
    bb = bb_defended_att;
    while bb.is_not_empty() {
        let sq = bb.pop_lsb();
        let pc = p.tp_on_sq(sq).index();
        mg += pst::ATT_ON_DEF_MG[pc];
        eg += pst::ATT_ON_DEF_EG[pc];
    }

    // Unattacked and undefended
    bb = bb_undefended;
    while bb.is_not_empty() {
        let sq = bb.pop_lsb();
        let pc = p.tp_on_sq(sq).index();
        mg += pst::UNATT_UNDEF_MG[pc];
        eg += pst::UNATT_UNDEF_EG[pc];
    }

    eval_data::add(
        e,
        sd,
        (par.w_threats * mg) / 100,
        (par.w_threats * eg) / 100,
    );
}
