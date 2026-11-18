"""TSAO progression data for Numic AO, run after convert.py (build.cmd):

- Premios.dat (tournament point prizes), ItemsDonaciones.dat (donation credit packs) and
  Mejorados.dat (upgrades with the Octarine gem) as TSAO has them;
- CanjeTS.dat: the TS point shop, hard-coded in TCP_HandleData1.bas `FTSPTS`;
- ClassBonus.dat: the class bonuses of levels 53/56/60 (Modulo_UsUaRiOs.bas CheckUserLevel),
  each option with the effect the combat code gave its text (SistemaCombate.bas,
  modHechizos.bas, Trabajo.bas, TCP_HandleData1.bas `BOF`);
- LevelPoints.dat: tournament/TS points for reaching a level (Modulo_UsUaRiOs.bas);
- Quests.DAT: the tournament points, TS points and credits of QUESTS.DAT (modQuests.bas);
- Consultas.dat: the poll that is open when the server starts (consultas.dat).

usage: python -I progress.py <tsao checkout> <resources>
"""
import os
import re
import sys


def read(path):
    b = open(path, "rb").read()
    if b[:2] == b"\xff\xfe":
        return b[2:].decode("utf-16-le")
    return b.decode("latin-1")


def write(path, text):
    open(path, "w", encoding="latin-1", newline="\r\n").write(text.replace("\r\n", "\n"))


def sections(text):
    secs, cur = {}, None
    for line in text.splitlines():
        line = line.split("'")[0].strip()
        if line.startswith("[") and line.endswith("]"):
            cur = secs.setdefault(line[1:-1].upper(), {})
        elif "=" in line and cur is not None:
            k, v = line.split("=", 1)
            cur[k.strip().lower()] = v.strip()
    return secs


# TCP_HandleData1.bas FTSPTS: (object, amount, TS points). Cases 5 and 6 both gave object
# 1050 for 40: the duplicate is dropped.
TS_SHOP = [(1055, 1, 10), (1033, 1, 15), (915, 1, 25), (1227, 1, 35), (1215, 1, 30), (1050, 1, 40),
           (1539, 2, 5), (1035, 1, 30), (1059, 1, 65), (1060, 1, 70), (1535, 1, 20)]

# Effect of each ClassBonus.dat text, in tenths of a percent except Hp (points) and Stab
# (points off the stab roll range, Trabajo.bas: 95 -> 93 one pick, 90 both).
# TSAO added "+0.04" to PoderEvasion / PoderAtaque (integers in the hundreds): the evasion,
# shield and hit bonuses did nothing. Here they are the 4% they were meant to be.
# "Aumenta tu daño con flechas." had no code at all: it is 4% arrow damage here.
EFFECTS = [
    (r"evasion", "Evasion", 40),
    (r"pegar con armas", "WeaponHit", 40),
    (r"pegar con flechas", "ArrowHit", 40),
    (r"daño con flechas", "ArrowDamage", 40),
    (r"daño con armas", "WeaponDamage", 40),
    (r"bloquear con escudos", "Shield", 40),
    (r"resistencia magica", "MagicResist", 15),
    (r"daño magico", "MagicDamage", 15),
    (r"apuñalar", "Stab", 2),
]


def effect(text):
    m = re.search(r"en (\d+) puntos (tu|la) vida", text)
    if m:
        return "Hp", int(m.group(1))
    for pat, name, amount in EFFECTS:
        if pat in text.lower():
            return name, amount
    raise SystemExit(f"ClassBonus.dat: texto sin efecto conocido: {text!r}")


def main(src, dst):
    sdat = os.path.join(src, "Servidor", "Dat")
    ddat = os.path.join(dst, "Dat")
    for f in ("Premios.dat", "ItemsDonaciones.dat", "Mejorados.dat"):
        write(os.path.join(ddat, f), read(os.path.join(sdat, f)))

    out = ["[INIT]", f"Num={len(TS_SHOP)}", ""]
    for i, (obj, amount, price) in enumerate(TS_SHOP, 1):
        out += [f"[ITEM{i}]", f"Obj={obj}", f"Amount={amount}", f"Price={price}", ""]
    write(os.path.join(ddat, "CanjeTS.dat"), "\n".join(out))

    # Class bonuses: TSAO offered them to these classes only, at these levels.
    bonus = sections(read(os.path.join(sdat, "ClassBonus.dat")))
    out = ["[INIT]", "Levels=53,56,60", ""]
    for cls, keys in bonus.items():
        out.append(f"[{cls}]")
        for tier in (1, 2, 3):
            for opt in (1, 2):
                text = keys[f"nivel{tier}opcion{opt}"]
                name, amount = effect(text)
                out.append(f"Tier{tier}Option{opt}={name}:{amount}:{text}")
        out.append("")
    write(os.path.join(ddat, "ClassBonus.dat"), "\n".join(out))

    # Modulo_UsUaRiOs.bas CheckUserLevel: 50 -> 50 tournament points, 60 -> 200, 65 -> 2 TS,
    # 70 -> 200 tournament and 5 TS (once per character: flags llegolvl50 / Llegolvlmax).
    write(os.path.join(ddat, "LevelPoints.dat"), "[INIT]\nNum=4\n\n[LEVEL1]\nLevel=50\nTournament=50\n\n[LEVEL2]\nLevel=60\nTournament=200\n\n[LEVEL3]\nLevel=65\nTs=2\n\n[LEVEL4]\nLevel=70\nTournament=200\nTs=5\n")

    quests = sections(read(os.path.join(sdat, "QUESTS.DAT")))
    qpath = os.path.join(ddat, "Quests.DAT")
    # Idempotent: drop what an earlier run added.
    text = re.sub(r"(?im)^Reward(Torneo|TS|Creditos)=.*\n", "", read(qpath).replace("\r\n", "\n"))

    def add(m):
        q = quests.get(f"QUEST{m.group(1)}", {})
        extra = "".join(f"{k}={q.get(v, '0')}\n" for k, v in (("RewardTorneo", "ptstorneo"), ("RewardTS", "ptsts"), ("RewardCreditos", "creditos")))
        return m.group(0) + extra
    text, n = re.subn(r"(?im)^\[QUEST(\d+)\]\n", add, text)
    write(qpath, text)

    polls = sections(read(os.path.join(sdat, "consultas.dat")))
    init = polls.get("INIT", {})
    num = init.get("consultaactual", "0").strip()
    opts = polls.get(f"ENCUESTA{num}", {})
    count = int(opts.get("cantopciones", "0") or 0)
    out = ["[INIT]", f"Question={init.get('consultaactualtexto', '').strip()}", f"Level={init.get('nivelrequerido', '1').strip()}", f"Options={count}"]
    out += [f"Option{i}={opts.get(f'opcion{i}', '').strip()}" for i in range(1, count + 1)]
    write(os.path.join(ddat, "Consultas.dat"), "\n".join(out) + "\n")
    print(f"progress: {len(TS_SHOP)} TS items, {len(bonus)} class bonus sets, {n} quests, poll with {count} options")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
