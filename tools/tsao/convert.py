"""Converts the Tierras Sagradas (AO 0.11.5 DX8) client/server data into the AO20-style
`Recursos` layout the engine reads: init/*.ini indexes, Mapas/mapaN.csm, Dat/.

usage: python -I convert.py <tsao_root> <out_root>
Graphics come from unpack_grh.py (out_root/Graficos).
"""
import os
import re
import struct
import sys

HEADER = 263  # tCabecera: Desc*255 + CRC + MagicWord


def read_ind_graphics(path):
    """Graficos.ind -> {grh: (file, sx, sy, w, h)} | {grh: ([frames], speed)}."""
    d = open(path, "rb").read()
    _ver, count = struct.unpack_from("<ii", d, 0)
    p, out = 8, {}
    while p + 6 <= len(d):
        grh, nf = struct.unpack_from("<ih", d, p)
        p += 6
        if grh <= 0 or nf <= 0:
            break
        if nf > 1:
            frames = struct.unpack_from("<%di" % nf, d, p)
            p += 4 * nf
            (speed,) = struct.unpack_from("<f", d, p)
            p += 4
            out[grh] = (list(frames), speed)
        else:
            out[grh] = struct.unpack_from("<ihhhh", d, p)
            p += 12
    return out


def write_graficos(grhs, path):
    lines = ["[INIT]", f"NumGrh={max(grhs)}", "", "[Graphics]"]
    for g in sorted(grhs):
        v = grhs[g]
        if isinstance(v[0], list):
            frames, speed = v
            lines.append(f"Grh{g}={len(frames)}-" + "-".join(map(str, frames)) + f"-{speed:g}")
        else:
            lines.append(f"Grh{g}=1-" + "-".join(map(str, v)))
    open(path, "w", newline="\r\n").write("\n".join(lines) + "\n")


def read_ind_records(path, fmt):
    d = open(path, "rb").read()
    (n,) = struct.unpack_from("<h", d, HEADER)
    size = struct.calcsize(fmt)
    return [struct.unpack_from(fmt, d, HEADER + 2 + i * size) for i in range(n)]


def write_sections(path, count_key, n, prefix, rows):
    lines = ["[INIT]", f"{count_key}={n}", ""]
    for i, kv in rows:
        lines.append(f"[{prefix}{i}]")
        lines += [f"{k}={v}" for k, v in kv]
        lines.append("")
    open(path, "w", newline="\r\n").write("\n".join(lines))


# Faces per gender-race (TCP_HandleData1.bas `CABEZI`, the surgeon; the engine uses the
# same table at character creation). Races: 1 human, 2 elf, 3 drow, 4 gnome, 5 dwarf
# (TSAO has no orcs).
HEADS = {
    (1, 1): (1, 30), (1, 2): (101, 113), (1, 3): (202, 209), (1, 4): (401, 406), (1, 5): (301, 305),
    (2, 1): (70, 76), (2, 2): (170, 176), (2, 3): (270, 280), (2, 4): (470, 474), (2, 5): (370, 373),
}


def write_head_ranges(path, existing):
    """Only heads that have graphics; contiguous runs become ranges."""
    lines = []
    for (g, r), (a, b) in sorted(HEADS.items()):
        hs = [h for h in range(a, b + 1) if h in existing]
        runs, start, prev = [], None, None
        for h in hs:
            if start is None:
                start = prev = h
            elif h == prev + 1:
                prev = h
            else:
                runs.append((start, prev))
                start = prev = h
        if start is not None:
            runs.append((start, prev))
        lines += [f"[{g}-{r}]", "Ranges=" + ",".join(f"{x}-{y}" for x, y in runs), ""]
    open(path, "w", newline="\r\n").write("\n".join(lines))


def clean_ini(src, dst):
    """Copies an INI dropping the ' comments after values."""
    text = open(src, encoding="latin-1").read()
    text = re.sub(r"[ \t]*'[^\r\n]*", "", text)
    open(dst, "w", encoding="latin-1", newline="\r\n").write(text.replace("\r\n", "\n"))


def ini_sections(path):
    out, cur = {}, None
    for line in open(path, encoding="latin-1"):
        line = line.split("'")[0].strip()
        if line.startswith("[") and line.endswith("]"):
            cur = out.setdefault(line[1:-1].lower(), {})
        elif "=" in line and cur is not None:
            k, v = line.split("=", 1)
            cur[k.strip().lower()] = v.strip()
    return out


def vb_string(s):
    b = s.encode("cp1252", "replace")
    return struct.pack("<h", len(b)) + b


