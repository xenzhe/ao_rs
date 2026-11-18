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
| Bonus de clase a niveles 53/56/60 (elegir mejora, `ClassBonus.dat`) | ❌ | |
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
| Apuñalar / críticos | 🟡 | Crítico TSAO contra criaturas (2 de 5, x2) hecho; apuñalar sigue AO20 |
| **Mascotas: domar, invocar, acompañantes** | ✅ | `systems/pets.rs`: domar (`UK Domar`), elementales (uno de cada), zombies, mascotas de Tipo 7, `/quitarmascota`, máximo 3, 120 s de vida; mueren o se sueltan al morir o salir el dueño; las domadas viajan con él; defienden al dueño |
| Entrenador (NpcType 3) | 🟡 | `/entrenar` y `/entrenar N` (máx. 7); sin ventana en el cliente |
| Mascotas guardadas con el personaje | ❌ | TSAO las guardaba en el .chr; aquí vuelven a lo salvaje al salir |
| **Criatura contra criatura** (`NpcAtacaNpc`, hechizos de elementales) | ✅ | La experiencia y el botín son del dueño |
| **Golpe crítico mágico contra criaturas** | ✅ | Más x1.4, x1.3 a mascotas; inmunes a Relámpago por datos |
| **Parálisis e inmovilizar criaturas** | ✅ | 50 s; inmunes por `AfectaParalisis` |
| **Objetos especiales por número** (matadragones, túnicas, tiara, anillo, laúd) | ✅ | Claves nuevas en obj.dat que escribe el conversor |
| Manoplas que inmovilizan | ❌ | Ningún objeto de TSAO tiene `Inmoviliza`: nada que portar |
| IA: TSAO `Movement=3` (perseguir) | ✅ | 179 criaturas (dragones incluidos) quedaban quietas: el conversor lo pasa a perseguir |
| Rey del Castillo, arietes, dioses, ARAM, evento faccionario, Gran Poder | ❌ | Son de eventos: van con su sistema |

## 4. Economía e inventario

| Sistema | Estado | Notas |
|---|---|---|
| Comercio con NPC y entre jugadores | ✅ | |
| Banco | ✅ | |
| Oro y botín de criaturas como TSAO | ✅ | |
| **Cofres al azar (Starterpack, `Cofres.dat`)** | ✅ | Esta tanda |
| Correo con paquetes | ✅ | |
| Mejorar objetos (`Mejorados.dat`) | ❌ | |
| Pergaminos de experiencia/oro (scrolls) | ❌ | |
| Canje por puntos (torneo, TS, donación) | ❌ | Faltan esas monedas |
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
| Parejas (/PAREJA) | ❌ | |
| Emoticonos | ❌ | |
| Votaciones (/VOTAR) | ❌ | |
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
| Misiones (QUESTS.DAT) | ✅ | Sin puntos de torneo |

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

## Orden propuesto

1. **Flujo newbie** (esta tanda): premios por nivel, salida a nivel 10, Starterpack, cofres, límite newbie 9, /REGRESAR.
2. **Clásicos que faltan**: robar, ocultarse, navegar, mascotas/invocaciones, tesoros.
3. **Progresión TSAO**: bonus de clase 53/56/60, mejorar objetos, scrolls, monedas (puntos de torneo/TS) y canje.
4. **Social TSAO**: ciudadanía, nobleza, dioses, parejas, ranking, votaciones, emoticonos.
5. **Eventos**: castillos, CVC, desafíos por arena, torneos automáticos, captura la bandera, juegos del hambre, misiones diarias.
6. **Mundo**: casas, monturas, bots.

## Revisión automática

`cargo run -p ao-formats --example world_audit` recorre todos los mapas y datos: salidas alcanzables, gráficos que existen y caben en su imagen, NPCs con gráfico, sonidos del motor. Quedan 73 gráficos que ni el índice de TSAO tiene y 7 salidas en x=8 del mapa 179 (rotas también en el original).
