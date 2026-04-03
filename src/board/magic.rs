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

//! Magic bitboard slider attack tables (Pradyumna Kannan, shortened by Pawel Koziol).
//!
//! Architecture: Fancy magic — per-square index into a shared attack database.
//! Rook database: 102,400 entries. Bishop database: 5,248 entries.

use std::sync::atomic::{AtomicPtr, Ordering};

use super::bitboard::Bitboard;

// ============================================================================
// Static tables — initialized once, accessed via raw pointer (no atomic)
// ============================================================================

struct MagicDb {
    bish: Vec<u64>,
    rook: Vec<u64>,
}

// Raw pointer — initialized once in init(), then zero-overhead access.
static MAGIC_PTR: AtomicPtr<MagicDb> = AtomicPtr::new(std::ptr::null_mut());

#[inline(always)]
fn db() -> &'static MagicDb {
    // SAFETY: init() is always called before any slider attack lookup.
    unsafe { &*MAGIC_PTR.load(Ordering::Relaxed) }
}

// Per-square offsets into the database arrays
#[rustfmt::skip]
const BISH_OFFSETS: [usize; 64] = [
    4992, 2624,  256,  896, 1280, 1664, 4800, 5120,
    2560, 2656,  288,  928, 1312, 1696, 4832, 4928,
       0,  128,  320,  960, 1344, 1728, 2304, 2432,
      32,  160,  448, 2752, 3776, 1856, 2336, 2464,
      64,  192,  576, 3264, 4288, 1984, 2368, 2496,
      96,  224,  704, 1088, 1472, 2112, 2400, 2528,
    2592, 2688,  832, 1216, 1600, 2240, 4864, 4960,
    5056, 2720,  864, 1248, 1632, 2272, 4896, 5184,
];

#[rustfmt::skip]
const ROOK_OFFSETS: [usize; 64] = [
    86016, 73728, 36864, 43008, 47104, 51200, 77824, 94208,
    69632, 32768, 38912, 10240, 14336, 53248, 57344, 81920,
    24576, 33792,  6144, 11264, 15360, 18432, 58368, 61440,
    26624,  4096,  7168,     0,  2048, 19456, 22528, 63488,
    28672,  5120,  8192,  1024,  3072, 20480, 23552, 65536,
    30720, 34816,  9216, 12288, 16384, 21504, 59392, 67584,
    71680, 35840, 39936, 13312, 17408, 54272, 60416, 83968,
    90112, 75776, 40960, 45056, 49152, 55296, 79872, 98304,
];

// ============================================================================
// Magic constants (from Pradyumna Kannan)
// ============================================================================

#[rustfmt::skip]
const ROOK_SHIFT: [u32; 64] = [
    52, 53, 53, 53, 53, 53, 53, 52,
    53, 54, 54, 54, 54, 54, 54, 53,
    53, 54, 54, 54, 54, 54, 54, 53,
    53, 54, 54, 54, 54, 54, 54, 53,
    53, 54, 54, 54, 54, 54, 54, 53,
    53, 54, 54, 54, 54, 54, 54, 53,
    53, 54, 54, 54, 54, 54, 54, 53,
    53, 54, 54, 53, 53, 53, 53, 53,
];

#[rustfmt::skip]
const ROOK_MAGIC: [u64; 64] = [
    0x0080001020400080, 0x0040001000200040, 0x0080081000200080,
    0x0080040800100080, 0x0080020400080080, 0x0080010200040080,
    0x0080008001000200, 0x0080002040800100, 0x0000800020400080,
    0x0000400020005000, 0x0000801000200080, 0x0000800800100080,
    0x0000800400080080, 0x0000800200040080, 0x0000800100020080,
    0x0000800040800100, 0x0000208000400080, 0x0000404000201000,
    0x0000808010002000, 0x0000808008001000, 0x0000808004000800,
    0x0000808002000400, 0x0000010100020004, 0x0000020000408104,
    0x0000208080004000, 0x0000200040005000, 0x0000100080200080,
    0x0000080080100080, 0x0000040080080080, 0x0000020080040080,
    0x0000010080800200, 0x0000800080004100, 0x0000204000800080,
    0x0000200040401000, 0x0000100080802000, 0x0000080080801000,
    0x0000040080800800, 0x0000020080800400, 0x0000020001010004,
    0x0000800040800100, 0x0000204000808000, 0x0000200040008080,
    0x0000100020008080, 0x0000080010008080, 0x0000040008008080,
    0x0000020004008080, 0x0000010002008080, 0x0000004081020004,
    0x0000204000800080, 0x0000200040008080, 0x0000100020008080,
    0x0000080010008080, 0x0000040008008080, 0x0000020004008080,
    0x0000800100020080, 0x0000800041000080, 0x00FFFCDDFCED714A,
    0x007FFCDDFCED714A, 0x003FFFCDFFD88096, 0x0000040810002101,
    0x0001000204080011, 0x0001000204000801, 0x0001000082000401,
    0x0001FFFAABFAD1A2,
];

