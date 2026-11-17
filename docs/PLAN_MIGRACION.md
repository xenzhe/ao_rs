# Plan de migración completa de AO20 a Rust

Objetivo: todos los sistemas conocidos de Argentum Online 2.0 en nuestro proyecto, con
la misma esencia y una mejora sustancial (cuentas con varios personajes, creación de
personajes rolera, interfaces buenas, widescreen, mundo continuo, efectos).

## 1. Qué hay que migrar (medido sobre el AO20 clonado)

Servidor oficial: ~94.000 líneas de VB6 en 151 módulos. Datos: 5.199 objetos (`obj.dat`),
907 NPCs (`npcs.dat`), 213 hechizos (`Hechizos.dat`), 415 misiones (`Quests.DAT`),
`Balance.dat` (clases y razas), ciudades, crafteos. Base de datos oficial: SQLite
(tablas `account, user, inventory_item, bank_item, spell, skillpoint, quest, quest_done,
pet, punishment, house_key...`). Cliente: 86 formularios, 150 FX, partículas, 2.003
sonidos OGG, 756 imágenes de interfaz, `MatrixMapas.dat` (cuadrícula del mundo).

| Sistema | Módulos VB6 (líneas) | Prioridad |
|---|---|---|
| Cuentas, personajes, persistencia | CharacterPersistence 1.8k, modDatabase 0.9k, Database_Queries 0.4k | P1 |
| Clases, razas, atributos, skills | Declares/Balance.dat, Modulo_UsUaRiOs 3.6k (parte) | P1 |
| Inventario, equipo, objetos, drops | InvUsuario 4.3k, Modulo_InventANDobj 0.3k | P1 |
| Combate (cuerpo a cuerpo, rango, PvP) | SistemaCombate 2.6k | P1 |
| Hechizos y efectos en el tiempo | modHechizos 4.3k, EffectsOverTime 0.8k + clases *Effect* | P1 |
| NPCs: IA, comerciantes, entrenadores, mascotas | MODULO_NPCs 2.5k, AI_NPC 1.7k, PathFinding 0.6k | P1 |
| Bóveda (banco) | modBanco 0.3k | P1 |
| Comercio con NPC y entre jugadores | Comercio 0.3k, mdlCOmercioConUsuario 0.3k | P2 |
| Clanes | modGuilds 1.2k, clsClan 0.6k | P2 |
| Grupos (party), facciones | ModGrupos 0.5k, ModFacciones 0.5k | P2 |
| Misiones | ModQuest 0.9k, ModGlobalQuests 0.3k | P2 |
| Trabajo: minería, tala, pesca, herrería, carpintería, sastrería, alquimia | Trabajo 2.2k, modFishing, modCrafteos, modSmelting... | P3 |
| Comandos de GM, penas, logs | Protocol_GmCommands 4k, Admin, Penas | P3 |
| Eventos: retos, torneos, castillos, invasiones, subastas | ModRetos, ModTorneos, ModCastle, ModInvasion, ModSubasta... | P4 |

Lo que NO se migra tal cual: el protocolo de VB6 (Protocol*.bas, 17k líneas) ni la red; ya
tenemos los nuestros, más seguros. Se migran las **reglas y fórmulas**.

## 2. Principio rector: probar rapidísimo y sin depender de una imagen

Antes de migrar nada (Prioridad 0) se construye un banco de pruebas **headless** (sin
ventana ni GPU) que todos los agentes usan para demostrar que su sistema funciona:

1. **`ao-testkit`** (crate de pruebas):
   - `TestServer::start()`: servidor en el mismo proceso, puerto libre, base de datos
     SQLite temporal en memoria, semilla fija y **reloj virtual** (la IA, la regeneración y
     los enfriamientos avanzan al instante, sin esperar segundos reales).
   - `Bot`: cliente programable (crear cuenta, crear personaje, caminar, atacar, lanzar,
     comerciar, aceptar en un clan...) con esperas por condición, nunca por tiempo fijo.
   - Escenarios: un test describe una historia («dos jugadores forman clan, uno muere y
     el otro lo resucita») y comprueba eventos y estado final.
   - Cada sistema trae su test de **trampa** (qué pasa si un cliente miente).
