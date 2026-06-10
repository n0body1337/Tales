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

//! EPD test-suite runner — parses an EPD file, runs a fixed-time search per
//! position with the opening book disabled, and reports pass rate against
//! the `bm` (best-move) opcode.
//!
//! Invocation: `tales --suite <path> --time <ms> [--hash <mb>] [--verbose] [--csv <path>]`
//!
//! The runner deliberately does NOT go through `parse_go`, so neither the
//! internal nor the external opening book is ever probed during the suite —
//! a book hit on any position would short-circuit the search and produce
//! false-positive (or false-negative) results.

use std::time::Instant;

use crate::board::moves::*;
use crate::board::position::{Position, Undo};
use crate::board::types::*;
use crate::eval;
use crate::movegen::generate;
use crate::movegen::movelist::MoveList;
use crate::search;
use crate::tt::TransTable;

// ============================================================================
// EPD record
// ============================================================================

/// One parsed EPD line.
struct EpdRecord {
    /// FEN portion (the first 4 fields, plus optional clocks).
    fen: String,
    /// `bm` (best move) target, in SAN, with annotations stripped.
    bm: String,
    /// `id` opcode value (for reporting). Empty if absent.
    id: String,
}

// ============================================================================
// EPD parser
// ============================================================================

/// Strip trailing PGN/SAN annotations (`+`, `#`, `!`, `?`, `!!`, `??`, `!?`, `?!`)
/// so two SAN strings can be compared by piece+square+disambig only.
fn strip_san_annotations(s: &str) -> String {
    s.trim_end_matches(['+', '#', '!', '?']).to_string()
}

/// Parse a single EPD line. Returns `None` for blank/comment lines.
///
/// Format: `<piece-placement> <stm> <castling> <ep> [opcode value; ...]`
/// Recognized opcodes: `bm`, `id`. Quoted values supported for `id`/`c0`.
fn parse_epd_line(line: &str) -> Option<EpdRecord> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }

    // Split into FEN and opcodes at the 4th space (after EP square).
    let mut spaces = 0;
    let mut split_at = 0;
    for (i, ch) in line.char_indices() {
        if ch == ' ' {
            spaces += 1;
            if spaces == 4 {
                split_at = i;
                break;
            }
        }
    }
    if split_at == 0 {
        return None;
    }

    let fen = line[..split_at].to_string();
    let rest = line[split_at + 1..].trim();

    // Tokenize opcodes: each ends with a `;`. Values may be quoted.
    let mut bm = String::new();
    let mut id = String::new();
    for opc in rest.split(';') {
        let opc = opc.trim();
        if opc.is_empty() {
            continue;
        }
        // Split on the first whitespace into (name, value).
        let (name, value) = match opc.split_once(char::is_whitespace) {
            Some((n, v)) => (n, v.trim()),
            None => (opc, ""),
        };
        // Strip surrounding quotes if any.
        let value = value.trim_matches('"');
        match name {
            "bm" => bm = strip_san_annotations(value.split_whitespace().next().unwrap_or("")),
            "id" => id = value.to_string(),
            _ => {}
        }
    }

    if bm.is_empty() {
        return None;
    }
    Some(EpdRecord { fen, bm, id })
}

// ============================================================================
// SAN converter — `Move` ↔ algebraic notation
// ============================================================================

/// Build a list of fully-legal moves at the current position (captures + quiet).
fn legal_moves(pos: &mut Position) -> Vec<Move> {
    let mut list = MoveList::new();
    generate::generate_captures(pos, &mut list);
    generate::generate_quiet(pos, &mut list);
    let mut out = Vec::with_capacity(list.count);
    for i in 0..list.count {
        let mv = list.get(i);
        let mut u = Undo::new();
        pos.do_move(mv, &mut u);
        let illegal = pos.illegal();
        pos.undo_move(mv, &u);
        if !illegal {
            out.push(mv);
        }
    }
    out
}