def convert_map(num, maps_dir, info):
    """0.11.5 .map/.inf -> legacy 4-layer .csm (file layers 1..4 -> engine 1,2,4,5)."""
    m = open(os.path.join(maps_dir, f"Mapa{num}.map"), "rb").read()
    inf_path = os.path.join(maps_dir, f"Mapa{num}.inf")
    inf = open(inf_path, "rb").read()
    pm, pi = 273, 10
    blocked, layers, triggers, particles, lights, objs, npcs, exits = [], [[], [], [], []], [], [], [], [], [], []
    for y in range(1, 101):
        for x in range(1, 101):
            f = m[pm]
            pm += 1
            if f & 1:
                blocked.append((x, y, 0x0F))
            for li, bit in enumerate((0, 2, 4, 8)):
                if li == 0 or f & bit:
                    (g,) = struct.unpack_from("<H", m, pm)
                    pm += 2
                    if g:
                        layers[li].append((x, y, g))
            if f & 16:
                (t,) = struct.unpack_from("<h", m, pm)
                pm += 2
                if t:
                    triggers.append((x, y, t))
            if f & 32:
                (pg,) = struct.unpack_from("<h", m, pm)
                pm += 2
                if pg > 0:
                    particles.append((x, y, pg))
            if f & 64:
                rng, r, g, b = struct.unpack_from("<4h", m, pm)
                pm += 8
                if rng > 0:
                    lights.append((x, y, r & 255, g & 255, b & 255, min(rng, 255)))
            fi = inf[pi]
            pi += 1
            if fi & 1:
                em, ex, ey = struct.unpack_from("<3h", inf, pi)
                pi += 6
                if em > 0:
                    exits.append((x, y, em, ex, ey))
            if fi & 2:
                (npc,) = struct.unpack_from("<h", inf, pi)
                pi += 2
                if npc > 0:
                    npcs.append((x, y, npc))
            if fi & 4:
                oi, amount = struct.unpack_from("<2h", inf, pi)
                pi += 4
                if oi > 0:
                    objs.append((x, y, oi, amount))
    out = bytearray()
    out += struct.pack("<i", len(blocked))
    out += struct.pack("<4i", *(len(l) for l in layers))
    out += struct.pack("<6i", len(triggers), len(lights), len(particles), len(npcs), len(objs), len(exits))
    out += struct.pack("<ii", 0, 0)  # tMapSize
    out += vb_string(info.get("name", f"Mapa {num}"))
    # tMapDat music_numberHi, music_numberLow: the client plays the Low one.
    out += b"\x00" + vb_string("") + struct.pack("<ii", 0, int(info.get("musicnum", "0") or 0))
    # FileIO.bas: `Pk=0` -> MapInfo.Pk = True (PvP allowed); `Pk=1` -> safe map.
    out += bytes([1 if info.get("pk", "0").strip() == "1" else 0])
    out += vb_string(info.get("zona", "")) + vb_string(info.get("terreno", "")) + vb_string("")
    out += bytes(16) + vb_string("")
    out += bytes([0, 0, 0])  # rain, snow, fog
    for x, y, v in blocked:
        out += struct.pack("<hhB", x, y, v)
    for l in layers:
        for x, y, g in l:
            out += struct.pack("<hhi", x, y, g)
    for x, y, t in triggers:
        out += struct.pack("<hhh", x, y, t)
    for x, y, pg in particles:
        out += struct.pack("<hhi", x, y, pg)
    for x, y, r, g, b, rng in lights:
        out += struct.pack("<hhBBBBB", x, y, b, g, r, 255, rng)
    for x, y, oi, amount in objs:
        out += struct.pack("<hhhh", x, y, oi, amount)
    for x, y, npc in npcs:
        out += struct.pack("<hhh", x, y, npc)
    for x, y, em, ex, ey in exits:
        out += struct.pack("<hhhhh", x, y, em, ex, ey)
    return bytes(out), info.get("name", "")


AO20_DAT = r"X:/dev/ao20/Recursos/Dat"

# TSAO eNPCType -> engine NpcType (AO20 ids). TSAO-only types keep their number in
# `TsaoType` and become ordinary NPCs until their system is ported (5 is not a TSAO type:
# its NPCs are scripted by number in the VB code).
NPC_TYPE = {0: 0, 1: 1, 2: 2, 3: 3, 4: 4, 6: 6, 7: 7, 8: 8, 11: 17, 12: 26, 19: 102, 22: 101, 23: 100}
# NPCs whose TSAO type lies about them (the VB code dispatched them by number).
NPC_FIX = {185: 102}  # second "Cirujano", typed 17 (QuestNoble)

# frmViajar + `/VIAJAR` (TCP_HandleData3.bas): name-map-x-y-price-minLevel-priceFromLv30.
TRAVEL = [
    "Tanaris-28-54-35-1000-0-5000",
    "Anvilmar-29-46-85-1000-0-5000",
    "Kahlimdor-27-50-48-1000-0-5000",
    "Thir-25-74-45-1000-0-5000",
    "Ruvendel-26-51-52-1000-0-5000",
    "Jhumbel-69-44-44-1000-0-5000",
    "Helka-136-52-55-1000-0-5000",
    "Inthak-130-50-57-5000-30-5000",
]


