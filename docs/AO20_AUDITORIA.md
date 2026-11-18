# Auditoría AO20 → Rust (18/11/2026)

Comparación lógica por lógica del servidor oficial (`argentum-online-server-classic`, 151
módulos VB6, ~94.000 líneas) con nuestro código. El detalle por sistema y por función está
en las tres partes:

- [Parte 1: combate, magia y NPCs](audit/AO20_parte1_combate_magia_npcs.md): **~45 %**
- [Parte 2: objetos, oficios, comercio y mundo](audit/AO20_parte2_objetos_oficios_mundo.md): **~55 %**
- [Parte 3: social, eventos, GM y cuentas](audit/AO20_parte3_social_eventos_gm.md): **~55 %**

**Total estimado: ~52 %** del AO20 completo. Lo que se juega a diario (moverse, pelear,
inventario, comercio, banco, oficios, clanes, grupos, facciones, misiones, cuentas)
está entre el 75 y el 95 %. Lo que más baja la media es lo que falta casi entero:
efectos en el tiempo, hechizos de área, IA avanzada, reglas del mundo (triggers,
restricciones de mapa), eventos automáticos y la mitad de los comandos de GM.

## Hecho tras la auditoría (18/11)

- **Puertas**: abrir y cerrar con clic, bloqueos según el `Subtipo` (7 formas), puertas con
  llave, palancas. Servidor y cliente aplican la misma regla (`ao_rules::doors`). Probado
  con bots (`tests/doors.rs`) y en captura.
- **Objetos fijos del mapa y del suelo**: el cliente no dibujaba ni las puertas, ni los
  carteles, ni lo que hay tirado en el suelo (oro, botín). Ahora sí.
- **Recursos oficiales completos** (`tools/ao20/build_full.cmd`): todos los cuerpos de NPC y
  gráficos de mapa; los tests usan estos datos por defecto.

## Fase 1 hecha (18/11)

- **Resistencia mágica**: `GetUserMR` / `GetUserMRForNpc` (objetos, skill Resistencia x0.04 / x0.4,
  clase) y la de los NPC (`MagicResistance`, `MagicDef`, `DEFm`). Antes era siempre 0.
- **Skills por uso** (`SubirSkill`): armas / proyectiles / sin armas al acertar, tácticas cuando
  un NPC te ataca, defensa con escudo al bloquear, apuñalar, magia al lanzar y resistencia
  al recibir daño mágico.
- **Invisibles y ocultos**: los NPC no los persiguen salvo que estén pegados; los hechizos de
  NPC tampoco los alcanzan.
- **`GetExpPenalty`**: más de 4 niveles por encima de la criatura, -5 % de experiencia por nivel
  (por miembro del grupo).
- **Hechizos con efectos en el tiempo aún no portados**: avisan y no gastan maná.

## Fase 2 hecha (18/11)

- **Restricciones de mapa** (`CheckMapRestrictions`): solo newbies, solo ciudadanos o criminales,
  solo clanes, solo grupos y rango de niveles, leídos de la cabecera de cada mapa
  (`restrict_mode`, `level`). Las salidas los paran con su aviso. Los mapas Patreon quedan
  cerrados (no hay Patreon).
- **Triggers de casilla**: resurrección automática (7), salida solo para muertos (14), casillas
  solo de trabajadores (13) y de Patreon (20).
- **Teleports**: el `Radio` del objeto teleport reparte la llegada. Pendiente: la red de
  "Transporte dudoso" de las minas (subtipo 2).
- **Limpieza del suelo** (mejora, el oficial no la tiene): lo tirado desaparece a los 15 minutos
  (`floor_cleanup_minutes`, 0 = nunca).
- **Llaves del inventario** (clave ≥ 1000) sobre la última puerta clicada: abren las cerradas con
  llave (la llave se gasta) o cierran con llave. Pendiente: el llavero de casas (base de datos).
- **Reloj del mundo**: el servidor manda la hora; día de 120 minutos (`day_length_minutes`),
  todos ven la misma hora; empieza a las 08:00 al arrancar.

## Fase 3 hecha (18/11)

