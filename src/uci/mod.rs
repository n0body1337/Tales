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

// UCI protocol implementation.
// Supports single-threaded and Lazy SMP multi-threaded search.
// All standard UCI options are implemented.

use crate::board::moves::Move;
use crate::board::position::Position;
use crate::board::types::*;
use crate::eval;
use crate::search;
use crate::tt::TransTable;
use std::io;

const ENGINE_NAME: &str = "Tales 1.0.0";
const ENGINE_AUTHOR: &str = "Andre MARTINS";

// ============================================================================
// Engine-wide mutable state managed by the UCI loop
// ============================================================================

struct EngineState {
    par: eval::params::EvalParams,
    tt: TransTable,
    eval_hash: Vec<eval::EvalHashEntry>,
    pawn_tt: eval::pawn_hash::PawnHash,
    searcher: search::ordering::Searcher,
    num_threads: usize,
    // UCI options
    multi_pv: usize,
    use_book: bool,
    verbose_book: bool,
    book_file: String,
    ponder_enabled: bool,
    time_buffer: i64,
}

impl EngineState {
    fn new() -> Self {
        let par = eval::params::EvalParams::new();
        eval::global_pst::init(&par);
        EngineState {
            par,
            tt: TransTable::new(16),
            eval_hash: eval::new_eval_hash(),
            pawn_tt: eval::pawn_hash::PawnHash::new(),
            searcher: search::ordering::Searcher::new(),
            num_threads: 1,
            multi_pv: 1,
            use_book: false,
            verbose_book: false,
            book_file: String::from("book.bin"),
            ponder_enabled: false,
            time_buffer: 50,
        }
    }
}

// ============================================================================
// Main UCI loop
// ============================================================================

pub fn uci_loop() {
    // EngineState::new() initializes global PST tables — must happen before any Position operations
    let mut state = EngineState::new();

    let mut pos = Position::new();
    pos.set_position(START_POS);

    // Read one line at a time (don't hold stdin lock across parse_go — needed
    // for check_timeout() stdin polling during search to work without deadlock).
    loop {
        let mut line = String::new();
        if io::stdin().read_line(&mut line).is_err() || line.is_empty() {
            break; // EOF or error
        }
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let mut tokens = line.split_whitespace();
        let Some(cmd) = tokens.next() else {
            continue;
        };

        match cmd {
            "uci" => {
                println!("id name {ENGINE_NAME}");
                println!("id author {ENGINE_AUTHOR}");
                println!();
                // Options.cpp
                println!("option name Hash type spin default 16 min 1 max 1024");
                println!("option name Threads type spin default 1 min 1 max 16");
                println!("option name MoveOverhead type spin default 50 min 0 max 5000");
                println!("option name MultiPV type spin default 1 min 1 max 64");
                println!("option name Clear Hash type button");
                println!("option name Ponder type check default false");
                println!("option name UseBook type check default false");
                println!("option name VerboseBook type check default false");
                println!("option name MainBookFile type string default book.bin");
                println!("option name TimeBuffer type spin default 50 min 0 max 1000");
                println!("option name Contempt type spin default 0 min -100 max 100");
                println!("option name EvalBlur type spin default 0 min 0 max 40");
                println!("option name NpsLimit type spin default 0 min 0 max 1000000");
                println!("option name UCI_Elo type spin default 2800 min 800 max 2800");
                println!("option name UCI_LimitStrength type check default false");
                println!("option name SlowMover type spin default 100 min 10 max 200");
                println!("option name Selectivity type spin default 175 min 100 max 500");
                println!("uciok");
            }

            "isready" => {
                println!("readyok");
            }

            "ucinewgame" => {
                state.tt.clear();
                state.searcher.clear_all();
                pos.set_position(START_POS);
                // Generate new game_key for eval_blur randomness
                state.searcher.game_key = pos.hash_key
                    ^ (std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos() as u64);
            }

            "setoption" => {
                let rest: Vec<&str> = tokens.collect();
                parse_setoption(&rest, &mut state);
            }

            "position" => {
                let rest: Vec<&str> = tokens.collect();
                parse_position(&mut pos, &rest);
            }

            "go" => {
                let rest: Vec<&str> = tokens.collect();
                parse_go(&mut pos, &mut state, &rest);
            }

            "ponderhit" => {
                // When pondering, ponderhit converts ponder to normal search
                // For now, this is handled by the timeout system
            }

            "stop" => {
                state.searcher.abort_search = true;
            }

            "bench" => {
                let depth: i32 = tokens.next().and_then(|t| t.parse().ok()).unwrap_or(8);
                run_bench(&mut pos, &mut state, depth);
            }

            "print" | "d" => {
                pos.print_board();
            }

            "step" => {
                // Apply moves without resetting position
                for token in tokens {
                    let mv = pos.str_to_move(token);
                    if mv != Move::NONE && pos.legal(mv) {
                        let mut u = crate::board::position::Undo::default();
                        pos.do_move(mv, &mut u);
                        if pos.rev_moves == 0 {
                            pos.head = 0;
                        }
                    }
                }
            }

            "stepp" => {
                // Apply moves and print the board
                for token in tokens {
                    let mv = pos.str_to_move(token);
                    if mv != Move::NONE && pos.legal(mv) {
                        let mut u = crate::board::position::Undo::default();
                        pos.do_move(mv, &mut u);
                        if pos.rev_moves == 0 {
                            pos.head = 0;
                        }
                    }
                }
                pos.print_board();
            }

            "quit" | "exit" => {
                return;
            }

            _ => {
                // Unknown command, ignore
            }
        }
    }
}

