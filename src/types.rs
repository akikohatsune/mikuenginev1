use std::fmt;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Color {
    White = 0,
    Black = 1,
}

impl Color {
    #[inline(always)]
    pub fn flip(self) -> Color {
        match self {
            Color::White => Color::Black,
            Color::Black => Color::White,
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PieceType {
    Pawn = 0,
    Knight = 1,
    Bishop = 2,
    Rook = 3,
    Queen = 4,
    King = 5,
}

pub const NUM_PIECE_TYPES: usize = 6;
pub const NUM_COLORS: usize = 2;
pub const NUM_SQUARES: usize = 64;

pub const SEE_PIECE_VALUES: [i32; 6] = [100, 300, 300, 500, 900, 0];

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Piece(pub u8); // Color and type encoded together

impl Piece {
    #[inline(always)]
    pub fn new(color: Color, piece_type: PieceType) -> Self {
        Piece((color as u8) * 6 + (piece_type as u8))
    }

    #[inline(always)]
    pub fn color(self) -> Color {
        if self.0 < 6 {
            Color::White
        } else {
            Color::Black
        }
    }

    #[inline(always)]
    pub fn piece_type(self) -> PieceType {
        match self.0 % 6 {
            0 => PieceType::Pawn,
            1 => PieceType::Knight,
            2 => PieceType::Bishop,
            3 => PieceType::Rook,
            4 => PieceType::Queen,
            5 => PieceType::King,
            _ => unreachable!(),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Square(pub u8);

impl Square {
    #[inline(always)]
    pub fn new(sq: u8) -> Self {
        debug_assert!(sq < 64);
        Square(sq)
    }

    #[inline(always)]
    pub fn rank(self) -> u8 {
        self.0 / 8
    }

    #[inline(always)]
    pub fn file(self) -> u8 {
        self.0 % 8
    }
}

/// A move encoded as a 16-bit unsigned integer.
/// Bits 0-5: Source square (0-63)
/// Bits 6-11: Destination square (0-63)
/// Bits 12-15: 4-bit Flag
///   0: Quiet
///   1: Double pawn push
///   2: King-side castle
///   3: Queen-side castle
///   4: Capture
///   5: En-passant capture
///   8: Knight promotion
///   9: Bishop promotion
///  10: Rook promotion
///  11: Queen promotion
///  12: Knight promo capture
///  13: Bishop promo capture
///  14: Rook promo capture
///  15: Queen promo capture
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Move(pub u16);

impl Move {
    pub const FLAG_QUIET: u16 = 0x0000;
    pub const FLAG_DBL_PUSH: u16 = 0x1000;
    pub const FLAG_K_CASTLE: u16 = 0x2000;
    pub const FLAG_Q_CASTLE: u16 = 0x3000;
    pub const FLAG_CAPTURE: u16 = 0x4000;
    pub const FLAG_EP: u16 = 0x5000;
    pub const FLAG_PR_KNIGHT: u16 = 0x8000;
    pub const FLAG_PR_BISHOP: u16 = 0x9000;
    pub const FLAG_PR_ROOK: u16 = 0xA000;
    pub const FLAG_PR_QUEEN: u16 = 0xB000;
    pub const FLAG_PC_KNIGHT: u16 = 0xC000;
    pub const FLAG_PC_BISHOP: u16 = 0xD000;
    pub const FLAG_PC_ROOK: u16 = 0xE000;
    pub const FLAG_PC_QUEEN: u16 = 0xF000;

    #[inline(always)]
    pub fn new(from: u8, to: u8, flags: u16) -> Self {
        Move(flags | ((to as u16) << 6) | (from as u16))
    }

    #[inline(always)]
    pub fn none() -> Self {
        Move(0)
    }

    #[inline(always)]
    pub fn from_sq(self) -> u8 {
        (self.0 & 0x3F) as u8
    }

    #[inline(always)]
    pub fn to_sq(self) -> u8 {
        ((self.0 >> 6) & 0x3F) as u8
    }

    #[inline(always)]
    pub fn flag(self) -> u16 {
        self.0 & 0xF000
    }

    #[inline(always)]
    pub fn is_capture(self) -> bool {
        (self.0 & 0x4000) != 0
    }

    #[inline(always)]
    pub fn is_en_passant(self) -> bool {
        self.flag() == Self::FLAG_EP
    }

    #[inline(always)]
    pub fn is_promotion(self) -> bool {
        (self.0 & 0x8000) != 0 // Flags >= 8000 are all promotions
    }

    #[inline(always)]
    pub fn promotion_type(self) -> PieceType {
        match (self.0 >> 12) & 0x3 {
            0 => PieceType::Knight,
            1 => PieceType::Bishop,
            2 => PieceType::Rook,
            3 => PieceType::Queen,
            _ => unreachable!(),
        }
    }
}

impl fmt::Debug for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let files = b"abcdefgh";
        let ranks = b"12345678";
        let from = self.from_sq();
        let to = self.to_sq();

        let from_file = files[(from % 8) as usize] as char;
        let from_rank = ranks[(from / 8) as usize] as char;
        let to_file = files[(to % 8) as usize] as char;
        let to_rank = ranks[(to / 8) as usize] as char;

        write!(f, "{}{}{}{}", from_file, from_rank, to_file, to_rank)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct CastlingRights(pub u8);

impl CastlingRights {
    pub const WK: u8 = 1;
    pub const WQ: u8 = 2;
    pub const BK: u8 = 4;
    pub const BQ: u8 = 8;
    pub const ANY: u8 = 15;

    #[inline(always)]
    pub fn new(rights: u8) -> Self {
        CastlingRights(rights)
    }

    #[inline(always)]
    pub fn has_wk(self) -> bool {
        (self.0 & Self::WK) != 0
    }

    #[inline(always)]
    pub fn has_wq(self) -> bool {
        (self.0 & Self::WQ) != 0
    }

    #[inline(always)]
    pub fn has_bk(self) -> bool {
        (self.0 & Self::BK) != 0
    }

    #[inline(always)]
    pub fn has_bq(self) -> bool {
        (self.0 & Self::BQ) != 0
    }
}
