//! Aplica un turno completo y resuelve el flujo: quién juega a
//! continuación, y con qué dados.
//!
//! Regla clave: un dado sin movimiento legal **no se pierde en el
//! momento** — sigue disponible para reintentarlo más adelante en el
//! mismo turno, después de que otro movimiento cambie el tablero. Recién
//! cuando **ninguno** de los dados que quedan sin jugar tiene movimiento
//! posible, esos se dan por definitivamente sin jugar. En ese punto:
//!
//! - si esta tirada era **propia** (recién tirada), esos dados **pasan al
//!   rival**: en su próximo turno los juega directo en vez de tirar los
//!   suyos (`PlayerState.pending_dice`).
//! - si esta tirada ya era **heredada** del rival, lo que no se puede
//!   jugar se **pierde para siempre** — no rebota de nuevo.
//!
//! `play_turn` es la versión "todo de una" pensada para un loop
//! bloqueante como el CLI, donde se le puede preguntar algo al jugador y
//! obtener la respuesta al instante. Para un servidor (donde la pregunta
//! y la respuesta quedan separadas por un viaje de red), las piezas de
//! abajo están expuestas por separado: `apply_move` para aplicar una sola
//! jugada, y `finish_turn` para cerrar el turno una vez que no queda nada
//! más por intentar — `play_turn` está construida encima de esas mismas
//! dos funciones, no duplica la lógica.

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

/// Qué pasó al intentar jugar un dado puntual dentro del turno.
#[derive(Debug, Clone, Copy)]
pub enum DieOutcome {
    /// El movimiento aplicado, y si ESTE movimiento fue el que hizo que
    /// el postillón llegara a su cuadrante final (para poder avisarlo).
    Applied(Move, bool),
    /// El dado quedó definitivamente sin jugar: en el momento en que se
    /// resolvió, ningún dado restante (incluido este) tenía movimiento
    /// legal. Ver `TurnLog.leftover_dice` para qué pasa con él.
    Unplayable(u8),
}

/// Resultado completo de un turno.
#[derive(Debug)]
pub struct TurnLog {
    pub outcomes: Vec<DieOutcome>,
    /// true si el turno pasa al rival al terminar.
    pub turn_passes: bool,
    /// true si fue un triple propio, se jugó completo sin dados sueltos,
    /// y por eso el mismo jugador vuelve a tirar (RULES.md sección 3).
    pub repeats_turn: bool,
    /// Si alguien ganó la partida durante este turno.
    pub winner: Option<Player>,
    /// Dados que quedaron definitivamente sin jugar. Si esta tirada era
    /// propia (`leftover_discarded == false`), ya quedaron guardados en
    /// `pending_dice` del rival. Si era heredada (`leftover_discarded ==
    /// true`), se perdieron para siempre.
    pub leftover_dice: Vec<u8>,
    pub leftover_discarded: bool,
}

/// Chequeo de derrota por postillón atrapado (RULES.md sección 6), a
/// hacer antes de tirar/jugar dados heredados. No depende de los dados
/// del turno. Devuelve el `TurnLog` de derrota si aplica, o `None` si el
/// jugador puede seguir jugando normalmente.
pub fn check_courier_trapped(state: &GameState, player: Player) -> Option<TurnLog> {
    if !state.player_state(player).courier.arrived
        && crate::rules::is_courier_trapped(&state.board, player)
    {
        Some(TurnLog {
            outcomes: Vec::new(),
            turn_passes: false,
            repeats_turn: false,
            winner: Some(player.opponent()),
            leftover_dice: Vec::new(),
            leftover_discarded: false,
        })
    } else {
        None
    }
}

/// true si, con el tablero como está ahora, ninguno de los `remaining`
/// tiene movimiento legal para `player`.
pub fn all_remaining_dead(state: &GameState, player: Player, remaining: &[u8]) -> bool {
    let courier_arrived = state.player_state(player).courier.arrived;
    !remaining
        .iter()
        .any(|&d| !legal_moves_for_die(&state.board, player, d, courier_arrived).is_empty())
}

/// Aplica un solo movimiento sobre `state`. Devuelve true si ESTE
/// movimiento fue el que hizo que el postillón de `player` llegara a su
/// cuadrante final (para poder avisarlo).
pub fn apply_move(state: &mut GameState, player: Player, mv: Move) -> bool {
    match mv {
        Move::OnBoard { from, to } => {
            remove_piece(&mut state.board, player, from);
            add_piece(&mut state.board, player, to);
            mark_courier_if_arrived(state, player, to)
        }
        Move::BearOff { from } => {
            remove_piece(&mut state.board, player, from);
            state.player_state_mut(player).borne_off += 1;
            false
        }
    }
}

