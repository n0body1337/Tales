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

//! Tunable evaluation parameters (~200 weights) and derived tables.
//!
//! Derived tables (PST, mobility, passers, backward, danger) are computed by
//! [`EvalParams::recalculate`].

use super::pst;
use crate::board::types::*;

// Special PST indices (for sp_pst)
pub const DEF_MG: usize = 0;
pub const PHA_MG: usize = 1;
pub const DEF_EG: usize = 2;
pub const PHA_EG: usize = 3;

// Crafty-style material imbalance table entries.
// Each variant maps to a tunable EvalParams field (and its negation).
#[derive(Clone, Copy)]
enum Imb {
    Zero, // no adjustment
    Exc,
    NExc, //  a_exc / -a_exc  (exchange advantage)
    Min,
    NMin, //  a_min / -a_min  (minor piece advantage)
    Maj,
    NMaj, //  a_maj / -a_maj  (major piece advantage)
    Two,
    NTwo, //  a_two / -a_two  (two minors vs rook)
    All,
    NAll, //  a_all / -a_all  (combined advantage)
}

use Imb::*;

/// Material imbalance table — indexed by \[major_balance+4\]\[minor_balance+4\].
///
/// Major balance = R_diff + 2·Q_diff, minor balance = N_diff + B_diff.
/// Both clamped to [-4, +4], then offset by +4 to index this 9×9 grid.
///
///   Columns: minor balance  -4 ..  0  .. +4
///   Rows:    major balance  -4 ..  0  .. +4
#[rustfmt::skip]
const IMBALANCE: [[Imb; 9]; 9] = [
    //       -4     -3     -2     -1      0     +1     +2     +3     +4
    [     NAll,  NAll,  NAll,  NAll,  NMaj,  Zero,  Zero,  Zero,  Zero], // -4  major
    [     NAll,  NAll,  NAll,  NAll,  NMaj,  Zero,  Zero,  Zero,  Zero], // -3
    [     NAll,  NAll,  NAll,  NAll,  NMaj,  Zero,  Zero,  Zero,  Zero], // -2
    [     NAll,  NAll,  NAll,  NAll,  NMaj,  NExc,   Two,  Zero,  Zero], // -1
    [     NMin,  NMin,  NMin,  NMin,  Zero,   Min,   Min,   Min,   Min], //  0  even
    [     Zero,  Zero,  NTwo,   Exc,   Maj,   All,   All,   All,   All], // +1
    [     Zero,  Zero,  Zero,  Zero,   Maj,   All,   All,   All,   All], // +2
    [     Zero,  Zero,  Zero,  Zero,   Maj,   All,   All,   All,   All], // +3
    [     Zero,  Zero,  Zero,  Zero,   Maj,   All,   All,   All,   All], // +4  major
];

#[derive(Clone)]
pub struct EvalParams {
    // Piece values
    pub p_mid: i32,
    pub p_end: i32,
    pub n_mid: i32,
    pub n_end: i32,
    pub b_mid: i32,
    pub b_end: i32,
    pub r_mid: i32,
    pub r_end: i32,
    pub q_mid: i32,
    pub q_end: i32,

    // Material adjustments
    pub b_pair: i32,
    pub n_pair: i32,
    pub r_pair: i32,
    pub eleph: i32,
    pub a_exc: i32,
    pub a_min: i32,
    pub a_maj: i32,
    pub a_two: i32,
    pub a_all: i32,
    pub n_cl: i32,
    pub r_op: i32,

    // King attack values
    pub n_att1: i32,
    pub n_att2: i32,
    pub b_att1: i32,
    pub b_att2: i32,
    pub r_att1: i32,
    pub r_att2: i32,
    pub q_att1: i32,
    pub q_att2: i32,
    pub n_chk: i32,
    pub b_chk: i32,
    pub r_chk: i32,
    pub q_chk: i32,
    pub r_contact: i32,
    pub q_contact: i32,

    // King tropism
    pub ntr_mg: i32,
    pub ntr_eg: i32,
    pub btr_mg: i32,
    pub btr_eg: i32,
    pub rtr_mg: i32,
    pub rtr_eg: i32,
    pub qtr_mg: i32,
    pub qtr_eg: i32,

