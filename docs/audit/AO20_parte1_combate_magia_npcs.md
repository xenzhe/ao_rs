# Auditoría AO20 — Parte 1: Combate, Magia y NPCs

Fecha: 2026-11-18. Auditoría de solo lectura: se comparó el código VB6 de `X:\dev\ao20\argentum-online-server-classic\Codigo` con `X:\dev\ao_rs\crates`. Se dejó fuera Tierras Sagradas (las ramas `tsao_*` cuentan solo como código existente, no como cobertura de AO20).
Método: inventario de funciones VB por módulo, búsqueda de su equivalente en Rust con grep y lectura de las rutas principales (`melee.rs`, `combat.rs`, `magic.rs`, `npc_ai.rs`, `pets.rs`, `creatures.rs`, `ao-rules/src/{combat,magic,pathfinding}.rs`). No se compiló nada.

Leyenda: **HECHO** = igual o mejor · **PARCIAL** = existe pero falta algo o difiere · **FALTA** = no existe.

## 1. Tabla por sistema

### SistemaCombate.bas

| Sistema (funciones VB) | Rust | Estado | Notas |
|---|---|---|---|
| Ataque y evasión: `AttackPower`, `PoderAtaqueArma/Proyectil/Wrestling`, `AttackPowerDaggers`, `PoderEvasion`, `PoderEvasionEscudo`, `ModificadorPoderAtaque*`, `ModEvasionDeEscudoClase` | `ao-rules/combat.rs` `attack_power`, `evasion_power`, `shield_evasion`, `attack_skill_modifier`, `weapon_hit_bonus`; `melee.rs::attack_power_of` | HECHO | Con tests. |
| Impacto: `UserImpactoNpc`, `UsuarioImpacto`, `NpcImpacto`, `NpcImpactoNpc` | `rules::user_hit_chance`, `npc_hit_chance`, `meditating_hit_chance` | PARCIAL | El de jugador a jugador está completo. `user_attacks_npc` pasa un `weapon_mod` de 0, así que contra NPCs no se aplica `ImprovedMelee/RangedHitChance`. `NpcImpactoNpc` solo existe para mascotas. |
| Daño del jugador: `GetUserDamage`, `GetUserDamageWithItem`, `GetClassAttackModifier`, `ModicadorDañoClase*` | `melee.rs::base_damage`, `weapon_roll`; `rules::user_damage`, `class_damage_modifier`, `hit_range` | HECHO | Incluye la munición sumada al arma. |
| Daño a NPC: `UserDamageNpc`, `UserDamageToNpc` | `melee.rs::user_attacks_npc`, `combat.rs::damage_npc` | PARCIAL | Defensa, penetración y robo de vida del guerrero bien. Faltan los modificadores del NPC (`GetPhysicDamageReduction`, `GetDefenseBonus`, `GetPhysicalDamageModifier`) y los ganchos EOT (`TargetApplyDamageReduction`, `BonusDamageEffect`). |
| Daño a jugador: `UserDamageToUser`, `UserDoDamageToUser` | `melee.rs::user_attacks_user` | HECHO | Parte del cuerpo, casco o armadura más escudo, penetración, golpe por la espalda. |
| Críticos y apuñalar: `PuedeApuñalar`, `PuedeGolpeCritico`, `GetStabbingChance*`, `GetCriticalHitChance*`, `GetBackHitBonusChanceAgainstUsers`, `GetWeaponExtraChance`, `ModicadorApuñalarClase`, `GetStabbingNPCMin/MaxForClass` | `melee.rs::stab_chance`, `critical_chance`, `weapon_extra_chance`; `rules::can_stab/can_critical` | HECHO | `critical_chance` usa `Skill::Wrestling` (es el del bandido); conviene confirmarlo contra `GetCriticalHitChanceBase`. |
| NPC ataca a jugador: `NpcAtacaUser`, `NpcDamage`, `NpcDoDamageToUser`, `GetNpcDamage` | `combat.rs::npc_attacks_player` | PARCIAL | Impacto, bloqueo y cabeza o cuerpo bien. Faltan el veneno del NPC (`NpcEnvenenarUser`), `UserDañoEspecial`, `GetHitBonus` y `GetLinearDamageBonus` del NPC, y los ganchos EOT (`TargetWillAttack/DidHit/FailedAttack`). |
| NPC contra NPC: `NpcAtacaNpc`, `NpcDamageNpc`, `NpcDamageToNpc`, `NpcPerformAttackNpc` | `creatures.rs::npc_attacks_npc`, `npc_damaged_by_npc` | PARCIAL | Solo con mascotas. Las criaturas no devuelven el golpe a las mascotas (lo dice `pets.rs`). |
| `UsuarioAtaca`, `UserAttackPosition`, `UsuarioAtacaNpc`, `UsuarioAtacaUsuario`, `UsuarioAtacadoPorUsuario` | `melee.rs::on_melee`, `target_at`, `user_attacks_*`, `combat.rs::hurt_player` | HECHO | Intervalos, aguante 1..10, bloqueo por dirección. |
| `PuedeAtacar`, `TriggerZonaPelea`, `PeleaSegura` | `melee.rs::can_attack_user`, `magic.rs::spell_can_hit_user/is_safe_for_fight`, `faction_attack_refusal` | PARCIAL | Zonas y facciones bien. Faltan GM, «en consulta», grupo o clan, el seguro del ciudadano e `Inmunidad`. Las zonas están implementadas dos veces, distinto en melee y en magia. |
| Desarmar: `DesequiparObjetoDeUnGolpe`, `PuedeDesequiparDeUnGolpe`, `ProbabilidadDesequipar` | — | FALTA | |
| Experiencia: `CalcularDarExp`, `GetExpForUser`, `CalcularDarExpGrupal`, `GetExpPenalty`, `CalcularDarOroGrupal` | `combat.rs::damage_npc`, `give_exp`, `share_exp`; `rules::exp_for_damage` | PARCIAL | Experiencia por parte del daño y reparto en grupo, sí. **Falta `GetExpPenalty`** (`PenaltyExpUserPerLevel` con un delta mayor que 4). |
| `AllMascotasAtacanUser/NPC`, `NotifyPetOwnerOfDamage` | `pets.rs::pets_assist`, `pets_defend` | PARCIAL | Mascotas contra un usuario: no. |
| Proyectiles: `ThrowProjectileToTarget`, `ConsumeAmunition`, `GetProjectileView` | `melee.rs::on_shoot`, `consume_ammo`, `has_ammo_for` | HECHO | |
| `MultiShot`, `ThrowArrowToTargetDir`, `ThrowArrowToTile` | — | FALTA | |
| `CalculateElementalTagsModifiers` | — | FALTA | Reconocido en `bank.rs` y `trade.rs`. |
| `GetHitRangeValues`, `GetSkillRequiredForWeapon`, `GetShieldBlockSound` | `rules::hit_range`, `weapon_skill` | PARCIAL | Falta el sonido de bloqueo (cosmético). |
| `SubirSkill` en combate (Armas, Tácticas, Defensa con escudo, Apuñalar, Proyectiles, Wrestling) | — | **FALTA** | Solo `work.rs::work_raise_skill` sube skills. |

