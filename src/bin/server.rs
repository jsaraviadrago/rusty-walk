//! Servidor multijugador de Jacquet.
//!
//! Reusa el motor de reglas (`jacquet::rules`, `jacquet::turn`) tal cual
//! está — no reimplementa ninguna regla acá. Este archivo solo se encarga
//! de:
//!
//! - Salas de partida identificadas por un código corto (ej. "AB3F").
//! - Conectar hasta 2 jugadores por sala vía WebSocket.
//! - Ser el árbitro: cada jugada que llega del cliente se valida contra
//!   `legal_moves_for_die` antes de aplicarse.
//! - Avisarle a ambos jugadores, en vivo, cada cambio de estado.
//!
//! Limitaciones conocidas de esta primera versión (documentadas a
//! propósito, no son bugs no vistos):
//! - Si alguien se desconecta a mitad de una tirada y vuelve a conectar,
//!   recibe el estado actual del tablero, pero no un reenvío exacto de un
//!   `choose_die`/`choose_move` que hubiera quedado pendiente para él.
//! - No hay reconexión con el mismo "asiento" garantizada más allá de
//!   quedar libre el lugar que dejó — si otro se conecta primero a esa
//!   sala, toma ese lugar.
//! - Todo vive en memoria: si el proceso se reinicia, las salas
//!   desaparecen.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use jacquet::rules::{legal_moves_for_die, new_game, Move};
use jacquet::turn::{all_remaining_dead, apply_move, check_courier_trapped, finish_turn, DieOutcome};
use jacquet::{GameState, Player, Roll};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tower_http::cors::CorsLayer;

type Senders = [Option<mpsc::UnboundedSender<Message>>; 2];

struct TurnProgress {
    remaining: Vec<u8>,
    is_inherited: bool,
    repeats_turn_from_roll: bool,
}

struct Room {
    state: GameState,
    in_progress: Option<TurnProgress>,
    winner: Option<Player>,
    started: bool,
    senders: Senders,
}

impl Room {
    fn new() -> Self {
        Room {
            state: new_game(),
            in_progress: None,
            winner: None,
            started: false,
            senders: [None, None],
        }
    }
}

type Rooms = Arc<Mutex<HashMap<String, Room>>>;

#[tokio::main]
async fn main() {
    let rooms: Rooms = Arc::new(Mutex::new(HashMap::new()));

    let app = Router::new()
        .route("/rooms", post(create_room))
        .route("/ws/:code", get(ws_handler))
        .layer(CorsLayer::permissive())
        .with_state(rooms);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("no se pudo abrir el puerto 3000");

    println!("Jacquet server escuchando en http://0.0.0.0:3000");
    axum::serve(listener, app).await.expect("el servidor se cayó");
}

async fn create_room(State(rooms): State<Rooms>) -> Json<serde_json::Value> {
    let mut guard = rooms.lock().unwrap();
    let code = loop {
        let candidate = random_room_code();
        if !guard.contains_key(&candidate) {
            break candidate;
        }
    };
    guard.insert(code.clone(), Room::new());
    Json(serde_json::json!({ "code": code }))
}

async fn ws_handler(
    Path(code): Path<String>,
    State(rooms): State<Rooms>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, code, rooms))
}