- **Pociones**: fuerza y agilidad temporales (duran `DuracionEfecto` segundos, tope x2, nunca se
  guardan subidas), cambio de cara y de sexo, invisibilidad, poción negra (suicidio).
- **Cofres** de AO20 (subtipos 1-3: todo, N al azar, cada uno con su probabilidad).
- **Botella vacía**: se llena a dos casillas del agua. **Instrumentos y cuernos de facción**
  suenan (cuernos solo para su ejército y fuera de zona segura). Mensaje nuevo `S2C::Sound`.
- **`/hogar`**: un fantasma vuelve a Ullathorpe tras `HomeTimer` (150 s; GMs 5 s).
- **Objetos de misión** (`QuestOpenByObj`): ofrecen su misión sin NPC; se gastan al aceptarla.
- No había nada que portar en pasajes de barco ni runas de regreso: los datos oficiales no tienen
  ninguno.
- **Pendiente**: monturas (necesitan el sistema de velocidad: muertos, barcos, monturas,
  hechizos), fogatas, cartas coleccionables y aspectos
  (skins, unos 430 objetos estéticos).
## Fase 5 hecha (18/11): GM y moderación

Código en `crates/server/src/systems/gm_mod.rs` (los comandos de siempre siguen en `gm.rs`),
migración `0014_gm_moderation.sql` y protocolo 17 (`GmS2C::Sos`). Rangos como en el VB:

- **Ban temporal** (`Admin.BanTemporal`): `/BANT nick@motivo@días` (SemiDiós, 1 a 255 días;
  el VB solo lo mandaba desde el panel). Se guarda en `gm_ban.until`, caduca solo (barrido
  cada minuto con aviso a los GM, y también al intentar entrar) y `/UNBAN` lo levanta antes.
  Echa al personaje si está conectado. Botón «Banear días» en el panel.
- **Inspección**: `/STAT`, `/INV`, `/BOV`, `/SKILLS` (SemiDiós), `/BAL` (Admin), `/NICK2IP`,
  `/IP2NICK`, `/ONLINEGM`, `/ONLINEMAP [mapa]` (SemiDiós; a un SemiDiós no le salen los Dioses
  ni los Admins, como en el VB). Botones Stats, Inventario e IP en el panel.
- **Moderación**: `/ESTUPIDO` y `/NOESTUPIDO` (Dios), `/BORRARPENA nick@n@texto` (cambia el
  texto de la pena n, como `CambiarPenaDatabase`), `/ECHARTODOSPJS` (solo usuarios), `/GRABAR`.
- **Ayuda y denuncias** (`Ayuda`, `QuestionGM`, `Denounce`): `/GM [tipo@]texto` (una cada
  5 minutos), `/DENUNCIAR nick` (lleva las últimas 15 líneas que dijo el denunciado y responde
  igual si no está conectado), `/SHOW SOS`, `/ATENDER id` (Consejero; avisa al jugador),
  `/BORRAR SOS` (Dios). La cola está en la base de datos. El panel de GM tiene la pestaña
  «Pedidos de ayuda» con Ir y Atendido.
- **Mapa y mundo**: `/CT mapa x y [radio]` y `/DT [x y]` (Dios), `/MODMAPINFO PK|RESTRINGIR|
  MAGIASINEFECTO|INVISINEFECTO|TERRENO|ZONA` (Dios, hasta reiniciar), `/MOTDCAMBIA` y `/MOTD`
  (el MOTD se muestra al entrar), `/GLOBAL` (Dios), `/CR segundos` (Consejero: cuenta a todos
  e inmoviliza a los de alrededor), `/LIMPIAR` (SemiDiós).

**Bugs del VB corregidos**: el ban temporal no miraba el rango ni si el personaje existía, no
echaba al conectado, un segundo ban creaba otra entrada (la primera en vencer lo desbaneaba) y
el barrido borraba dentro de un `For` (se saltaba la entrada siguiente); `CleanWorld` solo
escribía el mensaje; `GlobalOnOff` anunciaba sin cambiar nada; `ChangeMapInfoPK` guardaba el
seguro al revés; `NoMagic`/`NoInvi` solo escribían el registro; `MakeDumb` no comparaba
rangos; la cola de ayuda se perdía al reiniciar y `SOSRemove` buscaba por el texto entero.