// ============================================================================
// Parse "position [startpos|fen <fen>] [moves <move1> <move2> ...]"
// ============================================================================

fn parse_position(pos: &mut Position, tokens: &[&str]) {
    if tokens.is_empty() {
        return;
    }

    let mut idx = 0;

    if tokens[0] == "startpos" {
        pos.set_position(START_POS);
        idx = 1;
    } else if tokens[0] == "fen" {
        // Collect FEN tokens until "moves" or end
        let mut fen_parts = Vec::new();
        idx = 1;
        while idx < tokens.len() && tokens[idx] != "moves" {
            fen_parts.push(tokens[idx]);
            idx += 1;
        }
        let fen = fen_parts.join(" ");
        pos.set_position(&fen);
    }

    // Parse moves
    if idx < tokens.len() && tokens[idx] == "moves" {
        idx += 1;
        while idx < tokens.len() {
            let mv = pos.str_to_move(tokens[idx]);
            if mv != Move::NONE && pos.legal(mv) {
                let mut u = crate::board::position::Undo::default();
                pos.do_move(mv, &mut u);
                // After an irreversible move, reset rep list head
                if pos.rev_moves == 0 {
                    pos.head = 0;
                }
            }
            idx += 1;
        }
    }
}

// ============================================================================
// Parse "go" command with time management
// ============================================================================

fn parse_go(pos: &mut Position, state: &mut EngineState, tokens: &[&str]) {
    let mut wtime: i64 = -1;
    let mut btime: i64 = -1;
    let mut winc: i64 = 0;
    let mut binc: i64 = 0;
    let mut movestogo: i64 = 40;
    let mut max_depth: i32 = 64;
    let mut movetime: i64 = -1;
    let mut nodes_limit: u64 = 0;
    let mut _infinite = false;

    let mut i = 0;
    while i < tokens.len() {
        match tokens[i] {
            "depth" => {
                i += 1;
                if i < tokens.len() {
                    max_depth = tokens[i].parse().unwrap_or(64);
                }
            }
            "movetime" => {
                i += 1;
                if i < tokens.len() {
                    movetime = tokens[i].parse().unwrap_or(5000);
                }
            }
            "nodes" => {
                i += 1;
                if i < tokens.len() {
                    nodes_limit = tokens[i].parse().unwrap_or(0);
                }
            }
            "wtime" => {
                i += 1;
                if i < tokens.len() {
                    wtime = tokens[i].parse().unwrap_or(-1);
                }
            }
            "btime" => {
                i += 1;
                if i < tokens.len() {
                    btime = tokens[i].parse().unwrap_or(-1);
                }
            }
            "winc" => {
                i += 1;
                if i < tokens.len() {
                    winc = tokens[i].parse().unwrap_or(0);
                }
            }
            "binc" => {
                i += 1;
                if i < tokens.len() {
                    binc = tokens[i].parse().unwrap_or(0);
                }
            }
            "movestogo" => {
                i += 1;
                if i < tokens.len() {
                    movestogo = tokens[i].parse().unwrap_or(40);
                }
            }
            "infinite" => {
                _infinite = true;
                movetime = 999_999_999;
            }
            "ponder" => {
                // Pondering: search infinitely until ponderhit/stop
                _infinite = true;
                movetime = 999_999_999;
            }
            _ => {}
        }
        i += 1;
    }

    // Calculate time_limit_ms for Searcher
    let time_limit_ms;
    if movetime > 0 {
        time_limit_ms = movetime as u64;
    } else if wtime >= 0 || btime >= 0 {
        let base = if pos.side == WC { wtime } else { btime };
        let inc = if pos.side == WC { winc } else { binc };
        time_limit_ms = compute_move_time(
            base,
            inc,
            movestogo,
            state.time_buffer,
            state.par.time_percentage,
        );
    } else {
        time_limit_ms = 999_999_999;
    }

    // Set asymmetric parameters based on engine side
    state.par.init_asymmetric(pos.side);

    // Probe the opening book (always-on, not gated by UseBook)
    if let Some(book_mv) = crate::book::internal::probe(pos, state.verbose_book) {
        println!("bestmove {book_mv}");
        return;
    }

    // Route through thread pool (handles both single and multi-threaded)
    search::threads::lazy_smp_search(
        pos,
        &mut state.tt,
        &state.par,
        &mut state.eval_hash,
        &mut state.pawn_tt,
        state.num_threads,
        max_depth,
        time_limit_ms,
        state.searcher.move_overhead_ms,
        state.searcher.game_key,
        nodes_limit,
        state.par.nps_limit,
        state.multi_pv,
    );
}