async fn handle_socket(socket: WebSocket, code: String, rooms: Rooms) {
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();

    // Asignar un lugar (White o Black) dentro de la sala, si hay uno libre.
    let assigned_player: Option<Player> = {
        let mut guard = rooms.lock().unwrap();
        match guard.get_mut(&code) {
            None => None,
            Some(room) => {
                if room.senders[0].is_none() {
                    room.senders[0] = Some(tx.clone());
                    Some(Player::White)
                } else if room.senders[1].is_none() {
                    room.senders[1] = Some(tx.clone());
                    Some(Player::Black)
                } else {
                    None
                }
            }
        }
    };

    // Tarea que reenvía todo lo que se le encole a este jugador hacia el
    // socket real. Vive mientras dure la conexión.
    let forward_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if ws_sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    let player = match assigned_player {
        Some(p) => p,
        None => {
            let _ = tx.send(ws_text(&ServerMsg::Error {
                message: "la sala no existe o ya está completa".to_string(),
            }));
            drop(tx);
            let _ = forward_task.await;
            return;
        }
    };

    // Avisarle a este jugador quién es, y a ambos el estado actual.
    {
        let guard = rooms.lock().unwrap();
        if let Some(room) = guard.get(&code) {
            send_to_player(
                room,
                player,
                &ServerMsg::Joined {
                    player: player_str(player),
                    code: code.clone(),
                },
            );
            broadcast_state(room);
        }
    }

    // Si con este jugador ya están los dos conectados y todavía no arrancó
    // ningún turno, arrancamos el primero.
    {
        let mut guard = rooms.lock().unwrap();
        if let Some(room) = guard.get_mut(&code) {
            let both_here = room.senders[0].is_some() && room.senders[1].is_some();
            if both_here && !room.started {
                room.started = true;
                start_new_turn(room);
            }
        }
    }

    while let Some(Ok(msg)) = ws_receiver.next().await {
        if let Message::Text(text) = msg {
            match serde_json::from_str::<ClientMsg>(&text) {
                Ok(client_msg) => handle_client_msg(&rooms, &code, player, client_msg),
                Err(_) => {
                    let guard = rooms.lock().unwrap();
                    if let Some(room) = guard.get(&code) {
                        send_to_player(
                            room,
                            player,
                            &ServerMsg::Error {
                                message: "mensaje inválido".to_string(),
                            },
                        );
                    }
                }
            }
        }
    }

    // Se desconectó: liberar su lugar para que alguien más se pueda unir.
    {
        let mut guard = rooms.lock().unwrap();
        if let Some(room) = guard.get_mut(&code) {
            let idx = if player == Player::White { 0 } else { 1 };
            room.senders[idx] = None;
        }
    }
    let _ = forward_task.await;
}

fn handle_client_msg(rooms: &Rooms, code: &str, player: Player, msg: ClientMsg) {
    match msg {
        ClientMsg::Roll => on_roll(rooms, code, player),
        ClientMsg::ChooseDie { index } => on_choose_die(rooms, code, player, index),
        ClientMsg::ChooseMove { index } => on_choose_move(rooms, code, player, index),
    }
}

fn on_roll(rooms: &Rooms, code: &str, player: Player) {
    {
        let mut guard = rooms.lock().unwrap();
        let room = match guard.get_mut(code) {
            Some(r) => r,
            None => return,
        };
        if room.winner.is_some() {
            return;
        }
        if room.state.turn != player {
            send_to_player(
                room,
                player,
                &ServerMsg::Error {
                    message: "no es tu turno".to_string(),
                },
            );
            return;
        }
        if room.in_progress.is_some() {
            send_to_player(
                room,
                player,
                &ServerMsg::Error {
                    message: "ya hay una tirada en curso".to_string(),
                },
            );
            return;
        }
        if room.state.player_state(player).pending_dice.is_some() {
            send_to_player(
                room,
                player,
                &ServerMsg::Error {
                    message: "tenés dados heredados, no se tira".to_string(),
                },
            );
            return;
        }

        let dice = roll_three_dice();
        let expanded = Roll { dice }.expand();
        room.in_progress = Some(TurnProgress {
            remaining: expanded.moves,
            is_inherited: false,
            repeats_turn_from_roll: expanded.repeats_turn,
        });
        broadcast(
            room,
            &ServerMsg::Dice {
                player: player_str(player),
                dice: dice.to_vec(),
                inherited: false,
                repeats_possible: expanded.repeats_turn,
            },
        );
    }
    advance_turn(rooms, code, None, None);
}

fn on_choose_die(rooms: &Rooms, code: &str, player: Player, index: usize) {
    {
        let guard = rooms.lock().unwrap();
        let room = match guard.get(code) {
            Some(r) => r,
            None => return,
        };
        if room.winner.is_some() || room.state.turn != player || room.in_progress.is_none() {
            return;
        }
    }
    advance_turn(rooms, code, Some(index), None);
}