# `CPn=CLASS` (classes that cannot use the object). TSAO's 12 classes (General.bas
# ListaClases) end with Artesano and Recolector, the engine's work class is Trabajador;
# the 0.11.5 work classes (Pescador, Minero...) left in Obj.dat no longer exist in TSAO
# (the VB loader just never matched them), so they are dropped.
CLASS_MAP = {"ARTESANO": "TRABAJADOR", "RECOLECTOR": "TRABAJADOR"}
DEAD_CLASSES = {"PESCADOR", "MINERO", "LEÑADOR", "LENADOR", "CARPINTERO", "HERRERO", "SASTRE", "ALQUIMISTA"}


# TSAO eOBJType (Declares.bas) -> engine ObjType. 1-6, 8, 9, 11, 13, 14, 16-19, 22-24,
# 26-28, 31-34 mean the same; the rest differ (TSAO 47 "cofre al azar" is engine 47
# "mail"). Types without an engine system yet become decorations (20) and keep their
# TSAO number in `TsaoObjType` for when it is ported.
OBJ_TYPE = {7: 20, 10: 8, 15: 20, 35: 20, 36: 20, 37: 20, 38: 20, 39: 20, 40: 20, 41: 20, 42: 20, 43: 20, 45: 20, 46: 20,
            47: 48, 48: 44, 49: 20, 50: 20, 51: 20, 52: 20, 53: 20, 54: 20, 55: 20}


# Objects the VB recognised by number (now data the server reads):
# - SistemaCombate.CalcularDaño: the dragon slayer (1053) hits dragons for 220..225 (max
#   350 in the strength term) and users for 1; UserDañoNpc takes it away when the dragon
#   dies -> `DragonHit=220-225`, `DragonHitMax=350`, `UserHit=1`, `LostOnDragonKill=1`;
# - CalcularDarExp: 1051, 1052, 1455, 1496 add a quarter of the experience -> `ExpBonusPct`;
# - NPCTirarOro: 1049, 1050, 1456, 1497 add a quarter of the gold -> `GoldBonusPct`;
# - NPC_TIRAR_ITEMS: 917, 918, 1455, 1456, 1496, 1497 add 3 in 200 to each drop -> `DropLuck`;
# - General.calcularDefCasco / calcularDefAnillo: the golden tiara (1035) and the ring of
#   immunity (1540) give a fixed magic defence by class -> `MagicDefByClass=CLASS:n,...,*:n`;
# - modHechizos (HechizoEstadoUsuario / NpcLanzaSpellSobreUser): the ring resists
#   paralysis 1 time in 10 from users and 1 in 8 from creatures -> `ParalysisResist=10,8`;
# - HechizoPropUsuario: the magic lute (696) adds 4% to spells on users -> `SpellDamagePct=4`.
OBJ_KEYS = {
    1053: ["DragonHit=220-225", "DragonHitMax=350", "UserHit=1", "LostOnDragonKill=1"],
    917: ["DropLuck=3"], 918: ["DropLuck=3"],
    1049: ["GoldBonusPct=25"], 1050: ["GoldBonusPct=25"],
    1051: ["ExpBonusPct=25"], 1052: ["ExpBonusPct=25"],
    1455: ["ExpBonusPct=25", "DropLuck=3"], 1496: ["ExpBonusPct=25", "DropLuck=3"],
    1456: ["GoldBonusPct=25", "DropLuck=3"], 1497: ["GoldBonusPct=25", "DropLuck=3"],
    1035: ["MagicDefByClass=PALADIN:17,BARDO:25,CLERIGO:17,ASESINO:24,*:26"],
    1540: ["MagicDefByClass=PALADIN:9,GUERRERO:11,*:18", "ParalysisResist=10,8"],
    696: ["SpellDamagePct=4"],
}


def remap_obj_classes(m):
    head, body = m.group(1), m.group(2)
    num = int(re.match(r"\[OBJ(\d+)\]", head).group(1))
    body = "".join(k + "\n" for k in OBJ_KEYS.get(num, [])) + body
    # UsuarioAtacaUsuario: a weapon with `Inmoviliza=1` paralyses for IntervaloParalizado
    # (500 ticks of the 40 ms game timer = 20 s) `probInmov`% of its hits.
    if re.search(r"(?im)^Inmoviliza=1", body):
        body = "ImmobilizeSecs=20\n" + body
    t = re.search(r"(?im)^ObjType=(\d+)", body)
    if t and int(t.group(1)) in OBJ_TYPE:
        old = int(t.group(1))
        body = body[:t.start()] + f"ObjType={OBJ_TYPE[old]}\nTsaoObjType={old}" + body[t.end():]
    cps = []
    for v in re.findall(r"(?im)^CP\d+=([^\r\n]*)", body):
        c = v.strip().upper()
        c = CLASS_MAP.get(c, c)
        if c and c not in DEAD_CLASSES and c not in cps:
            cps.append(c)
    body = re.sub(r"(?im)^CP\d+=[^\n]*\n?", "", body)
    return head + "".join(f"CP{i + 1}={c}\n" for i, c in enumerate(cps)) + body


