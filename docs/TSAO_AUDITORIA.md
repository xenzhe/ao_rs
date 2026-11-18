# Auditoría Numic AO frente a Tierras Sagradas (TSAO)

Fuente: servidor VB6 de TSAO (`X:\dev\tsao\Servidor\Codigo`, 70 módulos) y su cliente.
Estado al 2026-11-17. Leyenda: ✅ hecho · 🟡 parcial · ❌ falta.

## 1. Núcleo del personaje

| Sistema | Estado | Notas |
|---|---|---|
| Creación (razas, clases, cabezas, kit inicial, habilidades a 100) | ✅ | Kit y rangos de cabeza de TSAO; servidor valida la cabeza |
| Experiencia y curva (70 niveles) | ✅ | `Experiencia.dat` |
| Subida de nivel (vida por tabla clase/raza, maná, golpe, stamina) | ✅ | `tsao/data/levels.toml` |
| **Premios por nivel** (oro <10, nivel 10 sale del Newbie Dungeon + Starterpack, nivel 20 Fuego Mágico) | ✅ | Esta tanda |
| **Límite newbie (nivel 9)** y objetos newbie que se quitan | ✅ | Era 12 (AO20) |
| Bonus de clase a niveles 53/56/60 (elegir mejora, `ClassBonus.dat`) | ✅ | Ventana «Bonificación de clase», una vez por tramo, `/bonus` la reabre. Efecto aplicado: vida, daño/resistencia mágica PvP. Evasión, escudo, golpe, flechas y apuñalar quedan en `tsao_bonus()` para el dueño del combate |
| **Monedas TSAO** (puntos de torneo, puntos TS, créditos) | ✅ | Por nivel 50/60/65/70, `GivePTS` de criaturas, misiones; `/puntos` |
| Vidas / Gran Poder / StatusMith / transformaciones | ❌ | Mecánicas propias de TSAO |
| Atributos por raza | ✅ | |

## 2. Mundo y movimiento

| Sistema | Estado | Notas |
|---|---|---|
| Mapas, salidas, zonas seguras (Pk), agua | ✅ | Bordes 0.11.5 (9-92 / 7-94) para cruzar de mapa; agua y recortes corregidos |
| Viajero entre ciudades | ✅ | |
| **/REGRESAR (volver a la ciudad)** | ✅ | Esta tanda |
| Navegar (barcas, galeras) | ✅ | Nivel 30; desembarcar solo junto a tierra |
| Monturas (/MONTAR) | ❌ | |
| Casas (`Casas.dat`, NPC Propiedades) | ❌ | |
| Tesoros (/DESENTERRAR, `Tesoros.dat`) | ✅ | Leer el mapa del tesoro, cavar 30 s |
| Mundo continuo entre mapas | 🟡 | TSAO no trae la rejilla de mapas vecinos |

## 3. Combate y magia

| Sistema | Estado | Notas |
|---|---|---|
| Golpe, evasión, escudo, daño (fórmulas 0.11.5) | ✅ | |
| Balance PvP por clase y clase contra clase | ✅ | |
| Hechizos (daño x3.1, bastones, defensa mágica) | ✅ | |
| Hechizos exclusivos de clase | ✅ | |
| Robar (/UK Robar) | ✅ | Tecla J y clic |
| Ocultarse | ✅ | Tecla B |
| Apuñalar / críticos | 🟡 | Versión AO20 |
| Mascotas e invocaciones (Invokar.dat, /QUITARMASCOTA, entrenador) | ❌ | Siguiente: necesitan IA de mascota |
| Golpe crítico mágico contra criaturas | ❌ | |
| Objetos especiales por número (matadragones, túnicas, anillos) | ❌ | Pasarlos a datos |

## 4. Economía e inventario

| Sistema | Estado | Notas |
|---|---|---|
| Comercio con NPC y entre jugadores | ✅ | |
| Banco | ✅ | |
| Oro y botín de criaturas como TSAO | ✅ | |
| **Cofres al azar (Starterpack, `Cofres.dat`)** | ✅ | Esta tanda |
| Correo con paquetes | ✅ | |
| Mejorar objetos (`Mejorados.dat`) | ✅ | Usar la Gema Octarina (o `/mejorar`): ventana «Mejorar objetos» |
| Pergaminos de experiencia/oro (scrolls) | ❌ | El código existe en TSAO pero `Obj.dat` no trae ningún objeto de tipo 50: nada que portar hasta que haya datos |
| Canje por puntos (torneo, TS, donación) | ✅ | `/canje`: `Premios.dat`, tienda TS (`CanjeTS.dat`), `ItemsDonaciones.dat`; packs con premium/skins visibles pero no a la venta (no hay esos sistemas) |
| Subastas | ✅ | (versión Numic) |

## 5. Trabajo

| Sistema | Estado | Notas |
|---|---|---|
| Talar, minar, pescar, fundir | 🟡 | Motor AO20; revisar contra Trabajo.bas |
| Herrería | 🟡 | Recetas de TSAO cargadas, reglas AO20 |
| Carpintería, sastrería, alquimia | ❌ | TSAO no las trae; vacías |
| **Tipos de objeto TSAO** (cofre, montura, gemas, scrolls…) | 🟡 | Remapeados; los sin sistema quedan como decorativos con su tipo TSAO guardado |