fn on_choose_move(rooms: &Rooms, code: &str, player: Player, index: usize) {
    {
        let guard = rooms.lock().unwrap();
        let room = match guard.get(code) {
            Some(r) => r,
            None => return,
        };
        if room.winner.is_some() || room.state.turn != player || room.in_progress.is_none() {
            return;
        }
    }
    advance_turn(rooms, code, None, Some(index));
}

/// El corazón del servidor: intenta avanzar el turno todo lo que se
/// pueda sin volver a preguntarle nada al jugador, y se detiene (soltando
/// el lock) apenas necesita una respuesta real. `forced_die_idx` y
/// `forced_move_idx` traen la respuesta a la última pregunta que se hizo,
/// si la hay.
fn advance_turn(
    rooms: &Rooms,
    code: &str,
    mut forced_die_idx: Option<usize>,
    mut forced_move_idx: Option<usize>,
) {
    let mut guard = rooms.lock().unwrap();
    let room = match guard.get_mut(code) {
        Some(r) => r,
        None => return,
    };
    if room.winner.is_some() {
        return;
    }

    loop {
        let (mut remaining, is_inherited, repeats_turn_from_roll) = match &room.in_progress {
            Some(p) => (p.remaining.clone(), p.is_inherited, p.repeats_turn_from_roll),
            None => return,
        };

        let player = room.state.turn;

        if remaining.is_empty() {
            room.in_progress = None;
            return;
        }

        if all_remaining_dead(&room.state, player, &remaining) {
            let outcomes: Vec<DieOutcome> =
                remaining.iter().map(|&d| DieOutcome::Unplayable(d)).collect();
            for &d in &remaining {
                broadcast(
                    room,
                    &ServerMsg::Unplayable {
                        player: player_str(player),
                        die: d,
                    },
                );
            }
            room.in_progress = None;
            let log = finish_turn(
                &mut room.state,
                outcomes,
                remaining,
                is_inherited,
                repeats_turn_from_roll,
            );
            broadcast(
                room,
                &ServerMsg::TurnEnded {
                    next_player: player_str(room.state.turn),
                    leftover: log.leftover_dice,
                    discarded: log.leftover_discarded,
                    repeats: log.repeats_turn,
                },
            );
            broadcast_state(room);
            if let Some(winner) = log.winner {
                room.winner = Some(winner);
                broadcast(
                    room,
                    &ServerMsg::GameOver {
                        winner: player_str(winner),
                    },
                );
                return;
            }
            start_new_turn(room);
            forced_die_idx = None;
            forced_move_idx = None;
            continue;
        }

        let courier_arrived = room.state.player_state(player).courier.arrived;
        let alive: Vec<usize> = remaining
            .iter()
            .enumerate()
            .filter(|&(_, &d)| {
                !legal_moves_for_die(&room.state.board, player, d, courier_arrived).is_empty()
            })
            .map(|(i, _)| i)
            .collect();

        let die_idx = match forced_die_idx.take() {
            Some(idx) if alive.contains(&idx) => idx,
            Some(_) => {
                send_to_player(
                    room,
                    player,
                    &ServerMsg::Error {
                        message: "ese dado no tiene movimiento ahora".to_string(),
                    },
                );
                return;
            }
            None if alive.len() == 1 => alive[0],
            None => {
                let mut alive_flags = vec![false; remaining.len()];
                for &i in &alive {
                    alive_flags[i] = true;
                }
                send_to_player(
                    room,
                    player,
                    &ServerMsg::ChooseDie {
                        player: player_str(player),
                        remaining: remaining.clone(),
                        alive: alive_flags,
                    },
                );
                return;
            }
        };

        let die = remaining[die_idx];
        let legal = legal_moves_for_die(&room.state.board, player, die, courier_arrived);

        let mv = match forced_move_idx.take() {
            Some(idx) => match legal.get(idx) {
                Some(&m) => m,
                None => {
                    send_to_player(
                        room,
                        player,
                        &ServerMsg::Error {
                            message: "opción inválida".to_string(),
                        },
                    );
                    return;
                }
            },
            None if legal.len() == 1 => legal[0],
            None => {
                let options: Vec<MoveDto> = legal.iter().map(|&m| m.into()).collect();
                send_to_player(
                    room,
                    player,
                    &ServerMsg::ChooseMove {
                        player: player_str(player),
                        die,
                        options,
                    },
                );
                return;
            }
        };

        let just_arrived = apply_move(&mut room.state, player, mv);
        remaining.remove(die_idx);
        if let Some(p) = room.in_progress.as_mut() {
            p.remaining = remaining;
        }

        broadcast(
            room,
            &ServerMsg::Applied {
                player: player_str(player),
                mv: mv.into(),
                courier_arrived: just_arrived,
            },
        );
        broadcast_state(room);

        if room.state.has_won(player) {
            room.winner = Some(player);
            room.in_progress = None;
            broadcast(
                room,
                &ServerMsg::GameOver {
                    winner: player_str(player),
                },
            );
            return;
        }
    }
}