def set_section(text, name, lines):
    """Replaces `[name]` of an INI text (or appends it)."""
    body = "\n".join(lines) + "\n\n"
    pat = re.compile(r"(?ims)^\[" + re.escape(name) + r"\][^\n]*\n.*?(?=^\[|\Z)")
    if pat.search(text):
        return pat.sub(lambda _: f"[{name}]\n" + body, text, count=1)
    return text + f"\n[{name}]\n" + body


def convert_balance(experiencia, ddat):
    """AO20 Balance.dat keeps the class/combat modifiers the engine reads; TSAO replaces
    the experience curve (Experiencia.dat, cap 70), the race bonuses (TCP.bas
    ConnectNewUser) and the hit gained per level (CheckUserLevel AumentoHIT)."""
    path = os.path.join(ddat, "Balance.dat")
    text = open(path, encoding="latin-1").read().replace("\r\n", "\n")
    exp = {int(m.group(1)): int(m.group(2)) for m in re.finditer(r"(?im)^Nivel(\d+)=(\d+)", experiencia)}
    cap = max(exp) + 1
    text = set_section(text, "EXP", [f"{i}={exp.get(i, 0)}" for i in range(1, cap)] + [f"{cap}=0"])
    races = {"Humano": (2, 0, 0, 3, 2), "Elfo": (-1, 2, 2, 2, 0), "ElfoOscuro": (1, 1, 2, 1, 1), "Enano": (3, -1, -2, -1, 4), "Gnomo": (-4, 3, 3, 1, -1), "Orco": (0, 0, 0, 0, 0)}
    lines = []
    for r, (f, a, i, c, con) in races.items():
        lines += [f"{r}Fuerza={f}", f"{r}Agilidad={a}", f"{r}Inteligencia={i}", f"{r}Carisma={c}", f"{r}Constitucion={con}"]
    text = set_section(text, "MODRAZA", lines)
    pre = {"Guerrero": 3, "Cazador": 3, "Paladin": 3, "Bandido": 3, "Asesino": 3, "Pirata": 3, "Ladron": 3, "Clerigo": 2, "Bardo": 2, "Mago": 1, "Druida": 2, "Trabajador": 2}
    post = {"Guerrero": 2, "Cazador": 2, "Paladin": 1, "Bandido": 2, "Asesino": 1, "Pirata": 2, "Ladron": 2, "Clerigo": 2, "Bardo": 2, "Mago": 1, "Druida": 2, "Trabajador": 2}
    # The classic class modifiers come from TSAO's own Balance.dat (modBalance.bas reads
    # them; classes it lacks use its `Case Else` 0.7).
    tsao_bal = {}
    cur = None
    for line in open(TSAO_BALANCE, encoding="latin-1"):
        line = line.split("'")[0].strip()
        if line.startswith("["):
            cur = line[1:-1].lower()
        elif "=" in line and cur:
            k, v = line.split("=", 1)
            tsao_bal.setdefault(cur, {})[k.strip().lower()] = v.strip()
    ours = {"MODEVASION": "modificadorevasion", "MODATAQUEARMAS": "modificadorpoderataquearmas", "MODATAQUEPROYECTILES": "modificadorpoderataqueproyectiles",
            "MODDANOARMAS": "modicadordaño" + "clasearmas", "MODDANOPROYECTILES": "modicadordaño" + "claseproyectiles", "MODESCUDO": "modevasiondeescudoclase"}
    for sec, src in ours.items():
        vals = tsao_bal.get(src, {})
        text = set_section(text, sec, [f"{k}={vals.get(k.lower(), '0.7')}" for k in pre])
    text = set_section(text, "GOLPE_PRE_36", [f"{k}={v}" for k, v in pre.items()])
    text = set_section(text, "GOLPE_POST_36", [f"{k}={v}" for k, v in post.items()])
    open(path, "w", encoding="latin-1", newline="\r\n").write(text)


QUEST_NPC = 160  # "Quests" (TSAO type 11): offers every quest of QUESTS.DAT