## 6. Social

| Sistema | Estado | Notas |
|---|---|---|
| Clanes, alianzas, guerras | ✅ | (versión AO20) |
| Bóveda de clan | ✅ | |
| Grupos (party) | ✅ | |
| Facciones (armada/caos), renunciar | 🟡 | NPC «Renunciar» de TSAO sin conectar |
| Ciudadanía | ❌ | |
| Nobleza (quest de nobleza, ItemsNoble) | ❌ | |
| Dioses y almas (Mifrit, Poseidón, Erebros, Tarraske) | ❌ | |
| Matrimonio (/CASAR, /DIVORCIARSE) | ✅ | Guardado en BD; divorcio aunque la pareja esté desconectada |
| Parejas 2v2 (/PAREJA, duelo pagado) | ❌ | Evento: pendiente |
| Emoticonos | ❌ | |
| Votaciones (/VOTAR, /RESULTADOS, /ENCUESTA de GM) | ✅ | Consulta de `consultas.dat` al arrancar; un voto por cuenta, guardado |
| Ranking | ❌ | |

## 7. Eventos y PvP organizado

| Sistema | Estado | Notas |
|---|---|---|
| Retos 1v1 / 2v2 | 🟡 | Retos Numic; desafíos TSAO por arena no |
| Torneos | 🟡 | Torneo GM Numic; automáticos TSAO no |
| Castillos (asedio, Rey del Castillo) | ❌ | |
| Clan contra clan (CVC) | ❌ | |
| Captura la bandera | ❌ | |
| Juegos del hambre | ❌ | |
| Evento faccionario, evento luz, ARAM, batalla mística | ❌ | |
| Misiones diarias | ❌ | |
| Misiones (QUESTS.DAT) | ✅ | Con puntos de torneo, TS y créditos |

## 8. Otros

| Sistema | Estado | Notas |
|---|---|---|
| Correos, cirujano | ✅ | |
| Bots IA (Mod_BOTS) | ❌ | |
| Centinela antimacro | ❌ | |
| Pretorianos | ❌ | |
| Comandos de jugador | 🟡 | /regresar y /online hechos; resto por interfaz |
| Teclas configurables | ✅ | |
| Sonido y música de TSAO | ✅ | |

## Progresión y social TSAO (rama `worktree-agent-a3119a629ac9ab613`)

Hecho y probado (`crates/server/tests/tsao_progress.rs`, capturas invisibles de las ventanas
Canje, Bonificación de clase, Votación y Mejorar objetos): monedas TSAO y de dónde salen,
canje de las tres monedas, bonus de clase, mejorar con Gema Octarina, votaciones, matrimonio.
Datos: `tools/tsao/progress.py` (lo llama `build.cmd`). Migración `0013_tsao_progress.sql`,
protocolo 14 (`C2S::Tsao` / `S2C::Tsao`). Comandos: `/canje`, `/puntos` (`/est`), `/bonus`,
`/mejorar`, `/votar`, `/resultados`, `/casar`, `/divorciarse`; GM `/encuesta`.

Pendiente de esta área, por orden de valor:
- Efecto de los bonus de evasión, escudo, golpe, flechas y apuñalar en el combate: el valor
  está en `World::tsao_bonus(id)` (décimas de %); falta usarlo en `tsao_evasion`,
  `attack_power_of` y el apuñalar (código del dueño del combate).
- Ciudadanía (NPC 13), renunciar (NPC 154) y nobleza: dependen de `StatusMith` (ciudadano /
  criminal / neutral de TSAO), que aún no existe en el motor.
- Dioses y almas, ranking, emoticonos, créditos por sacos (tipo 51: TSAO no trae ninguno en
  `Obj.dat`), pergaminos (tipo 50: ninguno en `Obj.dat`).
- Eventos: castillos y asedio, CVC, desafíos por arena y `/PAREJA` (2v2), torneos
  automáticos, captura la bandera, juegos del hambre, evento faccionario, evento luz, ARAM,
  batalla mística, misiones diarias, casas, monturas, centinela, apuestas.
- Comandos de jugador que faltan: `/INFORMACION`, `/PING`, `/NICK`, `/EVENTOS`, `/PARTICIPAR`.

## Orden propuesto

1. **Flujo newbie** (esta tanda): premios por nivel, salida a nivel 10, Starterpack, cofres, límite newbie 9, /REGRESAR.
2. **Clásicos que faltan**: robar, ocultarse, navegar, mascotas/invocaciones, tesoros.
3. **Progresión TSAO**: bonus de clase 53/56/60, mejorar objetos, scrolls, monedas (puntos de torneo/TS) y canje.
4. **Social TSAO**: ciudadanía, nobleza, dioses, parejas, ranking, votaciones, emoticonos.
5. **Eventos**: castillos, CVC, desafíos por arena, torneos automáticos, captura la bandera, juegos del hambre, misiones diarias.
6. **Mundo**: casas, monturas, bots.

## Revisión automática

`cargo run -p ao-formats --example world_audit` recorre todos los mapas y datos: salidas alcanzables, gráficos que existen y caben en su imagen, NPCs con gráfico, sonidos del motor. Quedan 73 gráficos que ni el índice de TSAO tiene y 7 salidas en x=8 del mapa 179 (rotas también en el original).