    // Piece parameters
    pub n_fwd: i32,
    pub b_fwd: i32,
    pub r_fwd: i32,
    pub q_fwd: i32,
    pub n_owh: i32,
    pub b_owh: i32,
    pub n_reach: i32,
    pub b_reach: i32,
    pub bn_shield: i32,
    pub n_trap: i32,
    pub n_block: i32,
    pub b_trap_a2: i32,
    pub b_trap_a3: i32,
    pub b_block: i32,
    pub b_fianch: i32,
    pub b_badf: i32,
    pub b_king: i32,
    pub b_bf_mg: i32,
    pub b_bf_eg: i32,
    pub b_wing: i32,
    pub b_own_p: i32,
    pub b_opp_p: i32,
    pub b_touch: i32,
    pub b_return: i32,

    // Rook/queen parameters
    pub rsr_mg: i32,
    pub rsr_eg: i32,
    pub rs2_mg: i32,
    pub rs2_eg: i32,
    pub rof_mg: i32,
    pub rof_eg: i32,
    pub rgh_mg: i32,
    pub rgh_eg: i32,
    pub rbh_mg: i32,
    pub rbh_eg: i32,
    pub roq_mg: i32,
    pub roq_eg: i32,
    pub r_block_mg: i32,
    pub r_block_eg: i32,
    pub qsr_mg: i32,
    pub qsr_eg: i32,

    // King parameters
    pub k_no_luft: i32,
    pub k_castle: i32,

    // Tempo bonus
    pub tempo_mg: i32,
    pub tempo_eg: i32,

    // Pawn structure
    pub db_mid: i32,
    pub db_end: i32,
    pub iso_mg: i32,
    pub iso_eg: i32,
    pub iso_of: i32,
    pub bk_mid: i32,
    pub bk_end: i32,
    pub bk_ope: i32,
    pub p_bind: i32,
    pub p_badbind: i32,
    pub p_isl: i32,
    pub p_thr: i32,

    // Pawn chain
    pub p_bigchain: i32,
    pub p_smallchain: i32,
    pub p_cs1: i32,
    pub p_cs2: i32,
    pub p_csfail: i32,

    // Pawn shield
    pub p_sh_none: i32,
    pub p_sh_2: i32,
    pub p_sh_3: i32,
    pub p_sh_4: i32,
    pub p_sh_5: i32,
    pub p_sh_6: i32,
    pub p_sh_7: i32,
    pub p_st_open: i32,
    pub p_st_3: i32,
    pub p_st_4: i32,
    pub p_st_5: i32,

    // Passed/candidate pawn bonuses per rank
    pub passed_bonus_mg: [[i32; 8]; 2],
    pub passed_bonus_eg: [[i32; 8]; 2],
    pub cand_bonus_mg: [[i32; 8]; 2],
    pub cand_bonus_eg: [[i32; 8]; 2],
    pub p_bl_mul: i32,
    pub p_ourstop_mul: i32,
    pub p_oppstop_mul: i32,
    pub p_defmul: i32,
    pub p_stopmul: i32,

    // Weights
    pub w_material: i32,
    pub w_pst: i32,

    pub w_threats: i32,
    pub w_tropism: i32,
    pub w_fwd: i32,
    pub w_passers: i32,
    pub w_mass: i32,
    pub w_chains: i32,
    pub w_outposts: i32,
    pub w_lines: i32,
    pub w_struct: i32,
    pub w_shield: i32,
    pub w_storm: i32,
    pub w_center: i32,

    // Derived tables
    pub mg_pst: [[[i32; 64]; 6]; 2],
    pub eg_pst: [[[i32; 64]; 6]; 2],
    pub sp_pst: [[[i32; 64]; 6]; 2],
    pub n_mob_mg: [i32; 9],
    pub n_mob_eg: [i32; 9],
    pub b_mob_mg: [i32; 16],
    pub b_mob_eg: [i32; 16],
    pub r_mob_mg: [i32; 16],
    pub r_mob_eg: [i32; 16],
    pub q_mob_mg: [i32; 32],
    pub q_mob_eg: [i32; 32],
    pub danger: [i32; 512],
    pub np_table: [i32; 9],
    pub rp_table: [i32; 9],
    pub backward_malus_mg: [i32; 8],
    pub imbalance: [[i32; 9]; 9],