/// Render a `Move` as SAN, *without* the trailing `+`/`#` suffix.
///
/// The check/mate suffix is intentionally omitted because it is redundant for
/// move identification and our matcher strips annotations from both sides
/// before comparison.
fn move_to_san(pos: &mut Position, mv: Move, legal: &[Move]) -> String {
    let from = mv.from_sq();
    let to = mv.to_sq();
    let ftp = pos.tp_on_sq(from);
    let ttp = pos.tp_on_sq(to);
    let mt = mv.move_type();

    // Castling
    if mt == CASTLE {
        return if to > from {
            "O-O".to_string()
        } else {
            "O-O-O".to_string()
        };
    }

    let is_capture = ttp != NO_TP || mt == EP_CAP;

    // Promotion suffix
    let prom_suffix = if mv.is_prom() {
        let ch = match mt {
            N_PROM => 'N',
            B_PROM => 'B',
            R_PROM => 'R',
            Q_PROM => 'Q',
            _ => '?',
        };
        format!("={ch}")
    } else {
        String::new()
    };

    let to_str = format!(
        "{}{}",
        (b'a' + file_of(to) as u8) as char,
        (b'1' + rank_of(to) as u8) as char,
    );

    // Pawn moves
    if ftp == P {
        if is_capture {
            // Pawn captures always carry the from-file: `exd5`, `exd8=Q`.
            let from_file = (b'a' + file_of(from) as u8) as char;
            return format!("{from_file}x{to_str}{prom_suffix}");
        }
        return format!("{to_str}{prom_suffix}");
    }

    // Piece moves — pick a letter and figure out disambiguation.
    let piece_letter = match ftp {
        N => 'N',
        B => 'B',
        R => 'R',
        Q => 'Q',
        K => 'K',
        _ => '?',
    };

    // Find other legal moves of the same piece type that also land on `to`.
    // Track the from-files and from-ranks of those movers (excluding `mv`'s own from).
    let mut other_from_files: Vec<i32> = Vec::new();
    let mut other_from_ranks: Vec<i32> = Vec::new();
    let mut any_other = false;
    for &cand in legal {
        if cand == mv {
            continue;
        }
        if cand.to_sq() != to {
            continue;
        }
        if pos.tp_on_sq(cand.from_sq()) != ftp {
            continue;
        }
        any_other = true;
        other_from_files.push(file_of(cand.from_sq()));
        other_from_ranks.push(rank_of(cand.from_sq()));
    }

    let disambig = if !any_other {
        String::new()
    } else {
        let from_file = file_of(from);
        let from_rank = rank_of(from);
        let file_unique = !other_from_files.contains(&from_file);
        let rank_unique = !other_from_ranks.contains(&from_rank);
        if file_unique {
            ((b'a' + from_file as u8) as char).to_string()
        } else if rank_unique {
            ((b'1' + from_rank as u8) as char).to_string()
        } else {
            // Both file and rank ambiguous — emit full square.
            format!(
                "{}{}",
                (b'a' + from_file as u8) as char,
                (b'1' + from_rank as u8) as char,
            )
        }
    };

    let cap = if is_capture { "x" } else { "" };
    format!("{piece_letter}{disambig}{cap}{to_str}")
}

/// Convert a SAN string (annotation-tolerant) to an internal `Move` by
/// generating the legal-move list and matching its rendered SAN.
///
/// Returns `None` if no legal move matches.
pub fn san_to_move(pos: &mut Position, san: &str) -> Option<Move> {
    let target = strip_san_annotations(san);
    let legal = legal_moves(pos);
    legal
        .iter()
        .copied()
        .find(|&mv| move_to_san(pos, mv, &legal) == target)
}

// ============================================================================
// CLI argument parsing
// ============================================================================

struct SuiteArgs {
    path: String,
    time_ms: u64,
    /// Per-position node budget. When > 0 it overrides `time_ms` with a
    /// CPU-invariant limit, so several suite runs can be measured in parallel
    /// without their wall-clock budgets distorting each other's pass rate.
    nodes: u64,
    hash_mb: usize,
    verbose: bool,
    csv: Option<String>,
    /// Eval-parameter overrides from repeated `--set key=value`, applied before
    /// the run so a tuning sweep can be driven from one binary in parallel.
    overrides: Vec<(String, i32)>,
}

