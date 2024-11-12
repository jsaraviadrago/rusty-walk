use std::fmt;
use std::io;

#[derive(Copy, Clone, PartialEq)]
enum PieceType {
    King,
    Queen,
    Rook,
    Bishop,
    Knight,
    Pawn,
}

#[derive(Copy, Clone, PartialEq)]
enum Color {
    White,
    Black,
}

#[derive(Copy, Clone)]
struct Piece {
    piece_type: PieceType,
    color: Color,
}

struct ChessBoard {
    board: [[Option<Piece>; 8]; 8],
    current_turn: Color,
}

impl ChessBoard {
    fn new() -> ChessBoard {
        let mut board = [[None; 8]; 8];

        // Initialize pawns
        for i in 0..8 {
            board[1][i] = Some(Piece { piece_type: PieceType::Pawn, color: Color::White });
            board[6][i] = Some(Piece { piece_type: PieceType::Pawn, color: Color::Black });
        }

        // Initialize other pieces
        let piece_order = [
            PieceType::Rook,
            PieceType::Knight,
            PieceType::Bishop,
            PieceType::Queen,
            PieceType::King,
            PieceType::Bishop,
            PieceType::Knight,
            PieceType::Rook,
        ];

        for (i, &piece_type) in piece_order.iter().enumerate() {
            board[0][i] = Some(Piece { piece_type, color: Color::White });
            board[7][i] = Some(Piece { piece_type, color: Color::Black });
        }

        ChessBoard {
            board,
            current_turn: Color::White,
        }
    }

    fn is_valid_move(&self, from: (usize, usize), to: (usize, usize)) -> bool {
        if from == to {
            return false;
        }

        let piece = match self.board[from.0][from.1] {
            Some(p) => p,
            None => return false,
        };

        if piece.color != self.current_turn {
            return false;
        }

        // Check if destination contains a piece of the same color
        if let Some(dest_piece) = self.board[to.0][to.1] {
            if dest_piece.color == piece.color {
                return false;
            }
        }

        match piece.piece_type {
            PieceType::Pawn => self.is_valid_pawn_move(from, to, piece.color),
            PieceType::Rook => self.is_valid_rook_move(from, to),
            PieceType::Knight => self.is_valid_knight_move(from, to),
            PieceType::Bishop => self.is_valid_bishop_move(from, to),
            PieceType::Queen => self.is_valid_queen_move(from, to),
            PieceType::King => self.is_valid_king_move(from, to),
        }
    }

    fn is_valid_pawn_move(&self, from: (usize, usize), to: (usize, usize), color: Color) -> bool {
        let direction = if color == Color::White { 1 } else { -1 };
        let start_row = if color == Color::White { 1 } else { 6 };

        let forward = (from.0 as i32 + direction) as usize;
        let double_forward = (from.0 as i32 + 2 * direction) as usize;

        // Normal move forward
        if to.0 == forward && to.1 == from.1 && self.board[to.0][to.1].is_none() {
            return true;
        }

        // Initial double move
        if from.0 == start_row && to.0 == double_forward && to.1 == from.1 {
            return self.board[forward][from.1].is_none() && self.board[to.0][to.1].is_none();
        }

        // Capture
        if to.0 == forward && (to.1 as i32 - from.1 as i32).abs() == 1 {
            return self.board[to.0][to.1].is_some();
        }

        false
    }

    fn is_valid_rook_move(&self, from: (usize, usize), to: (usize, usize)) -> bool {
        from.0 == to.0 || from.1 == to.1
    }

    fn is_valid_knight_move(&self, from: (usize, usize), to: (usize, usize)) -> bool {
        let dx = (to.0 as i32 - from.0 as i32).abs();
        let dy = (to.1 as i32 - from.1 as i32).abs();
        (dx == 2 && dy == 1) || (dx == 1 && dy == 2)
    }

    fn is_valid_bishop_move(&self, from: (usize, usize), to: (usize, usize)) -> bool {
        let dx = (to.0 as i32 - from.0 as i32).abs();
        let dy = (to.1 as i32 - from.1 as i32).abs();
        dx == dy
    }

    fn is_valid_queen_move(&self, from: (usize, usize), to: (usize, usize)) -> bool {
        self.is_valid_rook_move(from, to) || self.is_valid_bishop_move(from, to)
    }

    fn is_valid_king_move(&self, from: (usize, usize), to: (usize, usize)) -> bool {
        let dx = (to.0 as i32 - from.0 as i32).abs();
        let dy = (to.1 as i32 - from.1 as i32).abs();
        dx <= 1 && dy <= 1
    }

    fn make_move(&mut self, from: (usize, usize), to: (usize, usize)) -> bool {
        if !self.is_valid_move(from, to) {
            return false;
        }

        self.board[to.0][to.1] = self.board[from.0][from.1];
        self.board[from.0][from.1] = None;
        self.current_turn = if self.current_turn == Color::White {
            Color::Black
        } else {
            Color::White
        };
        true
    }
}

impl fmt::Display for ChessBoard {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "  a b c d e f g h")?;
        for i in (0..8).rev() {
            write!(f, "{} ", i + 1)?;
            for j in 0..8 {
                match self.board[i][j] {
                    Some(piece) => {
                        let symbol = match (piece.piece_type, piece.color) {
                            (PieceType::King, Color::White) => "K",
                            (PieceType::King, Color::Black) => "k",
                            (PieceType::Queen, Color::White) => "Q",
                            (PieceType::Queen, Color::Black) => "q",
                            (PieceType::Rook, Color::White) => "R",
                            (PieceType::Rook, Color::Black) => "r",
                            (PieceType::Bishop, Color::White) => "B",
                            (PieceType::Bishop, Color::Black) => "b",
                            (PieceType::Knight, Color::White) => "N",
                            (PieceType::Knight, Color::Black) => "n",
                            (PieceType::Pawn, Color::White) => "P",
                            (PieceType::Pawn, Color::Black) => "p",
                        };
                        write!(f, "{} ", symbol)?;
                    }
                    None => write!(f, ". ")?,
                }
            }
            writeln!(f, "{}", i + 1)?;
        }
        writeln!(f, "  a b c d e f g h")?;
        Ok(())
    }
}

fn parse_position(input: &str) -> Option<(usize, usize)> {
    if input.len() != 2 {
        return None;
    }
    let mut chars = input.chars();
    let file = chars.next()?;
    let rank = chars.next()?;

    if !('a'..='h').contains(&file) || !('1'..='8').contains(&rank) {
        return None;
    }

    let x = rank.to_digit(10)? as usize - 1;
    let y = (file as u8 - b'a') as usize;
    Some((x, y))
}

fn main() {
    let mut board = ChessBoard::new();
    let mut input = String::new();

    loop {
        println!("{}", board);
        println!("Current turn: {}",
                 if board.current_turn == Color::White { "White" } else { "Black" });
        println!("Enter move (e.g., 'e2 e4') or 'quit' to exit:");

        input.clear();
        io::stdin().read_line(&mut input).unwrap();
        let input = input.trim();

        if input == "quit" {
            break;
        }

        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.len() != 2 {
            println!("Invalid input format. Use 'from to' notation (e.g., 'e2 e4')");
            continue;
        }

        let from = match parse_position(parts[0]) {
            Some(pos) => pos,
            None => {
                println!("Invalid 'from' position");
                continue;
            }
        };

        let to = match parse_position(parts[1]) {
            Some(pos) => pos,
            None => {
                println!("Invalid 'to' position");
                continue;
            }
        };

        if !board.make_move(from, to) {
            println!("Invalid move!");
            continue;
        }
    }
}