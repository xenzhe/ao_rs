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

## Hallazgo importante: datos del servidor desfasados

El servidor lee `Dat/` (npcs.dat, obj.dat, misiones…) del repositorio público, pero los
mapas oficiales colocan algunos NPCs con números más nuevos (Ullathorpe: Makrom 408, con una
misión, en vez de 407, con seis). El cliente de Steam no trae los datos del servidor. Hay que
revisar mapa por mapa qué NPCs quedan con otro papel y corregir `npcs.dat` (o buscar un
`Dat` oficial más reciente). Mientras tanto, los tests de misiones usan los mapas públicos.

## Orden recomendado

| Fase | Qué | Por qué |
|---|---|---|
| 1 | Bugs de equilibrio: resistencia mágica (hoy 0), subir skills de combate y magia, NPCs que atacan a invisibles, `GetExpPenalty`, hechizos con efecto no soportado que cobran el maná | Cambian lo que ya funciona; esfuerzo S cada uno |
| 2 | Mundo: restricciones de mapa, triggers de casilla, teleports del `.csm`, limpieza del suelo, llaves y llavero, día y noche | Sin esto el mapa no se comporta como AO20 |
| 3 | Uso de objetos: pociones temporales y las demás, botella, fogatas, runas y /hogar | ~20 tipos de objeto sin efecto |
| 4 | Magia completa: motor de efectos en el tiempo, área, terreno, IA a distancia y de hechizos de NPC | Lo más grande de combate (L) |
| 5 | GM y moderación: ban temporal, inspección, SOS/denuncias | Necesario antes de abrir a jugadores |
| 6 | Eventos: lobby y escenarios, eventos por hora, quests globales, castillo, captura | Contenido de largo plazo |
| 7 | Datos: cuadrar `npcs.dat` con los mapas oficiales | Ver el hallazgo |
