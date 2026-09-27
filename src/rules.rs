//! Generación de movimientos legales.
//! Ver RULES.md para las reglas y DESIGN.md para el razonamiento de diseño.
//!
//! Modelo de posición: cada ficha se ubica en una posición *relativa* al
//! camino propio del jugador, de 0 (su punto de partida) a 23 (el último
//! punto de su cuadrante final). `absolute_index` traduce esa posición
//! relativa al índice absoluto (0..=23) del tablero compartido, que es lo
//! que usamos para chequear bloqueos contra el rival.
//!
//! No hace falta una lista de fichas aparte: como el tablero (`Board`) ya
//! guarda cuántas fichas de qué jugador hay en cada punto absoluto, basta
//! con invertir `absolute_index` para saber, para un jugador dado, cuántas
//! fichas tiene en cada posición relativa de su propio camino.

use crate::types::{Board, Courier, GameState, Player, PlayerState, Point};
use std::collections::BTreeMap;

/// Cantidad de puntos del tablero (y por lo tanto, longitud del camino de
/// cada jugador).
pub const PATH_LEN: u8 = 24;

/// Primera posición relativa del cuadrante final (las últimas 6 casillas
/// del camino, 18..=23). RULES.md, sección 2 y 7.
pub const HOME_START: u8 = 18;

/// Un movimiento legal de una sola ficha con un solo dado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    /// Mueve una ficha de una posición relativa a otra, ambas en el tablero.
    OnBoard { from: u8, to: u8 },
    /// Retira una ficha del tablero (bear off) desde `from`.
    BearOff { from: u8 },
}

/// Traduce una posición relativa del camino de `player` a un índice
/// absoluto del tablero (0..=23).
///
/// Asumimos que los puntos de partida de ambos jugadores están separados
/// por medio tablero (12 puntos), ya que están en esquinas diagonalmente
/// opuestas (RULES.md, sección 2). Si el layout real difiere, solo hay que
/// ajustar `start_offset`.
pub fn absolute_index(player: Player, relative: u8) -> usize {
    let start = start_offset(player);
    ((start as u16 + relative as u16) % PATH_LEN as u16) as usize
}

/// Inversa de `absolute_index`: dado un punto absoluto y un jugador,
/// devuelve la posición relativa en el camino de ese jugador.
fn relative_index(player: Player, absolute: usize) -> u8 {
    let start = start_offset(player) as i16;
    ((absolute as i16 - start).rem_euclid(PATH_LEN as i16)) as u8
}

fn start_offset(player: Player) -> u8 {
    match player {
        Player::White => 0,
        Player::Black => 12,
    }
}

/// Devuelve, para `player`, un mapa de posición relativa -> cantidad de
/// fichas propias en esa posición. Se deriva directamente del tablero, no
/// se guarda por separado (ver DESIGN.md).
pub fn player_positions(board: &Board, player: Player) -> BTreeMap<u8, u8> {
    let mut positions = BTreeMap::new();
    for abs_idx in 0..24usize {
        if let Some((owner, count)) = board.points[abs_idx] {
            if owner == player {
                let rel = relative_index(player, abs_idx);
                positions.insert(rel, count);
            }
        }
    }
    positions
}

/// true si el jugador tiene todas sus fichas restantes en el tablero
/// dentro de su cuadrante final (condición para habilitar el bear off,
/// RULES.md sección 7).
pub fn can_bear_off(board: &Board, player: Player) -> bool {
    player_positions(board, player)
        .keys()
        .all(|&rel| rel >= HOME_START)
}

/// Genera los movimientos legales para `player` con un único valor de dado.
///
/// Antes de que el postillón llegue a su cuadrante final, esta función
/// **solo devuelve movimientos para el postillón** (RULES.md, sección 5):
/// se identifica como la ficha que ya se separó del resto de la pila
/// inicial (o, si ninguna se movió todavía, se elige una del punto de
/// partida para que empiece a serlo).
///
/// Una vez que el postillón llegó a su cuadrante final (`courier_arrived`),
/// se generan movimientos para cualquier ficha, incluyendo bear off cuando
/// corresponde.
pub fn legal_moves_for_die(
    board: &Board,
    player: Player,
    die: u8,
    courier_arrived: bool,
) -> Vec<Move> {
    let positions = player_positions(board, player);

    if !courier_arrived {
        return legal_courier_moves(board, player, die, &positions);
    }

    legal_normal_moves(board, player, die, &positions)
}