### modHechizos.bas

| Sistema | Rust | Estado | Notas |
|---|---|---|---|
| `TieneHechizo`, `AgregarHechizo`, `UpdateUserHechizos`, `ChangeUserHechizo`, `DesplazarHechizo`, `UseSpellSlot` | `magic.rs::learn_spell_from_item`, `send_spellbook`, `world/spellbook.rs` | HECHO | |
| `PuedeLanzar` | `magic.rs::can_cast` | HECHO | Las transformaciones se rechazan siempre. |
| `GetSpellManaCostModifierByClass`, `GetSpellStaminaCost` | `rules::stamina_cost`; `after_cast` | PARCIAL | No hay modificador de maná por clase. |
| `DecirPalabrasMagicas`, `InfoHechizo` | `after_cast` (Effect/Fx) | PARCIAL | Las palabras mágicas no se ven en `magic.rs`. |
| `LanzarHechizo` | `on_cast`, `resolve_target` | PARCIAL | Usuario, NPC, ambos e invocación. Área, terreno y mascota: «todavía no está disponible». |
| `HechizoEstadoUsuario` | `status_on_user` | PARCIAL | Invisibilidad, veneno, curar veneno, parálisis e inmovilizar (anillo e inmunidad de 4 s), remover parálisis, ceguera, estupidez, resucitar. Faltan mimetismo, revelar u ocultar, quemar, transformar y modificadores de atributos. |
| `HechizoPropUsuario` | `props_on_user` | PARCIAL | Vida, maná y aguante. Faltan hambre y sed y los buffs de atributos. **`GetUserMR` está fijo en 0.** |
| `HechizoEstadoNPC`, `HechizoPropNPC`, `HandleHechizoNPC` | `cast_on_npc`, `creatures.rs::status_on_npc` | PARCIAL | **Resistencia mágica del NPC `Default::default()` (0).** Faltan `AdjustNpcStatWithCasterLevel` y `GetMagicDamageModifier/Reduction`. |
| `HechizoCombinados` | `SpellType::Combined` | HECHO | |
| `HechizoInvocacion` | `pets.rs::cast_summon` | HECHO | |
| `HechizoSobreArea`, `AreaHechizo`, `AreaSpellTileDistance`, `ApplyAreaSpellFalloff`, `CanAreaSpellHitNpc`, `SendAreaSpellTargetFx` | — | **FALTA** | |
| `HandleHechizoTerreno`, `HechizoTerrenoEstado`, `HechizoMaterializacion`, `CastOnTerrain` | — | FALTA | Salvo la invocación. |
| `HandlePetSpell` | — | FALTA | |
| `HandlePhysicalSkill` | `can_cast` (solo el aguante) | PARCIAL | |
| `NpcLanzaSpellSobreUser` | `magic.rs::npc_cast_spell` | PARCIAL | Sin `MagicBonus` del NPC ni `GetUserMRForNpc`. |
| `NpcLanzaSpellSobreNpc` / `NpcLanzaSpellSobreArea` | `creatures.rs::npc_casts_on_npc` (mascotas) / — | PARCIAL / FALTA | |
| `checkHechizosEfectividad`, `GetSkinSpellSafe` | — | FALTA | Baja prioridad. |
| `SubirSkill(Magia)` | — | **FALTA** | `TODO(progression)` en `after_cast`. |

