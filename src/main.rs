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

//! Tales — a UCI chess engine written in Rust, featuring aggressive Tal-like play.

mod board;
mod book;
mod eval;
mod movegen;
mod search;
mod tt;
mod uci;

fn main() {
    // Initialize static tables (must happen before anything)
    board::init();

    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && args[1] == "--test" {
        run_tests();
    } else if args.len() > 1 && args[1] == "--bench" {
        run_bench();
    } else if args.len() > 1 && args[1] == "--suite" {
        // EPD test-suite runner. Bypasses UCI/parse_go entirely so the
        // opening book is never probed during measurement.
        uci::epd::run_suite(&args[1..]);
    } else {
        // UCI mode
        uci::uci_loop();
    }
}

fn run_tests() {
    let par = eval::params::EvalParams::new();
    eval::global_pst::init(&par);

    let mut pos = board::position::Position::new();
    let mut eval_hash = eval::new_eval_hash();
    let mut pawn_tt = eval::pawn_hash::PawnHash::new();

    // ===== Perft =====
    pos.set_position(board::types::START_POS);
    println!("Tales 1.0a");
    println!("Running perft from startpos...");
    for depth in 1..=5 {
        let nodes = movegen::generate::perft(&mut pos, depth);
        println!("  perft({depth}) = {nodes}");
    }

    pos.set_position("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -");
    println!("\nKiwipete perft:");
    for depth in 1..=4 {
        let nodes = movegen::generate::perft(&mut pos, depth);
        println!("  perft({depth}) = {nodes}");
    }

    // ===== Eval =====
    println!("\n--- Evaluation Tests ---");
    pos.set_position(board::types::START_POS);
    println!(
        "Startpos eval: {} cp",
        eval::evaluate(&pos, &par, &mut eval_hash, &mut pawn_tt, 0)
    );

    pos.set_position("rnbqkb1r/pppppppp/8/8/8/5N2/PPPPPPPP/RNBQKB1R w KQkq -");
    println!(
        "White+N eval: {} cp",
        eval::evaluate(&pos, &par, &mut eval_hash, &mut pawn_tt, 0)
    );

    pos.set_position("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -");
    println!(
        "Kiwipete eval: {} cp",
        eval::evaluate(&pos, &par, &mut eval_hash, &mut pawn_tt, 0)
    );

    // ===== Search =====
    println!("\n--- Search Tests ---");
    let mut trans = tt::TransTable::new(16);
    let mut searcher = search::ordering::Searcher::new();

    // Mate in 1
    pos.set_position("6k1/5ppp/8/8/8/8/8/4Q2K w - -");
    println!("\nMate-in-1 test:");
    searcher.time_limit_ms = 5000;
    {
        let lmr = search::alphabeta::lmr_table();
        let mut ctx = search::ordering::SearchCtx {
            searcher: &mut searcher,
            tt: &mut trans,
            par: &par,
            eval_hash: &mut eval_hash,
            pawn_tt: &mut pawn_tt,
            lmr,
        };
        search::alphabeta::iterate(&mut ctx, &mut pos, 6);
        search::alphabeta::print_bestmove(&ctx);
    }

    // Mate in 2
    pos.set_position("2bqkbn1/2pppp2/np2N3/r3P1p1/p2N2B1/5Q2/PPPPPP1P/RNB1K2R w KQ -");
    println!("\nMate-in-2 test:");
    searcher.time_limit_ms = 10000;
    trans.clear();
    searcher.clear_all();
    {
        let lmr = search::alphabeta::lmr_table();
        let mut ctx = search::ordering::SearchCtx {
            searcher: &mut searcher,
            tt: &mut trans,
            par: &par,
            eval_hash: &mut eval_hash,
            pawn_tt: &mut pawn_tt,
            lmr,
        };
        search::alphabeta::iterate(&mut ctx, &mut pos, 8);
        search::alphabeta::print_bestmove(&ctx);
    }

    // Startpos depth 6
    pos.set_position(board::types::START_POS);
    println!("\nStartpos depth-6:");
    searcher.time_limit_ms = 30000;
    trans.clear();
    searcher.clear_all();
    {
        let lmr = search::alphabeta::lmr_table();
        let mut ctx = search::ordering::SearchCtx {
            searcher: &mut searcher,
            tt: &mut trans,
            par: &par,
            eval_hash: &mut eval_hash,
            pawn_tt: &mut pawn_tt,
            lmr,
        };
        search::alphabeta::iterate(&mut ctx, &mut pos, 6);
        search::alphabeta::print_bestmove(&ctx);
    }
}

fn run_bench() {
    use std::time::Instant;

    let par = eval::params::EvalParams::new();
    eval::global_pst::init(&par);

    let mut pos = board::position::Position::new();
    let mut eval_hash = eval::new_eval_hash();
    let mut pawn_tt = eval::pawn_hash::PawnHash::new();
    let mut trans = tt::TransTable::new(64);
    let mut searcher = search::ordering::Searcher::new();

    let positions = [
        ("Startpos", board::types::START_POS, 12),
        (
            "Kiwipete",
            "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq -",
            12,
        ),
        (
            "Endgame KRPvKR",
            "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - -",
            14,
        ),
        (
            "Middle game",
            "r1bqkb1r/pppppppp/2n2n2/4P3/2B5/5N2/PPPP1PPP/RNBQK2R b KQkq -",
            12,
        ),
        (
            "Complex tactics",
            "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ -",
            11,
        ),
    ];

    println!("Tales Benchmark");
    println!("================");
    println!(
        "{:<20} {:>6} {:>12} {:>10} {:>8}",
        "Position", "Depth", "Nodes", "NPS", "Time(ms)"
    );
    println!("{}", "-".repeat(62));

    let mut total_nodes: u64 = 0;
    let bench_start = Instant::now();

    for (name, fen, depth) in &positions {
        pos.set_position(fen);
        searcher.time_limit_ms = 120_000;
        trans.clear();
        searcher.clear_all();

        let t0 = Instant::now();
        let lmr = search::alphabeta::lmr_table();
        let mut ctx = search::ordering::SearchCtx {
            searcher: &mut searcher,
            tt: &mut trans,
            par: &par,
            eval_hash: &mut eval_hash,
            pawn_tt: &mut pawn_tt,
            lmr,
        };
        search::alphabeta::iterate(&mut ctx, &mut pos, *depth);
        search::alphabeta::print_bestmove(&ctx);
        let elapsed_ms = t0.elapsed().as_millis() as u64;
        let nodes = ctx.searcher.nodes;
        let nps = if elapsed_ms > 0 {
            nodes * 1000 / elapsed_ms
        } else {
            0
        };
        total_nodes += nodes;

        println!(
            "{:<20} {:>6} {:>12} {:>10} {:>8}",
            name, depth, nodes, nps, elapsed_ms
        );
    }

    let total_ms = bench_start.elapsed().as_millis() as u64;
    let total_nps = if total_ms > 0 {
        total_nodes * 1000 / total_ms
    } else {
        0
    };
    println!("{}", "-".repeat(62));
    println!(
        "{:<20} {:>6} {:>12} {:>10} {:>8}",
        "TOTAL", "", total_nodes, total_nps, total_ms
    );
    println!("\n{} nodes {} nps", total_nodes, total_nps);
}