2. **`client-core`**: toda la lógica del cliente (estado, mensajes, predicción, reglas)
   separada de macroquad. Los bots y los tests usan el mismo núcleo que el cliente real.
3. **`ao-snap`** (capturas sin ventana): un compositor por CPU que dibuja un mapa con sus
   capas, personajes, cuerpos, cabezas y equipo a PNG desde el estado del juego.
   - `ao-snap --map 1 --at 50,50 --out x.png` o desde un test (`scenario.snapshot("x.png")`).
   - **Comparación con imágenes de referencia** (diferencia por píxel con tolerancia):
     un cambio visual involuntario rompe un test.
   - Las capturas por GPU (`AO_SHOT`) quedan solo para la revisión final de efectos y luces.
4. Todo en `cargo test`: objetivo < 60 s para la batería completa.

## 3. Arquitectura que se añade

- **Base de datos**: SQLite (como el oficial) con `rusqlite` en un hilo propio (actor de
  persistencia: el mundo nunca espera al disco), migraciones versionadas en
  `crates/server/migrations/`, guardado periódico y al salir, contraseñas con Argon2.
- **Datos oficiales leídos directamente** de `Recursos/Dat/*.dat` con lectores en
  `ao-formats` (obj, npcs, hechizos, balance, quests, crafteos), validados al arrancar;
  `data/*.toml` solo para nuestros ajustes y mejoras encima.
- **Protocolo por módulos** (`account`, `world`, `inventory`, `combat`, `magic`, `guild`,
  `trade`, `chat`...), versión negociada.
- **Un actor por mapa** (P3) para escalar; el mundo continuo exige que el traspaso entre
  mapas vecinos sea atómico (sin duplicar ni perder al jugador).
- **Mundo continuo en el cliente**: con `MatrixMapas.dat` se cargan y dibujan los mapas
  vecinos en segundo plano; cruzar el borde no tiene pantalla de carga.

## 4. Fases

### P0 — Cimientos (yo + 1 Opus + 2 Sonnet)
- git del proyecto (necesario para trabajar con agentes en paralelo, cada uno en su rama).
- `ao-testkit` con reloj virtual y bots; `client-core` separado; `ao-snap` con referencias.
- Lectores de `obj.dat`, `npcs.dat`, `Hechizos.dat`, `Balance.dat` (+ tests sobre los
  archivos reales, como ya hacemos con los 842 mapas).
- Persistencia SQLite + migraciones + cuentas (crear, entrar, varios personajes, borrar).
- Contratos: protocolo por módulos y tipos compartidos (los fijo yo antes de P1).
- **Salida**: un bot crea cuenta y dos personajes, entra, camina, sale, vuelve y todo
  persiste; una captura `ao-snap` de Ullathorpe coincide con su referencia.

### P1 — Núcleo jugable fiel a AO20 (4 Opus + 2 Sonnet en paralelo)
- Clases, razas, atributos, skills y **creación de personaje** completa (servidor).
- Inventario, equipo (armas, armaduras, escudos, cascos, anillos), objetos en el suelo,
  drops de NPC, oro.
- Combate con las fórmulas de `SistemaCombate.bas` (golpe, evasión, defensa, críticos,
  armas a distancia, PvP con zonas seguras y facciones), muerte, fantasma, resurrección.
- Hechizos: los 213 de `Hechizos.dat` con sus tipos (daño, curación, paralizar, invisibilidad,
  invocar, estados) y efectos en el tiempo.
- NPCs de los mapas reales (`npcs.dat` + posiciones del `.csm`): IA, rutas, comerciantes,
  sacerdotes, entrenadores, guardias, mascotas.
- Bóveda (banco por personaje y por cuenta).
- **Salida**: un escenario de bots que crea un mago y un guerrero, matan NPCs, suben de
  nivel, equipan lo que cae, guardan en la bóveda; todo con sus tests de trampa.

### P2 — Social y economía (3 Opus + 3 Sonnet)
- Comercio con NPC y entre jugadores (con confirmación doble anti-estafa).
- Clanes completos (fundación, rangos, aspirantes, alianzas/guerras, noticias), grupos con
  reparto de experiencia, facciones, canales de chat (clan, grupo, susurro, global).
- Misiones.