#[rustfmt::skip]
const ROOK_MASK: [u64; 64] = [
    0x000101010101017E, 0x000202020202027C, 0x000404040404047A,
    0x0008080808080876, 0x001010101010106E, 0x002020202020205E,
    0x004040404040403E, 0x008080808080807E, 0x0001010101017E00,
    0x0002020202027C00, 0x0004040404047A00, 0x0008080808087600,
    0x0010101010106E00, 0x0020202020205E00, 0x0040404040403E00,
    0x0080808080807E00, 0x00010101017E0100, 0x00020202027C0200,
    0x00040404047A0400, 0x0008080808760800, 0x00101010106E1000,
    0x00202020205E2000, 0x00404040403E4000, 0x00808080807E8000,
    0x000101017E010100, 0x000202027C020200, 0x000404047A040400,
    0x0008080876080800, 0x001010106E101000, 0x002020205E202000,
    0x004040403E404000, 0x008080807E808000, 0x0001017E01010100,
    0x0002027C02020200, 0x0004047A04040400, 0x0008087608080800,
    0x0010106E10101000, 0x0020205E20202000, 0x0040403E40404000,
    0x0080807E80808000, 0x00017E0101010100, 0x00027C0202020200,
    0x00047A0404040400, 0x0008760808080800, 0x00106E1010101000,
    0x00205E2020202000, 0x00403E4040404000, 0x00807E8080808000,
    0x007E010101010100, 0x007C020202020200, 0x007A040404040400,
    0x0076080808080800, 0x006E101010101000, 0x005E202020202000,
    0x003E404040404000, 0x007E808080808000, 0x7E01010101010100,
    0x7C02020202020200, 0x7A04040404040400, 0x7608080808080800,
    0x6E10101010101000, 0x5E20202020202000, 0x3E40404040404000,
    0x7E80808080808000,
];

#[rustfmt::skip]
const BISH_SHIFT: [u32; 64] = [
    58, 59, 59, 59, 59, 59, 59, 58,
    59, 59, 59, 59, 59, 59, 59, 59,
    59, 59, 57, 57, 57, 57, 59, 59,
    59, 59, 57, 55, 55, 57, 59, 59,
    59, 59, 57, 55, 55, 57, 59, 59,
    59, 59, 57, 57, 57, 57, 59, 59,
    59, 59, 59, 59, 59, 59, 59, 59,
    58, 59, 59, 59, 59, 59, 59, 58,
];

#[rustfmt::skip]
const BISH_MAGIC: [u64; 64] = [
    0x0002020202020200, 0x0002020202020000, 0x0004010202000000,
    0x0004040080000000, 0x0001104000000000, 0x0000821040000000,
    0x0000410410400000, 0x0000104104104000, 0x0000040404040400,
    0x0000020202020200, 0x0000040102020000, 0x0000040400800000,
    0x0000011040000000, 0x0000008210400000, 0x0000004104104000,
    0x0000002082082000, 0x0004000808080800, 0x0002000404040400,
    0x0001000202020200, 0x0000800802004000, 0x0000800400A00000,
    0x0000200100884000, 0x0000400082082000, 0x0000200041041000,
    0x0002080010101000, 0x0001040008080800, 0x0000208004010400,
    0x0000404004010200, 0x0000840000802000, 0x0000404002011000,
    0x0000808001041000, 0x0000404000820800, 0x0001041000202000,
    0x0000820800101000, 0x0000104400080800, 0x0000020080080080,
    0x0000404040040100, 0x0000808100020100, 0x0001010100020800,
    0x0000808080010400, 0x0000820820004000, 0x0000410410002000,
    0x0000082088001000, 0x0000002011000800, 0x0000080100400400,
    0x0001010101000200, 0x0002020202000400, 0x0001010101000200,
    0x0000410410400000, 0x0000208208200000, 0x0000002084100000,
    0x0000000020880000, 0x0000001002020000, 0x0000040408020000,
    0x0004040404040000, 0x0002020202020000, 0x0000104104104000,
    0x0000002082082000, 0x0000000020841000, 0x0000000000208800,
    0x0000000010020200, 0x0000000404080200, 0x0000040404040400,
    0x0002020202020200,
];