fn parse_args(args: &[String]) -> Result<SuiteArgs, String> {
    let mut path: Option<String> = None;
    let mut time_ms: u64 = 500;
    let mut nodes: u64 = 0;
    let mut hash_mb: usize = 16;
    let mut verbose = false;
    let mut csv: Option<String> = None;
    let mut overrides: Vec<(String, i32)> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--suite" => {
                i += 1;
                path = Some(args.get(i).cloned().ok_or("--suite requires a path")?);
            }
            "--time" => {
                i += 1;
                time_ms = args
                    .get(i)
                    .and_then(|t| t.parse().ok())
                    .ok_or("--time requires a number of milliseconds")?;
            }
            "--nodes" => {
                i += 1;
                nodes = args
                    .get(i)
                    .and_then(|t| t.parse().ok())
                    .ok_or("--nodes requires a node count")?;
            }
            "--hash" => {
                i += 1;
                hash_mb = args
                    .get(i)
                    .and_then(|t| t.parse().ok())
                    .ok_or("--hash requires a size in MB")?;
            }
            "--verbose" => verbose = true,
            "--csv" => {
                i += 1;
                csv = Some(args.get(i).cloned().ok_or("--csv requires a path")?);
            }
            "--set" => {
                i += 1;
                let kv = args.get(i).ok_or("--set requires key=value")?;
                let (k, v) = kv.split_once('=').ok_or("--set expects key=value")?;
                let val: i32 = v.parse().map_err(|_| format!("--set: bad value in '{kv}'"))?;
                overrides.push((k.to_string(), val));
            }
            _ => {}
        }
        i += 1;
    }

    Ok(SuiteArgs {
        path: path.ok_or("missing --suite <path>")?,
        time_ms,
        nodes,
        hash_mb,
        verbose,
        csv,
        overrides,
    })
}

/// Apply a single `--set key=value` eval override. Returns false for an unknown
/// key. Covers the king-attack / threat / tropism / contempt levers exercised by
/// the sacrifice-tuning campaign; callers re-derive tables afterwards.
fn apply_override(par: &mut eval::params::EvalParams, key: &str, val: i32) -> bool {
    match key {
        // King-attack gating (the keystone knobs)
        "att_min_wood" => par.att_min_wood = val,
        "no_queen_att_pct" => par.no_queen_att_pct = val,
        "danger_coeff" => par.danger_coeff_milli = val,
        // Asymmetric king-attack scaling (source for sd_att)
        "att_own" => par.att_own = val,
        "att_opp" => par.att_opp = val,
        // King-attack accumulator constants
        "n_att1" => par.n_att1 = val,
        "n_att2" => par.n_att2 = val,
        "b_att1" => par.b_att1 = val,
        "b_att2" => par.b_att2 = val,
        "r_att1" => par.r_att1 = val,
        "r_att2" => par.r_att2 = val,
        "q_att1" => par.q_att1 = val,
        "q_att2" => par.q_att2 = val,
        "n_chk" => par.n_chk = val,
        "b_chk" => par.b_chk = val,
        "r_chk" => par.r_chk = val,
        "q_chk" => par.q_chk = val,
        "r_contact" => par.r_contact = val,
        "q_contact" => par.q_contact = val,
        // Tropism components
        "ntr_mg" => par.ntr_mg = val,
        "ntr_eg" => par.ntr_eg = val,
        "btr_mg" => par.btr_mg = val,
        "btr_eg" => par.btr_eg = val,
        "rtr_mg" => par.rtr_mg = val,
        "rtr_eg" => par.rtr_eg = val,
        "qtr_mg" => par.qtr_mg = val,
        "qtr_eg" => par.qtr_eg = val,
        // Global weights
        "w_threats" => par.w_threats = val,
        "w_tropism" => par.w_tropism = val,
        "w_material" => par.w_material = val,
        "w_shield" => par.w_shield = val,
        "w_storm" => par.w_storm = val,
        "w_lines" => par.w_lines = val,
        "w_outposts" => par.w_outposts = val,
        "w_center" => par.w_center = val,
        // Contempt and piece-keeping
        "draw_score" => par.draw_score = val,
        "keep_q" => par.keep_pc[Q.index()] = val,
        "keep_r" => par.keep_pc[R.index()] = val,
        // Mobility asymmetry and tempo
        "mob_own" => par.mob_own = val,
        "mob_opp" => par.mob_opp = val,
        "tempo_mg" => par.tempo_mg = val,
        // Search knobs (read via ctx.par on the search path)
        "sac_lmr_relief" => par.sac_lmr_relief = val,
        "sac_ext_quiet" => par.sac_ext_quiet = val,
        "sac_ext_cap_add" => par.sac_ext_cap_add = val,
        "qs_delta" => par.qs_delta_margin = val,
        _ => return false,
    }
    true
}

