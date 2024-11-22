use ggez::{Context, ContextBuilder, GameResult};
use ggez::event::{self, EventHandler};
use ggez::graphics::{self, Color, DrawParam, Image, Rect};
use ggez::input::mouse::MouseButton;
use std::path;

#[derive(Copy, Clone, PartialEq)]
enum PieceType {
    King, Queen, Rook, Bishop, Knight, Pawn,
}

#[derive(Copy, Clone, PartialEq)]
enum PieceColor {
    White,
    Black,
}

#[derive(Copy, Clone)]
struct Piece {
    piece_type: PieceType,
    color: PieceColor,
}

struct ChessBoard {
    board: [[Option<Piece>; 8]; 8],
    current_turn: PieceColor,
}

impl ChessBoard {
    fn new() -> ChessBoard {
        let mut board = [[None; 8]; 8];

        // Initialize pawns
        for i in 0..8 {
            board[1][i] = Some(Piece { piece_type: PieceType::Pawn, color: PieceColor::White });
            board[6][i] = Some(Piece { piece_type: PieceType::Pawn, color: PieceColor::Black });
        }

        // Initialize other pieces
        let piece_order = [
            PieceType::Rook, PieceType::Knight, PieceType::Bishop, PieceType::Queen,
            PieceType::King, PieceType::Bishop, PieceType::Knight, PieceType::Rook,
        ];

        for (i, &piece_type) in piece_order.iter().enumerate() {
            board[0][i] = Some(Piece { piece_type, color: PieceColor::White });
            board[7][i] = Some(Piece { piece_type, color: PieceColor::Black });
        }

        ChessBoard {
            board,
            current_turn: PieceColor::White,
        }
    }

    fn is_valid_move(&self, _from: (usize, usize), _to: (usize, usize)) -> bool {
        true // Placeholder logic
    }


    fn make_move(&mut self, from: (usize, usize), to: (usize, usize)) -> bool {
        if !self.is_valid_move(from, to) {
            return false;
        }

        self.board[to.0][to.1] = self.board[from.0][from.1];
        self.board[from.0][from.1] = None;
        self.current_turn = if self.current_turn == PieceColor::White {
            PieceColor::Black
        } else {
            PieceColor::White
        };
        true
    }
}

struct MainState {
    chess_board: ChessBoard,
    piece_images: std::collections::HashMap<String, Image>,
    selected_square: Option<(usize, usize)>,
    square_size: f32,
}

impl MainState {
    fn new(ctx: &mut Context) -> GameResult<MainState> {
        let chess_board = ChessBoard::new();
        let mut piece_images = std::collections::HashMap::new();
        let square_size = 60.0;

        // Load piece images
        let pieces = vec![
            "wk", "wq", "wr", "wb", "wn", "wp",
            "bk", "bq", "br", "bb", "bn", "bp"
        ];

        for piece in pieces {
            let image = Image::new(ctx, format!("/pieces/{}.png", piece))?;
            piece_images.insert(piece.to_string(), image);
        }

        Ok(MainState {
            chess_board,
            piece_images,
            selected_square: None,
            square_size,
        })
    }

    fn get_square_from_coords(&self, x: f32, y: f32) -> Option<(usize, usize)> {
        let file = (x / self.square_size) as usize;
        let rank = 7 - (y / self.square_size) as usize;

        if file < 8 && rank < 8 {
            Some((rank, file))
        } else {
            None
        }
    }
}

impl EventHandler for MainState {
    fn update(&mut self, _ctx: &mut Context) -> GameResult {
        Ok(())
    }

    fn draw(&mut self, ctx: &mut Context) -> GameResult {
        graphics::clear(ctx, Color::from_rgb(40, 40, 40));

        // Draw chessboard and pieces
        for rank in 0..8 {
            for file in 0..8 {
                let color = if (rank + file) % 2 == 0 {
                    Color::from_rgb(238, 238, 210)
                } else {
                    Color::from_rgb(118, 150, 86)
                };

                let rect = Rect::new(
                    file as f32 * self.square_size,
                    (7 - rank) as f32 * self.square_size,
                    self.square_size,
                    self.square_size,
                );

                let rect_mesh = graphics::Mesh::new_rectangle(
                    ctx,
                    graphics::DrawMode::fill(),
                    rect,
                    color,
                )?;
                graphics::draw(ctx, &rect_mesh, DrawParam::default())?;

                // Highlight selected square
                if let Some(selected) = self.selected_square {
                    if selected == (rank, file) {
                        let highlight = graphics::Mesh::new_rectangle(
                            ctx,
                            graphics::DrawMode::stroke(2.0),
                            rect,
                            Color::from_rgb(255, 255, 0),
                        )?;
                        graphics::draw(ctx, &highlight, DrawParam::default())?;
                    }
                }

                // Draw pieces
                if let Some(piece) = self.chess_board.board[rank][file] {
                    let piece_name = match (piece.color, piece.piece_type) {
                        (PieceColor::White, PieceType::King) => "wk",
                        (PieceColor::White, PieceType::Queen) => "wq",
                        (PieceColor::White, PieceType::Rook) => "wr",
                        (PieceColor::White, PieceType::Bishop) => "wb",
                        (PieceColor::White, PieceType::Knight) => "wn",
                        (PieceColor::White, PieceType::Pawn) => "wp",
                        (PieceColor::Black, PieceType::King) => "bk",
                        (PieceColor::Black, PieceType::Queen) => "bq",
                        (PieceColor::Black, PieceType::Rook) => "br",
                        (PieceColor::Black, PieceType::Bishop) => "bb",
                        (PieceColor::Black, PieceType::Knight) => "bn",
                        (PieceColor::Black, PieceType::Pawn) => "bp",
                    };

                    if let Some(image) = self.piece_images.get(piece_name) {
                        graphics::draw(
                            ctx,
                            image,
                            DrawParam::default()
                                .dest([
                                    file as f32 * self.square_size,
                                    (7 - rank) as f32 * self.square_size,
                                ])
                                .scale([
                                    self.square_size / image.width() as f32,
                                    self.square_size / image.height() as f32,
                                ]),
                        )?;
                    }
                }
            }
        }

        graphics::present(ctx)?;
        Ok(())
    }

    fn mouse_button_down_event(
        &mut self,
        _ctx: &mut Context,
        button: MouseButton,
        x: f32,
        y: f32,
    ) {
        if button == MouseButton::Left {
            if let Some(square) = self.get_square_from_coords(x, y) {
                match self.selected_square {
                    None => {
                        // Select piece
                        if self.chess_board.board[square.0][square.1].is_some() {
                            self.selected_square = Some(square);
                        }
                    }
                    Some(from) => {
                        // Move piece
                        if self.chess_board.make_move(from, square) {
                            self.selected_square = None;
                        } else {
                            if self.chess_board.board[square.0][square.1].is_some() {
                                self.selected_square = Some(square);
                            } else {
                                self.selected_square = None;
                            }
                        }
                    }
                }
            }
        }
    }
}

fn main() -> GameResult {
    let resource_dir = path::PathBuf::from("./resources");

    let (mut ctx, event_loop) = ContextBuilder::new("chess", "you")
        .window_setup(ggez::conf::WindowSetup::default().title("Chess"))
        .window_mode(ggez::conf::WindowMode::default().dimensions(480.0, 480.0))
        .add_resource_path(resource_dir)
        .build()?;

    let state = MainState::new(&mut ctx)?;
    event::run(ctx, event_loop, state)
}