**Pruebas**: `crates/server/tests/p5_gm.rs` (bots: cada comando sin rango, ban temporal,
inspección con filtros por rango, moderación, SOS y denuncias, teleport andando, MOTD, global,
cuenta atrás), tests de `db/gm.rs` y `systems/gm_mod.rs` (vencimiento del ban, cuenta atrás,
teleport). Panel capturado con `scripts/captura_invisible.ps1`.

**Falta**: inspeccionar personajes desconectados (el VB tampoco lo hace); `/MODMAPINFO`
no se guarda en el mapa (en el VB, `Backup`, `Terreno` y `Zona` iban al `.dat`) y no hay
`BACKUP` ni `RESUSINEFECTO` (no existen esos datos aquí); los teleports de `/CT` desaparecen al
reiniciar; el cliente no mueve al azar a un jugador estúpido (solo lo marca); siguen sin portar
`WarpMeToTarget`, `NPCFollow`, `SpawnCreature`, `TalkAsNPC`, `CerrarCliente`, `AlterName`,
`Working`/`Hiding`, clima y sonido, y los comandos de eventos y lobby (fase 6).
## Fase 4 hecha (18/11): magia completa

- **Motor de efectos en el tiempo** (`systems/eot.rs`, lector completo en
  `ao-formats/dat/eots.rs`, reglas puras en `ao-rules/eot.rs`): efectos sobre jugadores **y
  criaturas**, límites de `FindEffectOnTarget` (incluido `SharedTypeId`), `Override`,
  requisitos (arma, escudo, armadura, casco, transformación) comprobados en cada tick.
  Tipos portados: 1 vida, 9 maná, 2 modificadores (Afilar, Refuerzo, Fervor del Paladín,
  Rejuvenecimiento, Falange), 4 provocado (como modificadores), 15 escudo (Protección Divina
  1-3, Protección de Tute, núcleo enano), 17 daño extra (Penitencia), 16 transformación (Forma
  de Lobo, Oso y Espíritu del Bosque: aspecto, maná por tick, solo hechizos sobre uno mismo,
  `RequireTransform` de Desgarro), 8 efecto al golpear (Filo de Toxina, Veneno 2/3, Veneno de
  maná, Desgarro), 14 golpes múltiples (Golpes Rápidos) y 10 canciones de grupo (Serenata
  Esquiva, Precisión Armónica, Partitura de Regeneración).
- **Modificadores en el combate**: golpe, evasión, defensa, daño lineal (PvE aparte), daño
  físico y mágico hecho y recibido, curación recibida y dada; en cuerpo a cuerpo, hechizos de
  jugador y de criatura, y en lo que pega una criatura. Inmunidad a control (`CCInmunity`).
- **Hechizos de área** (`HechizoSobreArea`): cuadrado de lado `AreaRadio` centrado en el tile,
  20 % menos por tile; sin nadie alcanzado no se cobra.
- **IA de criaturas** (`systems/npc_tactics.rs`): `AI_RangeAttack` (disparo a distancia,
  distancia preferida, retirada y órbita con los valores de `Server.ini`), `SupportAndAttack`
  (primero quitar parálisis o curar, luego paralizar o dañar, enfriamiento por hechizo) y
  `NpcLanzaSpellInmovilizado`.
- **Dueño de la criatura y veneno** (`systems/npc_owner.rs`): `GetOwnedBy` 10 s,
  `CanAttackNotOwner`, robar con seguro = rechazado antes de golpear o lanzar (sin gastar),
  sin seguro = criminal; el área nunca roba ni vuelve criminal; `NpcEnvenenarUser`
  (`creature_poison` en la configuración, encendido como en AO20).
- **Bugs del VB corregidos**: Protección Divina 3 sobre la 1 solo renovaba la pequeña; el
  escudo del núcleo enano (EOT62, "80 segundos") duraba 2 s; los dos golpes extra de Golpes
  Rápidos salían en frames seguidos; el daño de área usaba otra cuenta de resistencia (ahora
  la misma que el hechizo de un objetivo). Bug nuestro corregido: los hechizos `IsSkill`
  subían Magia (en el VB no).