// ============================================================================
// Runner
// ============================================================================

/// Per-position result.
struct PosResult {
    id: String,
    bm: String,
    engine_uci: String,
    engine_san: String,
    depth_reached: i32,
    time_ms: u64,
    nodes: u64,
    passed: bool,
}

/// Run the EPD suite. Entry point invoked by `--suite ...` from `main`.
pub fn run_suite(args: &[String]) {
    let parsed = match parse_args(args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!(
                "usage: tales --suite <path> [--time <ms>] [--nodes <n>] [--hash <mb>] [--verbose] [--csv <path>]"
            );
            return;
        }
    };

    // Read file
    let content = match std::fs::read_to_string(&parsed.path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read '{}': {e}", parsed.path);
            return;
        }
    };

    let records: Vec<EpdRecord> = content.lines().filter_map(parse_epd_line).collect();
    if records.is_empty() {
        eprintln!("error: no EPD records parsed from '{}'", parsed.path);
        return;
    }

    let budget = if parsed.nodes > 0 {
        format!("{} nodes/pos", parsed.nodes)
    } else {
        format!("{} ms/pos", parsed.time_ms)
    };
    println!(
        "[suite] file = {}, positions = {}, budget = {}, hash = {} MB",
        parsed.path,
        records.len(),
        budget,
        parsed.hash_mb,
    );
    println!("[suite] internal book = OFF, external book = OFF");

    // Set up search resources (single-threaded; aspiration via iterate()).
    let mut par = eval::params::EvalParams::new();
    // Apply eval overrides, then re-derive every table that depends on weights
    // (PST/mobility/passers via recalculate, danger curve via init_tables).
    if !parsed.overrides.is_empty() {
        for (k, v) in &parsed.overrides {
            if apply_override(&mut par, k, *v) {
                println!("[suite] set {k} = {v}");
            } else {
                eprintln!("warning: unknown --set key '{k}' (ignored)");
            }
        }
        par.recalculate();
        par.init_tables();
    }
    eval::global_pst::init(&par);
    println!();
    let mut tt = TransTable::new(parsed.hash_mb);
    let mut eval_hash = eval::new_eval_hash();
    let mut pawn_tt = eval::pawn_hash::PawnHash::new();
    let mut searcher = search::ordering::Searcher::new();
    let mut pos = Position::new();

    let mut results: Vec<PosResult> = Vec::with_capacity(records.len());
    let suite_start = Instant::now();

    for (idx, rec) in records.iter().enumerate() {
        // Fresh state per position so TT/history/killers/eval caches don't leak.
        pos.set_position(&rec.fen);
        tt.clear();
        // The eval hash and pawn hash are per-position caches too: leaking them
        // across positions lets a stale (different-prog_side) entry be reused on
        // a key match and adds measurement noise. Clear both, matching `run_one`.
        eval_hash.fill(eval::EvalHashEntry { key: 0, score: 0 });
        pawn_tt.clear();
        searcher.clear_all();
        searcher.nodes = 0;
        searcher.dp_completed = 0;
        searcher.pv_eng = [Move::NONE; 2];
        searcher.abort_search = false;
        searcher.is_pondering = false;
        searcher.ponder_enabled = false;
        searcher.silent = true;
        // Node-limited mode (--nodes) overrides the wall-clock budget with a
        // CPU-invariant one so parallel runs don't steal each other's time.
        if parsed.nodes > 0 {
            searcher.time_limit_ms = u64::MAX;
            searcher.nodes_limit = parsed.nodes;
        } else {
            searcher.time_limit_ms = parsed.time_ms;
            searcher.nodes_limit = 0;
        }
        searcher.move_overhead_ms = 0;
        searcher.nps_limit = 0;
        searcher.multi_pv = 1;
        par.init_asymmetric(pos.side);

        // Resolve the bm SAN against this position so we know the target Move.
        let target_mv = san_to_move(&mut pos, &rec.bm);

        let t0 = Instant::now();
        {
            let lmr = search::alphabeta::lmr_table();
            let mut ctx = search::ordering::SearchCtx {
                searcher: &mut searcher,
                tt: &mut tt,
                par: &par,
                eval_hash: &mut eval_hash,
                pawn_tt: &mut pawn_tt,
                lmr,
            };
            search::alphabeta::iterate(&mut ctx, &mut pos, MAX_PLY as i32);
        }
        let elapsed_ms = t0.elapsed().as_millis() as u64;

        let engine_mv = searcher.pv_eng[0];
        let engine_uci = if engine_mv.is_none() {
            "0000".to_string()
        } else {
            engine_mv.to_uci_string()
        };
        let legal = legal_moves(&mut pos);
        let engine_san = if engine_mv.is_none() {
            String::new()
        } else {
            move_to_san(&mut pos, engine_mv, &legal)
        };
        let passed = match target_mv {
            Some(t) => t == engine_mv,
            None => false,
        };

        let result = PosResult {
            id: rec.id.clone(),
            bm: rec.bm.clone(),
            engine_uci,
            engine_san,
            depth_reached: searcher.dp_completed,
            time_ms: elapsed_ms,
            nodes: searcher.nodes,
            passed,
        };

        // Per-position log line.
        let tag = if result.passed { "PASS" } else { "FAIL" };
        if parsed.verbose || !result.passed {
            println!(
                "[{tag} d={d:>2} {t:>4}ms n={n:>9}] {idx:>3}/{total} {id} bm={bm} got={got}",
                tag = tag,
                d = result.depth_reached,
                t = result.time_ms,
                n = result.nodes,
                idx = idx + 1,
                total = records.len(),
                id = if result.id.is_empty() {
                    "-"
                } else {
                    &result.id
                },
                bm = result.bm,
                got = if result.engine_san.is_empty() {
                    &result.engine_uci
                } else {
                    &result.engine_san
                },
            );
        } else {
            println!(
                "[{tag} d={d:>2} {t:>4}ms] {idx:>3}/{total} {bm}",
                tag = tag,
                d = result.depth_reached,
                t = result.time_ms,
                idx = idx + 1,
                total = records.len(),
                bm = result.bm,
            );
        }

        results.push(result);
    }

    let suite_ms = suite_start.elapsed().as_millis() as u64;

    // Aggregate summary
    let total = results.len();
    let pass = results.iter().filter(|r| r.passed).count();
    let total_nodes: u64 = results.iter().map(|r| r.nodes).sum();
    let mut depths: Vec<i32> = results.iter().map(|r| r.depth_reached).collect();
    depths.sort_unstable();
    let median_depth = if depths.is_empty() {
        0
    } else {
        depths[depths.len() / 2]
    };
    let nps = (total_nodes * 1000).checked_div(suite_ms).unwrap_or(0);

    println!();
    println!("=== Suite summary ===");
    println!(
        "  passed:        {pass} / {total}  ({pct:.1}%)",
        pct = 100.0 * pass as f64 / total as f64
    );
    println!("  median depth:  {median_depth}");
    println!("  total nodes:   {total_nodes}");
    println!("  total time:    {suite_ms} ms");
    println!("  effective nps: {nps}");

    // CSV output (optional)
    if let Some(csv_path) = parsed.csv {
        let mut out = String::new();
        out.push_str("idx,id,bm,got_san,got_uci,depth,nodes,time_ms,passed\n");
        for (i, r) in results.iter().enumerate() {
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{},{}\n",
                i + 1,
                csv_escape(&r.id),
                csv_escape(&r.bm),
                csv_escape(&r.engine_san),
                csv_escape(&r.engine_uci),
                r.depth_reached,
                r.nodes,
                r.time_ms,
                if r.passed { 1 } else { 0 },
            ));
        }
        if let Err(e) = std::fs::write(&csv_path, out) {
            eprintln!("error: could not write csv '{csv_path}': {e}");
        } else {
            println!("  csv written:   {csv_path}");
        }
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') {
        let q = s.replace('"', "\"\"");
        format!("\"{q}\"")
    } else {
        s.to_string()
    }
}