#[rustfmt::skip]
const BISH_MASK: [u64; 64] = [
    0x0040201008040200, 0x0000402010080400, 0x0000004020100A00,
    0x0000000040221400, 0x0000000002442800, 0x0000000204085000,
    0x0000020408102000, 0x0002040810204000, 0x0020100804020000,
    0x0040201008040000, 0x00004020100A0000, 0x0000004022140000,
    0x0000000244280000, 0x0000020408500000, 0x0002040810200000,
    0x0004081020400000, 0x0010080402000200, 0x0020100804000400,
    0x004020100A000A00, 0x0000402214001400, 0x0000024428002800,
    0x0002040850005000, 0x0004081020002000, 0x0008102040004000,
    0x0008040200020400, 0x0010080400040800, 0x0020100A000A1000,
    0x0040221400142200, 0x0002442800284400, 0x0004085000500800,
    0x0008102000201000, 0x0010204000402000, 0x0004020002040800,
    0x0008040004081000, 0x00100A000A102000, 0x0022140014224000,
    0x0044280028440200, 0x0008500050080400, 0x0010200020100800,
    0x0020400040201000, 0x0002000204081000, 0x0004000408102000,
    0x000A000A10204000, 0x0014001422400000, 0x0028002844020000,
    0x0050005008040200, 0x0020002010080400, 0x0040004020100800,
    0x0000020408102000, 0x0000040810204000, 0x00000A1020400000,
    0x0000142240000000, 0x0000284402000000, 0x0000500804020000,
    0x0000201008040200, 0x0000402010080400, 0x0002040810204000,
    0x0004081020400000, 0x000A102040000000, 0x0014224000000000,
    0x0028440200000000, 0x0050080402000000, 0x0020100804020000,
    0x0040201008040200,
];

// ============================================================================
// Slow attack generators (for init only)
// ============================================================================

fn init_rook_moves(square: i32, occ: u64) -> u64 {
    let mut ret: u64 = 0;
    let row_bits: u64 = 0xFF << (8 * (square / 8));

    // North
    let mut bit: u64 = 1u64 << square;
    loop {
        bit <<= 8;
        if bit == 0 {
            break;
        }
        ret |= bit;
        if bit & occ != 0 {
            break;
        }
    }
    // South
    bit = 1u64 << square;
    loop {
        bit >>= 8;
        if bit == 0 {
            break;
        }
        ret |= bit;
        if bit & occ != 0 {
            break;
        }
    }
    // East
    bit = 1u64 << square;
    loop {
        bit <<= 1;
        if bit & row_bits != 0 {
            ret |= bit;
        } else {
            break;
        }
        if bit & occ != 0 {
            break;
        }
    }
    // West
    bit = 1u64 << square;
    loop {
        bit >>= 1;
        if bit & row_bits != 0 {
            ret |= bit;
        } else {
            break;
        }
        if bit & occ != 0 {
            break;
        }
    }
    ret
}

fn init_bishop_moves(square: i32, occ: u64) -> u64 {
    let mut ret: u64 = 0;
    let row_bits: u64 = 0xFF << (8 * (square / 8));

    // NW
    let mut bit: u64 = 1u64 << square;
    let mut bit2: u64 = bit;
    loop {
        bit <<= 7;
        bit2 >>= 1;
        if bit2 & row_bits != 0 {
            ret |= bit;
        } else {
            break;
        }
        if bit == 0 || bit & occ != 0 {
            break;
        }
    }
    // NE
    bit = 1u64 << square;
    bit2 = bit;
    loop {
        bit <<= 9;
        bit2 <<= 1;
        if bit2 & row_bits != 0 {
            ret |= bit;
        } else {
            break;
        }
        if bit == 0 || bit & occ != 0 {
            break;
        }
    }
    // SE
    bit = 1u64 << square;
    bit2 = bit;
    loop {
        bit >>= 7;
        bit2 <<= 1;
        if bit2 & row_bits != 0 {
            ret |= bit;
        } else {
            break;
        }
        if bit == 0 || bit & occ != 0 {
            break;
        }
    }
    // SW
    bit = 1u64 << square;
    bit2 = bit;
    loop {
        bit >>= 9;
        bit2 >>= 1;
        if bit2 & row_bits != 0 {
            ret |= bit;
        } else {
            break;
        }
        if bit == 0 || bit & occ != 0 {
            break;
        }
    }
    ret
}