// ============================================================================
// Time management
// ============================================================================

fn compute_move_time(
    mut base: i64,
    inc: i64,
    movestogo: i64,
    overhead: i64,
    time_percentage: i32,
) -> u64 {
    if base < 0 {
        return 5000;
    } // fallback

    let mtg = movestogo.max(1);

    // Time control safety: deduct safety margin on last move of time control
    if mtg == 1 {
        base -= 1000_i64.min(base / 10);
    }

    let mut time = (base + inc * (mtg - 1)) / mtg;

    // Apply SlowMover percentage (only when safe)
    if 2 * time > base {
        time = (time * time_percentage as i64) / 100;
    }

    // Safety: don't use more than base allows
    if time > base {
        time = base;
    }

    // Subtract buffer for lag
    time -= overhead;
    if time < 0 {
        time = 0;
    }

    // Bullet correction
    time = bullet_correction(time);

    time as u64
}

fn bullet_correction(time: i64) -> i64 {
    if time < 200 {
        (time * 23) / 32
    } else if time < 400 {
        (time * 26) / 32
    } else if time < 1200 {
        (time * 29) / 32
    } else {
        time
    }
}

// ============================================================================
// Parse "setoption name <name> value <value>"
// ============================================================================

fn parse_setoption(tokens: &[&str], state: &mut EngineState) {
    // Expect: ["name", "<name>", ...] or ["name", "<name>", "value", "<value>"]
    if tokens.len() < 2 {
        return;
    }
    if tokens[0] != "name" {
        return;
    }

    // Build the option name (can be multi-word, e.g. "Clear Hash")
    let value_pos = tokens.iter().position(|&t| t.to_lowercase() == "value");
    let name_end = value_pos.unwrap_or(tokens.len());
    let name = tokens[1..name_end].join(" ").to_lowercase();

    // Get value (if present)
    let value = if let Some(vi) = value_pos {
        if vi + 1 < tokens.len() {
            Some(tokens[vi + 1])
        } else {
            None
        }
    } else {
        None
    };

    match name.as_str() {
        "hash" => {
            if let Some(v) = value {
                if let Ok(mb) = v.parse::<usize>() {
                    let mb = mb.clamp(1, 1024);
                    state.tt = TransTable::new(mb);
                }
            }
        }
        "moveoverhead" => {
            if let Some(v) = value {
                if let Ok(val) = v.parse::<u64>() {
                    state.searcher.move_overhead_ms = val.min(5000);
                }
            }
        }
        "threads" => {
            if let Some(v) = value {
                if let Ok(val) = v.parse::<usize>() {
                    state.num_threads = val.clamp(1, 16);
                }
            }
        }
        "multipv" => {
            if let Some(v) = value {
                if let Ok(val) = v.parse::<usize>() {
                    state.multi_pv = val.clamp(1, 64);
                }
            }
        }
        "clear hash" => {
            state.tt.clear();
        }
        "ponder" => {
            if let Some(v) = value {
                state.ponder_enabled = v == "true";
            }
        }
        "usebook" => {
            if let Some(v) = value {
                state.use_book = v == "true";
            }
        }
        "verbosebook" => {
            if let Some(v) = value {
                state.verbose_book = v == "true";
            }
        }
        "mainbookfile" => {
            if let Some(v) = value {
                state.book_file = v.to_string();
            }
        }
        "timebuffer" => {
            if let Some(v) = value {
                if let Ok(val) = v.parse::<i64>() {
                    state.time_buffer = val.clamp(0, 1000);
                }
            }
        }
        "contempt" => {
            if let Some(v) = value {
                if let Ok(val) = v.parse::<i32>() {
                    state.par.draw_score = val.clamp(-100, 100);
                }
            }
        }
        "evalblur" => {
            if let Some(v) = value {
                if let Ok(val) = v.parse::<i32>() {
                    state.par.eval_blur = val.clamp(0, 40);
                }
            }
        }
        "npslimit" => {
            if let Some(v) = value {
                if let Ok(val) = v.parse::<i32>() {
                    state.par.nps_limit = val.clamp(0, 1_000_000);
                }
            }
        }
        "uci_elo" => {
            if let Some(v) = value {
                if let Ok(val) = v.parse::<i32>() {
                    state.par.elo = val.clamp(800, 2800);
                    state.par.set_speed();
                }
            }
        }
        "uci_limitstrength" => {
            if let Some(v) = value {
                state.par.fl_weakening = v == "true";
                state.par.set_speed();
            }
        }
        "slowmover" => {
            if let Some(v) = value {
                if let Ok(val) = v.parse::<i32>() {
                    state.par.time_percentage = val.clamp(10, 200);
                }
            }
        }
        "selectivity" => {
            if let Some(v) = value {
                if let Ok(val) = v.parse::<i32>() {
                    state.par.hist_perc = val.clamp(100, 500);
                    // hist_limit = -MAX_HIST + ((MAX_HIST * hist_perc) / 100)
                    const MAX_HIST: i32 = 1 << 15; // 32768
                    state.par.hist_limit = -MAX_HIST + ((MAX_HIST * state.par.hist_perc) / 100);
                }
            }
        }
        _ => {}
    }
}