### P3 — Cliente remasterizado (2 Opus + 4 Sonnet, en paralelo con P2)
- Cuentas y **creación de personaje rolera**: vista previa animada del personaje, razas
  y clases con descripción e ilustración, tirada de atributos, elección de cabeza/cuerpo.
- Ventanas: inventario y equipo con arrastrar y soltar, bóveda, comercio, hechizos con
  atajos, clan, grupo, misiones, mapa del mundo y minimapa. Estilo rolero coherente,
  escalado a cualquier resolución.
- **Widescreen**: vista ampliable (el servidor fija el rango de visión para todos; lo
  extra se dibuja como mundo sin entidades o con niebla, sin ventaja en PvP).
- **Mundo continuo**: mapas vecinos precargados y dibujados a la vez; cruce sin carga.
- Efectos: los 150 FX y las partículas oficiales, luces del mapa, clima (lluvia, nieve,
  niebla según el mapa), equipo visible en el personaje, sonidos y música.
- Un actor por mapa en el servidor.

### P4 — Contenido y operación (según prioridades del momento)
- Trabajo y oficios, comandos de GM y penas, eventos (retos, torneos, castillos,
  invasiones, subastas), estadísticas, despliegue y copias de seguridad de la base.

## 5. Reparto de agentes (máximo 4 Opus + 4 Sonnet a la vez)

Yo (sesión principal) hago de **arquitecto e integrador**: fijo contratos (tipos,
mensajes, esquema de base), reviso cada entrega, la integro y corro la batería entera.

| Rol | Modelo | Para qué |
|---|---|---|
| Lógica fiel compleja | **Opus** | Portar sistemas con fórmulas y casos raros: combate, hechizos, inventario/equipo, NPCs/IA, clanes, mundo continuo y render. Leen el VB6 oficial y lo traducen con tests. |
| Trabajo acotado y verificable | **Sonnet** | Lectores de `.dat` con sus tests, esquema y repositorios de la base, ventanas de interfaz a partir de una especificación, escenarios de test, documentación. |

Reglas para cada agente (van en su prompt):
- Trabaja en **su propia rama/worktree** y **solo en sus archivos** (lista explícita).
- No cambia contratos compartidos (protocolo, tipos de `ao-rules`, esquema): me los pide.
- Entrega con tests en `ao-testkit`, incluido el de trampa, y `cargo test` + `clippy` limpios.
- Ante una duda de regla, manda el VB6 oficial (`X:\dev\ao20`); cita el archivo y la función.
- «Haz el trabajo tú mismo; no lances otros agentes ni subagentes».

Reparto previsto:
- **P0**: Opus A → `ao-testkit` + reloj virtual + `client-core`; Sonnet 1 → lectores de
  `obj.dat`/`npcs.dat`/`Hechizos.dat`/`Balance.dat`; Sonnet 2 → `ao-snap` + referencias.
  Yo → git, persistencia/cuentas, contratos de P1.
- **P1**: Opus A → combate; Opus B → hechizos y efectos; Opus C → inventario, equipo y
  objetos; Opus D → NPCs e IA; Sonnet 1 → clases/razas/creación (servidor); Sonnet 2 →
  bóveda + escenarios de P1.
- **P2/P3**: Opus A → clanes/grupos/facciones; Opus B → comercio + misiones; Opus C →
  mundo continuo + actor por mapa; Opus D → efectos, partículas y luces del cliente;
  Sonnet 1-4 → ventanas (cuenta y creación, inventario/bóveda/comercio, clan/grupo/
  misiones, mapa/minimapa/opciones).

## 6. Riesgos y cómo se controlan

- **Agentes que pisan el trabajo de otros** → ramas separadas, archivos asignados,
  contratos fijados antes; integro yo, de a una rama, con la batería completa.
- **Fidelidad** (que se juegue como AO) → cada fórmula con test contra valores calculados
  a mano desde el VB6; escenarios que comparan con lo esperado del original.
- **Coste** → los agentes trabajan por tandas con salida verificable; no se lanza la
  siguiente tanda sin la anterior integrada y en verde.
- **Licencia de gráficos** → uso personal; si algún día hubiera fines comerciales, licencia
  de Noland Studios o arte propio.