fn init_occ(squares: &[i32], linocc: u64) -> u64 {
    let mut ret: u64 = 0;
    for (i, &sq) in squares.iter().enumerate() {
        if linocc & (1u64 << i) != 0 {
            ret |= 1u64 << sq;
        }
    }
    ret
}

// ============================================================================
// Bitscan for init (de Bruijn)
// ============================================================================

#[rustfmt::skip]
const BITPOS64: [i32; 64] = [
    63,  0, 58,  1, 59, 47, 53,  2,
    60, 39, 48, 27, 54, 33, 42,  3,
    61, 51, 37, 40, 49, 18, 28, 20,
    55, 30, 34, 11, 43, 14, 22,  4,
    62, 57, 46, 52, 38, 26, 32, 41,
    50, 36, 17, 19, 29, 10, 13, 21,
    56, 45, 25, 31, 35, 16,  9, 12,
    44, 24, 15,  8, 23,  7,  6,  5,
];

// ============================================================================
// Public API
// ============================================================================

/// Initialize magic bitboard tables. Must be called once at startup.
pub fn init() {
    let mut bish = vec![0u64; 5248];
    let mut rook = vec![0u64; 102400];

    // Bishop tables
    for sq in 0..64 {
        let mut squares = Vec::new();
        let mut temp = BISH_MASK[sq];
        while temp != 0 {
            let bit = temp & temp.wrapping_neg();
            let idx = BITPOS64[((bit.wrapping_mul(0x07EDD5E59A4E28C2)) >> 58) as usize];
            squares.push(idx);
            temp ^= bit;
        }
        let num = squares.len();
        for t in 0..(1u64 << num) {
            let occ = init_occ(&squares, t);
            let attacks = init_bishop_moves(sq as i32, occ);
            let index =
                BISH_OFFSETS[sq] + ((occ.wrapping_mul(BISH_MAGIC[sq])) >> BISH_SHIFT[sq]) as usize;
            bish[index] = attacks;
        }
    }

    // Rook tables
    for sq in 0..64 {
        let mut squares = Vec::new();
        let mut temp = ROOK_MASK[sq];
        while temp != 0 {
            let bit = temp & temp.wrapping_neg();
            let idx = BITPOS64[((bit.wrapping_mul(0x07EDD5E59A4E28C2)) >> 58) as usize];
            squares.push(idx);
            temp ^= bit;
        }
        let num = squares.len();
        for t in 0..(1u64 << num) {
            let occ = init_occ(&squares, t);
            let attacks = init_rook_moves(sq as i32, occ);
            let index =
                ROOK_OFFSETS[sq] + ((occ.wrapping_mul(ROOK_MAGIC[sq])) >> ROOK_SHIFT[sq]) as usize;
            rook[index] = attacks;
        }
    }
    let db = Box::leak(Box::new(MagicDb { bish, rook }));
    MAGIC_PTR.store(db as *mut MagicDb, Ordering::Release);
}

/// Bishop attacks for a given square and occupancy.
#[inline(always)]
pub fn bishop_attacks(occ: Bitboard, sq: i32) -> Bitboard {
    let sq = sq as usize;
    let db = db();
    unsafe {
        let index = *BISH_OFFSETS.get_unchecked(sq)
            + (((occ.0 & *BISH_MASK.get_unchecked(sq)).wrapping_mul(*BISH_MAGIC.get_unchecked(sq)))
                >> *BISH_SHIFT.get_unchecked(sq)) as usize;
        Bitboard(*db.bish.get_unchecked(index))
    }
}

/// Rook attacks for a given square and occupancy.
#[inline(always)]
pub fn rook_attacks(occ: Bitboard, sq: i32) -> Bitboard {
    let sq = sq as usize;
    let db = db();
    unsafe {
        let index = *ROOK_OFFSETS.get_unchecked(sq)
            + (((occ.0 & *ROOK_MASK.get_unchecked(sq)).wrapping_mul(*ROOK_MAGIC.get_unchecked(sq)))
                >> *ROOK_SHIFT.get_unchecked(sq)) as usize;
        Bitboard(*db.rook.get_unchecked(index))
    }
}

/// Queen attacks = bishop + rook attacks.
#[inline(always)]
pub fn queen_attacks(occ: Bitboard, sq: i32) -> Bitboard {
    bishop_attacks(occ, sq) | rook_attacks(occ, sq)
}
