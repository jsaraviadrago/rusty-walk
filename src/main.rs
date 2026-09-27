//! Loop de juego mínimo por consola. Sirve para probar el motor de punta a
//! punta; cuando haya un front, esta lógica de "tirar, mostrar opciones,
//! aplicar elección" se reemplaza por lo que sea que mande la UI, pero
//! `play_turn` y el resto del motor no deberían necesitar cambios.

use jacquet::rules::{is_courier_trapped, new_game, player_positions, Move};
use jacquet::turn::{play_turn, DieOutcome};
use jacquet::{GameState, Player, Roll};
use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

/// Generador de números pseudoaleatorios mínimo (xorshift), sin
/// dependencias externas. No es criptográficamente seguro ni de calidad
/// estadística perfecta, pero de sobra para tirar dados de un juego de
/// mesa. Evita compilar crates externos (rand) mientras se diagnostica el
/// SIGSEGV del compilador en esta máquina.
struct Rng(u64);

impl Rng {
    fn new() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x2545F4914F6CDD1D);
        Rng(seed | 1) // nunca 0
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn die(&mut self) -> u8 {
        (self.next_u64() % 6) as u8 + 1
    }
}

fn main() {
    let mut state = new_game();
    let mut rng = Rng::new();

    loop {
        println!("\n=== Turno de {:?} ===", state.turn);
        print_board(&state);

        // Chequeo informativo antes de tirar (play_turn lo vuelve a
        // chequear igual, pero así el jugador entiende por qué terminó
        // la partida si pasa).
        let courier_arrived = state.player_state(state.turn).courier.arrived;
        if !courier_arrived && is_courier_trapped(&state.board, state.turn) {
            println!(
                "El postillón de {:?} quedó atrapado. ¡{:?} gana!",
                state.turn,
                state.turn.opponent()
            );
            break;
        }

        let dice_to_play: Vec<u8>;
        let is_inherited: bool;
        let repeats_turn_from_roll: bool;

        if let Some(pending) = state.player_state(state.turn).pending_dice.clone() {
            println!(
                "{:?} heredó estos dados del rival (no tira, los juega directo): {:?}",
                state.turn, pending
            );
            dice_to_play = pending;
            is_inherited = true;
            repeats_turn_from_roll = false;
        } else {
            let dice = [rng.die(), rng.die(), rng.die()];
            println!("Dados: {:?}", dice);
            let roll = Roll { dice };
            let expanded = roll.expand();
            println!(
                "Movimientos a jugar: {:?}{}",
                expanded.moves,
                if expanded.repeats_turn {
                    " (triple: se repite el turno si se juegan todos)"
                } else {
                    ""
                }
            );
            dice_to_play = expanded.moves;
            is_inherited = false;
            repeats_turn_from_roll = expanded.repeats_turn;
        }

        let order = pick_order(&dice_to_play);

        let log = play_turn(
            &mut state,
            &order,
            repeats_turn_from_roll,
            is_inherited,
            pick_move,
            |outcome| match outcome {
                DieOutcome::Applied(mv, courier_just_arrived) => {
                    println!("  -> {:?}", mv);
                    if courier_just_arrived {
                        println!("     ¡El postillón llegó a destino! Se libera el resto del ejército.");
                    }
                }
                DieOutcome::Unplayable(die) => {
                    println!("  -> dado {} sin movimiento legal por ahora", die)
                }
            },
        );

        if !log.leftover_dice.is_empty() {
            if log.leftover_discarded {
                println!(
                    "  Estos dados heredados tampoco se pudieron jugar y se pierden para siempre: {:?}",
                    log.leftover_dice
                );
            } else {
                println!(
                    "  {:?} no pudo jugar estos dados, quedan para el rival: {:?}",
                    state.turn.opponent(),
                    log.leftover_dice
                );
            }
        }

        println!("\nTablero después de la jugada:");
        print_board(&state);

        if let Some(winner) = log.winner {
            println!("\n¡{:?} gana la partida!", winner);
            break;
        }

        if log.repeats_turn {
            println!("(triple completo: {:?} vuelve a tirar)", state.turn);
        }
    }
}