def convert_quests(text, ddat):
    """TSAO QUESTS.DAT (Tipo 1 kill NPC, Tipo 2 kill users) -> engine Quests.DAT.
    Tournament/TS points and donation credits have no equivalent yet: the gold stays."""
    secs = {}
    cur = None
    for line in text.splitlines():
        line = line.split("'")[0].strip()
        if line.startswith("[") and line.endswith("]"):
            cur = secs.setdefault(line[1:-1].lower(), {})
        elif "=" in line and cur is not None:
            k, v = line.split("=", 1)
            cur[k.strip().lower()] = v.strip()
    n = int(secs.get("init", {}).get("num", "0"))
    out = ["[INIT]", f"NumQuests={n}", ""]
    for i in range(1, n + 1):
        q = secs.get(f"quest{i}", {})
        name = q.get("name", f"Misión {i}").strip("¡!¿? ")
        out += [f"[QUEST{i}]", f"Nombre={name}", "Repetible=1", f"RewardGLD={q.get('oro', '0')}"]
        if q.get("tipo") == "1":
            out += [f"Desc=¡{name}! Vuelve conmigo cuando lo hayas hecho.", "RequiredNPCs=1", f"RequiredNPC1={q.get('matanpc', '0')}-{q.get('cant', '1')}"]
        else:
            out += [f"Desc=¡{name}! Solo cuentan rivales de nivel 13 o más, fuera de las arenas.", f"RequiredUserKills={q.get('usuarios', '1')}"]
        out.append("")
    open(os.path.join(ddat, "Quests.DAT"), "w", encoding="latin-1", newline="\r\n").write("\n".join(out))
    return n


# Behaviours the VB dispatched by NPC number (now data the server reads):
# - modHechizos.HechizoInvocacion: one elemental of each kind per player (92 water, 93
#   fire, 94 earth) -> `UniqueSummon=1`;
# - AI_NPC.AiNpcAtacaNpc: the fire and earth elementals cast their spell on the creature
#   they fight instead of hitting it -> `CastsOnNpcs=1`;
# - NpcLanzaSpellSobreUser: the fire elemental's spell hits users for 70..90 -> `SpellDamage`;
# - General.bas (PasarSegundo): the water elemental heals its druid owner with spell 59
#   every 4 s while hurt and within 10 tiles -> `HealsOwner=spell,secs,range,class`;
# - modHechizos.InvocarMascota: the companions of the "Invocar Mascota" spells (follow the
#   owner, never fight) -> `Companion=1`.
NPC_KEYS = {
    92: ["UniqueSummon=1", "HealsOwner=59,4,10,DRUIDA"],
    93: ["UniqueSummon=1", "CastsOnNpcs=1", "SpellDamage=70-90"],
    94: ["UniqueSummon=1", "CastsOnNpcs=1"],
    156: ["Companion=1"], 157: ["Companion=1"], 158: ["Companion=1"], 181: ["Companion=1"], 182: ["Companion=1"],
}
# AI_NPC.bas TipoAI -> engine Movement: TSAO 3 `NpcMaloAtacaUsersBuenos` (IrUsuarioCercano:
# goes for the nearest user) and 10 `NpcPathfinding` (walks a path to its target) are the
# engine's chasing hostile wanderer (2); the engine's 3 is FixedInPos and its 10 a guard
# mode, so 179 TSAO creatures (dragons among them) used to stand still.
MOVEMENT = {3: 2, 10: 2}
# modHechizos.HechizoPropNPC: `AfectaRelampago=1` creatures ignore the spell named
# "Relampago" (HECHIZO12) -> the list of spells they ignore.
RELAMPAGO = 12


def remap_npc(m):
    head, num, body = m.group(1), int(m.group(2)), m.group(3)
    t = re.search(r"(?im)^NpcType=(\d+)", body)
    tsao = int(t.group(1)) if t else 0
    new = NPC_FIX.get(num, NPC_TYPE.get(tsao, 0))
    if t:
        body = body[:t.start()] + f"NpcType={new}" + body[t.end():]
    extra = f"TsaoType={tsao}\n" if tsao not in NPC_TYPE and num not in NPC_FIX else ""
    extra += "".join(k + "\n" for k in NPC_KEYS.get(num, []))
    mv = re.search(r"(?im)^Movement=(\d+)", body)
    hostile = re.search(r"(?im)^Hostile=1", body)
    # Only hostile ones: the royal guard (10) and the ram (3) are not, and stand.
    if mv and hostile and int(mv.group(1)) in MOVEMENT:
        body = body[:mv.start()] + f"Movement={MOVEMENT[int(mv.group(1))]}\nTsaoMovement={mv.group(1)}" + body[mv.end():]
    if re.search(r"(?im)^AfectaRelampago=1", body):
        extra += f"ImmuneSpells={RELAMPAGO}\n"
    if num == QUEST_NPC:
        extra += "NumQuest=31\n" + "".join(f"QuestNumber{i}={i}\n" for i in range(1, 32))
    if new == 17 and num != QUEST_NPC:
        body = re.sub(r"(?im)^NpcType=17", "NpcType=0", body)  # 161 "Carcel": jail warden, not quests
    if new == 26:
        extra += f"CityCount={len(TRAVEL)}\n" + "".join(f"City{i + 1}={c}\n" for i, c in enumerate(TRAVEL))
    return head + extra + body


