# Auditoría AO20 — Parte 2: objetos, oficios, comercio y mundo

Fecha: 2026-11-18. Auditoría de solo lectura (grep + lectura de código; sin compilar ni ejecutar tests).
Referencia: `X:\dev\ao20\argentum-online-server-classic\Codigo`. Rust: `X:\dev\ao_rs\crates`.
Se ignora lo TSAO (`tsao*.rs`, `gm_tsao.rs`), pero se avisa cuando una ruta AO20 depende de algo TSAO.

Leyenda: **HECHO** = igual o mejor que VB · **PARCIAL** = existe pero falta o difiere algo · **FALTA** = nada.

## 1. Tabla por sistema

### 1.1 Inventario y objetos (InvUsuario.bas, Modulo_InventANDobj.bas)

| Sistema VB | Rust | Estado | Notas |
|---|---|---|---|
| `MeterItemEnInventario`, `HayLugarEnInventario`, `GetSlotForItemInInventory`, `QuitarUserInvItem`, `UpdateUserInv` | `systems/inventory.rs` `add_item`, `slot_for`, `remove_from_slot`, `send_slot/send_inventory` | HECHO | Mochila de 42 slots probada (`backpack_unlocks_42_slots...`). |
| `DropObj`, `TirarOro`, `MakeObj`, `EraseObj`, `PickObj`, `PickObjAt`, `TryInsertGroundObjectIntoInventorySlot` | `drop_obj`, `throw_gold`, `make_obj`, `erase_obj`, `on_pick_up`, `free_tile`, `drop_to_floor` | HECHO | Con tests de soltar/recoger vistos por otro jugador. |
| `TirarTodosLosItems`, `ItemSeCae`, `ItemNewbie`, `ItemNewbieProtegidoAlMorir`, `PirataCaeItem` | `drop_inventory_on_death`, `ao_rules::items::falls_on_death`, `is_newbie` | PARCIAL | Falta `PirataCaeItem` (el pirata con barco conserva un %). Revisar `ItemNewbieProtegidoAlMorir`. |
| `ClasePuedeUsarItem`, `RazaPuedeUsarItem`, `SexoPuedeUsarItem`, `FaccionPuedeUsarItem`, `JerarquiaPuedeUsarItem`, `CheckClaseTipo` | `ao_rules::items::can_use`, `class_can_use`, `race_can_use`, `gender_can_use` | HECHO | Incluye nivel mín/máx, facción/rango y skill requerido. |
| `EquiparInvItem`, `Desequipar`, `UpdateCharWithEquipedItems`, `ObtenerRopaje` | `equip`, `unequip`, `look_of`, `restore_equipment`, `items::role_of`, `clothing_for` | HECHO | Arma, escudo, casco, armadura, mochila, herramienta, munición, anillo y amuleto. |
| `EquipArrow`, `ValidateEquippedArrow` | `combat.rs` / `melee.rs` (`ObjType::Arrows`) | HECHO | |
| `EquipAura`, `SkinEquip`, `AddSkin`, `HaveThisSkin`, `CanEquipSkin`, `DesequiparSkin`, `ResetUserSkinsInventory`, `SkinRequireObject` | — (solo `trade.rs::is_skin`, para que no se vendan) | FALTA | 7 tipos de skin (38-43, 46) sin uso; `UseInvItem` no tiene el caso de skins. |
| `EquiparBarco` / `Trabajo.DoNavega` | `thief.rs::use_boat`, `walk::can_enter_as` | PARCIAL | Embarcar, desembarcar y moverse por agua funcionan. El nivel mínimo sale de `balance.boat_min_level` (30, de TSAO). Faltan: skill de Navegación, galeón/galera por clase, nadar (`VALIDONADO`, `NADOBAJOTECHO`), skins de barco y pescar con red desde el barco. |
| `IsItemInCooldown`, cooldown por tipo | `on_use_item` (`USE_TIMER`, `cooldown_type`) | HECHO | |
| `IsConsumableFreeZone` | `items::consumable_free_map` | HECHO | |
| `UseInvItem` → `otUseOnce` (comida) | `on_use_item` | PARCIAL | Solo suma `MinHam`. Faltan el subtipo `eFish` y los efectos extra (`AddOrResetEffect`). |
| `UseInvItem` → `otGoldCoin`, `otDrinks`, `otFullBottle`, `otParchment` | `on_use_item` | HECHO | |
| `UseInvItem` → `otEmptyBottle` (llenar en agua) | — | FALTA | |
| `otPotions`: HP, MP, Sta, veneno, parálisis, `HealsAllStatusEffects` | `use_potion` | HECHO | |
| `otPotions`: Agilidad y Fuerza temporales (`General.DuracionPociones`), cara aleatoria y legendaria, sexo, invisibilidad, divorcio, partículas temporales, poción negra (21) | `use_potion` → "Ese efecto todavía no está disponible" | FALTA | 8 subtipos sin hacer. Fuerza/Agilidad se usan muchísimo en PvP. |
| `otWeapon` (usar con click: arco), `UseArpon`, `UseHandCannon` | — | FALTA | No hay arpón ni cañón. |
| `otWorkingTools` / `otMinerals` (abrir oficio o pedir objetivo) | `craft.rs::craft_use_tool`, `work.rs::on_work` | HECHO | |
| `otChest` | `open_chest` | PARCIAL | Usa `chest_kind` y `chest_key`; no hay test que lo compare con los cofres AO20. |
| `otKeys` / `ModLlaves.bas` (`UsarLlave`, `SacarLlaveDeLLavero`, `EnviarLlaves`), `EliminarLlaves` | — | FALTA | Lo está haciendo el equipo principal. Ojo: el llavero (llaves fuera del inventario) también es de ModLlaves. |
| `otMusicalInstruments`, `otMagicalInstrument` | solo `role_of` (como anillo) | FALTA | Ni sonido ni efecto (los mágicos lanzan un efecto en área). |
| `otSaddles` / `Trabajo.DoMontar` | solo `role_of` y `clothing_for` | FALTA | No hay montura ni cambio de velocidad. |
| `otDonator`, `otPassageTicket`, `otRecallStones` (runas ReturnHome / MesonSafePassage / FastTravel), `otMap` | — | FALTA | Pasajes y runas de regreso son básicos en AO20. |
| `OtQuest`, `otAmulets` (`TargetUsable`, `eProtectedResources`), `otUsableOntarget`, `UserTargetableItem` | — | FALTA | |
| `otFactionForgiveness`, `otCastleSpawner`, `otCollectibleCard` | — | FALTA | |
| `ResurrectWithItem` | — | FALTA | |
| `PlaceTrap` (+ `clsTrap`, `ModMap.CanAddTrapAt` / `ActivateTrap`) | — | FALTA | El test `traps_change_nothing` es de anti-trampas, no de trampas del juego. |
| `TryRepairFishingRod` | — | FALTA | |
| `EnivarArmasConstruibles`, `EnivarObjConstruibles*`, `EnivarArmadurasConstruibles`, `SendCraftableElementRunes` | `craft.rs::craft_open`, `craft_recipes` | HECHO | `CanElementalTagBeApplied` FALTA (no existen las ElementalTags). |
| `CargarInvent`, `QuitarNpcInvItem`, `QuedanItems`, `EncontrarCant`, `ResetNpcInv` | `trade.rs` `original_stock`, `stock`, `take_from_stock` | HECHO | Con reposición probada. |
| `NpcDropObj`, `NpcDropQuestObj`, `TirarItemAlPiso` | `drop_npc_loot`, `drop_creature_loot`, `quest.rs` | HECHO | |

