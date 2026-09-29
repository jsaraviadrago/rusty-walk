//! Servidor multijugador de Jacquet.
//!
//! Reusa el motor de reglas (`jacquet::rules`, `jacquet::turn`) tal cual
//! está — no reimplementa ninguna regla acá. Este archivo se encarga de:
//!
//! - Salas de partida identificadas por un código corto (ej. "AB3F").
//! - Conectar hasta 2 jugadores por sala vía WebSocket.
//! - Ser el árbitro: cada jugada que llega del cliente se valida contra
//!   `legal_moves_for_die` antes de aplicarse.
//! - Avisarle a ambos jugadores, en vivo, cada cambio de estado.
//! - Si hay credenciales de Supabase configuradas (`SUPABASE_URL` +
//!   `SUPABASE_SECRET_KEY`), guardar cada partida y cada jugada ahí para
//!   poder consultarlas después. Sin esas variables, el servidor funciona
//!   igual — simplemente no guarda historial.
//!
//! Limitaciones conocidas (documentadas a propósito, no son bugs no
//! vistos):
//! - Si alguien se desconecta a mitad de una tirada y vuelve a conectar,
//!   recibe el estado actual del tablero, pero no un reenvío exacto de un
//!   `choose_die`/`choose_move` que hubiera quedado pendiente para él.
//! - No hay reconexión con el mismo "asiento" garantizada más allá de
//!   quedar libre el lugar que dejó.
//! - El estado de las salas vive en memoria: si el proceso se reinicia,
//!   las salas activas desaparecen (el historial ya guardado en Supabase
//!   no se pierde, solo las partidas que estaban a mitad de jugarse).
//! - El guardado en Supabase es "mejor esfuerzo": si falla (por ejemplo,
//!   Supabase caído un momento), la partida sigue jugándose igual, solo
//!   no queda registrada esa jugada puntual.

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
use jacquet::rules::{legal_moves_for_die, new_game};
use jacquet::turn::{all_remaining_dead, apply_move, check_courier_trapped, finish_turn, DieOutcome};
use jacquet::{GameState, Player, Roll};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tower_http::cors::CorsLayer;

type Senders = [Option<mpsc::UnboundedSender<Message>>; 2];

struct TurnProgress {
    remaining: Vec<u8>,
    is_inherited: bool,
    repeats_turn_from_roll: bool,
    /// Si el servidor ya le preguntó al jugador "con cuál ficha" para un
    /// dado puntual (porque tenía más de un movimiento legal), acá queda
    /// el índice de ESE dado dentro de `remaining`, para no tener que
    /// volver a preguntar "cuál dado" cuando llegue la respuesta.
    awaiting_move_for_die_idx: Option<usize>,
}

struct Room {
    state: GameState,
    in_progress: Option<TurnProgress>,
    winner: Option<Player>,
    started: bool,
    senders: Senders,
    /// Id de la fila en la tabla `games` de Supabase, si el guardado de
    /// historial está activo y la inserción inicial funcionó.
    game_id: Option<i64>,
    /// Cuántas jugadas se llevan aplicadas en esta partida (para el
    /// número de secuencia de cada fila en `moves`, y para el total que
    /// se guarda en `games` al terminar).
    move_seq: u32,
}

impl Room {
    fn new() -> Self {
        Room {
            state: new_game(),
            in_progress: None,
            winner: None,
            started: false,
            senders: [None, None],
            game_id: None,
            move_seq: 0,
        }
    }
}

type Rooms = Arc<Mutex<HashMap<String, Room>>>;

/// Estado combinado que axum necesita como un solo tipo para `.with_state`.
/// Las funciones internas del juego reciben `rooms` y `supabase` por
/// separado (no este struct), para no acoplarlas a axum.
#[derive(Clone)]
struct AppState {
    rooms: Rooms,
    supabase: Option<Arc<Supabase>>,
}

