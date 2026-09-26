//! Aplica una tirada completa (3-6 movimientos según dobles/triples) y
//! resuelve el flujo de turno: quién juega a continuación y por qué.
//!
//! Dos decisiones de diseño a tener presentes:
//!
//! 1. El **orden** en que se intentan los dados lo elige quien llama a
//!    `play_turn` (representa la elección libre del jugador humano —
//!    RULES.md sección 3). El motor no busca automáticamente el orden que
//!    permite jugar más dados; eso queda para una capa de UI/IA más
//!    adelante si hace falta.
//! 2. En cuanto un dado no tiene ningún movimiento legal, **se pierde ese
//!    dado y el turno termina ahí mismo** — no se siguen probando los
//!    dados restantes de la tirada. Esto es lo que confirmaste sobre la
//!    regla general de dados sin movimiento (RULES.md sección 3).

use crate::rules::{legal_moves_for_die, Move, HOME_START};
use crate::types::{Board, GameState, Player, PlayerState};

impl GameState {
    pub fn player_state(&self, player: Player) -> &PlayerState {
        match player {
            Player::White => &self.white,
            Player::Black => &self.black,
        }
    }

    pub fn player_state_mut(&mut self, player: Player) -> &mut PlayerState {
        match player {
            Player::White => &mut self.white,
            Player::Black => &mut self.black,
        }
    }
}

/// Qué pasó al intentar jugar un dado puntual dentro de la tirada.
#[derive(Debug, Clone, Copy)]
pub enum DieOutcome {
    Applied(Move),
    /// El dado no tenía movimiento legal: se pierde y termina el turno.
    Forfeited(u8),
}

/// Resultado completo de aplicar una tirada.
#[derive(Debug)]
pub struct TurnLog {
    pub outcomes: Vec<DieOutcome>,
    /// true si el turno pasa al rival al terminar esta tirada.
    pub turn_passes: bool,
    /// true si fue un triple y el mismo jugador vuelve a tirar
    /// (RULES.md sección 3). Mutuamente excluyente con `turn_passes`.
    pub repeats_turn: bool,
    /// Si alguien ganó la partida durante esta tirada.
    pub winner: Option<Player>,
}

/// Aplica una tirada ya expandida (ver `Roll::expand`) sobre `state`.
///
/// `order` es la secuencia de valores de dado que el jugador quiere
/// intentar, en el orden que elija (debe ser una permutación de los
/// movimientos expandidos). `choose` decide, cuando un dado tiene más de
/// un movimiento legal posible (varias fichas pueden jugarlo), cuál
/// aplicar — típicamente vendría de la elección del jugador en la UI.
pub fn play_turn(
    state: &mut GameState,
    order: &[u8],
    repeats_turn_from_roll: bool,
    mut choose: impl FnMut(&[Move]) -> Move,
) -> TurnLog {
    let player = state.turn;

    // Chequeo de derrota por postillón atrapado (RULES.md sección 6): se
    // resuelve antes de tirar, no depende de los dados de esta jugada.
    if !state.player_state(player).courier.arrived
        && crate::rules::is_courier_trapped(&state.board, player)
    {
        return TurnLog {
            outcomes: Vec::new(),
            turn_passes: false,
            repeats_turn: false,
            winner: Some(player.opponent()),
        };
    }

    let mut outcomes = Vec::new();
    let mut forfeited = false;

    for &die in order {
        let courier_arrived = state.player_state(player).courier.arrived;
        let legal = legal_moves_for_die(&state.board, player, die, courier_arrived);

        if legal.is_empty() {
            outcomes.push(DieOutcome::Forfeited(die));
            forfeited = true;
            break;
        }

        let mv = choose(&legal);
        apply_move(state, player, mv);
        outcomes.push(DieOutcome::Applied(mv));

        if state.has_won(player) {
            return TurnLog {
                outcomes,
                turn_passes: false,
                repeats_turn: false,
                winner: Some(player),
            };
        }
    }

    // Un dado perdido corta el turno de inmediato, aunque la tirada haya
    // sido un triple: no llegaste a terminar de jugarla, así que no hay
    // repetición de turno posible.
    let turn_passes = forfeited || !repeats_turn_from_roll;
    let repeats_turn = !forfeited && repeats_turn_from_roll;

    if turn_passes {
        state.turn = player.opponent();
    }

    TurnLog {
        outcomes,
        turn_passes,
        repeats_turn,
        winner: None,
    }
}

fn apply_move(state: &mut GameState, player: Player, mv: Move) {
    match mv {
        Move::OnBoard { from, to } => {
            remove_piece(&mut state.board, player, from);
            add_piece(&mut state.board, player, to);
            mark_courier_if_arrived(state, player, to);
        }
        Move::BearOff { from } => {
            remove_piece(&mut state.board, player, from);
            state.player_state_mut(player).borne_off += 1;
        }
    }
}

fn remove_piece(board: &mut Board, player: Player, from_relative: u8) {
    let idx = crate::rules::absolute_index(player, from_relative);
    if let Some((owner, count)) = board.points[idx] {
        debug_assert_eq!(owner, player, "quitando ficha del punto equivocado");
        board.points[idx] = if count > 1 {
            Some((owner, count - 1))
        } else {
            None
        };
    }
}

fn add_piece(board: &mut Board, player: Player, to_relative: u8) {
    let idx = crate::rules::absolute_index(player, to_relative);
    board.points[idx] = match board.points[idx] {
        Some((owner, count)) if owner == player => Some((owner, count + 1)),
        _ => Some((player, 1)),
    };
}

fn mark_courier_if_arrived(state: &mut GameState, player: Player, to_relative: u8) {
    let ps = state.player_state_mut(player);
    if !ps.courier.arrived && to_relative >= HOME_START {
        ps.courier.arrived = true;
    }
}