### EffectsOverTime.bas, carpeta EffectOverTime y *.cls

| Sistema | Rust | Estado | Notas |
|---|---|---|---|
| Motor: `CreateEffect`, `AddEffect`, `RemoveEffect`, `UpdateEffectOverTime`, `FindEffectOnTarget`, `ClearEffectList` | `magic.rs::parse_eots`, `eot_table` (caché), `start_eot`, `tick_eots` | PARCIAL | Solo sobre jugadores y solo los tipos 1 y 9; el resto se descarta en silencio. |
| `UpdateHpOverTime.cls` (tipo 1) | `tick_eots` | HECHO | |
| `UpdateManaOverTime.cls` (tipo 9) | `tick_eots` | HECHO | |
| `StatModifier.cls`, `ApplyEotModifier`, `RemoveEotModifier`, `ConvertToClientBuff` | — | FALTA | La base de los buffs y debuffs. |
| `ApplyEffectOnHit.cls`, `ApplyEffectOnParty.cls` | — | FALTA | |
| `DrunkEffect.cls` | — | FALTA | |
| `EffectProvoke.cls`, `EffectProvoked.cls` | — | FALTA | |
| `TranslationEffect.cls`, `AttrackEffect.cls` | — | FALTA | |
| `BonusDamageEffect`, `MultipleAttacks`, `ProtectEffect`, `TransformEffect`, `UnequipItem`, `DelayedBlast`, `clsTrap` (`CreateTrap`) | — | FALTA | |
| Ganchos `TargetUseMagic`, `TargetWillAttack`, `TargetDidHit`, `TargetFailedAttack`, `TargetApplyDamageReduction`, `TargetWasDamaged`, `TargetWillAttackPosition`, `TargetUpdateTerrain`, `ChangeOwner` | — | FALTA | Sin ellos no puede funcionar ningún EOT reactivo. |

