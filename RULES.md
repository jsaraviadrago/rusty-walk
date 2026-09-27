# Jacquet — Reglas Oficiales de la Familia Saravia

Sep 26, 2026 · Documentado por @Juan Carlos Saravia

## 1. Introducción

El Jacquet es una variante de la familia de juegos de mesa que incluye al backgammon, jugada desde hace siglos en Francia y otros países europeos. Se caracteriza por ser un juego de carrera de movimiento paralelo: a diferencia del backgammon estándar, ambos jugadores mueven sus fichas en la misma dirección alrededor del tablero, y no existe la captura de fichas rivales ("hitting").

Este documento recoge la variante de Jacquet transmitida oralmente en la familia Saravia, enseñada por la abuela de Juan Carlos. Si bien conserva la estructura central del Jacquet clásico (movimiento paralelo, la figura del "courier" o postillón, y la obligación de reunir todas las fichas antes de retirarlas), incorpora varias reglas propias que la distinguen de las versiones documentadas en fuentes como bkgm.com — entre ellas, el uso de tres dados en lugar de dos, un bloqueo de puntos más estricto, y una condición de derrota inédita en el Jacquet clásico.

El propósito de este documento es fijar por escrito estas reglas antes de que se pierdan, y servir como especificación base para una implementación en software (Rust) del juego.

## 2. El tablero

El tablero tiene **24 puntos**, agrupados en **4 cuadrantes** de 6 puntos cada uno. Es un tablero compartido: ambos jugadores usan las mismas 24 casillas.

- Cada jugador arranca con sus **15 fichas apiladas en una sola columna**, en el punto de partida — no repartidas en varios puntos como en el backgammon estándar.
- Los puntos de partida de cada jugador están en **esquinas diagonalmente opuestas** del tablero.
- Ambos jugadores mueven sus fichas en la **misma dirección** (movimiento paralelo, no contrario como en backgammon).
- Cada jugador tiene su propio **cuadrante final** — la sección donde debe reunir sus 15 fichas antes de poder retirarlas ("cuadrante de llegada"). Este cuadrante está ubicado en la diagonal opuesta al punto de partida de ese jugador. Como el tablero es compartido, las fichas del rival pasan físicamente por tu cuadrante final durante su propio recorrido — por eso es posible que te lo bloqueen (ver sección 6).

## 3. Los dados

Se juega con **tres dados** por turno (a diferencia del Jacquet clásico, que usa dos).

- **Números distintos:** cada dado es un movimiento independiente — 3 movimientos en total.
- **Doble parcial** (dos de los tres dados salen iguales, ej. 4-4-2): el valor duplicado se juega **cuatro veces**, más el valor suelto restante. Ejemplo: 4-4-2 se juega como 4, 4, 4, 4, 2 — cinco movimientos en total. El jugador elige libremente el orden en que los aplica. El turno **no se repite**.
- **Triple** (los tres dados salen iguales, ej. 5-5-5): el valor se juega **seis veces**. Además, al terminar de jugarlos, **el turno se repite** (el mismo jugador vuelve a tirar).
- **Dado sin movimiento legal:** si un dado no tiene movimiento posible en el momento de intentarlo, no se pierde ahí mismo — sigue disponible para reintentarlo más adelante en el mismo turno, si otro movimiento cambia el tablero. Recién cuando ninguno de los dados que quedan tiene movimiento posible, quedan definitivamente sin jugar: si la tirada era propia, pasan al rival, que los juega directo en su próximo turno en vez de tirar los suyos; si esos dados ya eran heredados y tampoco sirven, se pierden para siempre, sin rebotar de nuevo.
- **Ficha tocada es ficha jugada:** una vez que aplicás un movimiento con un dado, esa jugada queda hecha — no se puede deshacer para probar un orden distinto. Por eso el chequeo de "no queda nada por jugar" se hace contra el tablero *tal como quedó después de tus jugadas ya hechas*, no contra todos los órdenes posibles que existían al principio del turno. Conviene pensar el orden antes de tocar una ficha, no ir probando hasta que algo funcione.

## 4. Ocupación y bloqueo de puntos

- Un punto solo puede tener fichas de **un jugador a la vez**. Es "first-come, first-served": quien llega primero a un punto vacío se lo queda.
- **Basta 1 sola ficha** para bloquear el paso a un punto — no se necesitan 2 o más como en el backgammon estándar o en el Jacquet clásico documentado en bkgm.com.
- **No existe el hitting (captura).** Nunca. Una ficha rival en un punto simplemente actúa como un muro; no se puede aterrizar ahí bajo ninguna circunstancia, pero tampoco se la "come" ni se la manda de vuelta al punto de partida.

## 5. El postillón

El **postillón** es la **primera ficha** que sale del punto de partida.