fn legal_courier_moves(
    board: &Board,
    player: Player,
    die: u8,
    positions: &BTreeMap<u8, u8>,
) -> Vec<Move> {
    // El postillón es la ficha en la mayor posición relativa > 0 si ya se
    // movió alguna vez; si todas siguen en el punto de partida (posición 0),
    // el postillón sale ahora desde ahí.
    let from = positions
        .keys()
        .copied()
        .filter(|&rel| rel > 0)
        .max()
        .unwrap_or(0);

    let to = from as u16 + die as u16;

    // El postillón no puede salir del tablero (bear off) antes de que las
    // 15 fichas estén reunidas — ver nota de supuesto en la respuesta.
    if to > 23 {
        return vec![];
    }

    let to = to as u8;
    if board.is_blocked_for(absolute_index(player, to), player) {
        return vec![];
    }

    vec![Move::OnBoard { from, to }]
}

fn legal_normal_moves(
    board: &Board,
    player: Player,
    die: u8,
    positions: &BTreeMap<u8, u8>,
) -> Vec<Move> {
    let bear_off_ok = positions.keys().all(|&rel| rel >= HOME_START);
    let mut moves = Vec::new();

    for (&from, &count) in positions.iter() {
        if count == 0 {
            continue;
        }

        let to = from as u16 + die as u16;

        if to <= 23 {
            let to = to as u8;
            if !board.is_blocked_for(absolute_index(player, to), player) {
                moves.push(Move::OnBoard { from, to });
            }
            continue;
        }

        // Movimiento que se pasa del tablero: solo válido como bear off.
        if !bear_off_ok {
            continue;
        }

        if to == 24 {
            moves.push(Move::BearOff { from });
        } else {
            // Sobrepasa el bear off exacto (ej. dado 6 desde la posición 20,
            // que solo necesita 4). Regla estándar de backgammon: se permite
            // si no queda ninguna ficha propia más lejos del final dentro
            // del cuadrante (posición relativa menor). Esto NO está
            // confirmado contra la variante de tu abuela — es un supuesto.
            let farther_exists = positions.keys().any(|&rel| rel < from && rel >= HOME_START);
            if !farther_exists {
                moves.push(Move::BearOff { from });
            }
        }
    }

    moves
}

/// true si `player` tiene al menos un movimiento legal en toda la tirada
/// expandida (para saber si se pierde el turno completo). Ver RULES.md,
/// sección 3: cada dado sin movimiento legal se pierde individualmente;
/// esta función solo chequea un valor puntual.
pub fn has_legal_move_for_die(
    board: &Board,
    player: Player,
    die: u8,
    courier_arrived: bool,
) -> bool {
    !legal_moves_for_die(board, player, die, courier_arrived).is_empty()
}

/// true si el postillón de `player` todavía no llegó a su cuadrante final
/// y el rival ocupa los 6 puntos de esa zona (RULES.md, sección 6): no hay
/// ningún valor de dado que lo pueda hacer entrar, así que el jugador
/// pierde la partida de inmediato.
///
/// No depende de la tirada actual: es una condición estructural del
/// tablero. Se llama al empezar el turno de `player`, antes de tirar los
/// dados (ver `turn.rs`).
pub fn is_courier_trapped(board: &Board, player: Player) -> bool {
    let opponent = player.opponent();
    (HOME_START..PATH_LEN).all(|relative| {
        let abs = absolute_index(player, relative);
        matches!(board.points[abs], Some((owner, _)) if owner == opponent)
    })
}

/// Arma el estado inicial de una partida: ambos jugadores con sus 15
/// fichas apiladas en su propio punto de partida, turno de White primero,
/// ningún postillón llegado todavía.
pub fn new_game() -> GameState {
    let mut points: [Point; 24] = [None; 24];
    points[absolute_index(Player::White, 0)] = Some((Player::White, 15));
    points[absolute_index(Player::Black, 0)] = Some((Player::Black, 15));

    let fresh_player = || PlayerState {
        courier: Courier {
            position: None,
            arrived: false,
        },
        borne_off: 0,
        pending_dice: None,
    };

    GameState {
        board: Board { points },
        turn: Player::White,
        white: fresh_player(),
        black: fresh_player(),
    }
}