/// Arranca el turno de `room.state.turn`: chequea postillón atrapado,
/// materializa dados heredados si los hay, o le avisa al jugador que
/// puede tirar.
fn start_new_turn(room: &mut Room) {
    let player = room.state.turn;

    if let Some(trapped) = check_courier_trapped(&room.state, player) {
        room.in_progress = None;
        if let Some(winner) = trapped.winner {
            room.winner = Some(winner);
            broadcast(
                room,
                &ServerMsg::GameOver {
                    winner: player_str(winner),
                },
            );
        }
        return;
    }

    if let Some(pending) = room.state.player_state(player).pending_dice.clone() {
        room.state.player_state_mut(player).pending_dice = None;
        room.in_progress = Some(TurnProgress {
            remaining: pending.clone(),
            is_inherited: true,
            repeats_turn_from_roll: false,
        });
        broadcast(
            room,
            &ServerMsg::Dice {
                player: player_str(player),
                dice: pending,
                inherited: true,
                repeats_possible: false,
            },
        );
    } else {
        room.in_progress = None;
        broadcast(
            room,
            &ServerMsg::AwaitRoll {
                player: player_str(player),
            },
        );
    }
    broadcast_state(room);
}

// ---------------------------------------------------------------------
// Envío de mensajes
// ---------------------------------------------------------------------

fn ws_text(msg: &ServerMsg) -> Message {
    Message::Text(serde_json::to_string(msg).unwrap_or_default())
}

fn send_to_player(room: &Room, player: Player, msg: &ServerMsg) {
    let idx = if player == Player::White { 0 } else { 1 };
    if let Some(tx) = &room.senders[idx] {
        let _ = tx.send(ws_text(msg));
    }
}

fn broadcast(room: &Room, msg: &ServerMsg) {
    for sender in room.senders.iter().flatten() {
        let _ = sender.send(ws_text(msg));
    }
}

fn broadcast_state(room: &Room) {
    broadcast(room, &ServerMsg::State { state: snapshot(room) });
}

// ---------------------------------------------------------------------
// Serialización del estado
// ---------------------------------------------------------------------

fn player_str(p: Player) -> &'static str {
    match p {
        Player::White => "white",
        Player::Black => "black",
    }
}

#[derive(Serialize)]
struct PointDto {
    idx: usize,
    owner: Option<&'static str>,
    count: u8,
}

#[derive(Serialize)]
struct PlayerDto {
    courier_arrived: bool,
    borne_off: u8,
    pending_dice: Option<Vec<u8>>,
}

#[derive(Serialize)]
struct StateDto {
    points: Vec<PointDto>,
    turn: &'static str,
    white: PlayerDto,
    black: PlayerDto,
    winner: Option<&'static str>,
}