Tipos de `obj.dat` (`e_OBJType`) **sin ningún uso** en el servidor Rust (sin contar TSAO): Doors (6), SignBoards (8), CastleSpawner (10), Teleport (19), Decorations (20), BlacksmithMaterial (29), EmptyBottle (33), PassageTicket (36), Map (37), RecallStones (45), Mail (47), Donator (50), Quest (51), FishingPool (52), UsableOnTarget (53), Plants (54), ElementalRune (55), FactionForgiveness (56) y CollectibleCard (57).
Con rol de equipo pero sin efecto propio: MusicalInstruments, MagicalInstrument, Saddles, Amulets, RingAccessory y Skins*.

### 1.2 Oficios (Trabajo.bas, ModMining, ModLumberjack, modFishing, modSmelting, modCrafteos)

| Sistema VB | Rust | Estado | Notas |
|---|---|---|---|
| `ModMining.MineMinerals`, `CanUserExtractMinerals` | `work.rs::work_gather`, `work_check_gather`; `ao_rules::work::{luck, gather_roll_max, extract_range}` | HECHO | La fórmula de `Suerte` es idéntica. Blodium exige el pico especial. |
| `ModLumberjack.ChopWood`, `CanUserExtractWood` | lo mismo (`ELVEN_WOOD`, `PINE_WOOD`) | HECHO | La leña élfica exige el hacha élfica. |
| `ActualizarRecurso`, `GetExtractResourceForLevel`, `GiveExpWhileWorking`, `ComputeWorkingExp` | `work_gather` (recarga por `regen_time`), `give_exp(JOB_EXP)` | PARCIAL | La exp es fija en 1 (`[JOBEXP]`). Comprobar si `ComputeWorkingExp` escala con el nivel. |
| `modFishing`: `PerformFishing`, `GetFishingChance`, `ObtenerPezRandom`, `CanUserFish`, `ValidateFishingPosition`, `IsStandingOnWater`, `IsAdjacentToWater`, `HasSpecialFishingRewards` | `work_fish`, `work_catch`, `rules::fishing_chance`, `catch_range`, `RecursosEspeciales.dat` | PARCIAL | Faltan: minijuego del pez especial (se entrega directo), `IsUniqueMapFish` (pez único del mapa y su vigía), `InitializeFishingBonuses`, recolocar los bancos de pesca (`CreateFishingPool`), red desde barco y `ThrowNetToTarget`. |
| `EntregarPezEspecial`, `WorldActions.HandleFishingDeliveryNpcInteraction`, `CalculateSpecialFishingRewards` | `interact.rs` → "todavía no está disponible" | FALTA | NPC `FishDelivery`. |
| `modSmelting`: `SmeltMinerals`, `CraftIngots`, `MineralsRequiredPerIngot`, `CanUserSmelt` | `work_smelt`, `rules::minerals_per_ingot` | HECHO | |
| `HerreroConstruirItem`, `CarpinteroConstruirItem`, `AlquimistaConstruirItem`, `SastreConstruirItem` con `*TieneMateriales`, `*QuitarMateriales`, `PuedeConstruir*`, `Mod*` por clase y `KnowsCraftingRecipe` | `craft.rs::craft_build`, `craft_step`; `ao_rules::craft::{materials, class_mod, effective_skill, can_build, units_per_step}` | HECHO | Mejor que VB: la cantidad la lleva el servidor y cada paso se vuelve a comprobar. |
| `WorldActions.HandleAnvilAction` | `craft_click_tile`, `craft_anvil_at` | HECHO | Incluye el yunque de Blodium. |
| `modCrafteos` (`DoCraftItem`, `CheckCraftingResult`, `CalculateCraftProb`, `GetRecipeKey`, `ReturnCraftingItems`) + `clsCrafteo` | — | FALTA | No existe la mesa de crafteo por combinación de ingredientes. |
| `SubirSkill` | `work_raise_skill`, `craft_raise_skill`, `rules::raise_roll_max`, `skill_cap` | HECHO | |
| `TratarDeHacerFogata`, `HandleCampfireTwigAction`, `General.ApagarFogatas` | — | FALTA | Fogatas. |
| `DoOcultarse`, `DoPermanecerOculto`, `DoRobar`, `RobarObjeto`, `ObjEsRobable`, `TieneObjetosRobables` | `thief.rs` | PARCIAL | Existe, pero el archivo mezcla código TSAO: hay que confirmar que siga la variante AO20. |
| `DoDomar` y compañía | `pets.rs` | HECHO (otra área) | |
| `DoMontar` | — | FALTA | Ver Saddles. |

