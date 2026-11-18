"""Extracts the TSAO level-up table (Modulo_UsUaRiOs.bas CheckUserLevel) into TOML.

Per class: hit gain (before/after level 35), mana per intelligence, stamina; per class and
race: the HP targets (VidaVeinte at level 20, RandomVeinteMin/Max, DefineRandomMin/Max for
21-29, RandomFinal from 30). usage: python -I levels.py <Modulo_UsUaRiOs.bas> <out.toml>
"""
import re
import sys

KEYS = ["VidaMinima", "VidaMaxima", "RandomVeinteMin", "RandomVeinteMax", "VidaVeinte", "DefineRandomMin", "DefineRandomMax", "RandomFinal"]


def main(src, out):
    text = open(src, encoding="latin-1").read()
    start = text.index("Sub CheckUserLevel")
    end = text.index("End Sub", start)
    body = text[start:end].splitlines()
    cls = race = None
    table = {}
    extra = {}
    depth_cls = None
    for line in body:
        s = line.split("'")[0].strip()
        m = re.match(r'Case "([A-Z ]+)"', s)
        if m:
            name = m.group(1)
            if name in ("HUMANO", "ELFO", "ELFO OSCURO", "GNOMO", "ENANO"):
                race = name
            else:
                cls, race = name, None
                extra.setdefault(cls, {})
            continue
        if s.startswith("Case Else"):
            race = "ELSE"
            continue
        m = re.match(r"(\w+) = (.+)$", s)
        if not m or cls is None:
            continue
        k, v = m.group(1), m.group(2)
        if k in KEYS and race and race != "ELSE":
            table.setdefault(cls, {}).setdefault(race, {})[k] = int(v)
        elif k == "AumentoHIT":
            m2 = re.match(r"IIf\(.*ELV > 35, (\d+), (\d+)\)", v)
            extra[cls]["hit"] = [int(m2.group(2)), int(m2.group(1))] if m2 else [int(v), int(v)]
        elif k == "AumentoMANA":
            m2 = re.match(r"([\d.]+) \* .*Inteligencia", v)
            extra[cls]["mana_per_int"] = float(m2.group(1)) if m2 else 1.0
        elif k == "AumentoSTA":
            extra[cls]["sta"] = {"AumentoSTDef": 15, "AumentoSTMago": 14}.get(v, 15)
    lines = [
        "# TSAO level-up table, extracted by tools/tsao/levels.py from CheckUserLevel.",
        "",
        "# Starting life and mana: init/profile.ini [Creation] (shared with the client).",
        "",
        "# CheckUserLevel: 600 * level of gold below 10, Fuego Magico at 20, out of the newbie",
        "# area at 10 (Tanaris 54,34) with a Starterpack (1561 dwarves and gnomes).",
        "[rewards]",
        "gold_below_level = 10",
        "gold_per_level = 600",
        "spells = [[20, 8]]",
        "[rewards.graduation]",
        "level = 10",
        "map = 28",
        "x = 54",
        "y = 34",
        "item = 1560",
        "item_short_races = 1561",
        "",
    ]
    for cls in table:
        e = extra.get(cls, {})
        lines += [f'[class."{cls.title()}"]', f"hit = {e.get('hit', [2, 2])}", f"mana_per_int = {e.get('mana_per_int', 0.0)}", f"sta = {e.get('sta', 15)}"]
        for race, v in table[cls].items():
            vals = ", ".join(f"{k} = {v[k]}" for k in KEYS if k in v)
            lines.append(f'race."{race.title()}" = {{ {vals} }}')
        lines.append("")
    open(out, "w", encoding="utf-8").write("\n".join(lines))
    print(len(table), "classes", {c: len(r) for c, r in table.items()})


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