- **Decisiones**: el efecto al golpear no se aplica en cuerpo a cuerpo PvP (el VB tiene esa
  llamada comentada) y sí en hechizos de daño contra jugadores y golpes a criaturas; el bonus
  de curación recibida no llega a las pociones (`inventory.rs`, otra sesión); el reto
  (`event.rs`) ahora termina los efectos bien (antes un transformado quedaba con el aspecto).
- **Falta**: provocar (3), trampas (5), borracho (6), empujar y atraer (7, 11), explosión
  retardada (12), desequipar (13); la velocidad de movimiento (Eco Veloz, Desplazamiento
  Veloz, Acelerar Mascotas) porque el paso es fijo y el cliente no recibe velocidad. Esos
  hechizos siguen avisando "todavía no está disponible" sin cobrar. También faltan terreno,
  materialización y `HandlePetSpell`, los comportamientos BG, y criaturas que atacan a otras
  criaturas con hechizos de apoyo por equipos. El buff no se ve en el cliente (no hay
  mensaje de buffs en el protocolo; `PROTOCOL_VERSION` sigue en 16).

## Velocidad, monturas, fogatas, cartas y aspectos (18/11)
## Fase 7 hecha (18/11): magia e IA restantes

`PROTOCOL_VERSION` 19 (`S2C::Buff`, `C2S::UseItemAt`).

- **Efectos en el tiempo nuevos** (`systems/eot_extra.rs`): provocar (3, Provocar 246: área
  de 20, la criatura persigue al que provoca, el jugador provocado pega -90 % a cualquier
  otro), trampas (5: objetos 3783/3784/3789, máximo 3, cae la más vieja, solo criaturas que
  el dueño puede atacar, se activan al pisarlas o al ser empujadas), borracho (6: vino y
  cerveza apilan, desde 5 copas estupidez hasta bajar a 3), empujar (7, Flecha Cargada 248),
  atraer (11, portado pero sin datos oficiales: probado con una definición de test),
  explosión retardada (12: Cañonazo 256 de criaturas y cañón de bolsillo 3808/3815),
  desequipar (13 + `DesequiparObjetoDeUnGolpe`: bandido y ladrón, por la parte del cuerpo;
  interruptor `bandit_unequip_bonus`, encendido como en AO20, bloquea el hueco 8 s y sube la
  probabilidad del bandido de 0,15 a 0,2 por skill).
- **Barra de buffs**: `S2C::Buff` (= `WriteSendSkillCdUpdate`) para todos los efectos del
  jugador, también los de la fase 4, y el enfriamiento de hechizos con `CdEffectId`. HUD:
  `client/src/ui/buffs.rs`, iconos de `init/Effects.ini`, anillo dorado/rojo/apagado,
  barrido del tiempo, segundos, pilas y tooltip. Captura invisible hecha.
- **Magia** (`systems/magic_extra.rs`): `HandlePhysicalSkill` (flecha de empuje y cañón),
  `RequireWeaponType` en `PuedeLanzar`, materializar (44, 45), `HechizoTerrenoEstado`,
  `MultiShot` (Danza de Flechas 250, 5 flechas en abanico con -0,7 de daño), `HandlePetSpell`
  (Mejorar Ataque de Mascotas 273), aturdimiento por munición (`StunNPc`/`StunPlayer`).
- **IA** (`systems/npc_guard.rs`): `TryGuardAttackHostileNpc` y `GuardiaPersigueNpc`; el apoyo
  de criaturas a jugadores usa `FactionCanHelpFaction` (`ao_rules::faction::faction_can_help`).
- **ElementalTags**: arma contra criatura en cuerpo a cuerpo y hechizos (daño y curación,
  como el VB), detrás de `elemental_tags` (apagado, como el `feature_toggle.ini` oficial).
- **Bugs del VB corregidos**: `TranslationInmune` no funcionaba (`SetBaseDot` borraba la
  marca); el provocado podía quedar con -90 % si fallaba el golpe; nuestro: la parte del
  cuerpo re-tirada en PvP se descartaba.