### MODULO_NPCs.bas

| Sistema | Rust | Estado | Notas |
|---|---|---|---|
| Heap e índices: `InitializeNpcIndexHeap`, `GetAvailableNpcIndex`, `SetNpcRef`, `IncreaseNpcVersionId`, `ReleaseNpc` | ids `u32` (`world/entities.rs`) | HECHO (mejor) | |
| `CrearNPC`, `SpawnNpc`, `OpenNPC`, `InitializeNpcFromInfo`, `BuildNpcInfoCache`, `TestSpawnTrigger` | `npc_ai.rs::spawn_creatures`, `populate_map`, `make_npc`, `new_npc`, `npc_spawn_ok`, `closest_spawn_pos` | HECHO | |
| `MuereNpc`, `QuitarNPC`, `NPCTirarOro`, `OnNpcKilledUpdateQuest` | `combat.rs::kill_npc`, `drop_npc_loot`, `quest_on_npc_killed` | HECHO | |
| `ReSpawnNpc`, `NpcRespawnMap`, `ProcessRespawnQueue`, `RespawnQueuedNpc` | `tick_respawns`, `respawn_npc` | HECHO | Con tests. |
| `MakeNPCChar`, `ChangeNPCChar`, `EraseNPCChar`, `MoveNPCChar`, `WarpNpcChar`, `MoveNpcToSide`, `AnimacionIdle`, `TranslateNpcChar` | `npc_move`, `npc_face` | PARCIAL | Faltan warp, empuje a un lado e idle. |
| `UpdateNpcSpeed`, `GetNpcSpeedModifiers` | — | FALTA | |
| `NpcEnvenenarUser` | — | FALTA | |
| `StunNPc`, `Inmovilize`, `Paralice`, `CanMove`, `CanAttack` | `creatures.rs::npc_paralyzed`, `npc_rooted`, `tick_npc_status` | PARCIAL | Falta el aturdimiento (stun). |
| `ModifyHealth`, `DoDamageOrHeal`, `UserCanAttackNpc`, `DummyTargetAttacked`, `KillRandomNpc` | `damage_npc`, `faction_npc_attack_refusal` | PARCIAL | Faltan el muñeco y `KillRandomNpc`. |
| `GetPhysicalDamageModifier`, `GetMagicDamageModifier/Reduction`, `GetPhysicDamageReduction`, `GetEvasionBonus`, `GetHitBonus`, `GetDefenseBonus`, `GetLinearDamageBonus`, `GetSelfHealingBonus`, `GetMagicHealingBonus` | — | FALTA | Dependen de los EOT. |
| `CanAttackUser`, `CanHelpUser`, `CanHelpNpc`, `CanAttackNpc`, `CanSeeUser`, `CanPerformAttackAction` | `npc_enemy`, `valid_target`, `in_vision` | PARCIAL | No se tiene en cuenta la invisibilidad. |
| `GetOwnedBy`, `CanAttackNotOwner` | — | FALTA | «P2» en `npc_ai.rs:742`. |
| Mascotas: `QuitarMascota`, `QuitarMascotaNpc`, `FollowAmo`, `AllFollowAmo`, `DoFollow` | `pets.rs` (`release_pet`, `pet_follow`, `tame`, `taming_power`, `pets_on_*`) | HECHO | Con tests. |

### AI_NPC.bas