fn snapshot(room: &Room) -> StateDto {
    let points = (0..24)
        .map(|idx| match room.state.board.points[idx] {
            Some((owner, count)) => PointDto {
                idx,
                owner: Some(player_str(owner)),
                count,
            },
            None => PointDto {
                idx,
                owner: None,
                count: 0,
            },
        })
        .collect();

    let player_dto = |p: Player| {
        let ps = room.state.player_state(p);
        PlayerDto {
            courier_arrived: ps.courier.arrived,
            borne_off: ps.borne_off,
            pending_dice: ps.pending_dice.clone(),
        }
    };

    StateDto {
        points,
        turn: player_str(room.state.turn),
        white: player_dto(Player::White),
        black: player_dto(Player::Black),
        winner: room.winner.map(player_str),
    }
}

#[derive(Serialize, Clone, Copy)]
#[serde(tag = "kind")]
enum MoveDto {
    #[serde(rename = "board")]
    Board { from: u8, to: u8 },
    #[serde(rename = "bearoff")]
    BearOff { from: u8 },
}

impl From<Move> for MoveDto {
    fn from(m: Move) -> Self {
        match m {
            Move::OnBoard { from, to } => MoveDto::Board { from, to },
            Move::BearOff { from } => MoveDto::BearOff { from },
        }
    }
}

// ---------------------------------------------------------------------
// Protocolo (mensajes servidor -> cliente y cliente -> servidor)
// ---------------------------------------------------------------------

#[derive(Serialize)]
#[serde(tag = "type")]
enum ServerMsg {
    #[serde(rename = "joined")]
    Joined { player: &'static str, code: String },
    #[serde(rename = "state")]
    State { state: StateDto },
    #[serde(rename = "await_roll")]
    AwaitRoll { player: &'static str },
    #[serde(rename = "dice")]
    Dice {
        player: &'static str,
        dice: Vec<u8>,
        inherited: bool,
        repeats_possible: bool,
    },
    #[serde(rename = "choose_die")]
    ChooseDie {
        player: &'static str,
        remaining: Vec<u8>,
        alive: Vec<bool>,
    },
    #[serde(rename = "choose_move")]
    ChooseMove {
        player: &'static str,
        die: u8,
        options: Vec<MoveDto>,
    },
    #[serde(rename = "applied")]
    Applied {
        player: &'static str,
        mv: MoveDto,
        courier_arrived: bool,
    },
    #[serde(rename = "unplayable")]
    Unplayable { player: &'static str, die: u8 },
    #[serde(rename = "turn_ended")]
    TurnEnded {
        next_player: &'static str,
        leftover: Vec<u8>,
        discarded: bool,
        repeats: bool,
    },
    #[serde(rename = "game_over")]
    GameOver { winner: &'static str },
    #[serde(rename = "error")]
    Error { message: String },
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ClientMsg {
    #[serde(rename = "roll")]
    Roll,
    #[serde(rename = "choose_die")]
    ChooseDie { index: usize },
    #[serde(rename = "choose_move")]
    ChooseMove { index: usize },
}

// ---------------------------------------------------------------------
// Utilidades sin dependencias externas (mismo criterio que en main.rs:
// nada de `rand`, para no repetir el problema de compilación de antes).
// ---------------------------------------------------------------------

fn xorshift_next(seed: &mut u64) -> u64 {
    let mut x = *seed;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *seed = x;
    x
}

fn fresh_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x2545F4914F6CDD1D);
    nanos | 1
}

fn roll_three_dice() -> [u8; 3] {
    let mut seed = fresh_seed();
    [
        (xorshift_next(&mut seed) % 6) as u8 + 1,
        (xorshift_next(&mut seed) % 6) as u8 + 1,
        (xorshift_next(&mut seed) % 6) as u8 + 1,
    ]
}

fn random_room_code() -> String {
    const CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789"; // sin 0/O ni 1/I
    let mut seed = fresh_seed();
    (0..4)
        .map(|_| CHARS[(xorshift_next(&mut seed) % CHARS.len() as u64) as usize] as char)
        .collect()
}