- **Datos que no hacen nada en AO20 (igual aquí)**: `Mimetiza` (Mimetizar, campo leído y sin
  uso), hechizos `Tipo=12/13` (Detectar Invisibilidad, Orbe de Luz), munición de aturdir
  (ninguna flecha oficial tiene `Porcentaje`).
- **Falta**: comportamientos BG (1 criatura cada uno), invasiones y rutas entre mapas (0
  criaturas oficiales con `Invasion`), `AI_NpcAtacaNpc`, criatura contra criatura con el
  cañón; las trampas no se dibujan en el cliente (falta el mensaje `eUpdateTrap`); el cañón
  de mano tira `MinHIT..MaxHIT` del objeto en vez de `GetUserDamageWithItem`; el arpón y la
  reliquia de resurrección; encantamientos elementales por objeto del inventario; la
  velocidad (otra sesión).
## Fase 6 hecha (18/11): eventos y comandos GM restantes

Código en `crates/server/src/systems/{lobby,capture,world_events,castle,gm_event}.rs`, reglas
puras en `crates/ao-rules/src/events.rs`, BD en `db/phase6.rs` + migración
`0016_phase6_events.sql` (renumerada al integrar), protocolo **21** tras integrar (`S2C::Music`, para `/FORCEMIDI`). Pruebas:
`systems/phase6_tests.rs` (mundo con reloj virtual) y `tests/p6_events.rs` (bots).

- **Lobby y escenarios** (`ModLobby`, `CustomScenarios`, `IBaseScenario`, `ScoreBoard`,
  `ScenarioDeathMatch`, `ScenarioHuntNpc`): `/CREAREVENTO LOBBY|DEATHMATCH|CACERIA
  participantes nivel_min nivel_max [precio] [equipo]` (SemiDios), anuncio oficial, 5 min de
  inscripción con recordatorio cada 30 s, arranque al llenarse o al tiempo con mínimo (si no,
  se cancela y se devuelve la inscripción), `/PARTICIPAR [sala clave]` (líder de grupo si los
  equipos son armados), `/SALAS` y el NPC EventMaster listan las salas, `/CONFIGLOBBY`
  completo (SPAWN, MAXLVL, MINLVL, CLASS, SUMPLAYER, SUMALL, RETURNPLAYER, RETALL, OPEN, START,
  END, LIST, KICK, FORCERESET, SETTEAMSIZE, ADDPLAYER, SETPRICE), `/CANCELAREVENTO`. Equipos al
  azar por nivel en ida y vuelta (`SortTeams`), premios 55/35 o 40/30/20 del pozo, marcador
  cada 20/30 s, reconexión (vuelve a su sitio al desconectarse y regresa al volver).
  Deathmatch: ring de postes que crece con los jugadores, pociones prohibidas guardadas y
  devueltas, pociones que aparecen, +3 matar / +1 asistencia (8 s) / -1 morir, reaparición a
  los 2 s, 5 min, sin facciones ni pérdida de objetos. Cacería: hace falta un garrote, solo
  suman las muertes a garrotazo, las criaturas reaparecen.
- **Eventos por hora** (`ModEventos`): tabla `[events] hourly` (hora -> `tipo-duración-multi`,
  vacía por defecto: los datos oficiales no la traen), `/CREAREVENTO tipo@duración@multi`
  (Admin, límites del VB), `/FINALIZAREVENTO`, `/EVENTO`; multiplicadores de experiencia, oro,
  drop y recolección (nuevo `gather_mult` en minería, tala y pesca). Evento automático del día
  (`Automatic_Event_Timer`): `[events] auto_hour` + `auto_days`.
- **Quests globales** (`ModGlobalQuests`): `GlobalQuests.dat`, fechas en hora local
  (`utc_offset_hours`), contador común y aportes en BD, oleada de jefes cada
  `GatheringInitialInstallments`, no se entrega con el jefe vivo, recompensas bloqueadas por
  umbral (`GlobalQuestThresholdNeeded`), fin por fecha o por umbral. Enganchado a `quest.rs`.