| Sistema | Rust | Estado | Notas |
|---|---|---|---|
| Despacho `NpcAI` | `npc_ai.rs::npc_ai` | PARCIAL | Solo `Wander`, `FixedInPos`, `Defend` y `Walk`; el resto cae en `_ => {}`. |
| `PerseguirUsuarioCercano`, `SelectNearestUser`, `SelectCurrentTarget`, `NPCHasAUserInFront`, `AI_AtacarUsuarioObjetivo` | `chase_nearest_user`, `user_in_front`, `attack_target` | PARCIAL | **Ignora la invisibilidad y el ocultarse.** |
| `AttackFromPos` | `attack_from_pos` | HECHO | |
| `SeguirAgresor`, `RestoreOldMovement` | `follow_aggressor`, `restore_old_movement` | HECHO | |
| `AI_CaminarSinRumbo*`, `AI_CaminarConRumbo` | `wander_near_origin`, `walk_towards` | HECHO | |
| `HacerCaminata`, `GoToNextWp`, `SetMovement` | `walk_route` | HECHO | |
| `AI_RangeAttack`, órbita, `ComputeNpcRangedRetreatDestination`, `NpcMarkTargetUnreachable` | — | FALTA | Los NPCs a distancia persiguen como si fueran de cuerpo a cuerpo (`npc_ai.rs:338`). |
| `TryGuardAttackHostileNpc`, `AI_GuardiaPersigueNpc` | `is_guard` | PARCIAL | Los guardias no atacan a NPCs. |
| `AI_NpcAtacaNpc`, `BuscarNpcEnArea`, `SelectNearestNpc` | solo `pet_ai` | FALTA | |
| `AI_SupportAndAttackNpc`, `SelectSupport/AttackSpellAndTarget`, `TryCastHelpSpell`, `TryCastAttackSpell`, `TrySupportThenAttackSpells`, `CanCastSpell`, `GetAvailableSpellEffects` | `npc_cast_one` (hechizo al azar) | PARCIAL | Sin elegir entre apoyo y ataque ni enfriamiento por hechizo. |
| `NpcLanzaSpellInmovilizado` | — | FALTA | |
| `AI_BgTank/Support/Ranged/Boss`, `AI_BGBossReturnToOrigin` | — | FALTA | |
| `MovimientoInvasion`, `HandleNpcCrossMapRoute` | — | FALTA | |
| `SeguirAmo` | `pets.rs::pet_ai` | HECHO | |
| `EsObjetivoValido`, `EsEnemigo`, `EnRangoVision`, `UsuarioAtacableConMagia/Melee` | `valid_target`, `npc_enemy`, `in_vision` | PARCIAL | Faltan invisibilidad, `Inmunidad`, mimetismo, GM y consulta. |

### PathFinding.bas y Matematicas.bas

| Sistema | Rust | Estado | Notas |
|---|---|---|---|
| A*: `SeekPath`, `MakePath`, `FollowPath`, `IsWalkable`, `ProcessAdjacent`, `Open/CloseVertex`, distancias | `ao-rules/pathfinding.rs`, `npc_ai.rs::follow_path`, `npc_walkable` | HECHO | Determinista, con tests. |
| `EnsurePathNoiseStrength`, `ShuffleHeadings` | rng | PARCIAL | |
| Strafe (`ResetNpcStrafeInfo`, `RegisterNpcDamageStrafe`, `SetNpcStrafeOffsetFromAttacker`, `ApplyNpcStrafeToDestination`) | — | FALTA | |
| Matemáticas (`Porcentaje`, `Distancia`, `RandomNumber`, `GetDirection`, máscaras) | `rules::porcentaje`, `distancia`, `rng.range`, `vb_round` | HECHO | Incluye el redondeo bancario de VB. |

## 2. Bugs o desvíos evidentes

