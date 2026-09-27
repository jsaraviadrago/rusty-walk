//! Tipos núcleo del motor de Jacquet.
//! Ver RULES.md para las reglas completas y DESIGN.md para el razonamiento
//! detrás de estas estructuras.

/// Un jugador. El tablero es compartido; cada uno se mueve en la misma
/// dirección, con puntos de partida/llegada en esquinas opuestas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Player {
    White,
    Black,
}

impl Player {
    pub fn opponent(self) -> Player {
        match self {
            Player::White => Player::Black,
            Player::Black => Player::White,
        }
    }
}

/// Un punto del tablero: vacío, o con N fichas de un jugador.
/// Basta 1 ficha para bloquear el punto al rival (RULES.md, sección 4).
/// No hay "blots": sin hitting, un punto de 1 ficha es tan sólido como uno de 15.
pub type Point = Option<(Player, u8)>;

/// El tablero: 24 puntos compartidos.
#[derive(Debug, Clone)]
pub struct Board {
    pub points: [Point; 24],
}

impl Board {
    /// Un punto está bloqueado para `player` si tiene fichas del rival.
    pub fn is_blocked_for(&self, point_index: usize, player: Player) -> bool {
        match self.points[point_index] {
            Some((owner, _)) => owner != player,
            None => false,
        }
    }
}

/// Estado del postillón de un jugador (RULES.md, sección 5).
#[derive(Debug, Clone, Copy)]
pub struct Courier {
    /// Punto actual del postillón (None si ya llegó a destino y se liberó).
    pub position: Option<usize>,
    /// true una vez que el postillón llegó a su cuadrante final.
    pub arrived: bool,
}

/// Estado de un jugador dentro de la partida.
#[derive(Debug, Clone)]
pub struct PlayerState {
    pub courier: Courier,
    /// Fichas ya retiradas del tablero (bear off). 0..=15.
    pub borne_off: u8,
    /// Dados heredados del rival (los que el rival no pudo jugar en su
    /// turno anterior): si hay algo acá, este jugador los juega
    /// directamente en su próximo turno en vez de tirar los suyos
    /// (RULES.md sección 3). Lo que de estos tampoco pueda jugar se
    /// pierde para siempre, sin rebotar de nuevo.
    pub pending_dice: Option<Vec<u8>>,
}

/// Los 3 dados crudos de una tirada.
#[derive(Debug, Clone, Copy)]
pub struct Roll {
    pub dice: [u8; 3],
}

/// Resultado de expandir una tirada según las reglas de dobles/triples
/// (RULES.md, sección 3).
#[derive(Debug, Clone)]
pub struct ExpandedRoll {
    /// Movimientos disponibles para jugar, en el orden que el jugador elija.
    pub moves: Vec<u8>,
    /// true si la tirada fue un triple (3 dados iguales): el turno se repite
    /// una vez jugados todos los movimientos.
    pub repeats_turn: bool,
}

impl Roll {
    /// Expande la tirada cruda a la secuencia de movimientos jugables.
    ///
    /// - 3 valores distintos -> 3 movimientos.
    /// - 2 iguales (ej. 4-4-2) -> el valor duplicado x4 + el suelto (4,4,4,4,2).
    /// - 3 iguales (ej. 5-5-5) -> el valor x6, y se repite el turno.
    pub fn expand(&self) -> ExpandedRoll {
        let [a, b, c] = self.dice;

        if a == b && b == c {
            return ExpandedRoll {
                moves: vec![a; 6],
                repeats_turn: true,
            };
        }

        if a == b || a == c || b == c {
            let (doubled, single) = if a == b {
                (a, c)
            } else if a == c {
                (a, b)
            } else {
                (b, a)
            };
            let mut moves = vec![doubled; 4];
            moves.push(single);
            return ExpandedRoll {
                moves,
                repeats_turn: false,
            };
        }

        ExpandedRoll {
            moves: vec![a, b, c],
            repeats_turn: false,
        }
    }
}

/// Estado completo de la partida.
#[derive(Debug, Clone)]
pub struct GameState {
    pub board: Board,
    pub turn: Player,
    pub white: PlayerState,
    pub black: PlayerState,
}

impl GameState {
    /// true si el jugador ganó: sus 15 fichas completaron la vuelta y
    /// fueron retiradas del tablero (RULES.md, sección 7).
    pub fn has_won(&self, player: Player) -> bool {
        let state = match player {
            Player::White => &self.white,
            Player::Black => &self.black,
        };
        state.borne_off == 15
    }
}