    // Per-side asymmetric weights
    pub sd_att: [i32; 2],
    pub sd_mob: [i32; 2],
    pub prog_side: Color,
    pub keep_pc: [i32; 7],

    // Search-related
    pub draw_score: i32,
    pub eval_blur: i32,
    pub hist_perc: i32,
    pub hist_limit: i32,

    // Strength-limiting
    pub nps_limit: i32,
    pub time_percentage: i32,
    pub is_weakening: bool,
    pub elo: i32,
}

impl Default for EvalParams {
    fn default() -> Self {
        let mut p = Self::default_weights();
        p.recalculate();
        p.init_tables();
        p
    }
}

impl EvalParams {
    pub fn new() -> Self {
        Self::default()
    }

    fn default_weights() -> Self {
        EvalParams {
            // Piece values — middlegame and endgame centipawn values.
            // Pawns anchor at 100cp; minor pieces ~325–340cp; rook ~500cp; queen ~950cp.
            p_mid: 100,
            p_end: 101,
            n_mid: 325,
            n_end: 320,
            b_mid: 340,
            b_end: 340,
            r_mid: 500,
            r_end: 505,
            q_mid: 950,
            q_end: 960,

            // Material adjustments — bonuses/penalties for piece combinations.
            b_pair: 70,  // bishop pair bonus
            n_pair: -10, // two-knight penalty
            r_pair: -9,  // two-rook redundancy penalty
            eleph: 4,    // queen devaluation per enemy minor on the board
            a_exc: -10,  // exchange advantage adjustment
            a_min: 53,   // bonus for minor piece advantage
            a_maj: 60,   // bonus for major piece advantage
            a_two: 44,   // bonus for two minors vs rook
            a_all: 80,   // bonus for combined major + minor advantage
            n_cl: 6,     // knight bonus per own pawn (prefers closed positions)
            r_op: 3,     // rook penalty per own pawn (prefers open positions)

            // King attack accumulator values — NOT direct bonuses.
            // These accumulate into an index for the non-linear danger[] table.
            // ATT1 = attack on squares undefended by enemy pawns
            // ATT2 = attack on squares defended by enemy pawns
            // CHK  = threatening check to enemy king
            // CONTACT = contact check threats (piece adjacent to king)
            n_att1: 9,    // Tal: 6 → 9 (1.5x — bumps weight of attackers in king zone)
            n_att2: 4,    // Tal: 3 → 4
            b_att1: 9,    // Tal: 6 → 9
            b_att2: 3,    // Tal: 2 → 3
            r_att1: 13,   // Tal: 9 → 13
            r_att2: 6,    // Tal: 4 → 6
            q_att1: 22,   // Tal: 16 → 22
            q_att2: 7,    // Tal: 5 → 7
            n_chk: 6,     // Tal: 4 → 6 (knight check threats more dangerous)
            b_chk: 8,     // Tal: 6 → 8
            r_chk: 15,    // Tal: 11 → 15
            q_chk: 16,    // Tal: 12 → 16
            r_contact: 24,
            q_contact: 36,

            // King tropism — bonus per unit of proximity to enemy king.
            // Higher values pull pieces toward the opponent's king.
            ntr_mg: 3,
            ntr_eg: 3,
            btr_mg: 2,
            btr_eg: 1,
            rtr_mg: 2,
            rtr_eg: 1,
            qtr_mg: 2,
            qtr_eg: 4,

            // Piece placement parameters
            n_fwd: 1,        // knight forwardness weight
            b_fwd: 1,        // bishop forwardness weight
            r_fwd: 2,        // rook forwardness weight
            q_fwd: 4,        // queen forwardness weight
            n_owh: -1,       // knight restricted to own half penalty
            b_owh: -7,       // bishop restricted to own half penalty
            n_reach: 11,     // knight can reach an outpost square
            b_reach: 2,      // bishop can reach an outpost square
            bn_shield: 5,    // minor piece shielded by own pawn
            n_trap: -168,    // trapped knight (e.g. Na7 with pawns a6+b7)
            n_block: -17,    // knight blocks c-pawn in queen-pawn openings
            b_trap_a2: -138, // bishop trapped on a2/a7 (or mirrored)
            b_trap_a3: -45,  // bishop trapped on a3/a6 (or mirrored)
            b_block: -45,    // blocked development (bishop behind own d2/e2 pawn)
            b_fianch: 4,     // fianchettoed bishop bonus
            b_badf: -27,     // enemy pawns hamper fianchettoed bishop
            b_king: 8,       // fianchettoed bishop near own castled king
            b_bf_mg: -12,    // fianchettoed bishop blocked by own pawn (e.g. Bg2 + Pf3)
            b_bf_eg: -20,
            b_wing: 3,    // bishop on expected wing (e.g. Pe4 with Bc5/Bb5/Ba4)
            b_own_p: -3,  // own pawn on square matching bishop's color
            b_opp_p: -1,  // enemy pawn on square matching bishop's color
            b_touch: 5,   // two bishops on adjacent squares
            b_return: 10, // bishop returning to initial square after castling

            // Rook and queen file/rank parameters
            rsr_mg: 16, // rook on 7th rank (middlegame)
            rsr_eg: 32, // rook on 7th rank (endgame)
            rs2_mg: 20, // bonus for two rooks on 7th rank
            rs2_eg: 31,
            rof_mg: 30, // rook on open file
            rof_eg: 2,
            rgh_mg: 15, // rook on half-open file, undefended enemy pawn
            rgh_eg: 20,
            rbh_mg: 0, // rook on half-open file, defended enemy pawn
            rbh_eg: 0,
            roq_mg: 9, // rook and queen on the same file
            roq_eg: 18,
            r_block_mg: -50, // rook blocked by own king (hasn't castled)
            r_block_eg: 0,
            qsr_mg: 0, // queen on 7th rank
            qsr_eg: 2,

            // King safety patterns
            k_no_luft: -11, // king boxed in with no pawn luft
            k_castle: 32,   // castling rights bonus

            // Tempo bonus — side-to-move advantage
            tempo_mg: 14,
            tempo_eg: 7,

            // Pawn structure penalties/bonuses
            db_mid: -12,   // doubled pawn (middlegame)
            db_end: -24,   // doubled pawn (endgame)
            iso_mg: -10,   // isolated pawn (middlegame)
            iso_eg: -20,   // isolated pawn (endgame)
            iso_of: -10,   // extra penalty for isolated pawn on open file
            bk_mid: -8,    // backward pawn (middlegame)
            bk_end: -10,   // backward pawn (endgame)
            bk_ope: -8,    // extra penalty for backward pawn on open file
            p_bind: 5,     // two pawns control a central square
            p_badbind: 10, // wing triangle penalty (e.g. a4-b3-c4)
            p_isl: 7,      // penalty per pawn island
            p_thr: 4,      // pawn advance threatens enemy minor

            // Pawn chain evaluation — penalty for enemy chain pointing at castled king
            p_bigchain: 38,   // compact chain fully blocked by own pawns
            p_smallchain: 27, // chain not fully blocked
            p_cs1: 12,        // pawn storm bonus next to fixed chain (e.g. g5 in KID)
            p_cs2: 3,         // secondary storm bonus (e.g. g4 in KID)
            p_csfail: 32,     // penalty for misplayed pawn storm next to chain

            // King's pawn shield — penalty by rank of the shielding pawn
            p_sh_none: -40, // no shelter pawn at all
            p_sh_2: 2,      // shelter pawn on 2nd rank (home)
            p_sh_3: -6,     // shelter pawn advanced to 3rd rank
            p_sh_4: -15,    // 4th rank
            p_sh_5: -23,    // 5th rank
            p_sh_6: -24,    // 6th rank
            p_sh_7: -35,    // 7th rank

            // Pawn storm — penalty by rank of approaching enemy pawn
            p_st_open: -6, // open storm file (no enemy pawn)
            p_st_3: -16,   // enemy pawn on 3rd rank
            p_st_4: -16,   // enemy pawn on 4th rank
            p_st_5: -3,    // enemy pawn on 5th rank

            // Passed and candidate pawn bonuses (populated by init_passers())
            passed_bonus_mg: [[0; 8]; 2],
            passed_bonus_eg: [[0; 8]; 2],
            cand_bonus_mg: [[0; 8]; 2],
            cand_bonus_eg: [[0; 8]; 2],
            p_bl_mul: 42,      // blocked passer penalty multiplier
            p_ourstop_mul: 27, // bonus: side with passer controls stop square
            p_oppstop_mul: 29, // penalty: opponent controls passer's stop square
            p_defmul: 6,       // bonus: passer defended by own pawn
            p_stopmul: 6,      // bonus: stop square defended by own pawn

            // Global evaluation weights — percentage multipliers (100 = neutral).
            // These scale entire evaluation components up or down.
            w_material: 48, // material counting weight
            w_pst: 100,     // piece-square table weight

            w_threats: 230,  // piece pressure / hanging piece threats (Tal: 190 → 230)
            w_tropism: 100,  // king tropism (Tal: 80 → 100)
            w_fwd: 0,        // forwardness bonus
            w_passers: 127,  // passed pawn evaluation
            w_mass: 100,     // pawn mass (phalanx + defended pawns)
            w_chains: 100,   // pawn chain evaluation
            w_outposts: 100, // knight/bishop outpost bonuses
            w_lines: 100,    // rook/queen on open files and 7th rank
            w_struct: 90,    // pawn structure (doubled/isolated/backward)
            w_shield: 189,   // king pawn shield
            w_storm: 181,    // pawn storm toward enemy king
            w_center: 50,    // central square control

            // Derived tables (populated by recalculate() and init_tables())
            mg_pst: [[[0; 64]; 6]; 2],
            eg_pst: [[[0; 64]; 6]; 2],
            sp_pst: [[[0; 64]; 6]; 2],
            n_mob_mg: [0; 9],
            n_mob_eg: [0; 9],
            b_mob_mg: [0; 16],
            b_mob_eg: [0; 16],
            r_mob_mg: [0; 16],
            r_mob_eg: [0; 16],
            q_mob_mg: [0; 32],
            q_mob_eg: [0; 32],
            danger: [0; 512],
            np_table: [0; 9],
            rp_table: [0; 9],
            backward_malus_mg: [0; 8],
            imbalance: [[0; 9]; 9],

            // Asymmetric side weights — aggressive own-attack scaling.
            // OWN attack weight (450) is much higher than OPP (100),
            // making the engine strongly prefer attacking positions.
            sd_att: [450, 100],
            sd_mob: [125, 100],
            prog_side: WC,
            // Piece-keeping tendency [P, N, B, R, Q, K, K+1]
            // Higher values = stronger reluctance to trade that piece type.
            keep_pc: [8, 10, 10, 0, 20, 0, 0],

            // Search and strength-limiting
            draw_score: 25,    // contempt: aggressively avoid draws (Tal style)
            eval_blur: 0,      // evaluation noise for strength limiting
            hist_perc: 175,    // LMR aggressiveness (history percentage)
            hist_limit: 24576, // LMR history threshold

            nps_limit: 0,
            time_percentage: 95,
            is_weakening: false,
            elo: 2800,
        }
    }

