//! Loop de juego mínimo por consola. Sirve para probar el motor de punta a
//! punta; cuando haya un front, esta lógica de "tirar, mostrar opciones,
//! aplicar elección" se reemplaza por lo que sea que mande la UI, pero
//! `play_turn` y el resto del motor no deberían necesitar cambios.

use rand::Rng;
use rusty_walk::rules::{is_courier_trapped, new_game, Move};
use rusty_walk::turn::{play_turn, DieOutcome};
use rusty_walk::{Player, Roll};
use std::io::{self, Write};

fn main() {
    let mut state = new_game();

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

        let dice = roll_three_dice();
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

        // Orden simple: tal como salió la expansión. Elegir otro orden
        // queda para cuando haya una UI que se lo pregunte al jugador.
        let order = expanded.moves.clone();

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

fn roll_three_dice() -> [u8; 3] {
    let mut rng = rand::thread_rng();
    [
        rng.gen_range(1..=6),
        rng.gen_range(1..=6),
        rng.gen_range(1..=6),
    ]
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
