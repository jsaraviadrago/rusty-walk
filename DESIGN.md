# Diseño del motor — Jacquet

Este documento describe las decisiones de diseño detrás de las estructuras de datos en `src/types.rs`, basadas en las reglas fijadas en [`RULES.md`](./RULES.md).

## Tablero (`Board`)

- 24 puntos, representados como un array fijo `[Point; 24]`.
- Cada `Point` es `Option<(Player, u8)>`: `None` si está vacío, o `Some((jugador, cantidad))` si tiene fichas. Como en esta variante **basta 1 ficha para bloquear** y **no hay hitting**, no hace falta modelar "blots" ni la barra — un punto es de un jugador o de nadie (Regla 4).
- El punto de partida y el cuadrante final de cada jugador se derivan de su índice inicial + desplazamiento, ya que el tablero es compartido y ambos avanzan en la misma dirección (Regla 2).

## Dados (`Roll`)

- Se modelan los 3 dados crudos (`[u8; 3]`) y una función separada que los expande a la **secuencia de movimientos jugables**, aplicando las reglas de dobles parciales y triples (Regla 3):
  - 3 valores distintos → 3 movimientos.
  - 2 iguales → 5 movimientos (el doble x4 + el suelto).
  - 3 iguales → 6 movimientos + flag de "turno se repite".
- Separar "dados crudos" de "movimientos jugables" simplifica testear la lógica de expansión de forma aislada.

## Postillón (`courier: Option<PieceId>` o índice fijo)

- Se modela como un campo del estado del jugador que apunta a la ficha designada como postillón, con un flag `courier_home: bool`.
- Mientras `courier_home == false`, el generador de movimientos legales **restringe todos los movimientos a esa ficha** (Regla 5). Esto se resuelve en la función `legal_moves()`, no como un tipo aparte — evita duplicar la lógica de movimiento.

## Turno (`Turn` / `GameState`)

- `GameState` guarda: el tablero, de quién es el turno, el estado de cada jugador (postillón liberado o no, fichas en bear off), y el historial de dados no jugados en el turno actual.
- La pérdida de un dado sin movimiento legal y el pase de turno al rival (Regla 3) se resuelven en el loop de turno, no en el modelo de datos — mantiene `GameState` simple y testeable.

## Condición de derrota por bloqueo (Regla 6)

- No se modela como un campo de estado permanente, sino como una **función de verificación** (`is_courier_trapped`) que se llama después de cada tirada: revisa si los 6 puntos del cuadrante final del jugador están ocupados por el rival y si ningún valor de dado disponible destraba al postillón.
- Esto evita tener que mantener un flag sincronizado manualmente y reduce el riesgo de bugs de estado inconsistente.

## Victoria / bear off (Regla 7)

- `Player` guarda un contador `borne_off: u8` (0–15). Se habilita el bear off solo cuando todas las fichas del jugador están en su cuadrante final — se verifica recorriendo el tablero, no con un flag cacheado, para evitar desincronización.

## Pendiente de definir

- Si el bloqueo de "1 ficha basta" aplica también dentro del propio cuadrante final para el bear off (afecta el orden en que se pueden sacar fichas apiladas).
- Representación exacta de "posiciones apiladas" del postillón + refuerzos en el cuadrante final (¿importa el orden interno, o solo la cantidad?).
