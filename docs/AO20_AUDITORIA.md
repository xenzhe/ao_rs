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