// ============================================================================
// Classifier coverage analysis — dev tool for the sacrifice campaign
// ============================================================================

/// Attack set of the piece sitting on `to` after `mv` is played (approximate
/// post-move occupancy: from cleared, to set; EP pawn ignored like
/// `gives_check`).
fn post_move_attacks(pos: &Position, mv: Move) -> crate::board::bitboard::Bitboard {
    use crate::board::attacks;
    use crate::board::bitboard::Bitboard;
    let from = mv.from_sq();
    let to = mv.to_sq();
    let mover = pos.tp_on_sq(from);
    let occ_after = (pos.occ_bb() ^ Bitboard::from_sq(from)) | Bitboard::from_sq(to);
    let landing = if mv.is_prom() { mv.prom_type() } else { mover };
    match landing {
        P => attacks::pawn_attacks(pos.side, to),
        N => attacks::knight_attacks(to),
        B => attacks::bishop_attacks(occ_after, to),
        R => attacks::rook_attacks(occ_after, to),
        Q => attacks::queen_attacks(occ_after, to),
        K => attacks::king_attacks(to),
        _ => Bitboard(0),
    }
}

/// `tales --classify <epd>` — for every record, print predicate columns for
/// the bm move plus per-position counts of quiet moves each classifier
/// variant would flag (the "classification load" that estimates move-ordering
/// noise and SEE cost). Output is CSV on stdout; analyze offline.
pub fn run_classify(args: &[String]) {
    use crate::board::{attacks, distance};
    use crate::movegen::see;
    use crate::search::ordering;

    let path = match args.first() {
        Some(p) => p,
        None => {
            eprintln!("usage: tales --classify <epd>");
            return;
        }
    };
    let content = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read '{path}': {e}");
            return;
        }
    };

    // do_move maintains incremental PST scores, so the global PST must exist.
    let par = eval::params::EvalParams::new();
    eval::global_pst::init(&par);

    let mut pos = Position::new();
    println!("idx,id,bm,cap,chk,zone,dist,ring,zring,see,cur,cand_d2,cand_ring,cand_zring,load_cur,load_d2,load_ring,load_zring");

    for (idx, rec) in content.lines().filter_map(parse_epd_line).enumerate() {
        pos.set_position(&rec.fen);
        let Some(bm) = san_to_move(&mut pos, &rec.bm) else {
            continue;
        };
        let enemy = !pos.side;
        let ksq = pos.king_sq(enemy);
        let ring = attacks::king_attacks(ksq);
        let zone = attacks::king_attack_zone(ksq, enemy);

        // Predicate columns for one move.
        let classify = |pos: &mut Position, mv: Move| -> (bool, bool, bool, i32, bool, bool, i32) {
            let to = mv.to_sq();
            let cap = pos.pc[to as usize] != NO_PC || mv.move_type() == EP_CAP;
            let chk = ordering::gives_check(pos, mv);
            let in_zone = zone.contains(to);
            let dist = distance::metric(to, ksq);
            let att = post_move_attacks(pos, mv);
            let hits_ring = (att & ring).is_not_empty();
            let hits_zone = (att & zone).is_not_empty();
            let sv = see::see_move(pos, mv);
            (cap, chk, in_zone, dist, hits_ring, hits_zone, sv)
        };

        let (cap, chk, in_zone, dist, hits_ring, hits_zone, sv) = classify(&mut pos, bm);
        let loses = sv < -ordering::SAC_THRESHOLD;
        let cur = (chk || in_zone) && loses;
        let cand_d2 = (chk || in_zone || dist <= 2) && loses;
        let cand_ring = (chk || in_zone || hits_ring) && loses;
        let cand_zring = (chk || in_zone || hits_zone) && loses;

        // Classification load over quiet legal moves.
        let legal = legal_moves(&mut pos);
        let (mut l_cur, mut l_d2, mut l_ring, mut l_zring) = (0, 0, 0, 0);
        for &mv in &legal {
            if pos.pc[mv.to_sq() as usize] != NO_PC || mv.move_type() == EP_CAP {
                continue; // quiet moves only
            }
            let (_c, qchk, qzone, qdist, qring, qzring, qsv) = classify(&mut pos, mv);
            let ql = qsv < -ordering::SAC_THRESHOLD;
            if (qchk || qzone) && ql {
                l_cur += 1;
            }
            if (qchk || qzone || qdist <= 2) && ql {
                l_d2 += 1;
            }
            if (qchk || qzone || qring) && ql {
                l_ring += 1;
            }
            if (qchk || qzone || qzring) && ql {
                l_zring += 1;
            }
        }

        println!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            idx + 1,
            csv_escape(&rec.id),
            csv_escape(&rec.bm),
            cap as u8,
            chk as u8,
            in_zone as u8,
            dist,
            hits_ring as u8,
            hits_zone as u8,
            sv,
            cur as u8,
            cand_d2 as u8,
            cand_ring as u8,
            cand_zring as u8,
            l_cur,
            l_d2,
            l_ring,
            l_zring,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tt::TransTable;

    fn init_engine() {
        crate::board::init();
        let par = eval::params::EvalParams::new();
        eval::global_pst::init(&par);
    }

    // ========================================================================
    // EPD parser
    // ========================================================================

    #[test]
    fn parse_epd_simple_bm() {
        let line = r#"8/8/8/8/8/8/8/4K2k w - - bm Ke1; id "tiny""#;
        let rec = parse_epd_line(line).expect("parses");
        assert_eq!(rec.fen, "8/8/8/8/8/8/8/4K2k w - -");
        assert_eq!(rec.bm, "Ke1");
        assert_eq!(rec.id, "tiny");
    }

    #[test]
    fn parse_epd_strips_check_suffix() {
        let line = r#"r1b1k2r/ppp4p/4p1p1/1q1pN2Q/8/2b5/P4PPP/R1B2RK1 w kq - bm Nxg6+; id "x""#;
        let rec = parse_epd_line(line).expect("parses");
        assert_eq!(rec.bm, "Nxg6"); // `+` stripped
    }

    #[test]
    fn parse_epd_skips_blank_and_comment() {
        assert!(parse_epd_line("").is_none());
        assert!(parse_epd_line("   ").is_none());
        assert!(parse_epd_line("# a comment").is_none());
    }

    // ========================================================================
    // SAN converter
    // ========================================================================

    #[test]
    fn san_roundtrip_basic() {
        init_engine();
        let mut pos = Position::new();
        pos.set_position(START_POS);
        // 1. e4
        let mv = san_to_move(&mut pos, "e4").expect("e4 resolves");
        let legal = legal_moves(&mut pos);
        assert_eq!(move_to_san(&mut pos, mv, &legal), "e4");
    }

    #[test]
    fn san_disambiguation_by_file() {
        init_engine();
        // Two knights on the same rank (b1, g1) — only one reaches d2 or f3
        // in startpos, but after 1. Nf3 Nf6 2. Nc3 Nc6, both sides have two
        // knights that could disambiguate. We just check the writer emits
        // the piece letter + file when two same-type movers reach a square.
        let mut pos = Position::new();
        pos.set_position("8/8/8/8/8/8/8/N1N1K2k w - -"); // two white knights a1/c1
        // Both can reach b3; SAN should include a disambiguating file.
        let mv = san_to_move(&mut pos, "Nab3").expect("Nab3 resolves");
        let legal = legal_moves(&mut pos);
        let san = move_to_san(&mut pos, mv, &legal);
        assert!(san == "Nab3", "expected Nab3, got {san}");
    }

    #[test]
    fn san_castling() {
        init_engine();
        let mut pos = Position::new();
        pos.set_position("r3k2r/pppppppp/8/8/8/8/PPPPPPPP/R3K2R w KQkq -");
        let mv = san_to_move(&mut pos, "O-O").expect("O-O resolves");
        let legal = legal_moves(&mut pos);
        assert_eq!(move_to_san(&mut pos, mv, &legal), "O-O");
    }

    #[test]
    fn san_promotion() {
        init_engine();
        let mut pos = Position::new();
        pos.set_position("4k3/P7/8/8/8/8/8/4K3 w - -");
        let mv = san_to_move(&mut pos, "a8=Q").expect("a8=Q resolves");
        let legal = legal_moves(&mut pos);
        assert_eq!(move_to_san(&mut pos, mv, &legal), "a8=Q");
    }

    // ========================================================================
    // End-to-end smoke test — a 3-position mini-suite that exercises the
    // sacrifice-friendly move ordering, the futility / LMP / LMR
    // relaxations for sac quiets, and the king-attack-amplified eval on
    // three classical sacrifices. A regression here signals a real
    // break in the sac-finding stack before the big 209-position suite
    // has to be run.
    // ========================================================================

    /// Run a single position through the same search path the suite uses
    /// and return `(engine_move_san, bm_was_found)`. Books are bypassed
    /// because we don't touch `parse_go`.
    fn run_one(fen: &str, bm: &str, time_ms: u64) -> (String, bool) {
        let mut par = eval::params::EvalParams::new();
        eval::global_pst::init(&par);
        let mut tt = TransTable::new(4);
        let mut eval_hash = eval::new_eval_hash();
        let mut pawn_tt = eval::pawn_hash::PawnHash::new();
        let mut searcher = search::ordering::Searcher::new();
        let mut pos = Position::new();

        pos.set_position(fen);
        tt.clear();
        searcher.clear_all();
        searcher.nodes = 0;
        searcher.dp_completed = 0;
        searcher.pv_eng = [Move::NONE; 2];
        searcher.abort_search = false;
        searcher.silent = true;
        searcher.time_limit_ms = time_ms;
        searcher.move_overhead_ms = 0;
        searcher.multi_pv = 1;
        par.init_asymmetric(pos.side);

        let target = san_to_move(&mut pos, bm);
        {
            let lmr = search::alphabeta::lmr_table();
            let mut ctx = search::ordering::SearchCtx {
                searcher: &mut searcher,
                tt: &mut tt,
                par: &par,
                eval_hash: &mut eval_hash,
                pawn_tt: &mut pawn_tt,
                lmr,
            };
            search::alphabeta::iterate(&mut ctx, &mut pos, MAX_PLY as i32);
        }

        let engine_mv = searcher.pv_eng[0];
        let passed = matches!(target, Some(t) if t == engine_mv);
        let legal = legal_moves(&mut pos);
        let san = if engine_mv.is_none() {
            "(none)".to_string()
        } else {
            move_to_san(&mut pos, engine_mv, &legal)
        };
        (san, passed)
    }

    /// Three shallow sacrificial positions the engine reliably finds at
    /// 250 ms / position on the development hardware. The test guards
    /// against the sac classifier, the move-ordering bonuses, the
    /// pruning relaxations, or the king-attack eval being broken
    /// silently.
    #[test]
    fn smoke_finds_shallow_sacrifices() {
        init_engine();
        let cases: &[(&str, &str)] = &[
            // Max Lange–Anderssen 1859 — Nxg6 demolishes the kingside.
            (
                "r1b1k2r/ppp4p/4p1p1/1q1pN2Q/8/2b5/P4PPP/R1B2RK1 w kq -",
                "Nxg6",
            ),
            // Zukertort–Anderssen 1865 — classical Greek gift Bxh7+.
            (
                "r1bqr1k1/p4ppp/2p1p3/b1B5/8/1R1B4/P1P2PPP/3Q1RK1 w - -",
                "Bxh7",
            ),
            // Alekhine–Diurnbaum 1910 — Rxh6+ tears the kingside open.
            (
                "r2b1r1k/1bnq2p1/p3p1Rp/1pPpP2Q/1P1N1P2/2PB3N/P2n3P/R6K w - -",
                "Rxh6",
            ),
        ];

        let mut passed = 0;
        for (fen, bm) in cases {
            let (got, ok) = run_one(fen, bm, 250);
            if ok {
                passed += 1;
            } else {
                eprintln!("smoke FAIL: fen={fen} bm={bm} got={got}");
            }
        }
        // Allow one miss in case of scheduler / CI thermal noise; a real
        // break in the sac-finding stack drops all three at once.
        assert!(
            passed >= 2,
            "smoke suite expected at least 2/3 sacrifices, got {passed}/3"
        );
    }
}