    /// Set asymmetric parameters based on the engine's side to move.
    /// Swaps `sd_att`/`sd_mob` so the engine always weights its own
    /// attacks/mobility more than the opponent's.
    /// OWN defaults: att=450, mob=125. OPP defaults: att=100, mob=100.
    pub fn init_asymmetric(&mut self, side: Color) {
        self.prog_side = side;
        if side == WC {
            self.sd_att = [450, 100]; // [WC]=OWN, [BC]=OPP
            self.sd_mob = [125, 100];
        } else {
            self.sd_att = [100, 450]; // [WC]=OPP, [BC]=OWN
            self.sd_mob = [100, 125];
        }
    }

    pub fn recalculate(&mut self) {
        self.init_pst();
        self.init_mobility();
        self.init_material_tweaks();
        self.init_backward();
        self.init_passers();
    }

    fn init_pst(&mut self) {
        let raw_tables_mg = [
            &pst::PST_PAWN_MG,
            &pst::PST_KNIGHT_MG,
            &pst::PST_BISHOP_MG,
            &pst::PST_ROOK_MG,
            &pst::PST_QUEEN_MG,
            &pst::PST_KING_MG,
        ];
        let raw_tables_eg = [
            &pst::PST_PAWN_EG,
            &pst::PST_KNIGHT_EG,
            &pst::PST_BISHOP_EG,
            &pst::PST_ROOK_EG,
            &pst::PST_QUEEN_EG,
            &pst::PST_KING_EG,
        ];
        let piece_val_mg = [
            self.p_mid, self.n_mid, self.b_mid, self.r_mid, self.q_mid, 0,
        ];
        let piece_val_eg = [
            self.p_end, self.n_end, self.b_end, self.r_end, self.q_end, 0,
        ];

        for sq in 0..64 {
            for sd_idx in 0..2 {
                let sd = if sd_idx == 0 { WC } else { BC };
                let rsq = sd.rel_sq(sq as i32) as usize;

                for pt in 0..6 {
                    self.mg_pst[sd_idx][pt][rsq] = (piece_val_mg[pt] * self.w_material) / 100
                        + (raw_tables_mg[pt][sq] * self.w_pst) / 100;
                    self.eg_pst[sd_idx][pt][rsq] = (piece_val_eg[pt] * self.w_material) / 100
                        + (raw_tables_eg[pt][sq] * self.w_pst) / 100;
                }

                // King has no piece value component
                self.mg_pst[sd_idx][K.index()][rsq] = (pst::PST_KING_MG[sq] * self.w_pst) / 100;
                self.eg_pst[sd_idx][K.index()][rsq] = (pst::PST_KING_EG[sq] * self.w_pst) / 100;

                // Special PST (outposts, pawn formations)
                self.sp_pst[sd_idx][N.index()][rsq] = pst::PST_KNIGHT_OUTPOST[sq];
                self.sp_pst[sd_idx][B.index()][rsq] = pst::PST_BISHOP_OUTPOST[sq];
                self.sp_pst[sd_idx][DEF_MG][rsq] = pst::PST_DEFENDED_PAWN_MG[sq];
                self.sp_pst[sd_idx][PHA_MG][rsq] = pst::PST_PHALANX_PAWN_MG[sq];
                self.sp_pst[sd_idx][DEF_EG][rsq] = pst::PST_DEFENDED_PAWN_EG[sq];
                self.sp_pst[sd_idx][PHA_EG][rsq] = pst::PST_PHALANX_PAWN_EG[sq];
            }
        }
    }