### 1.3 Comercio y banco (Comercio.bas, mdlCOmercioConUsuario.bas, modBanco.bas, ModShopAO20, ModSubasta)

| Sistema VB | Rust | Estado | Notas |
|---|---|---|---|
| `Comercio`, `IniciarComercioNPC`, `SlotEnNPCInv`, `UpdateNpcInv(ToAll)`, `Descuento`, `SalePrice`, `EsSkinNoComprablePorComerciante` | `trade.rs` `open_npc_trade`, `shop_buy`, `shop_sell`, `shop_changed`; `ao_rules::trade::{discount, buy_price, sale_price}` | HECHO | Falta `SubirSkill(Comerciar)` (subir el skill de comercio usándolo). |
| `IniciarComercioConUsuario`, `EnviarObjetoTransaccion`, `AceptarComercioUsu`, `FinComerciarUsu` | `trade_propose`, `trade_offer(_gold)`, `trade_confirm/accept`, `exchange` | HECHO (mejor) | Atómico y con doble confirmación. |
| `modBanco` (`IniciarDeposito`, `UserRetiraItem`, `UserDepositaItem`, `UserDejaObj`, `QuitarBancoInvItem`, oro) | `bank.rs` | HECHO | Además trae la bóveda de clan (extra de TSAO). No comprobé `SendUserBovedaTxt` (consulta de GM). |
| `ModShopAO20` (`is_purchaseable_item`, `RegisterTransaction`): tienda de créditos/Patreon | `interact.rs` → "no disponible" | FALTA | Depende de créditos y Patreon. |
| `ModSubasta` (`IniciarSubasta`, `FinalizarSubasta`, `DevolverItem`, `CancelarSubasta`) | `event.rs` (subastas, ventana U) | PARCIAL | Es un sistema propio, no el del VB (pregón con NPC subastador). Comprobar la devolución del objeto al desconectarse. |