/// Cierra el turno de `player` una vez que no queda nada más por intentar
/// (`leftover` son los dados que terminaron sin poder jugarse — puede
/// estar vacío). Decide si el turno pasa, si se repite, y a quién le
/// quedan `pending_dice`. No mira el resultado de partida (`has_won`):
/// eso se chequea aparte, en el momento de cada `apply_move`.
pub fn finish_turn(
    state: &mut GameState,
    outcomes: Vec<DieOutcome>,
    leftover: Vec<u8>,
    is_inherited: bool,
    repeats_turn_from_roll: bool,
) -> TurnLog {
    let player = state.turn;
    let opponent = player.opponent();

    if leftover.is_empty() {
        let repeats = !is_inherited && repeats_turn_from_roll;
        if !repeats {
            state.turn = opponent;
        }
        return TurnLog {
            outcomes,
            turn_passes: !repeats,
            repeats_turn: repeats,
            winner: None,
            leftover_dice: Vec::new(),
            leftover_discarded: false,
        };
    }

    let discarded = is_inherited;
    if !discarded {
        state.player_state_mut(opponent).pending_dice = Some(leftover.clone());
    }
    state.turn = opponent;

    TurnLog {
        outcomes,
        turn_passes: true,
        repeats_turn: false,
        winner: None,
        leftover_dice: leftover,
        leftover_discarded: discarded,
    }
}

/// Aplica un turno completo sobre `state`, de punta a punta, en un solo
/// llamado bloqueante — pensada para el CLI. Ver el módulo para el
/// diseño; internamente usa `apply_move` y `finish_turn`.
///
/// `dice` es el multiset completo a jugar este turno (los movimientos ya
/// expandidos de una tirada propia, o los dados heredados tal cual).
/// `repeats_turn_from_roll` solo importa cuando `is_inherited` es false.
///
/// `choose_die(state, remaining)` se llama en cada paso; SIEMPRE hay al
/// menos uno con movimiento legal en ese momento. Debe devolver el
/// índice, dentro de `remaining`, de uno de esos dados jugables.
///
/// `choose_move` decide, cuando un dado admite más de un movimiento
/// legal, cuál aplicar. `on_outcome` se llama en el momento en que se
/// resuelve cada dado.
pub fn play_turn(
    state: &mut GameState,
    dice: &[u8],
    repeats_turn_from_roll: bool,
    is_inherited: bool,
    mut choose_die: impl FnMut(&GameState, &[u8]) -> usize,
    mut choose_move: impl FnMut(&[Move]) -> Move,
    mut on_outcome: impl FnMut(DieOutcome),
) -> TurnLog {
    let player = state.turn;

    if is_inherited {
        // Estos dados se consumen ahora, vengan o no a jugarse todos.
        state.player_state_mut(player).pending_dice = None;
    }

    if let Some(trapped) = check_courier_trapped(state, player) {
        return trapped;
    }

    let mut remaining: Vec<u8> = dice.to_vec();
    let mut outcomes = Vec::new();

    loop {
        if remaining.is_empty() {
            break;
        }

        if all_remaining_dead(state, player, &remaining) {
            for &d in &remaining {
                let outcome = DieOutcome::Unplayable(d);
                on_outcome(outcome);
                outcomes.push(outcome);
            }
            break;
        }

        let courier_arrived = state.player_state(player).courier.arrived;
        let pick = choose_die(state, &remaining);
        let die = remaining[pick];
        let legal = legal_moves_for_die(&state.board, player, die, courier_arrived);

        if legal.is_empty() {
            // choose_die debía elegir uno jugable; si igual devolvió uno
            // sin movimiento, lo sacamos de la ronda para no colgar el
            // loop, sin tratarlo como definitivamente perdido del grupo.
            remaining.remove(pick);
            let outcome = DieOutcome::Unplayable(die);
            on_outcome(outcome);
            outcomes.push(outcome);
            continue;
        }

        remaining.remove(pick);
        let mv = choose_move(&legal);
        let courier_just_arrived = apply_move(state, player, mv);
        let outcome = DieOutcome::Applied(mv, courier_just_arrived);
        on_outcome(outcome);
        outcomes.push(outcome);

        if state.has_won(player) {
            return TurnLog {
                outcomes,
                turn_passes: false,
                repeats_turn: false,
                winner: Some(player),
                leftover_dice: Vec::new(),
                leftover_discarded: false,
            };
        }
    }

    let leftover: Vec<u8> = outcomes
        .iter()
        .filter_map(|o| match o {
            DieOutcome::Unplayable(d) => Some(*d),
            _ => None,
        })
        .collect();

    finish_turn(state, outcomes, leftover, is_inherited, repeats_turn_from_roll)
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

fn mark_courier_if_arrived(state: &mut GameState, player: Player, to_relative: u8) -> bool {
    let ps = state.player_state_mut(player);
    if !ps.courier.arrived && to_relative >= HOME_START {
        ps.courier.arrived = true;
        true
    } else {
        false
    }
}