1. **Los NPCs atacan en cuerpo a cuerpo a invisibles u ocultos** (`npc_ai.rs::valid_target` y `chase_nearest_user`). En el VB solo pueden si están pegados (`UsuarioAtacableConMelee`, línea 1436; `PerseguirUsuarioCercano`, línea 159).
2. **No sube ningún skill de combate ni de Magia** (`melee.rs`, `combat.rs`; TODO en `magic.rs::after_cast`).
3. **La resistencia mágica vale siempre 0**: `GetUserMR` en `props_on_user`, en `npc_cast_spell` y la del NPC en `cast_on_npc` (`Default::default()`). La magia pega más que en AO20.
4. **No hay `GetExpPenalty`**: se puede farmear criaturas bajas sin penalización.
5. No hay modificador de maná por clase (`GetSpellManaCostModifierByClass`).
6. `user_attacks_npc` usa `weapon_mod = 0`: falta la precisión mejorada del arma contra NPCs.
7. Las zonas seguras o de pelea están duplicadas (melee con `TRIGGER_FIGHT_ZONE/SAFE_ZONE`, magia con `TRIGGER_ARENA/SAFE`); hay riesgo de que diverjan.
8. `start_eot` descarta los EOT que no son de tipo 1 o 9, pero el hechizo se cobra igual: el jugador paga el maná y no pasa nada.
9. Menor: `on_meditate` comprueba `can_meditate` dos veces.

## 3. Faltantes priorizados

| Prioridad | Faltante | Esfuerzo |
|---|---|---|
| Alta | `SubirSkill` en combate y magia | S |
| Alta | Invisibilidad y ocultarse en la elección de objetivo de los NPCs | S |
| Alta | Resistencia mágica de usuario y de NPC (`GetUserMR`, `GetUserMRForNpc`, `MagicDef/DefM`) | S-M |
| Alta | Motor de EOT completo: `StatModifier` y ganchos `Target*` | L |
| Alta | Hechizos de área (`HechizoSobreArea`, falloff) | M |
| Alta | IA a distancia (`AI_RangeAttack`, órbita, retirada) | M |
| Alta | IA de hechizos de NPC (apoyo o ataque, enfriamientos) | M |
| Media | `GetExpPenalty` | S |
| Media | Veneno de NPC (`NpcEnvenenarUser`) | S |
| Media | Dueño de la criatura (`GetOwnedBy`, `CanAttackNotOwner`) | M |
| Media | `PuedeAtacar` completo (grupo, clan, seguro, GM, consulta, inmunidad) | M |
| Media | NPC contra NPC, guardias contra NPCs hostiles, criaturas que responden a las mascotas | M |
| Media | Provocar, proteger, desplazar o atraer, daño extra, ataques múltiples, desequipar (EOT) | L en conjunto |
| Media | Terreno y materialización, `HandlePetSpell` | M |
| Media | Mimetismo, aturdimiento, `UpdateNpcSpeed` | M |
| Media | `DesequiparObjetoDeUnGolpe` | S |
| Media | Maná por clase, palabras mágicas | S |
| Baja | Comportamientos BG y jefes, invasión y rutas entre mapas | L |
| Baja | `MultiShot` y flechas a un tile o una dirección | M |
| Baja | ElementalTags | M |
| Baja | `DelayedBlast`, trampas, `TransformEffect`, `DrunkEffect` | M |
| Baja | Strafe, idle, muñeco, sonido de bloqueo | S |

## 4. Cobertura estimada: ~45 %

- **Combate físico: ~75 %.** Las fórmulas son fieles y tienen tests. Faltan las subidas de skill, la penalización de experiencia, desarmar, el multidisparo y los modificadores.
- **Magia: ~45 %.** El objetivo único está bien. Faltan área, terreno, la resistencia mágica y los estados secundarios.
- **Efectos en el tiempo: ~15 %.** Existen 2 de unos 15 tipos y no hay ganchos reactivos.
- **NPCs, ciclo de vida y mascotas: ~80 %.**
- **IA: ~40 %.** Están 4 de los 15 modos de movimiento, el A* y la persecución básica.

Media ponderada (combate 30, magia 25, EOT 15, NPCs 10, IA 20) ≈ 52 %. La rebajo a **~45 %** porque los huecos de la sección 2 (resistencia mágica a 0, invisibilidad ignorada, skills que no suben) alteran el equilibrio de lo que ya «funciona».
