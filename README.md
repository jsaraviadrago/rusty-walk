# Jacquet

Implementación en Rust del **Jacquet**, una variante del backgammon transmitida de forma oral en mi familia — me la enseñó mi abuela, y este proyecto nace de las ganas de que ese juego no se pierda.

## Sobre el juego

El Jacquet es un juego de mesa de la familia del backgammon: dos jugadores, un tablero de 24 puntos, movimiento en paralelo (no en direcciones contrarias) y sin capturas de fichas rivales.

La variante que se juega en mi familia se aparta en varios puntos del Jacquet "clásico" documentado en fuentes como [bkgm.com](https://www.bkgm.com/variants/Jacquet.html) — entre otras cosas, usa **3 dados** en vez de 2, tiene reglas propias para dobles y triples, y una condición de bloqueo mucho más estricta.

Las reglas completas, con la comparación detallada contra el Jacquet clásico, están documentadas en [`RULES.md`](./RULES.md).

## Estado del proyecto

🚧 En desarrollo. Por ahora el foco está puesto en:

- [ ] Modelar el tablero y el estado del juego
- [ ] Implementar la generación de movimientos legales según las reglas de la variante
- [ ] Lógica de turnos, dados (dobles parciales, triples, repetición de turno)
- [ ] Lógica del postillón y condición de derrota por bloqueo
- [ ] Bear off y condición de victoria
- [ ] Motor de juego jugable (CLI o similar)
- [ ] Agente/oponente heurístico
- [ ] Posible empaquetado como app (escritorio/móvil)

## Por qué existe este proyecto

No es solo un ejercicio técnico: es una forma de preservar una tradición familiar que de otro modo solo vive en la memoria de quienes la jugaron. La idea es que las reglas queden documentadas y que el juego se pueda jugar (y enseñar) más allá de la mesa.

## Requisitos

- [Rust](https://www.rust-lang.org/tools/install) (edición y versión mínima a definir)

## Cómo correrlo

```bash
cargo build
cargo run
```

*(instrucciones a completar a medida que el proyecto avance)*

## Licencia

Por definir.