### 1.4 Mundo (GameLogic, General, ModMap, Hogar, WorldTime, ModClimas, InstanceManager, ModAreas, TLimpiezaItem, ModTesoros, ModUnderworld, ModPhoenix, WorldActions, modShipTravel, CustomScenarios, ModCollectibleCards)

| Sistema VB | Rust | Estado | Notas |
|---|---|---|---|
| `LegalPos`, `LegalWalk`, `InMapBounds`, `ClosestLegalPos`, `FindLegalPos` | `ao_rules::walk::{can_enter, can_enter_as, standable, line_of_sight}`, `free_tile_near` | HECHO | |
| `DoTileEvents` → `TileExit` | `movement.rs::on_walk` → `session.rs::teleport` | PARCIAL | Solo hace la salida simple. Faltan: `CheckMapRestrictions` (Newbie, NoPKs, NoCiudadanos, SoloClanes, MinLevel/MaxLevel, OnlyGroups, OnlyPatreon); `TileExit.Map < 0` (volver a `ReturnPos`); `EsMapaInterdimensional`; la red de transporte (`otTeleport` subtipo `eTransportNetwork`, `GetTransportNextIndex`); teleports como objeto en el suelo; `ClosestLegalPos` en barco; persecución entre mapas (`modNpcCrossMapPursuit`). El lector `.csm` descarta `restrict` (`csm.rs`, `_restrict`) y no se carga MapInfo. |
| Triggers de `DoTileEvents` y otros: `AUTORESU` (7), `TRANSFER_ONLY_DEAD` (14), `EMPEROR_CASTLE_ENTRY_*` (21+), `CARCEL` (19), `ONLY_PATREON_TILE` (20), `WORKERONLY` (13), `ESCALERA` (12), `ANTIPIQUETE` (5) | — | FALTA | Solo se usan `ZonaSegura` (4), `ZONAPELEA` (6), `POSINVALIDA` (3, para NPCs), `PESCAINVALIDA` (10) y el techo (`roof_triggers`). |
| `ModMap`: trampas (`CanAddTrapAt`, `ActivateTrap`, `clsTrap`) | — | FALTA | |
| `General`: `HayAgua`, `HayCosta`, `HayLava`, `EfectoLava`, `Intemperie`, `EfectoFrio` (nieve), `EsArbol` | `FLAG_WATER`; `AoMap.snow` se carga | PARCIAL | El agua sí. No hay lava, ni frío por nieve o intemperie, ni daño ambiental. |
| `HambreYSed`, `RecStamina`, `Sanar`, `EfectoVeneno`… | `regen.rs` | HECHO (otra área) | |
| `Bloquear`, `BloquearPuerta`, `HayPuerta`, `HandleDoorAction`, `HandleNpcDoorAction`, puertas Hoo (`ResolveHooDoorDestination`, `ExecuteHooHouseDoorAction`, `UserHasMatchingHooDoorKey`) | — | FALTA (en curso por el equipo) | Revisión breve: las puertas Hoo son puertas de casa que te llevan al interior si tienes la llave. Conviene que el diseño de puertas ya las contemple. |
| `HandleSignAction` (carteles) | — | FALTA | `otSignBoards` sin uso. Los objetos del `.csm` no se colocan en el mundo: solo los leen work y craft (`dat::craft::map_objects`). |
| `HandleWorldAction`, `IsWorldActionRequestValid`, `TryHandleNpcWorldAction`, `HandleNpcInteractionByType`, `HandleReviverNpcInteraction`, `HandleFactionRecruiterNpcInteraction`, `HandleGovernorNpcInteraction` | `interact.rs::on_interact`, `priest`, `faction_npc` | PARCIAL | Los NPCs comunes funcionan. Faltan la acción sobre objetos de varias casillas (`TryHandleMultiTileObjectWorldAction`) y los NPCs Pirata, gobernador (hogar), FishDelivery, Ao20Shop y EventMaster. |
| `Hogar.bas` (`goHome`, `TravelingEffect`, `HomeArrival`, `IntervaloGoHome`, `IsValidCity`) | `chat.rs` `/regresar` (versión TSAO, ciudad fija de `balance.home`) | PARCIAL | En AO20, /HOGAR tiene cuenta atrás que se cancela al moverse, la ciudad se elige en el gobernador y el intervalo cambia si estás muerto o vivo. |
| `modShipTravel` (barcos Nix-Arghal-Forgat, `MaybeRunShipTravel`, `GetPassSlot`, `MsnEnbarque`) | `travel.rs` (NPC transportador con destinos) | PARCIAL | Se puede viajar con NPC, pero no hay barco con horario ni pasaje (`otPassageTicket`). |
| `WorldTime.bas` (día/noche, `WorldTime_HandleHora`, `WorldTime_DayLenMs`, `WorldTime_Resync`) | — (`/HORA` solo como comando de GM) | FALTA | No hay ciclo de día y noche para el cliente. |
| `ModClimas` (nubes, lluvia/nieve, niebla, `ResetMeteo`) | `weather.rs::tick_weather` | HECHO | Falta `SetAtmosphericFogState` por mapa (`AoMap.fog`). |
| `InstanceManager` (`CloneMap`, `CloneMapWithTranslations`, heap de instancias) | `world/instance.rs` (una instancia por mapa) | PARCIAL | No se clonan mapas para eventos o mazmorras privadas. |
| `ModAreas` (áreas de visión, `CheckUpdateNeededUser/Npc`) | broadcast por mapa + `VISION_X/Y` | PARCIAL | Se envía a todo el mapa. El jugador ve lo correcto, pero escala peor con mucha gente. |
| `TLimpiezaItem` (limpieza periódica del suelo) | — | FALTA | `GroundItems` nunca caduca: los mapas se llenan de objetos tirados. |
| `ModTesoros` (`InitTesoro`, `InitRegalo`, `PerderTesoro`, `PerderRegalo`) | `thief.rs` `place_treasure`, `read_treasure_map`, `/desenterrar` | PARCIAL (es TSAO) | Lo que hay es el tesoro de TSAO. Falta el de AO20: un tesoro o regalo que aparece en un mapa con anuncio global y se pierde con el tiempo. |
| `ModUnderworld` (portales al inframundo, `KickUsersFromUnderworld`) | — | FALTA | |
| `ModPhoenix` (`MaybeSpawnFenix`) | — | FALTA | |
| `CustomScenarios` (`SetCustomScenario`, `UserCanPickUpItem`, `UserCanDropItem`, `ClearMap`) | `event.rs` (eventos propios) | PARCIAL | No hay ganchos por escenario para soltar o recoger objetos. |
| `EsMapaNoDrop`, `EsMapaEvento`, `ItemNoEsDeMapa`, `EsObjetoFijo`, `CanTransferWorldItemAt` | `legal_floor`, `Agarrable` en `on_pick_up` | PARCIAL | Faltan la lista de mapas sin drop y la regla de objetos fijos del mapa. |
| `ModCollectibleCards` (cartas de NPC con bonus de exp, daño y drop) | — | FALTA | |