#[tokio::main]
async fn main() {
    let rooms: Rooms = Arc::new(Mutex::new(HashMap::new()));
    let supabase = Supabase::from_env();

    if supabase.is_some() {
        println!("Supabase configurado: se va a guardar el historial de partidas.");
    } else {
        println!("Supabase NO configurado (faltan SUPABASE_URL / SUPABASE_SECRET_KEY): el servidor funciona igual, sin guardar historial.");
    }

    let state = AppState { rooms, supabase };

    let app = Router::new()
        .route("/rooms", post(create_room))
        .route("/ws/:code", get(ws_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("no se pudo abrir el puerto 3000");

    println!("Jacquet server escuchando en http://0.0.0.0:3000");
    axum::serve(listener, app).await.expect("el servidor se cayó");
}

async fn create_room(State(state): State<AppState>) -> Json<serde_json::Value> {
    let code = {
        let mut guard = state.rooms.lock().unwrap();
        let code = loop {
            let candidate = random_room_code();
            if !guard.contains_key(&candidate) {
                break candidate;
            }
        };
        guard.insert(code.clone(), Room::new());
        code
    };

    if let Some(sb) = &state.supabase {
        if let Some(game_id) = sb.create_game(&code).await {
            let mut guard = state.rooms.lock().unwrap();
            if let Some(room) = guard.get_mut(&code) {
                room.game_id = Some(game_id);
            }
        }
    }

    Json(serde_json::json!({ "code": code }))
}

async fn ws_handler(
    Path(code): Path<String>,
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, code, state.rooms, state.supabase))
}