/// Imprime el tablero como 4 cuadrantes de 6 puntos, tal como está descrito
/// en RULES.md sección 2 — mucho más fácil de leer que una fila plana de
/// 24 números. Debajo, el resumen por jugador en posiciones relativas
/// (para seguir el avance del postillón y el resto del ejército).
fn print_board(state: &GameState) {
    let cell = |idx: usize| -> String {
        match state.board.points[idx] {
            Some((Player::White, count)) => format!("W{}", count),
            Some((Player::Black, count)) => format!("B{}", count),
            None => ".".to_string(),
        }
    };

    let numbers_row = |range: std::ops::Range<usize>| -> String {
        range.map(|i| format!("{:>3}", i)).collect::<Vec<_>>().join(" ")
    };
    let pieces_row = |range: std::ops::Range<usize>| -> String {
        range.map(cell).map(|c| format!("{:>3}", c)).collect::<Vec<_>>().join(" ")
    };

    let quadrant_labels = [
        "Cuadrante 1 (salida White)",
        "Cuadrante 2",
        "Cuadrante 3 (salida Black)",
        "Cuadrante 4 (llegada White)",
    ];
    let ranges = [0..6usize, 6..12, 12..18, 18..24];

    println!();
    for (label, range) in quadrant_labels.iter().zip(ranges.iter()) {
        println!("  {}", label);
        println!("    {}", numbers_row(range.clone()));
        println!("    {}", pieces_row(range.clone()));
    }
    println!();

    for player in [Player::White, Player::Black] {
        let ps = state.player_state(player);
        let positions = player_positions(&state.board, player);
        let positions_str = positions
            .iter()
            .map(|(rel, count)| format!("{}×{}", rel, count))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "  {:?}: postillón {} | fuera del tablero: {}/15 | posiciones relativas: [{}]{}",
            player,
            if ps.courier.arrived {
                "llegó"
            } else {
                "en camino"
            },
            ps.borne_off,
            positions_str,
            match &ps.pending_dice {
                Some(d) => format!(" | dados heredados pendientes: {:?}", d),
                None => String::new(),
            }
        );
    }
}

/// Deja que el jugador elija en qué orden intentar los dados (RULES.md
/// sección 3: el orden es libre). Si aprieta Enter sin escribir nada, usa
/// el orden en que salieron los dados.
fn pick_order(rolled: &[u8]) -> Vec<u8> {
    println!(
        "  Orden de dados [{}] (Enter para dejarlo así, o escribilos separados por espacio en el orden que quieras, ej: {} {}):",
        rolled
            .iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join(" "),
        rolled.get(1).copied().unwrap_or(rolled[0]),
        rolled[0]
    );

    loop {
        print!("  > ");
        io::stdout().flush().ok();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            return rolled.to_vec();
        }

        let trimmed = input.trim();
        if trimmed.is_empty() {
            return rolled.to_vec();
        }

        let parsed: Result<Vec<u8>, _> = trimmed.split_whitespace().map(|s| s.parse()).collect();
        match parsed {
            Ok(order) if same_multiset(&order, rolled) => return order,
            _ => println!(
                "  Eso no es un reordenamiento válido de {:?}, probá de nuevo.",
                rolled
            ),
        }
    }
}

fn same_multiset(a: &[u8], b: &[u8]) -> bool {
    let mut a = a.to_vec();
    let mut b = b.to_vec();
    a.sort_unstable();
    b.sort_unstable();
    a == b
}

/// Si hay una sola opción, la toma sola. Si hay varias (más de una ficha
/// puede jugar el mismo dado), le pregunta al jugador humano por consola.
/// Acepta tanto el índice entre corchetes como la posición `from` de la
/// jugada (lo que sea más natural escribir).
fn pick_move(legal: &[Move]) -> Move {
    if legal.len() == 1 {
        return legal[0];
    }

    println!("  Elegí un movimiento (por índice [N] o escribiendo la posición 'from'):");
    for (i, mv) in legal.iter().enumerate() {
        println!("    [{}] {:?}", i, mv);
    }

    loop {
        print!("  > ");
        io::stdout().flush().ok();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            continue;
        }

        if let Ok(n) = input.trim().parse::<usize>() {
            // Primero probamos como índice de la lista.
            if let Some(&mv) = legal.get(n) {
                return mv;
            }
            // Si no es un índice válido, probamos como posición `from`.
            let n = n as u8;
            if let Some(&mv) = legal.iter().find(|mv| move_from(mv) == n) {
                return mv;
            }
        }

        println!("  Opción inválida, probá de nuevo.");
    }
}

fn move_from(mv: &Move) -> u8 {
    match mv {
        Move::OnBoard { from, .. } => *from,
        Move::BearOff { from } => *from,
    }
}
