# Jacquet

Implementación en Rust del **Jacquet**, una variante del backgammon transmitida de forma oral en mi familia — me la enseñó mi abuela, y este proyecto nace de las ganas de que ese juego no se pierda.

## Jugalo ahora

🎲 **[jacquet-game.netlify.app](https://jacquet-game.netlify.app/)** — tablero interactivo en el navegador, sin instalar nada.

## Sobre el juego

El Jacquet es un juego de mesa de la familia del backgammon: dos jugadores, un tablero de 24 puntos, movimiento en paralelo (no en direcciones contrarias) y sin capturas de fichas rivales.

La variante que se juega en mi familia se aparta en varios puntos del Jacquet "clásico" documentado en fuentes como [bkgm.com](https://www.bkgm.com/variants/Jacquet.html) — entre otras cosas, usa **3 dados** en vez de 2, tiene reglas propias para dobles, triples y dados heredados entre jugadores, y una condición de bloqueo mucho más estricta.

Las reglas completas, con el vocabulario tradicional de los dados (As, Don, Tren, Cuadra, Quina, Sena) y la comparación detallada contra el Jacquet clásico, están documentadas en [`RULES.md`](./RULES.md).

## Arquitectura: dos implementaciones, no conectadas (todavía)

Este repo tiene **dos versiones separadas** de las mismas reglas:

- **`src/`** — el motor real, en Rust. Es la fuente de verdad: tablero, generación de movimientos legales, postillón, dados heredados, bear off, todo probado a mano jugando partidas completas por consola.
- **`docs/index.html`** — el tablero visual que corre en [jacquet-game.netlify.app](https://jacquet-game.netlify.app/). Es una **reimplementación de las mismas reglas en JavaScript**, hecha para poder ver y probar el juego sin pasar por la terminal. No llama al código Rust — es lógica duplicada a propósito, más simple de tener funcionando rápido.

El riesgo de esto es que las dos versiones se desincronicen si se cambia una regla en una y no en la otra (ya pasó una vez, con los dados heredados). La forma correcta de unificarlas a futuro es compilar el motor de Rust a **WebAssembly** (con `wasm-bindgen` + `wasm-pack`) y que el front solo dibuje el tablero llamando a ese `.wasm` — queda pendiente como su propio proyecto.

## Estado del proyecto

- [x] Modelar el tablero y el estado del juego
- [x] Generación de movimientos legales según las reglas de la variante
- [x] Lógica de turnos: dobles parciales, triples (repite turno), dados heredados entre jugadores
- [x] Lógica del postillón y condición de derrota por bloqueo
- [x] Bear off y condición de victoria
- [x] Motor jugable por consola (CLI)
- [x] Tablero visual jugable en el navegador (JS, reglas duplicadas — ver arquitectura arriba)
- [ ] Unificar el motor con el front vía WebAssembly
- [ ] Agente/oponente heurístico
- [ ] Posible empaquetado como app (escritorio/móvil)

## Por qué existe este proyecto

No es solo un ejercicio técnico: es una forma de preservar una tradición familiar que de otro modo solo vive en la memoria de quienes la jugaron. La idea es que las reglas queden documentadas y que el juego se pueda jugar (y enseñar) más allá de la mesa.

## Requisitos

- [Rust](https://www.rust-lang.org/tools/install) (probado con 1.98)

## Cómo correrlo

### Por consola (el motor real)

```bash
cargo build
cargo run
```

Tira los dados, elegís qué jugar en cada paso, y ves el tablero completo en cada turno.

### En el navegador

Abrí [jacquet-game.netlify.app](https://jacquet-game.netlify.app/), o corré `docs/index.html` localmente abriéndolo en cualquier navegador (no necesita servidor, es un solo archivo autocontenido).

## Licencia

Por definir.