- Mientras el postillón no haya llegado a su cuadrante final, es la **única ficha que se puede mover** — ningún otro checker del jugador puede jugar, aunque los dados le servirían.
- El postillón puede entrar a su cuadrante final por **cualquiera de los 6 puntos** de esa zona, siempre que el punto de destino no esté bloqueado por el rival (ver sección 4).
- Una vez que el postillón llega a su cuadrante final, se **libera** el resto del ejército: desde ese momento, cualquier ficha se puede mover con cualquier dado.
- Las demás fichas del jugador pueden **apilarse sobre el postillón** en ese mismo punto del cuadrante final, sin límite.

## 6. Condición de derrota por bloqueo

Si el rival logra ocupar **los 6 puntos** del cuadrante final del postillón, y este no tiene ningún movimiento posible (con ninguna combinación de los dados disponibles) para entrar ahí, el jugador con el postillón atrapado **pierde la partida** de inmediato.

Esta situación es **poco común**, pero ocurre: suele darse cuando un jugador saca tiradas bajas seguidas mientras el rival saca tiradas altas, obligando a "regalar" dados (por no tener movimiento legal — ver sección 3) mientras el rival termina de cerrar los 6 puntos.

## 7. Bear off ("meter las fichas") y victoria

- Un jugador solo puede empezar a retirar fichas del tablero ("meter las fichas") una vez que **las 15 fichas** están reunidas en su cuadrante final. Ninguna ficha puede empezar a salir mientras quede aunque sea una atrasada en otro cuadrante.
- Una vez habilitado el bear off, el jugador puede retirar **tantas fichas como los movimientos disponibles en su tirada lo permitan**, en el orden que prefiera.
- **Gana la partida** el primer jugador en lograr que sus 15 fichas completen la vuelta y sean retiradas del tablero.
- No existe un sistema de puntuación adicional (como el "marcia"/gammon del Jacquet clásico o el backgammon estándar): se gana la partida y punto.

## 8. Comparación con el Jacquet clásico

Fuente de referencia: [bkgm.com/variants/Jacquet.html](https://www.bkgm.com/variants/Jacquet.html)

| Aspecto | Jacquet clásico (bkgm.com) | Variante familia Saravia |
| --- | --- | --- |
| Dados | 2 dados | 3 dados |
| Dobles | Se juegan 2 veces (4 movimientos) | Triple: 6 veces + repite turno. Doble parcial: 4 veces + el tercero suelto |
| Bloqueo de punto | Requiere 2+ fichas rivales | Basta 1 ficha |
| Hitting/captura | Existe (blot + barra) | No existe, nunca |
| El courier/postillón | Debe llegar a su cuadrante final | Igual: debe llegar a su cuadrante final (por cualquiera de sus 6 puntos) |
| Postillón atrapado | No hay derrota automática mencionada | Si el rival ocupa los 6 puntos del cuadrante final, el jugador pierde |
| Dado sin movimiento legal | Se pierde | Se reintenta luego en el turno; si nada sirve, pasa al rival (dados heredados); si tampoco a él, se pierde para siempre |
| Límites de bloqueo propio | Máx. 2 puntos cerrados en tablero inicial; máx. 2 fichas en el mid point | No confirmado / pendiente |
| Bear off | Requiere las 15 fichas en el cuadrante final | Igual |
| Puntuación | Sistema de "marcia" (1, 2 o 3 puntos) | No existe; solo victoria/derrota |

### Notas

- La regla de "1 sola ficha bloquea" y la ausencia total de hitting son las diferencias más profundas respecto al Jacquet clásico: cambian por completo la estrategia de bloqueo del postillón rival.
- La propia fuente de referencia (bkgm.com) señala que existen versiones "modernas" del Jacquet, descritas por Philippe Lalanne, que ya difieren bastante de las reglas de principios del siglo XIX que documenta el sitio. La variante familiar aquí descrita es, en ese sentido, una rama más de una tradición con múltiples variantes regionales y familiares — no una desviación de una única versión "oficial".

## 9. Vocabulario tradicional de los dados

En la mesa de la familia, los valores de los dados no se nombraban por su número — tenían nombre propio:

| Valor | Nombre |
| --- | --- |
| 1 | As |
| 2 | Don |
| 3 | Tren |
| 4 | Cuadra |
| 5 | Quina |
| 6 | Sena |

Cuando salía un doble parcial (dos de los tres dados iguales), se nombraba como **"[plural del que se repite] al/a la [nombre del suelto]"**. Ejemplo confirmado: **2, 2, 3 → "dones al tren"**.

Cuando salía un triple (los tres dados iguales), se nombraba como **"[plural] generales"**. Ejemplo confirmado: **5, 5, 5 → "quinas generales"**.

Siguiendo el mismo patrón, otro ejemplo sería **6, 6, 6 → "senas generales"**, o **1, 1, 4 → "ases a la cuadra"**.