async fn handle_socket(
    socket: WebSocket,
    code: String,
    rooms: Rooms,
    supabase: Option<Arc<Supabase>>,
) {
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

    {
        let mut guard = rooms.lock().unwrap();
        if let Some(room) = guard.get_mut(&code) {
            let both_here = room.senders[0].is_some() && room.senders[1].is_some();
            if both_here && !room.started {
                room.started = true;
                start_new_turn(room, &supabase);
            }
        }
    }

    while let Some(Ok(msg)) = ws_receiver.next().await {
        if let Message::Text(text) = msg {
            match serde_json::from_str::<ClientMsg>(&text) {
                Ok(client_msg) => handle_client_msg(&rooms, &supabase, &code, player, client_msg),
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

    {
        let mut guard = rooms.lock().unwrap();
        if let Some(room) = guard.get_mut(&code) {
            let idx = if player == Player::White { 0 } else { 1 };
            room.senders[idx] = None;
        }
    }
    let _ = forward_task.await;
}

fn handle_client_msg(
    rooms: &Rooms,
    supabase: &Option<Arc<Supabase>>,
    code: &str,
    player: Player,
    msg: ClientMsg,
) {
    match msg {
        ClientMsg::Roll => on_roll(rooms, supabase, code, player),
        ClientMsg::ChooseDie { index } => on_choose_die(rooms, supabase, code, player, index),
        ClientMsg::ChooseMove { index } => on_choose_move(rooms, supabase, code, player, index),
    }
}

fn on_roll(rooms: &Rooms, supabase: &Option<Arc<Supabase>>, code: &str, player: Player) {
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
            awaiting_move_for_die_idx: None,
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
    advance_turn(rooms, supabase, code, None, None);
}

fn on_choose_die(
    rooms: &Rooms,
    supabase: &Option<Arc<Supabase>>,
    code: &str,
    player: Player,
    index: usize,
) {
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
    advance_turn(rooms, supabase, code, Some(index), None);
}

fn on_choose_move(
    rooms: &Rooms,
    supabase: &Option<Arc<Supabase>>,
    code: &str,
    player: Player,
    index: usize,
) {
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
    advance_turn(rooms, supabase, code, None, Some(index));
}

/// El corazón del servidor: intenta avanzar el turno todo lo que se
/// pueda sin volver a preguntarle nada al jugador, y se detiene (soltando
/// el lock) apenas necesita una respuesta real. `forced_die_idx` y
/// `forced_move_idx` traen la respuesta a la última pregunta que se hizo,
/// si la hay.
fn advance_turn(
    rooms: &Rooms,
    supabase: &Option<Arc<Supabase>>,
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
        let (mut remaining, is_inherited, repeats_turn_from_roll, awaiting_die_idx) =
            match &room.in_progress {
                Some(p) => (
                    p.remaining.clone(),
                    p.is_inherited,
                    p.repeats_turn_from_roll,
                    p.awaiting_move_for_die_idx,
                ),
                None => return,
            };

        let player = room.state.turn;

        if remaining.is_empty() {
            room.in_progress = None;
            let log = finish_turn(
                &mut room.state,
                Vec::new(),
                Vec::new(),
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
                finish_game_if_configured(room, supabase, winner);
                room.winner = Some(winner);
                broadcast(
                    room,
                    &ServerMsg::GameOver {
                        winner: player_str(winner),
                    },
                );
                return;
            }
            start_new_turn(room, supabase);
            forced_die_idx = None;
            forced_move_idx = None;
            continue;
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
                finish_game_if_configured(room, supabase, winner);
                room.winner = Some(winner);
                broadcast(
                    room,
                    &ServerMsg::GameOver {
                        winner: player_str(winner),
                    },
                );
                return;
            }
            start_new_turn(room, supabase);
            forced_die_idx = None;
            forced_move_idx = None;
            continue;
        }

        let courier_arrived = room.state.player_state(player).courier.arrived;

        let die_idx = if let Some(idx) = awaiting_die_idx {
            idx
        } else {
            let alive: Vec<usize> = remaining
                .iter()
                .enumerate()
                .filter(|&(_, &d)| {
                    !legal_moves_for_die(&room.state.board, player, d, courier_arrived).is_empty()
                })
                .map(|(i, _)| i)
                .collect();

            match forced_die_idx.take() {
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
            }
        };

        let die = remaining[die_idx];
        let legal = legal_moves_for_die(&room.state.board, player, die, courier_arrived);

        if legal.is_empty() {
            if let Some(p) = room.in_progress.as_mut() {
                p.awaiting_move_for_die_idx = None;
            }
            send_to_player(
                room,
                player,
                &ServerMsg::Error {
                    message: "esa jugada ya no es válida, el tablero cambió mientras elegías"
                        .to_string(),
                },
            );
            return;
        }

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
                if let Some(p) = room.in_progress.as_mut() {
                    p.awaiting_move_for_die_idx = Some(die_idx);
                }
                let options: Vec<MoveDto> =
                    legal.iter().map(|&m| move_to_dto(player, m)).collect();
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

        if let Some(p) = room.in_progress.as_mut() {
            p.awaiting_move_for_die_idx = None;
        }

        let just_arrived = apply_move(&mut room.state, player, mv);
        remaining.remove(die_idx);
        if let Some(p) = room.in_progress.as_mut() {
            p.remaining = remaining;
        }

        let dto = move_to_dto(player, mv);

        broadcast(
            room,
            &ServerMsg::Applied {
                player: player_str(player),
                mv: dto,
                courier_arrived: just_arrived,
            },
        );
        broadcast_state(room);

        if let (Some(sb), Some(game_id)) = (supabase, room.game_id) {
            room.move_seq += 1;
            sb.log_move(game_id, room.move_seq, player, die, dto, just_arrived);
        }

        if room.state.has_won(player) {
            finish_game_if_configured(room, supabase, player);
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

fn finish_game_if_configured(room: &Room, supabase: &Option<Arc<Supabase>>, winner: Player) {
    if let (Some(sb), Some(game_id)) = (supabase, room.game_id) {
        sb.finish_game(game_id, winner, room.move_seq);
    }
}

/// Arranca el turno de `room.state.turn`: chequea postillón atrapado,
/// materializa dados heredados si los hay, o le avisa al jugador que
/// puede tirar.
fn start_new_turn(room: &mut Room, supabase: &Option<Arc<Supabase>>) {
    let player = room.state.turn;

    if let Some(trapped) = check_courier_trapped(&room.state, player) {
        room.in_progress = None;
        if let Some(winner) = trapped.winner {
            finish_game_if_configured(room, supabase, winner);
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
            awaiting_move_for_die_idx: None,
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

/// Convierte un movimiento (en posiciones relativas al camino del
/// jugador, como lo maneja el motor) a puntos ABSOLUTOS del tablero de
/// 24, que es lo que el cliente necesita para saber qué casillero
/// resaltar, y lo que guardamos en Supabase para que las jugadas de
/// White y Black se lean en el mismo sistema de coordenadas. Para White
/// coinciden por diseño; para Black hay que trasladarlos.
fn move_to_dto(player: Player, m: jacquet::rules::Move) -> MoveDto {
    match m {
        jacquet::rules::Move::OnBoard { from, to } => MoveDto::Board {
            from: jacquet::rules::absolute_index(player, from) as u8,
            to: jacquet::rules::absolute_index(player, to) as u8,
        },
        jacquet::rules::Move::BearOff { from } => MoveDto::BearOff {
            from: jacquet::rules::absolute_index(player, from) as u8,
        },
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
// Supabase: guardado de historial (opcional — ver from_env)
// ---------------------------------------------------------------------

struct Supabase {
    client: reqwest::Client,
    url: String,
    secret_key: String,
}

impl Supabase {
    /// Lee SUPABASE_URL y SUPABASE_SECRET_KEY del entorno. Si cualquiera
    /// de las dos falta, devuelve None y el servidor sigue funcionando
    /// sin guardar historial — nunca bloquea el arranque por esto.
    fn from_env() -> Option<Arc<Supabase>> {
        let url = env::var("SUPABASE_URL").ok()?;
        let secret_key = env::var("SUPABASE_SECRET_KEY").ok()?;
        Some(Arc::new(Supabase {
            client: reqwest::Client::new(),
            url: url.trim_end_matches('/').to_string(),
            secret_key,
        }))
    }

    fn headers(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        req.header("apikey", &self.secret_key)
            .header("Authorization", format!("Bearer {}", self.secret_key))
            .header("Content-Type", "application/json")
    }

    /// Inserta la fila inicial de una partida nueva. Devuelve el `id`
    /// generado, que después se usa para asociarle las jugadas. Si algo
    /// falla, devuelve None y esa partida simplemente no se guarda (el
    /// juego en sí sigue andando igual).
    async fn create_game(&self, room_code: &str) -> Option<i64> {
        #[derive(Deserialize)]
        struct Row {
            id: i64,
        }

        let res = self
            .headers(
                self.client
                    .post(format!("{}/rest/v1/games", self.url))
                    .header("Prefer", "return=representation"),
            )
            .json(&serde_json::json!({ "room_code": room_code }))
            .send()
            .await;

        let res = match res {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Supabase create_game: error de red: {e}");
                return None;
            }
        };

        if !res.status().is_success() {
            eprintln!("Supabase create_game: HTTP {}", res.status());
            return None;
        }

        match res.json::<Vec<Row>>().await {
            Ok(rows) => rows.into_iter().next().map(|r| r.id),
            Err(e) => {
                eprintln!("Supabase create_game: respuesta inesperada: {e}");
                None
            }
        }
    }

    /// Guarda una jugada aplicada. Se dispara en segundo plano (no
    /// bloquea el turno de nadie) — si falla, solo se pierde ese
    /// registro puntual, el juego sigue.
    fn log_move(
        self: &Arc<Self>,
        game_id: i64,
        seq: u32,
        player: Player,
        die: u8,
        mv: MoveDto,
        courier_arrived: bool,
    ) {
        let sb = Arc::clone(self);
        let player = player_str(player).to_string();
        let (kind, from_point, to_point) = match mv {
            MoveDto::Board { from, to } => ("board", from as i16, Some(to as i16)),
            MoveDto::BearOff { from } => ("bearoff", from as i16, None),
        };

        tokio::spawn(async move {
            let body = serde_json::json!({
                "game_id": game_id,
                "seq": seq,
                "player": player,
                "die": die,
                "kind": kind,
                "from_point": from_point,
                "to_point": to_point,
                "courier_arrived": courier_arrived,
            });

            let res = sb
                .headers(sb.client.post(format!("{}/rest/v1/moves", sb.url)))
                .json(&body)
                .send()
                .await;

            match res {
                Ok(r) if !r.status().is_success() => {
                    eprintln!("Supabase log_move: HTTP {}", r.status());
                }
                Err(e) => eprintln!("Supabase log_move: error de red: {e}"),
                _ => {}
            }
        });
    }

    /// Marca una partida como terminada (ganador + total de jugadas).
    /// `finished_at` lo llena solo un trigger en la base de datos.
    fn finish_game(self: &Arc<Self>, game_id: i64, winner: Player, total_moves: u32) {
        let sb = Arc::clone(self);
        let winner = player_str(winner).to_string();

        tokio::spawn(async move {
            let body = serde_json::json!({ "winner": winner, "total_moves": total_moves });

            let res = sb
                .headers(
                    sb.client
                        .patch(format!("{}/rest/v1/games?id=eq.{}", sb.url, game_id)),
                )
                .json(&body)
                .send()
                .await;

            match res {
                Ok(r) if !r.status().is_success() => {
                    eprintln!("Supabase finish_game: HTTP {}", r.status());
                }
                Err(e) => eprintln!("Supabase finish_game: error de red: {e}"),
                _ => {}
            }
        });
    }
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