    fn init_mobility(&mut self) {
        for i in 0..9 {
            self.n_mob_mg[i] = 4 * (i as i32 - 4);
            self.n_mob_eg[i] = 4 * (i as i32 - 4);
        }
        for i in 0..14 {
            self.b_mob_mg[i] = 5 * (i as i32 - 6);
            self.b_mob_eg[i] = 5 * (i as i32 - 6);
        }
        for i in 0..15 {
            self.r_mob_mg[i] = 2 * (i as i32 - 7);
            self.r_mob_eg[i] = 4 * (i as i32 - 7);
        }
        for i in 0..28 {
            self.q_mob_mg[i] = i as i32 - 14;
            self.q_mob_eg[i] = 2 * (i as i32 - 14);
        }
    }

    fn init_material_tweaks(&mut self) {
        for i in 0..9 {
            self.np_table[i] = pst::ADJ[i] * self.n_cl;
            self.rp_table[i] = pst::ADJ[i] * self.r_op;
        }

        for (i, row) in IMBALANCE.iter().enumerate() {
            for (j, &entry) in row.iter().enumerate() {
                self.imbalance[i][j] = match entry {
                    Zero => 0,
                    Exc => self.a_exc,
                    NExc => -self.a_exc,
                    Min => self.a_min,
                    NMin => -self.a_min,
                    Maj => self.a_maj,
                    NMaj => -self.a_maj,
                    Two => self.a_two,
                    NTwo => -self.a_two,
                    All => self.a_all,
                    NAll => -self.a_all,
                };
            }
        }
    }