// ============================================================================
// Bench command — search a set of standard positions
// ============================================================================

fn run_bench(pos: &mut Position, state: &mut EngineState, depth: i32) {
    let positions = [
        "r1bqkbnr/pp1ppppp/2n5/2p5/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq -",
        "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -",
        "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - -",
        "4rrk1/pp1n3p/3q2pQ/2p1pb2/2PP4/2P3N1/P2B2PP/4RRK1 b - - 7 19",
        "rq3rk1/ppp2ppp/1bnpb3/3N2B1/3NP3/7P/PPPQ1PP1/2KR3R w - - 7 14",
        "r1bq1r1k/1pp1n1pp/1p1p4/4p2Q/4Pp2/1BNP4/PPP2PPP/3R1RK1 w - - 2 14",
        "r3r1k1/2p2ppp/p1p1bn2/8/1q2P3/2NPQN2/PPP3PP/R4RK1 b - - 2 15",
        "r1bbk1nr/pp3p1p/2n5/1N4p1/2Np1B2/8/PPP2PPP/2KR1B1R w kq - 0 13",
        "r1bq1rk1/ppp1nppp/4n3/3p3Q/3P4/1BP1B3/PP1N2PP/R4RK1 w - - 1 16",
        "4r1k1/r1q2ppp/ppp2n2/4P3/5Rb1/1N1BQ3/PPP3PP/R5K1 w - - 1 17",
        "2rqkb1r/ppp2p2/2npb1p1/1N1Nn2p/2P1PP2/8/PP2B1PP/R1BQK2R b KQ - 0 11",
        "r1bq1r1k/b1p1npp1/p2p3p/1p6/3PP3/1B2NN2/PP3PPP/R2Q1RK1 w - - 1 16",
        "3r1rk1/p5pp/bpp1pp2/8/q1PP1P2/b3P3/P2NQRPP/1R2B1K1 b - - 6 22",
        "r1q2rk1/2p1bppp/2Pp4/p6b/Q1PNp3/4B3/PP1R1PPP/2K4R w - - 2 18",
        "4k2r/1pb2ppp/1p2p3/1R1p4/3P4/2r1PN2/P4PPP/1R4K1 b - - 3 22",
        "3q2k1/pb3p1p/4pbp1/2r5/PpN2N2/1P2P2P/5PP1/Q2R2K1 b - - 4 26",
    ];

    println!("Bench test started (depth {depth}):");
    state.tt.clear();
    state.searcher.clear_all();

    let start = std::time::Instant::now();
    let mut total_nodes = 0u64;

    for fen in &positions {
        pos.set_position(fen);
        state.par.init_asymmetric(pos.side);
        state.searcher.time_limit_ms = 999_999_999;
        state.searcher.abort_search = false;
        state.searcher.nodes = 0;
        state.searcher.dp_completed = 0;
        state.searcher.pv_eng = [Move::NONE; 2];
        // Don't call tt.new_search() here — iterate() handles it via
        // lazy_smp_search(). Bench() never increments tt_date.

        search::alphabeta::iterate(
            pos,
            &mut state.searcher,
            &mut state.tt,
            &state.par,
            &mut state.eval_hash,
            &mut state.pawn_tt,
            depth,
        );
        total_nodes += state.searcher.nodes;
    }

    let elapsed_ms = start.elapsed().as_millis().max(1) as u64;
    let nps = total_nodes * 1000 / elapsed_ms;
    println!("{total_nodes} nodes searched in {elapsed_ms}ms, speed {nps} nps");
}