def fix_trainers(npcs):
    """TSAO trainers (NpcType 3) name their creatures right (`CN1=Murciélago`) but number
    them with an older npcs.dat: O'Riley de Banderbill's `CI1=566` is the Gran Dragón de la
    Oscuridad (160000 HP). Each `CIn` becomes the hostile creature (>= 500) called `CNn`."""
    by_name = {}
    for m in re.finditer(r"(?ims)^\[NPC(\d+)\][^\n]*\n(.*?)(?=^\[|\Z)", npcs):
        num, name = int(m.group(1)), re.search(r"(?im)^Name=([^\n]*)", m.group(2))
        if name and num >= 500:
            by_name.setdefault(name.group(1).strip().lower(), num)

    def fix(m):
        head, body = m.group(1), m.group(2)
        if not re.search(r"(?im)^NpcType=3\s*$", body):
            return m.group(0)
        for i, cn in re.findall(r"(?im)^CN(\d+)=([^\n]*)", body):
            n = by_name.get(cn.strip().lower())
            if n:
                body = re.sub(rf"(?im)^CI{i}=\d+", f"CI{i}={n}", body)
        return head + body
    return re.sub(r"(?ims)^(\[NPC\d+\][^\n]*\n)(.*?)(?=^\[|\Z)", fix, npcs)


def convert_dats(src, dst):
    """TSAO world data (NPCs, objects, spells) over the AO20 files the engine also needs
    (balance, crafting, quests...) until each system is ported."""
    import shutil
    sdat = os.path.join(src, "Servidor", "Dat")
    ddat = os.path.join(dst, "Dat")
    if os.path.isdir(ddat):
        shutil.rmtree(ddat)
    shutil.copytree(AO20_DAT, ddat)
    def read(f):
        b = open(os.path.join(sdat, f), "rb").read()
        if b[:2] == b"\xff\xfe":  # some TSAO files were saved as UTF-16
            return b[2:].decode("utf-16-le")
        return b.decode("latin-1")
    # NPCs: friendly (< 500) and hostile (>= 500) live in two files in 0.11.5.
    npcs = read("NPCs.dat") + "\n" + re.sub(r"(?im)^\[INIT\][^\[]*", "", read("NPCs-HOSTILES.dat"))
    npcs = re.sub(r"(?ims)^(\[NPC(\d+)\][^\n]*\n)(.*?)(?=^\[|\Z)", remap_npc, npcs)
    npcs = fix_trainers(npcs)
    for f in os.listdir(ddat):
        if f.lower() in ("npcs.dat", "obj.dat", "hechizos.dat"):
            os.remove(os.path.join(ddat, f))
    open(os.path.join(ddat, "npcs.dat"), "w", encoding="latin-1", newline="\r\n").write(npcs.replace("\r\n", "\n"))
    convert_quests(read("QUESTS.DAT"), ddat)
    global TSAO_BALANCE
    TSAO_BALANCE = os.path.join(sdat, "Balance.dat")
    convert_balance(read("Experiencia.dat"), ddat)
    # TSAO's PvP layer (class percents, class versus class), read by the server as is.
    open(os.path.join(ddat, "pvp_balance.dat"), "w", encoding="latin-1", newline="\r\n").write(read("Balance.dat").replace("\r\n", "\n"))
    # AO20 files that name AO20 objects or maps: wrong things in a TSAO world. TSAO's own
    # version where it has one, else an empty set until that system is ported.
    for f in ("ArmasHerrero.dat", "ArmadurasHerrero.dat", "Ciudades.Dat", "Invokar.dat", "Tesoros.dat", "Map.dat", "Help.dat", "Motd.ini", "NombresInvalidos.txt"):
        open(os.path.join(ddat, f), "w", encoding="latin-1", newline="\r\n").write(read(f).replace("\r\n", "\n"))
    # Chests: the Random=0 ones (starter packs) swapped items by class in InvUsuario.bas.
    swaps = "Swap1=Asesino|Paladin:206-530\nSwap2=Guerrero|Cazador:206-478\nSwap3=Guerrero|Cazador:207-551-100\nSwap4=Guerrero|Cazador:1012-404\n"
    cofres = read("Cofres.dat").replace("\r\n", "\n")
    for n in (3, 4):
        cofres = re.sub(rf"(?im)^\[COFRE{n}\]\n", lambda m: m.group(0) + swaps, cofres)
    open(os.path.join(ddat, "Cofres.dat"), "w", encoding="latin-1", newline="\r\n").write(cofres)
    empty = {
        "ObjCarpintero.dat": "[INIT]\nNumObjs=0\n",
        "ObjSastre.dat": "[INIT]\nNumObjs=0\n",
        "ObjAlquimista.dat": "[INIT]\nNumObjs=0\n",
        "BlackSmithElementalRunes.dat": "[INIT]\nNumRunes=0\n",
        "RecursosEspeciales.dat": "[INIT]\nNum=0\n",
        "recompensas_faccion.dat": "[INIT]\nNum=0\n",
        "GlobalDropTable.dat": "[INIT]\nNum=0\n",
        "itemsmagicos.dat": "[INIT]\nNum=0\n",
        # TSAO fishing (Trabajo.bas DoPescar): always the same fish, obj 139.
        "pesca.dat": "[PECES]\nNumPeces=1\nMaxlvlcaña=6\nPez1=139-1000-1-0\n",
    }
    for f, text in empty.items():
        open(os.path.join(ddat, f), "w", encoding="latin-1", newline="\r\n").write(text)
    # Starting kit (TCP.bas ConnectNewUser): food, drink, a newbie dagger, potions, the
    # race's newbie clothes; casters (mage, cleric, assassin, bard, druid) know spell 2.
    kit = ["[COMMON]", "NumObjs=6", "Obj1=467,100,0", "Obj2=468,100,0", "Obj3=460,1,1", "Obj4=461,150,0", "Obj5=462,150,0", "Obj6=1491,150,0", ""]
    for race, obj in ((1, 463), (2, 464), (3, 465), (4, 466), (5, 466)):
        kit += [f"[RACE{race}]", "NumObjs=1", f"Obj1={obj},1,1", ""]
    for cls in (1, 2, 4, 5, 6):
        kit += [f"[CLASS{cls}]", "NumObjs=0", "NumSpells=1", "Spell1=2", ""]
    open(os.path.join(ddat, "EquipamientoInicial.dat"), "w", encoding="latin-1", newline="\r\n").write("\n".join(kit))
    objs = re.sub(r"(?ims)^(\[OBJ\d+\][^\n]*\n)(.*?)(?=^\[|\Z)", remap_obj_classes, read("Obj.dat"))
    open(os.path.join(ddat, "obj.dat"), "w", encoding="latin-1", newline="\r\n").write(objs.replace("\r\n", "\n"))
    open(os.path.join(ddat, "Hechizos.dat"), "w", encoding="latin-1", newline="\r\n").write(read("Hechizos.dat").replace("\r\n", "\n"))


