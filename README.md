# Tales

**Tales** is a UCI chess engine written in Rust, ported from [Rodent IV](https://github.com/nescitus/rodent-iv) with full mathematical parity. It features an aggressive, Tal-inspired playing style with tunable personality parameters.

## Features

- **UCI protocol** — compatible with any UCI-compliant GUI (Arena, CuteChess, etc.)
- **Lazy SMP** — multi-threaded search (up to 1024 threads)
- **Transposition table** — 4-bucket hash table with age-based eviction
- **Search** — Alpha-Beta with PVS, null-move pruning, LMR, futility pruning, singular extensions
- **3-layer quiescence search** — captures, checks, and evasions
- **Evaluation** — piece mobility, king safety, pawn structure, passed pawns, threats, endgame scaling
- **Pawn hash table** — cached pawn structure evaluation
- **Opening book** — compiled-in internal book with optional external Polyglot book support
- **MultiPV** — supports multiple principal variation analysis
- **Ponder** — full UCI ponder support with background search and `ponderhit` handling
- **KPK bitbase** — endgame draw detection for King+Pawn vs King
- **Personality system** — tunable playing style via UCI parameters (Selectivity, Contempt, etc.)

## Building

Requires **Rust 1.94.1+** (edition 2024).

```bash
cargo build --release
```

The release binary is built with full LTO and single codegen unit for maximum performance.

## Usage

Run the engine and connect it to a UCI GUI, or interact directly:

```
./tales
uci
setoption name Threads value 4
setoption name Hash value 128
isready
position startpos
go movetime 5000
```

### Built-in Commands

```
./tales --test    # Run perft + search correctness tests
./tales --bench   # Run benchmark suite (NPS measurement)
```

## UCI Options

| Option | Default | Range | Description |
|---|---|---|---|
| `Hash` | 16 | 1–33554432 | Hash table size in MB |
| `Threads` | 1 | 1–1024 | Number of search threads (Lazy SMP) |
| `MultiPV` | 1 | 1–64 | Number of principal variations |
| `Ponder` | false | — | Enable pondering |
| `UseBook` | false | — | Enable opening book |
| `Selectivity` | 175 | 100–500 | Search selectivity (higher = more pruning) |
| `Contempt` | 0 | -100–100 | Draw contempt (positive = avoid draws) |
| `MoveOverhead` | 50 | 0–5000 | Time buffer for move overhead (ms) |
| `UCI_LimitStrength` | false | — | Enable Elo-limited play |
| `UCI_Elo` | 2800 | 800–2800 | Target Elo when strength-limiting |

## What's New in v1.1.0-alpha

### Rust 2024 Edition
- Migrated the entire codebase to **Rust edition 2024** (requires rustc 1.94.1+)
- Modernized patterns throughout: replaced legacy `unsafe` blocks, `MaybeUninit` hacks, and C-style idioms with idiomatic Rust 2024 patterns
- Introduced `SearchCtx` and `SearchFrame` structs for cleaner search API ergonomics

### Ponder Support
- Implemented full UCI **ponder** functionality — the engine now correctly searches in the background on the expected opponent move, transitions to a normal search on `ponderhit`, and reports the best move + ponder move

### Performance Optimizations (+18% NPS)
- **OnceLock elimination** — replaced `OnceLock::get().unwrap()` with `AtomicPtr` (Relaxed load) for the three hottest lookup tables (leaper attacks, magic bitboards, between-rays). This was the single largest win (~14% alone)
- **Compile-time castle mask** — `castle_mask()` table is now a `const` array instead of runtime-initialized `OnceLock`
- **Unchecked hot-path access** — `get_unchecked` for eval hash, repetition list, and attack table lookups where indices are provably in-bounds
- **TT pointer arithmetic** — transposition table probe uses raw pointer iteration instead of per-bucket index computation
- **Stack allocation reduction** — `std::mem::zeroed()` for move arrays in search/quiescence, eliminating ~2KB of redundant memset per node

### Codebase Quality
- Comprehensive architectural audit — removed orphaned scope blocks, deduplicated evaluation logic, standardized naming conventions
- Full CI test suite: 149 tests covering perft, search correctness, and evaluation parity
- Verified via 20-game tournament matches against Rodent IV (10-0) and OpenTal (12.0-8.0)

## Architecture

```
src/
├── main.rs          # Entry point, CLI, benchmark
├── uci/             # UCI protocol handler (input, output, options)
├── search/          # Alpha-Beta, quiescence, move ordering, SMP threads
├── eval/            # Evaluation (pieces, pawns, passers, king safety, patterns)
├── board/           # Position, bitboards, attacks, move encoding, magic numbers
├── movegen/         # Move generation, move list, SEE
├── tt.rs            # Transposition table
└── book/            # Opening book (internal + Polyglot)
```

## License

Tales is free software, distributed under the **GNU General Public License v3** (GPLv3).

## Author

**Andre MARTINS**