- **Castillo del emperador** (`ModCastle`): reliquias 6362-6381 sobre un cartel de fundación
  (trigger 41, 21 mapas oficiales), muros, puerta al interior (758-772), triggers de entrada
  21-40 con lista blanca (`/CASTLE ADD|REMOVE nick`), mudanza cada 7 días, clic en la maqueta,
  20 castillos sembrados en la BD. Fundar exige Patreon Noble en AO20: aquí
  `events.castle_without_patreon` (apagado por defecto).
- **Captura de la bandera** (`ModCaptura`, `clsCaptura*`): `/CREAREVENTO CAPTURA participantes
  rondas nivel_min nivel_max precio`, 90 s de inscripción (una prórroga), equipos por nivel,
  sala de espera 278, bases 275/277, 7 s con la bandera enemiga en la base, espera 5 s + 2 por
  muerte, al mejor de N, premio = inscripción x2; sin fuego amigo.
- **Invasiones** (`ModInvasion`): portadas (avisos, oleadas, marcha a la muralla, daño, top 10,
  premio), pero `Eventos.dat` trae `Cantidad=0` y quedan apagadas igual que en AO20 (`/CE` no
  las arranca mientras los datos no las activen).
- **Comandos GM** (`gm_event.rs`): `/TELEPLOC`, `/SLOT`, `/SEGUIR`, `/CC` + `/SPAWN n`,
  `/TALKAS`, `/TRABAJANDO`, `/OCULTANDO`, `/LLUVIA`, `/NIEVE`, `/NIEBLA`, `/RESETINV`,
  `/MASSDEST`, `/RMATA`, `/MASSKILL`, `/CE`, `/EVENTOFACCIONARIO`, `/ANAME`, `/FORCEMIDI`,
  `/FORCEMIDIMAP`, `/FORCEWAV`, `/FORCEWAVMAP`, `/SETDESC`, `/GENIO`, `/DOBACKUP`,
  `/CERRARCLIENTE`; de jugador `/UPTIME`, `/AYUDA` (Help.dat), `/EST`.

**Bugs del VB corregidos**: `OpenLobby` rechazaba toda sala con punto de invocación;
`/PARTICIPAR` sin sala global leía `LobbyList(-1)`; `HandleStartEvent` no miraba el rango; un
segundo evento pisaba al que corría; `/CONFIGLOBBY KICK` no hacía nada; el deathmatch invocaba
a todos en la misma casilla; `RespawnItems` usaba la cantidad del ítem 0 para todos; los
muertos volvían a casa como fantasmas; el lobby genérico sin punto mandaba al mapa -1; el
marcador se perdía al reconectar; el cliente tenía `/CREAREVENTO` dos veces (el multiplicador
era inalcanzable); el temporizador que terminaba el evento por hora ya no existía (no terminaba
nunca); `FinalizarEvento` pisaba los `/EXP` del GM; `/EVENTO` describía el evento equivocado;
el dueño de un castillo entraba en todos (`IsEmperorCastleCreated` con `Or`); los triggers
21-40 de techo de los mapas 776/782/795 se tomaban como entradas de castillo; el castillo solo
se guardaba con el guardado general; `SumarScoreInvasion` duplicaba jugadores y no ordenaba;
la captura dejaba la bandera en el inventario de quien se desconectaba; `/GENIO` lo podía usar
un Consejero; `/SLOT` medía el inventario del GM; `/LLUVIA` cambiaba también la nieve.

**Decisiones**: sin instancias de mapa (el cliente carga el mapa por número): cada escenario
usa su mapa y solo uno a la vez por mapa. La cacería lee `NpcHunt.ini` y, como no viene en
los datos, `snakes.ini`. La lista de salas va por consola (`/SALAS` y el EventMaster), sin
ventana nueva. `/CERRARCLIENTE` echa al jugador (no hay "cerrar el cliente" en el protocolo).