    fn init_backward(&mut self) {
        self.backward_malus_mg[0] = self.bk_mid + 3; // FILE_A
        self.backward_malus_mg[1] = self.bk_mid + 1;
        self.backward_malus_mg[2] = self.bk_mid - 1;
        self.backward_malus_mg[3] = self.bk_mid - 3;
        self.backward_malus_mg[4] = self.bk_mid - 3;
        self.backward_malus_mg[5] = self.bk_mid - 1;
        self.backward_malus_mg[6] = self.bk_mid + 1;
        self.backward_malus_mg[7] = self.bk_mid + 3; // FILE_H
    }

    fn init_passers(&mut self) {
        let pmg = [0, 2, 2, 11, 33, 71, 135, 0];
        let peg = [0, 12, 21, 48, 93, 161, 266, 0];
        // Candidates: passer/3
        let cmg = [0, 0, 0, 3, 11, 23, 45, 0];
        let ceg = [0, 4, 7, 16, 31, 53, 88, 0];

        for rank in 0..8 {
            self.passed_bonus_mg[0][rank] = pmg[rank]; // White
            self.passed_bonus_mg[1][7 - rank] = pmg[rank]; // Black mirrored
            self.passed_bonus_eg[0][rank] = peg[rank];
            self.passed_bonus_eg[1][7 - rank] = peg[rank];
            self.cand_bonus_mg[0][rank] = cmg[rank];
            self.cand_bonus_mg[1][7 - rank] = cmg[rank];
            self.cand_bonus_eg[0][rank] = ceg[rank];
            self.cand_bonus_eg[1][7 - rank] = ceg[rank];
        }
    }