## 2. Faltantes priorizados

| # | Faltante | Impacto | Esfuerzo |
|---|---|---|---|
| 1 | Puertas y llaves (`HandleDoorAction`, `UsarLlave`, llavero, puertas Hoo de casa). En curso | Alta | M |
| 2 | `CheckMapRestrictions` + MapInfo (newbie, nivel mín/máx, ciudadano/criminal, clan, grupo) y `TileExit < 0` | Alta | S |
| 3 | Pociones de Fuerza/Agilidad temporales (`DuracionPociones`), invisibilidad, cara y sexo | Alta | S |
| 4 | Triggers de casilla: `AUTORESU`, `TRANSFER_ONLY_DEAD`, `CARCEL`, `WORKERONLY`, `ANTIPIQUETE` y entradas de castillo | Alta | M |
| 5 | Colocar en el mundo los objetos del `.csm`: teleports (`otTeleport`, red de transporte), carteles y decoraciones | Alta | M |
| 6 | Limpieza del suelo (`TLimpiezaItem`) | Alta | S |
| 7 | /HOGAR de AO20 (`Hogar.bas`) y runas de regreso (`otRecallStones`) | Media | S |
| 8 | Navegación completa: skill, barco por clase, nadar (`VALIDONADO`), red desde barco, `PirataCaeItem` | Media | M |
| 9 | Botella vacía (llenar en agua) y fogatas (`TratarDeHacerFogata`, `ApagarFogatas`) | Media | S |
| 10 | Mesa de crafteo por combinación (`modCrafteos`, `clsCrafteo`) | Media | M |
| 11 | Monturas (`otSaddles`, `DoMontar`) e instrumentos mágicos | Media | M |
| 12 | Día y noche (`WorldTime.bas`), niebla por mapa, frío y lava | Media | S |
| 13 | Pesca especial: minijuego, pez único del mapa, NPC de entrega (`EntregarPezEspecial`) | Media | M |
| 14 | Barcos con horario y pasaje (`modShipTravel`, `otPassageTicket`) | Media | M |
| 15 | Trampas (`PlaceTrap`, `clsTrap`, `ActivateTrap`) | Baja | M |
| 16 | Skins y auras (7 tipos de objeto) | Baja | M |
| 17 | Tesoro y regalo de AO20 (`ModTesoros`), inframundo (`ModUnderworld`), fénix (`ModPhoenix`) | Baja | M |
| 18 | Mapas clonados (`InstanceManager.CloneMap`) | Baja | L |
| 19 | Cartas coleccionables, perdón de facción, castle spawner, objetos de quest o con objetivo, `ResurrectWithItem`, arpón y cañón | Baja | M |
| 20 | Tienda AO20/Patreon (`ModShopAO20`), `ElementalTags`, `SubirSkill(Comerciar)` | Baja | S-M |
| 21 | Áreas de visión (`ModAreas`) en vez de enviar a todo el mapa (rendimiento) | Baja | M |