**Falta**: la **batalla naval** (`ScenarioNavalBoarding`, 1.183 líneas: barcos, bodega, fuego,
waypoints de NPC) porque necesita la navegación y la IA por waypoints de `npc_ai.rs`, que
lleva otra sesión; `/SHOWNAME` (el cliente no tiene cómo ocultar un nombre), y del resto de
`Protocol_GmCommands` siguen fuera `RequestUserList`, `PossUser`, `SetSpeed`, `Ignored`,
`ShowServerForm`, `FeatureToggle`, `DebugRequest`, `RoleMasterRequest`, `Gamble`,
`MapPriceEntrance`, `BusquedaTesoro`, `MarcaDeGM`, `CancelarExit`, `AskTrigger`,
`ChangeMapSetting` y `ChangeMapInfoBackup/NoResu` (herramientas del formulario del servidor,
sistemas no portados o datos que no existen aquí). Las salas del deathmatch y la cacería no se
ven como "bandera" sobre el personaje (el VB mandaba `UpdateFlag`). Con el `npcs.dat`
público, varios objetivos de `snakes.ini` son NPCs no atacables (ver el hallazgo de abajo).
Nada probado aún con el cliente gráfico.

## Hallazgo importante: datos del servidor desfasados

- **Velocidad** (`ActualizarVelocidadDeUsuario`): normal x1, fantasmas x1,4, barcos y monturas
  por su `Velocidad` (montura x nivel de jinete 1,05-1,45), armadura con `Velocidad` propia y los
  efectos con `SpeedModifier` (Eco Veloz, Desplazamiento Veloz). El servidor ajusta el ritmo de
  pasos permitido (antitrampas) y avisa al cliente (`S2C::Speed`), que acorta el paso y la
  animación. Protocolo 19.
- **Monturas** (`DoMontar`): cuerpo para razas altas o bajas, sin arma ni escudo a la vista;
  no en calabozos ni en combate; las de facción solo para su ejército; se desmonta al morir.
- **Fogatas**: daga sobre leña del suelo (fogata apagada), clic para encenderla, la lluvia
  las apaga en mapas con lluvia.
- **Cartas coleccionables**: colección por cuenta en la base de datos (bronce +1, plata +3, oro
  +5, tope 10) con los bonus oficiales contra la criatura (experiencia, daño hecho, daño
  recibido, botín). `/cartas` muestra la colección. Ajuste `collectible_cards`.
- **Aspectos**: armadura, casco o cabeza, escudo, arma y barco cambian la apariencia desde el
  inventario normal (en el oficial van en un inventario aparte de la tienda Patreon). Las alas
  y los aspectos de hechizo quedan equipados pero el cliente no tiene capa para dibujarlos.

## Datos del servidor frente a los mapas oficiales (revisado)

Comparando nombre por nombre con el cliente oficial (`init/sp_localindex.dat`), los NPCs,
hechizos y misiones de `Dat/` coinciden todos, y en objetos solo faltan los 818-820. Los
mapas oficiales simplemente colocan otros NPCs que los públicos (en Ullathorpe, el Makrom 408
con una misión en lugar del 407 con seis): es el diseño actual de AO20, no un desfase. Los
tests de misiones escritos para el Makrom de seis misiones usan los mapas públicos.

## Orden recomendado

| Fase | Qué | Por qué |
|---|---|---|
| 1 | Bugs de equilibrio: resistencia mágica (hoy 0), subir skills de combate y magia, NPCs que atacan a invisibles, `GetExpPenalty`, hechizos con efecto no soportado que cobran el maná | Cambian lo que ya funciona; esfuerzo S cada uno |
| 2 | Mundo: restricciones de mapa, triggers de casilla, teleports del `.csm`, limpieza del suelo, llaves y llavero, día y noche | Sin esto el mapa no se comporta como AO20 |
| 3 | Uso de objetos: pociones temporales y las demás, botella, fogatas, runas y /hogar | ~20 tipos de objeto sin efecto |
| 4 | Magia completa: motor de efectos en el tiempo, área, terreno, IA a distancia y de hechizos de NPC | Lo más grande de combate (L) |
| 5 | GM y moderación: ban temporal, inspección, SOS/denuncias | Necesario antes de abrir a jugadores |
| 6 | Eventos: lobby y escenarios, eventos por hora, quests globales, castillo, captura | Contenido de largo plazo |