def main(src, dst):
    cinit = os.path.join(src, "Cliente", "Data", "INIT")
    init = os.path.join(dst, "init")
    os.makedirs(init, exist_ok=True)
    grhs = read_ind_graphics(os.path.join(cinit, "Graficos.ind"))
    write_graficos(grhs, os.path.join(init, "graficos.ini"))
    bodies = read_ind_records(os.path.join(cinit, "Personajes.ind"), "<6h")
    write_sections(os.path.join(init, "cuerpos.dat"), "NumBodies", len(bodies), "BODY",
                   [(i + 1, [(f"Walk{k + 1}", b[k] & 0xFFFF) for k in range(4)] + [("HeadOffsetX", b[4]), ("HeadOffsetY", b[5])]) for i, b in enumerate(bodies) if b[0]])
    for name, out, key in (("Cabezas.ind", "cabezas.ini", "NumHeads"), ("Cascos.ind", "cascos.ini", "NumHeads")):
        heads = read_ind_records(os.path.join(cinit, name), "<4h")
        write_sections(os.path.join(init, out), key, len(heads), "HEAD",
                       [(i + 1, [(f"Head{k + 1}", h[k] & 0xFFFF) for k in range(4)]) for i, h in enumerate(heads) if h[0]])
    write_head_ranges(os.path.join(init, "head_ranges.ini"), {i + 1 for i, h in enumerate(read_ind_records(os.path.join(cinit, "Cabezas.ind"), "<4h")) if h[0]})
    # Resource-set numbers (ao_formats::profile): General.bas DarCuerpoDesnudo (elves fall
    # in its `Case Else`), Declares.bas iCuerpoMuertoN/iCabezaMuertoN, General.bas HayAgua.
    naked = {(1, 1): 21, (2, 1): 39, (1, 2): 21, (2, 2): 39, (1, 3): 32, (2, 3): 40, (1, 4): 53, (2, 4): 60, (1, 5): 53, (2, 5): 60}
    prof = ["[Bodies]"] + [f"Naked{g}-{r}={b}" for (g, r), b in sorted(naked.items())] + ["DeadBody=8", "DeadHead=500", "", "[Water]", "Ranges=1505-1520", "EmptyLayer2=1", "",
                                                                                                                            # TCP_HandleData2.bas /MEDITAR: FXMEDITARCHICO 4 (<15), MEDIANO 5 (<30), GRANDE 6 (<50), XGRANDE 43 (<60), CIUDA 44.
                                                                                                                            "[Meditate]", "FxByLevel=15:4,30:5,50:6,60:43,255:44", "",
                                                                                                                            "[Combat]", "Rules=tsao",
            # UserDañoNpc / HechizoPropNPC: RandomNumber(1, 5) hits 2 of 5 -> 40% critical x2;
            # spells on creatures x1.4; on someone's pet x1.5 (blows) and x1.3 (spells);
            # paladins +45 against creatures.
            "NpcCriticalPct=40", "NpcCriticalMult=2", "SpellVsNpcPct=140", "MeleeVsPetPct=150", "SpellVsPetPct=130", "NpcFlatBonus=PALADIN:45",
            # Server.ini IntervaloParalizado 500 ticks of TIMER_AI (100 ms) for creatures.
            "NpcParalysisSecs=50", "",
            # Declares.bas MAXMASCOTAS 3 / MAXMASCOTASENTRENADOR 7; IntervaloInvocacion 3000
            # ticks of the 40 ms game timer = 120 s; Trabajo.bas ModDomar.
            "[Pets]", "Max=3", "TrainerMax=7", "SummonSecs=120", "TameMod=DRUIDA:6,CAZADOR:6,CLERIGO:7,*:10", "",
            # 0.11.5 window 17x13: MinXBorder 9, MaxXBorder 92, MinYBorder 7, MaxYBorder 94 (map exits are there).
            "[Walk]", "Bounds=9,92,7,94", "",
            # DoPasosFx: always 23/24 (50 sailing); no meditation sound.
            "[Sounds]", "StepsSimple=1", "Meditate=0", "",
            # CheckUserLevel CFF 58; the big meditation (FXMEDITARGRANDE 6) for the creation scene.
            "[Fx]", "LevelUp=58", "Ritual=6", "Paralyze=8", "",
            # frmCrearPersonaje: 5 races, 8 classes; ConnectNewUser: 21 life, mage 100 + INT/3 (avg), casters 50.
            "[Creation]", "Races=1,2,3,4,5", "Classes=1,2,3,4,5,6,7,8", "Hp=21", "Mana1=100,3", "Mana2=50,0", "Mana4=50,0", "Mana5=50,0", "Mana6=50,0", "",
            # Login / character screens: the meadow north of Tanaris, a tour of the town, a campfire (obj 63 Fogata).
            "[Scene]", "Map=28", "Fire=40,19", "TownPath=32,48;45,48;58,50;70,50;62,62;45,62", "Folk=40,47;46,49;52,50;36,52;60,52;66,48", "BonfireGrh=1521", ""]
    # Client particle look per TSAO spell (Hechizos.dat names: dardo -> dart, descarga -> lightning...).
    spell_fx = {"cure": [1, 10, 38, 41, 43], "dart": [2, 6, 37], "heal": [3, 5, 40, 59], "wind": [4, 19], "electric_dart": [7], "fireball": [8, 34, 56],
                "paralyze": [9, 24, 29, 39], "resurrect": [11], "lightning": [12, 50], "buff": [13, 18, 20, 46, 47, 65], "invisibility": [14, 42],
                "fire_storm": [15, 16, 25, 35, 45, 57, 58], "summon": [17, 26, 27, 28, 51, 52, 53, 54, 55], "holy": [21, 22, 60, 61, 62, 63, 64],
                "electric": [23, 44], "dark": [30, 31, 32, 33, 36], "teleport": [48, 49]}
    prof += ["[SpellFx]"] + [f"{i}={n}" for i, n in sorted((i, n) for n, ids in spell_fx.items() for i in ids)] + [""]
    open(os.path.join(init, "profile.ini"), "w", newline="\r\n").write("\n".join(prof))
    fxs = read_ind_records(os.path.join(cinit, "Fxs.ind"), "<3h")
    write_sections(os.path.join(init, "FXs.ini"), "NumFxs", len(fxs), "FX",
                   [(i + 1, [("Animacion", f[0] & 0xFFFF), ("OffsetX", f[1]), ("OffsetY", f[2]), ("IsPNG", 1)]) for i, f in enumerate(fxs)])
    clean_ini(os.path.join(cinit, "Armas.dat"), os.path.join(init, "armas.dat"))
    clean_ini(os.path.join(cinit, "Escudos.dat"), os.path.join(init, "escudos.dat"))
    for empty in ("moldes.ini", "TreeGrh.dat"):
        open(os.path.join(init, empty), "w").write("")
    # Maps.
    smaps = os.path.join(src, "Servidor", "Maps")
    mdir = os.path.join(dst, "Mapas")
    os.makedirs(mdir, exist_ok=True)
    names, ok, bad = [], 0, []
    nums = sorted({int(m.group(1)) for f in os.listdir(smaps) if (m := re.match(r"(?i)mapa(\d+)\.map$", f))})
    for n in nums:
        dat = os.path.join(smaps, f"Mapa{n}.dat")
        info = ini_sections(dat).get(f"mapa{n}", {}) if os.path.exists(dat) else {}
        try:
            data, name = convert_map(n, smaps, info)
            open(os.path.join(mdir, f"mapa{n}.csm"), "wb").write(data)
            names.append(f"mapa{n}={name}")
            ok += 1
        except Exception as e:
            bad.append((n, repr(e)))
    open(os.path.join(init, "namemapa.dat"), "w", encoding="cp1252", newline="\r\n").write("[NameMapa]\n" + "\n".join(names) + "\n")
    convert_dats(src, dst)
    print(f"grh {len(grhs)}, bodies {len(bodies)}, maps ok {ok}, failed {bad[:5]}")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