## 3. Cobertura estimada: ~55 %

Por bloque:
- Inventario y equipo base: ~90 %. Sólido y con tests.
- Uso de objetos (`UseInvItem`): ~35 %. Están 9 de unos 30 casos. Falta la mitad de las pociones y todos los objetos de mundo y de viaje.
- Oficios: ~80 %. Recolección, fundición y construcción con herramienta están bien. Falta el crafteo por combinación, las fogatas y la pesca especial.
- Comercio NPC, entre jugadores y banco: ~90 %. En varios puntos es mejor que el VB.
- Mundo (triggers, restricciones de mapa, puertas, teleports, hogar, tiempo, limpieza, eventos de mundo): ~25 %. Es lo más flojo: al moverse solo se resuelve `TileExit`, y solo se usan 4 de los ~20 triggers.

Si los ponderamos así (objetos 30 %, oficios 25 %, comercio 20 %, mundo 25 %): 0,3·0,6 + 0,25·0,8 + 0,2·0,9 + 0,25·0,25 ≈ **0,62**. Pero muchos módulos de mundo están a cero (inframundo, fénix, tesoro AO20, día/noche, limpieza, trampas, cartas), así que la cifra realista para toda el área es **~55 %**.

Avisos:
- `work.rs` dice en su cabecera "no boats in this port", pero `use_boat` existe. El comentario está desactualizado y la pesca con red se rechaza siempre.
- Hay restos de TSAO en rutas AO20: `boat_min_level` (30), `/regresar`, el tesoro de `/desenterrar` y las ramas `tsao_type` dentro de `on_use_item`.
- No compilé ni ejecuté tests: los estados salen de leer el código.
