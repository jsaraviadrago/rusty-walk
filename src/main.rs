//! Loop de juego mínimo por consola. Sirve para probar el motor de punta a
//! punta; cuando haya un front, esta lógica de "tirar, mostrar opciones,
//! aplicar elección" se reemplaza por lo que sea que mande la UI, pero
//! `play_turn` y el resto del motor no deberían necesitar cambios.

use jacquet::rules::{is_courier_trapped, new_game, Move};
use jacquet::turn::{play_turn, DieOutcome};
use jacquet::Roll;
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

        let order = pick_order(&expanded.moves);

        let log = play_turn(&mut state, &order, expanded.repeats_turn, pick_move);

        for outcome in &log.outcomes {
            match outcome {
                DieOutcome::Applied(mv) => println!("  -> {:?}", mv),
                DieOutcome::Forfeited(die) => {
                    println!("  -> dado {} sin movimiento legal: se pierde el turno", die)
                }
            }
        }

        if let Some(winner) = log.winner {
            println!("\n¡{:?} gana la partida!", winner);
            break;
        }

        if log.repeats_turn {
            println!("(triple completo: {:?} vuelve a tirar)", state.turn);
        }
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
fn pick_move(legal: &[Move]) -> Move {
    if legal.len() == 1 {
        return legal[0];
    }

    println!("  Elegí un movimiento:");
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

        if let Ok(idx) = input.trim().parse::<usize>() {
            if idx < legal.len() {
                return legal[idx];
            }
        }

        println!("  Opción inválida, probá de nuevo.");
    }
}