    pub fn init_tables(&mut self) {
        // King attack danger table — non-linear mapping from accumulated
        // attack score (0..510) to centipawn danger value.
        // Coefficient 0.027 controls curve steepness; +8.0 caps per-step growth.
        self.danger[0] = 0;
        let mut t: f64 = 0.0;
        for i in 1..511 {
            t = (1280.0_f64).min((0.027 * (i as f64) * (i as f64)).min(t + 8.0));
            self.danger[i] = ((t * 100.0) / 256.0) as i32;
        }
    }

    // ========================================================================
    // Strength-limiting — SetSpeed / EloToBlur / EloToSpeed
    // ========================================================================

    /// Reconfigure weakening params from current Elo setting.
    pub fn set_speed(&mut self) {
        self.nps_limit = 0;
        self.eval_blur = 0;

        if self.is_weakening {
            self.nps_limit = Self::elo_to_speed(self.elo);
            self.eval_blur = Self::elo_to_blur(self.elo);
        }
    }

    /// Convert UCI_Elo to NPS limit .
    fn elo_to_speed(elo_in: i32) -> i32 {
        let lower = elo_in - 25;
        let upper = elo_in + 25;
        // Deterministic approximation (mid-range) instead of rand()
        let use_rating = i32::midpoint(lower, upper);
        let base: f64 = 1.0069555500567_f64;
        let exponent = ((use_rating as f64 / 1200.0) - 1.0) + (use_rating as f64 - 1200.0);
        let search_nodes = (base.powf(exponent) * 128.0) as i32;
        (search_nodes / 7) + (elo_in / 60)
    }

    /// Convert UCI_Elo to eval blur .
    pub fn elo_to_blur(elo_in: i32) -> i32 {
        if elo_in < 1500 {
            (1500 - elo_in) / 4
        } else {
            0
        }
    }
}
