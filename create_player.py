'''
File containing code to build a player character with gear and aggregate stats.

Author: Kastra (Asura server)
'''
from typing import Any, NamedTuple

from enemies import *
from wsdist_types import Buffs, EnemyStats, Gearset, Stats


class ArmorSetBonus(NamedTuple):
    """One gear-set bonus: which equipped pieces count and what they grant."""
    name_substring: str          # Counted when this appears in the piece's "Name".
    requires_plus1: bool         # Only "+1" versions of the piece count.
    slots: tuple[str, ...] | None  # Slots that can contribute; None means any slot.
    max_pieces: int              # Cap on the number of counted pieces.
    counts_first_piece: bool     # True: bonus = pieces*per_piece; False: (pieces-1)*per_piece.
    per_piece: float
    stats: tuple[str, ...]       # Stats receiving the bonus.


ARMOR_SET_SLOTS = ("head", "body", "hands", "legs", "feet", "ring1", "ring2")

ARMOR_SET_BONUSES = (
    ArmorSetBonus("Mummu", False, ARMOR_SET_SLOTS, 5, False, 8, ("DEX", "AGI", "VIT", "CHR")),
    ArmorSetBonus("Flamma", False, ARMOR_SET_SLOTS, 5, False, 8, ("STR", "DEX", "VIT")),
    ArmorSetBonus("Mallquis", False, ARMOR_SET_SLOTS, 5, False, 8, ("VIT", "INT", "MND")),
    ArmorSetBonus("Ayanmo", False, ARMOR_SET_SLOTS, 5, False, 8, ("STR", "VIT", "MND")),
    ArmorSetBonus("Adhemar", True, None, 16, True, 2, ("Crit Rate",)),
    ArmorSetBonus("Amalric", True, None, 16, True, 10, ("Magic Attack",)),
    ArmorSetBonus("Lustratio", True, None, 16, True, 2, ("Weapon Skill Damage",)),
    ArmorSetBonus("Ryuo", True, None, 16, True, 10, ("Attack",)),
)

# Artifact armor name prefix per job, used for the Regal Ring/Earring AF set bonus.
AF_ARMOR_PREFIX = {"war":"pummeler","mnk":"anchorite","whm":"theophany","blm":"spaekona","rdm":"atrophy","thf":"pillager","pld":"reverence","drk":"ignominy","bst":"totomic","brd":"brioso","rng":"orion","sam":"wakido","nin":"hachiya","drg":"vishap","smn":"convoker","blu":"assimilator","cor":"laksamana","pup":"foire","dnc":"maxixi","sch":"academic","geo":"geomancy","run":"runeist"}

# Relic weapon aftermath stats: weapon name -> (stat, value) pairs.
RELIC_MAIN_AFTERMATH: dict[str, tuple[tuple[str, float], ...]] = {
    "Mandau": (("Crit Rate", 5), ("Crit Damage", 5)),
    "Ragnarok": (("Crit Rate", 10), ("Accuracy", 15)),
    "Guttler": (("Attack%", 100./1024),),
    "Bravura": (("DT", -20),),
    "Apocalypse": (("JA Haste", 10), ("Accuracy", 15)),
    "Gungnir": (("Attack%", 50./1024), ("DA", 5)),
    "Kikoku": (("Attack%", 100./1024), ("Subtle Blow", 10)),
    "Amanomurakumo": (("Zanshin", 10), ("Store TP", 10)),
    "Mjollnir": (("Accuracy", 20), ("Magic Accuracy", 20)),
    "Claustrum": (("DT", -20),),
    "Spharai": (("Kick Attacks", 15), ("Subtle Blow", 10)),
}
RELIC_RANGED_AFTERMATH: dict[str, tuple[tuple[str, float], ...]] = {
    "Yoichinoyumi": (("Ranged Accuracy", 30),),
    "Annihilator": (("Ranged Attack%", 100./1024),),
}

# Mythic aftermath stats, assuming 85% potency for Lv1 and Lv2: weapon name ->
# ((Lv1 stat, value), (Lv2 stat, value)). Lv3 instead grants OA2/OA3 (melee only).
_MYTHIC_AM_SCALING = 0.85
_MYTH99 = (99-40)*_MYTHIC_AM_SCALING + 40
_MYTH49 = (49-30)*_MYTHIC_AM_SCALING + 30
_MYTHIC_MELEE = (("Accuracy", _MYTH49), ("Attack", _MYTH99))
_MYTHIC_RANGED = (("Ranged Accuracy", _MYTH49), ("Ranged Attack", _MYTH99))
_MYTHIC_MACC_ACC = (("Magic Accuracy", _MYTH49), ("Accuracy", _MYTH49))
_MYTHIC_MACC_MATK = (("Magic Accuracy", _MYTH49), ("Magic Attack", _MYTH49))

MYTHIC_AFTERMATH: dict[str, tuple[tuple[str, float], tuple[str, float]]] = {
    **{name: _MYTHIC_MELEE for name in ("Conqueror", "Glanzfaust", "Vajra", "Burtgang", "Liberator", "Aymur", "Kogarasumaru", "Nagi", "Nirvana", "Ryunohige", "Kenkonken", "Terpsichore", "Epeolatry")},
    **{name: _MYTHIC_RANGED for name in ("Gastraphetes", "Death Penalty")},
    "Tizona": (("Accuracy", _MYTH49), ("Magic Accuracy", _MYTH49)),
    **{name: _MYTHIC_MACC_ACC for name in ("Carnwenhan", "Yagrush")},
    **{name: _MYTHIC_MACC_MATK for name in ("Laevateinn", "Tupsimati", "Idris", "Murgleis")},
}

# Prime weapon aftermath: per-stat (AM1, AM2, AM3) value tiers for Stage4 ("IV")
# and Stage5 ("V") weapons, and the stats each weapon's aftermath grants.
# PDL tiers: https://www.ffxiah.com/forum/topic/45830/killer-instinct-the-beastmaster-compendium/172/#3683327
# Magic Damage / Magic Attack tiers need testing.
PRIME_AFTERMATH_POTENCY = 0.6
PRIME_AFTERMATH_TIERS: dict[str, dict[str, tuple[float, float, float]]] = {
    "PDL": {"IV": (4, 7, 10), "V": (6, 9, 12)},
    "Magic Damage": {"IV": (20, 20, 20), "V": (30, 30, 30)},
    "Magic Attack": {"IV": (20, 20, 20), "V": (30, 30, 30)},
}
PRIME_AFTERMATH_STATS: dict[str, tuple[str, ...]] = {
    **{name: ("PDL",) for name in ("Caliburnus", "Dokoku", "Earp", "Foenaria", "Gae Buide", "Helheim", "Kusanagi-no-Tsurugi", "Laphria", "Mpu Gandring", "Pinaka", "Spalirisos", "Varga Purnikawa")},
    "Lorg Mor": ("Magic Damage",),
    "Opashoro": ("Magic Damage", "Magic Attack"),
}

JOB_BASE_PARAMETERS: dict[str, dict[str, int]] = {
    "war":{"STR":97, "DEX":93, "VIT":90, "AGI":93, "INT":84, "MND":84, "CHR":87,},
    "mnk":{"STR":93, "DEX":95, "VIT":97, "AGI":84, "INT":81, "MND":90, "CHR":87,},
    "whm":{"STR":90, "DEX":84, "VIT":90, "AGI":87, "INT":87, "MND":97, "CHR":93,},
    "blm":{"STR":84, "DEX":93, "VIT":84, "AGI":93, "INT":97, "MND":87, "CHR":90,},
    "rdm":{"STR":90, "DEX":90, "VIT":87, "AGI":87, "INT":93, "MND":93, "CHR":90,},
    "thf":{"STR":90, "DEX":97, "VIT":90, "AGI":95, "INT":93, "MND":81, "CHR":81,},
    "pld":{"STR":95, "DEX":87, "VIT":97, "AGI":81, "INT":81, "MND":93, "CHR":93,},
    "drk":{"STR":97, "DEX":93, "VIT":93, "AGI":90, "INT":93, "MND":81, "CHR":81,},
    "bst":{"STR":90, "DEX":97, "VIT":90, "AGI":95, "INT":93, "MND":81, "CHR":81,}, # TODO: Do these match THF by coincidence or are these placeholders?
    "brd":{"STR":90, "DEX":90, "VIT":90, "AGI":84, "INT":90, "MND":90, "CHR":95,},
    "rng":{"STR":87, "DEX":90, "VIT":90, "AGI":97, "INT":87, "MND":90, "CHR":87,},
    "smn":{"STR":84, "DEX":87, "VIT":84, "AGI":90, "INT":95, "MND":95, "CHR":95,},
    "sam":{"STR":93, "DEX":93, "VIT":93, "AGI":90, "INT":87, "MND":87, "CHR":90,},
    "nin":{"STR":93, "DEX":95, "VIT":93, "AGI":95, "INT":90, "MND":81, "CHR":84,},
    "drg":{"STR":95, "DEX":90, "VIT":93, "AGI":90, "INT":84, "MND":87, "CHR":93,},
    "blu":{"STR":87, "DEX":87, "VIT":87, "AGI":87, "INT":87, "MND":87, "CHR":87,},
    "cor":{"STR":87, "DEX":93, "VIT":87, "AGI":95, "INT":93, "MND":87, "CHR":87,},
    "pup":{"STR":87, "DEX":95, "VIT":90, "AGI":93, "INT":87, "MND":84, "CHR":93,},
    "dnc":{"STR":90, "DEX":93, "VIT":87, "AGI":95, "INT":84, "MND":84, "CHR":95,},
    "sch":{"STR":84, "DEX":90, "VIT":87, "AGI":90, "INT":95, "MND":90, "CHR":93,},
    "geo":{"STR":84, "DEX":90, "VIT":90, "AGI":87, "INT":95, "MND":95, "CHR":87,},
    "run":{"STR":93, "DEX":90, "VIT":87, "AGI":95, "INT":90, "MND":90, "CHR":84,},
}


SUBJOB_BASE_PARAMETERS: dict[str, dict[str, int]] = {
    "war":{"STR":15, "DEX":12, "VIT":10, "AGI":12, "INT":7, "MND":7, "CHR":9,},
    "mnk":{"STR":12, "DEX":13, "VIT":15, "AGI":7, "INT":6, "MND":10, "CHR":9,},
    "whm":{"STR":10, "DEX":7, "VIT":10, "AGI":9, "INT":9, "MND":15, "CHR":12,},
    "blm":{"STR":7, "DEX":12, "VIT":7, "AGI":12, "INT":15, "MND":9, "CHR":10,},
    "rdm":{"STR":10, "DEX":10, "VIT":9, "AGI":9, "INT":12, "MND":12, "CHR":10,},
    "thf":{"STR":10, "DEX":15, "VIT":10, "AGI":13, "INT":12, "MND":6, "CHR":6,},
    "pld":{"STR":13, "DEX":9, "VIT":15, "AGI":6, "INT":6, "MND":12, "CHR":12,},
    "drk":{"STR":15, "DEX":12, "VIT":12, "AGI":10, "INT":12, "MND":6, "CHR":6,},
    "bst":{"STR":9, "DEX":9, "VIT":9, "AGI":9, "INT":9, "MND":9, "CHR":9,}, # Unknown parameter bonuses from BST, copying BLU which is balanced
    "brd":{"STR":9, "DEX":9, "VIT":9, "AGI":9, "INT":9, "MND":9, "CHR":9,}, # Unknown parameter bonuses from BRD, copying BLU which is balanced
    "rng":{"STR":9, "DEX":10, "VIT":10, "AGI":15, "INT":9, "MND":10, "CHR":9,},
    "smn":{"STR":7, "DEX":9, "VIT":7, "AGI":10, "INT":13, "MND":13, "CHR":13,},
    "sam":{"STR":12, "DEX":12, "VIT":12, "AGI":10, "INT":9, "MND":9, "CHR":10,},
    "nin":{"STR":12, "DEX":13, "VIT":12, "AGI":13, "INT":10, "MND":6, "CHR":7,},
    "drg":{"STR":13, "DEX":10, "VIT":12, "AGI":10, "INT":7, "MND":9, "CHR":12,},
    "blu":{"STR":9, "DEX":9, "VIT":9, "AGI":9, "INT":9, "MND":9, "CHR":9,},
    "cor":{"STR":9, "DEX":12, "VIT":9, "AGI":13, "INT":12, "MND":9, "CHR":9,},
    "pup":{"STR":9, "DEX":13, "VIT":10, "AGI":12, "INT":9, "MND":7, "CHR":12,},
    "dnc":{"STR":10, "DEX":12, "VIT":9, "AGI":13, "INT":7, "MND":7, "CHR":13,},
    "sch":{"STR":7, "DEX":9, "VIT":8, "AGI":9, "INT":13, "MND":9, "CHR":11,}, # TODO: These are apparently ML0 parameters (Lv49). Needs updated to ML20 (Lv53)
    "geo":{"STR":7, "DEX":10, "VIT":10, "AGI":8, "INT":13, "MND":13, "CHR":9,},
    "run":{"STR":12, "DEX":10, "VIT":9, "AGI":13, "INT":10, "MND":10, "CHR":7,},
    "none":{"STR":0, "DEX":0, "VIT":0, "AGI":0, "INT":0, "MND":0, "CHR":0,},
}


JOB_TRAITS: dict[str, dict[str, list[list[int]]]] = {"Accuracy":{"rng":[[96,73],[86,60],[70,48],[50,35],[30,22],[10,10]],"drg":[[76,35],[60,22],[30,10]],"dnc":[[76,35],[60,22],[30,10]],"run":[[90,35],[70,22],[50,10]]},
          "Ranged Accuracy":{"rng":[[96,73],[86,60],[70,48],[50,35],[30,22],[10,10]],"drg":[[76,35],[60,22],[30,10]],"dnc":[[76,35],[60,22],[30,10]],"run":[[90,35],[70,22],[50,10]]},
          "Attack":{"drk":[[99,96],[91,84],[83,72],[76,60],[70,48],[50,35],[30,22],[10,10]],"war":[[91,35],[65,22],[30,10]],"drg":[[91,22],[10,10]]},
          "Ranged Attack":{"drk":[[99,96],[91,84],[83,72],[76,60],[70,48],[50,35],[30,22],[10,10]],"war":[[91,35],[65,22],[30,10]],"drg":[[91,22],[10,10]]},
          "Barrage":{"rng":[[90,6],[70,5],[50,4],[30,3]]}, # Treat Barrage as a trait so we can adjust the number of extra shots based on level. 
          "Conserve TP":{"drg":[[97,26],[84,24],[71,21],[58,18],[45,15]],"dnc":[[97,21],[87,17],[77,15]],"rng":[[91,18],[80,15]]},
          "Crit Damage":{"thf":[[97,14],[91,11],[84,8],[78,5]],"dnc":[[99,11],[88,8],[80,5]],"drk":[[95,8],[85,5]], "war":[[95,8],[85,5]]},
          "Daken":{"nin":[[95,40],[70,35],[55,30],[40,25],[25,20]]},
          "PDL Trait":{"drk":[[80,50],[70,40],[55,30],[40,20],[20,10]],"mnk":[[90,30],[60,20],[30,10]],"rng":[[90,30],[60,20],[30,10]],"drg":[[90,30],[60,20],[30,10]],"war":[[80,20],[40,10]],"sam":[[80,20],[40,10]],"bst":[[90,20],[45,10]],"pup":[[90,20],[45,10]],"dnc":[[90,20],[45,10]],"thf":[[50,10]],"nin":[[50,10]],"rdm":[[60,10]]},
          "Ranged Crit Damage":{"rng":[[99,45],[90,40],[80,35],[70,30],[60,20],[50,10]]}, # Does not apply to weapon skills
          "DA":{"war":[[99,18],[85,16],[75,14],[50,12],[25,10]]},
          "Dual Wield":{"nin":[[85,35],[65,30],[45,25],[25,15],[10,10]],"dnc":[[80,30],[60,25],[40,15],[20,10]],"thf":[[98,25],[90,15],[83,10]]},
          "Evasion":{"thf":[[88,72],[76,60],[70,48],[50,35],[30,22],[10,10]],"dnc":[[86,48],[75,35],[45,22],[15,10]],"pup":[[76,48],[60,35],[40,22],[20,10]]},
          "Fencer":{"war":[[97,5],[84,4],[71,3],[58,2],[45,1]],"bst":[[94,3],[87,2],[80,1]],"brd":[[95,2],[85,1]]}, # Listed as tiers for now.
          "Kick Attacks":{"mnk":[[76,14],[71,12],[51,10]]},
          "Magic Burst Damage Trait":{"blm":[[97,13],[84,11],[71,9],[58,7],[45,5]],"sch":[[99,7],[89,7],[79,5]],"nin":[[90,7],[80,5]],"rdm":[[95,7],[85,5]]},
          "Magic Attack":{"blm":[[91,40],[81,36],[70,32],[50,28],[30,24],[10,20]],"rdm":[[86,28],[40,24],[20,20]]},
          "Magic Defense":{"run":[[99,22],[91,20],[76,18],[70,16],[50,14],[30,12],[10,10]],"whm":[[91,20],[81,18],[70,16],[50,14],[30,12],[10,10]],"rdm":[[96,14],[45,12],[25,10]]},
          "Martial Arts":{"mnk":[[82,200],[75,180],[61,160],[46,140],[31,120],[16,100],[1,80]],"pup":[[97,160],[87,140],[75,120],[50,100],[25,80]]},
          "Occult Acumen":{"drk":[[97,125],[84,100],[71,75],[58,50],[45,25]],"sch":[[98,75],[88,50],[78,25]],"blm":[[95,50],[85,25]]}, # Units: (TP / MP)
          "Recycle":{"rng":[[50,30],[35,20],[20,10]],"cor":[[90,30],[65,20],[35,10]]},
          "Skillchain Bonus":{"dnc":[[97,23],[84,20],[71,16],[58,12],[45,8]],"sam":[[98,16],[88,12],[78,8]],"nin":[[95,12],[85,8]],"mnk":[[95,12],[85,8]]},
          "Smite":{"drk":[[95,5],[75,4],[55,3],[35,2],[15,1],],"war":[[95,3],[65,2],[35,1],],"mnk":[[80,2],[40,1],],"drg":[[80,2],[40,1],],"pup":[[40,1],]}, # Listed as tiers for now.
          "Store TP":{"sam":[[90,30],[70,25],[50,20],[30,15],[10,10]]},
          "Subtle Blow":{"mnk":[[91,25],[65,20],[40,15],[25,10],[5,5]],"nin":[[75,25],[60,20],[45,15],[30,10],[15,5]],"dnc":[[86,20],[65,15],[45,10],[25,5]]},
          "TA":{"thf":[[95,6],[55,5]]},
          "True Shot":{"rng":[[98,7],[88,5],[78,3]],"cor":[[95,5],[85,3]]},
          "Weapon Skill Damage Trait":{"drg":[[95,21],[85,19],[75,17],[65,13],[55,10],[45,7]]},
          "Zanshin":{"sam":[[95,50],[75,45],[50,35],[35,25],[20,15]]},
}


JOB_COMBAT_SKILLS: dict[str, dict[str, int]] = {
                "war":{"Hand-to-Hand Skill":334,"Dagger Skill":388,"Sword Skill":398,"Great Sword Skill":404,"Axe Skill":417,"Great Axe Skill":424,"Scythe Skill":404,"Polearm Skill":388,"Club Skill":388,"Staff Skill":398,"Archery Skill":334,"Marksmanship Skill":334,"Throwing Skill":334,"Evasion Skill":373,},
                "mnk":{"Hand-to-Hand Skill":424,"Staff Skill":398,"Club Skill":378,"Throwing Skill":300,"Evasion Skill":404,},
                "whm":{"Club Skill":404,"Staff Skill":378,"Throwing Skill":300,"Evasion Skill":300,"Divine Magic Skill":417},
                "blm":{"Dagger Skill":334,"Scythe Skill":300,"Club Skill":378,"Staff Skill":388,"Throwing Skill":334,"Evasion Skill":300,"Elemental Magic Skill":424,"Dark Magic Skill":417,},
                "rdm":{"Dagger Skill":398,"Sword Skill":398,"Club Skill":334,"Archery Skill":334,"Throwing Skill":265,"Evasion Skill":334,"Divine Magic Skill":300,"Elemental Magic Skill":378,"Dark Magic Skill":300,},
                "thf":{"Hand-to-Hand Skill":300,"Dagger Skill":424,"Sword Skill":334,"Club Skill":300,"Archery Skill":368,"Marksmanship Skill":378,"Throwing Skill":334,"Evasion Skill":424,},
                "pld":{"Sword Skill":424,"Club Skill":417,"Staff Skill":417,"Great Sword Skill":398,"Dagger Skill":368,"Polearm Skill":300,"Evasion Skill":373,"Divine Magic Skill":404,},
                "drk":{"Dagger Skill":373,"Sword Skill":388,"Great Sword Skill":417,"Axe Skill":388,"Great Axe Skill":388,"Scythe Skill":424,"Club Skill":368,"Marksmanship Skill":300,"Evasion Skill":373,"Elemental Magic Skill":404,"Dark Magic Skill":417},
                "bst":{"Dagger Skill":378,"Sword Skill":300,"Axe Skill":424,"Scythe Skill":368,"Club Skill":334,"Evasion Skill":373,},
                "brd":{"Dagger Skill":388,"Sword Skill":368,"Club Skill":334,"Staff Skill":378,"Throwing Skill":300,"Evasion Skill":334,},
                "rng":{"Dagger Skill":388,"Sword Skill":334,"Axe Skill":388,"Club Skill":300,"Archery Skill":424,"Marksmanship Skill":424,"Throwing Skill":368,"Evasion Skill":300,},
                "smn":{"Dagger Skill":300,"Club Skill":378,"Staff Skill":398,"Evasion Skill":300,"Summoning Magic Skill":417,},
                "sam":{"Dagger Skill":300,"Sword Skill":378,"Polearm Skill":388,"Great Katana Skill":424,"Club Skill":300,"Archery Skill":378,"Throwing Skill":378,"Evasion Skill":404,},
                "nin":{"Hand-to-Hand Skill":300,"Dagger Skill":378,"Sword Skill":373,"Katana Skill":424,"Great Katana Skill":368,"Club Skill":300,"Archery Skill":300,"Marksmanship Skill":373,"Throwing Skill":424,"Evasion Skill":417,"Ninjutsu Skill":417,},
                "drg":{"Dagger Skill":300,"Sword Skill":368,"Polearm Skill":424,"Club Skill":300,"Staff Skill":388,"Evasion Skill":388,},
                "blu":{"Sword Skill":424,"Club Skill":388,"Evasion Skill":368,"Blue Magci Skill":424,},
                "cor":{"Dagger Skill":404,"Sword Skill":388,"Marksmanship Skill":398,"Throwing Skill":378,"Evasion Skill":334,},
                "pup":{"Hand-to-Hand Skill":424,"Dagger Skill":368,"Club Skill":334,"Throwing Skill":378,"Evasion Skill":398,},
                "dnc":{"Dagger Skill":424,"Hand-to-Hand Skill":334,"Sword Skill":334,"Throwing Skill":378,"Evasion Skill":404,},
                "sch":{"Dagger Skill":334,"Club Skill":378,"Staff Skill":378,"Throwing Skill":334,"Evasion Skill":300,"Divine Magic Skill":404,"Elemental Magic Skill":404,"Dark Magic Skill":404,},
                "geo":{"Club Skill":404,"Staff Skill":378,"Dagger Skill":368,"Evasion Skill":334,"Elemental Magic Skill":404,"Dark Skill":373,},
                "run":{"Great Sword Skill":424,"Sword Skill":417,"Great Axe Skill":398,"Axe Skill":388,"Club Skill":368,"Evasion Skill":404,"Divine Magic Skill":398,},
}


JOB_MASTERY_STATS: dict[str, dict[str, int]] = {
                    "war":{"Accuracy":26, "Ranged Accuracy":26, "Attack":70, "Ranged Attack":70, "Magic Accuracy":36,"Fencer TP Bonus":230,"Crit Rate":10,"Crit Damage":10,"DA":10,"Evasion":36,"Magic Evasion":36,"Weapon Skill Damage":3},
                    "mnk":{"Accuracy":41, "Ranged Accuracy":41, "Attack":40, "Ranged Attack":40, "Magic Accuracy":36,"Evasion":42,"Magic Evasion":36,"Subtle Blow":10,"Martial Arts":10,"Kick Attacks Attack":40,"Kick Attacks Accuracy":20},
                    "whm":{"Accuracy":14, "Ranged Accuracy":14, "Magic Accuracy":20+50, "Magic Attack":22,"Magic Defense":50,"Divine Magic Skill":36,},
                    "blm":{"Magic Burst Damage Trait":20+23, "Magic Accuracy":20, "Magic Damage":20+23, "Magic Defense":14, "Magic Attack":50, "Magic Evasion":42, "Magic Accuracy":32, "Elemental Magic Skill":36, "Dark Magic Skill":36,},
                    "rdm":{"Magic Attack":20+28,"Magic Accuracy":20+70,"Magic Defense":28,"Magic Evasion":56,"Accuracy":22,"Ranged Accuracy":22,"EnSpell Damage":23}, # Composure Accuracy+20 is added later
                    "thf":{"Sneak Attack Bonus":20,"Trick Attack Bonus":20,"Attack":50,"Ranged Attack":50,"Evasion":70,"Accuracy":36,"Ranged Accuracy":36,"Magic Evasion":36,"Magic Accuracy":36,"TA":8,"Crit Damage":8,"Dual Wield":5,"TA Attack":20},
                    "pld":{"Accuracy":28,"Ranged Accuracy":28,"Attack":28,"Ranged Attack":28,"Evasion":22,"Magic Evasion":42,"Divine Magic Skill":36,"Magic Accuracy":42},
                    "drk":{"Attack":106,"Ranged Attack":106,"Evasion":22,"Magic Evasion":36,"Accuracy":22,"Ranged Accuracy":22,"Magic Accuracy":42,"Dark Magic Skill":36,"Crit Damage":8,"Weapon Skill Damage":8,},
                    "bst":{"Attack":70,"Ranged Attack":70,"Accuracy":36,"Ranged Accuracy":36,"Magic Evasion":36,"Magic Accuracy":36,"Fencer TP Bonus":230,"Evasion":36,},
                    "brd":{"Evasion":22,"Accuracy":21,"Ranged Accuracy":21,"Magic Defense":15,"Magic Evasion":36,"Magic Accuracy":36,},
                    "rng":{"Double Shot":20,"Attack":70,"Ranged Attack":70,"Evasion":14,"Accuracy":70,"Ranged Accuracy":70,"Magic Evasion":36,"Converve TP":15,"True Shot":8,"Ranged Crit Damage":8,"Barrage":1,"Barrage Ranged Attack":60,},
                    "smn":{"Magic Defense":22,"Magic Evasion":22,"Evasion":22,"Summoning Magic Skill":36,},
                    "sam":{"Attack":70,"Ranged Attack":70,"Evasion":36,"Accuracy":36,"Ranged Accuracy":36,"Magic Evasion":36,"Zanshin":10,"Zanshin Attack":40,"Store TP":8,"Skillchain Bonus":8,}, 
                    "nin":{"Ninjutsu Magic Damage":40,"Ninjutsu Magic Accuracy":20, "Attack":70,"Ranged Attack":70,"Evasion":64,"Accuracy":56,"Ranged Accuracy":56,"Magic Attack":28,"Magic Evasion":50,"Magic Accuracy":50,"Ninjutsu Skill":36,"Daken":14,"Weapon Skill Damage":5},
                    "drg":{"Attack":70,"Ranged Attack":70,"Evasion":36,"Accuracy":64,"Ranged Accuracy":64,"Magic Evasion":36,"Magic Evasion":36,"Crit Damage":8,},
                    "blu":{"Attack":70,"Ranged Attack":70,"Evasion":36,"Accuracy":36,"Ranged Accuracy":36,"Magic Defense":36,"Magic Attack":36,"Magic Evasion":36,"Blue Magic Skill":36,},
                    "cor":{"Triple Shot":20,"True Shot":6,"Ranged Accuracy":20+36,"Attack":36,"Ranged Attack":36,"Accuracy":36,"Evasion":22,"Magic Attack":14,"Magic Evasion":36,"Magic Accuracy":36,"Quick Draw Damage":40}, # TODO: What is "Increases Damage from Sweet Spot"? Is this +20% or +20 weapon+ammo DMG?
                    "pup":{"Martial Arts":40,"Attack":42,"Ranged Attack":42,"Evasion":56,"Accuracy":50,"Ranged Accuracy":50,"Magic Evasion":36,"Magic Accuracy":36,},
                    "dnc":{"Flourish CHR%":20,"Building Flourish WSD":20,"Attack":42,"Ranged Attack":42,"Evasion":64,"Accuracy":64,"Ranged Accuracy":64,"Magic Evasion":36,"Magic Accuracy":36,"Subtle Blow":13,"Crit Damage":8,"Skillchain Bonus":8,"Dual Wield":5},
                    "sch":{"Magic Defense":22,"Magic Attack":36,"Magic Evasion":42,"Magic Accuracy":42,"Dark Magic Skill":36,"Elemental Magic Skill":36,"Magic Burst Damage Trait":13},
                    "geo":{"Magic Accuracy":20,"Magic Attack":20+42,"Magic Defense":28,"Magic Evasion":50,"Magic Accuracy":50,"Elemental Magic Skill":36,"Dark Magic Skill":36,"Magic Damage":13},
                    "run":{"Lunge Bonus":20,"Attack":50,"Ranged Attack":50,"Evasion":56,"Accuracy":56,"Ranged Accuracy":56,"Magic Defense":56,"Magic Evasion":70,"Magic Accuracy":36,},
}

# Stats that are obtained through merits. Merit bonuses gated behind ability
# toggles (SAM Overwhelm, DNC Closed Position) are applied separately in
# add_base_stats.
JOB_MERIT_STATS: dict[str, dict[str, int]] = {
    "war":{"DA":5,},
    "mnk":{"Kick Attacks":5,},
    "whm":{},
    "blm":{"Magic Attack":10, "Magic Accuracy":25,},
    "rdm":{"Magic Accuracy":15+25,"Accuracy":25*0, "EnSpell Damage":15},
    "thf":{"TA":5,"Accuracy":15,"Ranged Accuracy":15,},
    "pld":{},
    "drk":{},
    "bst":{},
    "brd":{},
    "rng":{"Recycle":25},
    "smn":{},
    "sam":{"Zanshin":5, "Store TP":10},
    "nin":{"Subtle Blow":5,"Ninjutsu Magic Accuracy":25,"Ninjutsu Magic Attack":20+10,"Ninjutsu Magic Damage":0}, # Including +10 matk from group1 and +20 matk from group 2.
    "drg":{},
    "blu":{},
    "cor":{},
    "pup":{},
    "dnc":{},
    "sch":{"Helix Magic Accuracy":15,"Helix Magic Attack":10},
    "geo":{},
    "run":{},
}

TWO_HANDED_SKILLS = ["Great Sword", "Great Katana", "Great Axe", "Polearm", "Scythe", "Staff"]

ALL_COMBAT_SKILLS = ["Hand-to-Hand Skill","Dagger Skill","Sword Skill","Great Sword Skill","Axe Skill","Great Axe Skill","Scythe Skill","Polearm Skill","Katana Skill","Great Katana Skill","Club Skill","Staff Skill","Archery Skill","Marksmanship Skill","Throwing Skill","Evasion Skill","Divine Magic Skill","Elemental Magic Skill","Dark Magic Skill","Ninjutsu Skill","Summoning Magic Skill","Blue Magic Skill",]

# Gear/buff metadata keys that never accumulate into the player stat sheet.
# DMG and Delay are read separately when calculating damage.
GEAR_METADATA_KEYS = ["Name","Name2","Type","DMG","Delay","Jobs","Skill Type","Rank"]

# Combat skills seen on main/sub weapons that apply only to their own slot;
# they accumulate as "main X Skill"/"sub X Skill" instead of globally.
# Smite trait: Attack% bonus per trait level for two-handed/H2H weapons.
SMITE_ATTACK_PCT = {5:304./1024, 4:256./1024, 3:204./1024, 2:152./1024, 1:100./1024, 0:0.}

# Fencer trait: (TP Bonus, Crit Rate) per trait level.
FENCER_BONUSES: dict[int, tuple[int, int]] = {0:(0,0), 1:(200,3), 2:(300,5), 3:(400,7), 4:(450,9), 5:(500,10), 6:(550,11), 7:(600,12), 8:(630,13)}

MAIN_SUB_ONLY_SKILLS = ["Hand-to-Hand Skill","Dagger Skill","Sword Skill","Great Sword Skill","Axe Skill","Great Axe Skill","Scythe Skill","Polearm Skill","Katana Skill","Great Katana Skill","Club Skill","Staff Skill","Evasion Skill","Divine Magic Skill","Elemental Magic Skill","Dark Magic Skill","Ninjutsu Skill","Summoning Magic Skill","Blue Magic Skill","Magic Accuracy Skill"]


class create_enemy:
    #
    # Create an enemy class so that we may modify its stats more easily.
    #
    def __init__(self, enemy: dict[str, Any]) -> None:
        #
        #
        #
        self.stats: EnemyStats = {}
        ignore_stats = ["Name", "Location"] # Ignore the strings to create a dictionary of numeric values
        for stat in enemy:
            if stat not in ignore_stats:
                self.stats[stat] = enemy[stat]


class create_player:
    #
    # Python class used to build a dictionary of player stats.
    #
    # Attributes:
    #    main_job
    #    sub_job
    #    main_job_level
    #    sub_job_level
    #    gearset
    #    buffs
    #    abilities
    #    stats
    #
    def __init__(self, main_job: str, sub_job: str, master_level: int = 20, gearset: Gearset = {}, buffs: Buffs = {}, abilities: dict[str, Any] = {},) -> None:
        #
        #
        #
        self.main_job = main_job.lower()
        self.sub_job = sub_job.lower()
        self.gearset = gearset
        self.buffs = buffs
        self.abilities = abilities
        self.master_level = 50 if master_level > 50 else 0 if master_level < 0 else master_level

        self.main_job_level = 99 # Assume Lv99 main job for now, but leave room to expand to arbitrary main jobs level later.
        self.sub_job_level = self.main_job_level//2 + self.master_level//5

        # Create a dictionary to contain all of the stats provided to the character from their selected <main_job>, <sub_job>, JOB_TRAITS, job points, and merits.
        self.stats: Stats = {}

        # Non-float stat data lives outside <stats> so the sheet stays purely numeric.
        # WSC: weapon-skill stat-contribution bonuses from gear, as (stat_name, coeff) pairs.
        self.wsc: list[tuple[str, float]] = []
        # Wyvern Bonus Attack%: flags the +20% attack a fully leveled Wyvern grants a DRG.
        self.wyvern_bonus_attack: bool = False

        # Add base stats (ignoring gear or buffs).
        self.add_base_stats()

        # Add stats from gear
        self.add_gear_stats()

        # Add stats from buffs.
        self.add_buffs()

        # Finalize stats by using the BG Wiki equations to calculate stats such as attack and accuracy per weapon, evasion from skill, etc
        self.finalize_stats()

        # Sort the player stats alphabetically.
        self.stats = dict(sorted(self.stats.items()))

    def add_stat(self, stat: str, amount: float) -> None:
        #
        # Accumulate <amount> onto a (possibly missing) player stat.
        #
        self.stats[stat] = self.stats.get(stat, 0) + amount

    def get_skill_accuracy(self, skill_level: float) -> float:
        #
        # Calculate accuracy from weapon-type skill.
        # The contribution from skill changes with different skill levels. We need to add the contributions separately
        #
        skill_accuracy = 0
        if skill_level > 200:
            skill_accuracy += int((min(skill_level,400)-200)*0.9) + 200
        else:
            skill_accuracy += skill_level
        if skill_level > 400:
            skill_accuracy += int((min(skill_level,600)-400)*0.8)
        if skill_level > 600:
            skill_accuracy += int((skill_level-600)*0.9)

        return(skill_accuracy)



    def finalize_stats(self,) -> None:
        #
        # Finalize the player stats. Create main/off-hand attacks/accuracies, increase Evasion based on skill/AGI, etc.
        # We ignore weapon skill bonuses (such as Blade: Shun's attack bonus) here.
        #

        # Define Dual Wield to simplify some code.
        dual_wield = self.gearset["sub"]["Type"] == "Weapon" or self.gearset["main"]["Skill Type"] == "Hand-to-Hand"

        # Compute Evasion, per-hand Attack/Accuracy, and Ranged Attack/Accuracy.
        self.finalize_offensive_stats()

        # Setup weapon DMG/Delay and zero-out stats that don't apply to the gearset.
        self.finalize_weapon_dmg_delay(dual_wield)

        # Calculate total haste, Hand-to-Hand specifics, and delay reduction.
        self.finalize_haste_and_delay()

        # We'll apply caps to certain stats in the main code.

    def finalize_offensive_stats(self,) -> None:
        #
        # Compute Evasion, per-hand Attack/Accuracy, and Ranged Attack/Accuracy from skill, STR/DEX/AGI, and buffs.
        #
        # Increase the Evasion stat based on contribution from Evasion Skill and AGI.
        # Evasion Skill is worth 0.8 Evasion after Evasion Skill > 300.
        # AGI is worth 0.5 Evasion.
        self.stats["Evasion"] = self.stats.get("Evasion",0) + int(0.5*self.stats.get("AGI",0)) + (300 + 0.8*(self.stats.get("Evasion Skill",0)-300) if self.stats.get("Evasion Skill",0) > 300 else self.stats.get("Evasion Skill",0))


        # Create "Attack1" for main-hand and "Attack2" for off-hand.
        # Off-hand attack uses STR/2
        main_skill = self.gearset["main"].get("Skill Type","None") + " Skill"
        sub_skill = self.gearset["sub"].get("Skill Type","None") + " Skill"
        self.stats["Attack1"] = 8 + self.stats.get(main_skill, 0) + self.stats["STR"] + self.stats.get("Attack",0) + self.stats.get(f"main {main_skill}", 0)
        self.stats["Attack2"] = 8 + self.stats.get(sub_skill, 0) + int(0.5*self.stats["STR"]) + self.stats.get("Attack",0) + self.stats.get(f"sub {sub_skill}",0)

        # Update Ranged Attack
        ranged_skill = self.gearset["ranged"].get("Skill Type", "None") + " Skill"
        ammo_skill = self.gearset["ammo"].get("Skill Type", "None") + " Skill"
        if ammo_skill=="Throwing Skill": # For Shuriken
            self.stats["Ranged Attack"] += 8 + self.stats.get(ammo_skill,0) + self.stats.get("STR",0)
        elif ranged_skill in ["Marksmanship Skill", "Archery Skill"]:
            self.stats["Ranged Attack"] += 8 + self.stats.get(ranged_skill,0) + self.stats.get("STR",0)
        else:
            self.stats["Ranged Attack"] = 0

        # Additive buffs to Attack and Accuracy from things like BRD have already been added.
        # We still need to add percent-based buffs such as Chaos Roll and Berserk. These have already been conveniently summed into the "Attack%" stat.
        self.stats["Attack1"] *= (1 + self.stats.get("Attack%",0))
        self.stats["Attack2"] *= (1 + self.stats.get("Attack%",0))
        self.stats["Ranged Attack"] *= (1 + self.stats.get("Ranged Attack%",0))

        # Attack from food applies after percent-based buffs, but the STR+ on food applies normally. We use the "Food Attack" stat to separate things.
        self.stats["Attack1"] += self.stats.get("Food Attack",0)
        self.stats["Attack2"] += self.stats.get("Food Attack",0)
        self.stats["Ranged Attack"] += self.stats.get("Food Ranged Attack",0)
        

        # Create "Accuracy1" for main-hand and "Accuracy2" for off-hand.
        main_skill_level = self.stats.get(main_skill,0) + self.stats.get(f"main {main_skill}",0)
        sub_skill_level = self.stats.get(sub_skill,0) + self.stats.get(f"sub {sub_skill}",0)
        ranged_skill_level = self.stats.get(ranged_skill,0)
        ammo_skill_level = self.stats.get(ammo_skill,0)

        self.stats["Accuracy1"] = int(0.75*(self.stats.get("DEX",0))) + self.stats.get("Accuracy",0) + self.get_skill_accuracy(main_skill_level)
        self.stats["Accuracy2"] = int(0.75*(self.stats.get("DEX",0))) + self.stats.get("Accuracy",0) + self.get_skill_accuracy(sub_skill_level)
        if ammo_skill=="Throwing Skill": # For Shuriken
            self.stats["Ranged Accuracy"] += int(0.75*(self.stats.get("AGI",0))) + self.get_skill_accuracy(ammo_skill_level)
        elif ranged_skill in ["Marksmanship Skill","Archery Skill"]:
            self.stats["Ranged Accuracy"] += int(0.75*(self.stats.get("AGI",0))) + self.get_skill_accuracy(ranged_skill_level)

    def finalize_weapon_dmg_delay(self, dual_wield: bool) -> None:
        #
        # Set weapon DMG/Delay stats and zero-out stats that don't apply to the gearset.
        #
        self.stats["Delay1"] = self.gearset["main"].get("Delay",480-self.stats.get("Martial Arts",0)) # Use base hand-to-hand delay if main-hand item does not have a Delay stat
        self.stats["Delay2"] = self.gearset["sub"].get("Delay",self.stats["Delay1"]) # Copy main-hand delay if off-hand item does not have delay stat
        self.stats["Ranged Delay"] = self.gearset["ranged"].get("Delay",0)
        self.stats["Ammo Delay"] = self.gearset["ammo"].get("Delay",0)

        self.stats["DMG1"] = self.gearset["main"].get("DMG",0)
        self.stats["DMG2"] = self.gearset["sub"].get("DMG",0)
        self.stats["Ranged DMG"] = self.gearset["ranged"].get("DMG",0)
        self.stats["Ammo DMG"] = self.gearset["ammo"].get("DMG",0)

        # Zero-out stats that don't apply for the given gearset. We have already dealt with Smite/LastResort/Hasso requiring 2-handed weapons too.
        two_handed = TWO_HANDED_SKILLS
        if self.gearset["ammo"].get("Type","None")!="Shuriken" or self.main_job.lower() != "nin":
            self.stats["Daken"] = 0
        if self.gearset["main"].get("Skill Type","None")!="Hand-to-Hand":
            self.stats["Kick Attacks"] = 0
            self.stats["Martial Arts"] = 0
        if self.gearset["main"]["Skill Type"] not in two_handed:
            self.stats["Zanshin"] = 0
        if (self.gearset["ranged"].get("Type") not in ["Gun","Bow","Crossbow"]) and (self.gearset["ammo"].get("Type","None") not in ["Bullet","Arrow","Bolt","Shuriken"]):
            self.stats["Ranged Attack"] = 0
            self.stats["Ranged Accuracy"] = 0
        if not self.abilities.get("True Shot",False): # Disable True Shot bonuses if True Shot is not enabled.
            self.stats["True Shot"] = 0

        # Treat off-hand stats when not dual wielding.
        if not dual_wield:
            self.stats["Attack2"] = 0
            self.stats["Accuracy2"] = 0
            self.stats["Delay2"] = self.stats["Delay1"]
            self.stats["Dual Wield"] = 0

    def finalize_haste_and_delay(self,) -> None:
        #
        # Calculate total haste, Hand-to-Hand DMG/Delay specifics, and the final delay reduction.
        #
        # Calculate total haste.
        self.stats["Gear Haste"] = self.stats.get("Gear Haste",0)/102.4
        self.stats["JA Haste"] = self.stats.get("JA Haste",0)/102.4
        self.stats["Magic Haste"] = self.stats.get("Magic Haste",0)
        total_haste = (self.stats["Gear Haste"] if self.stats["Gear Haste"] < 0.25 else 0.25) + (self.stats["JA Haste"] if self.stats["JA Haste"] < 0.25 else 0.25) + (self.stats["Magic Haste"] if self.stats["Magic Haste"] < 448./1024 else 448./1024)

        # Deal with the special case of Hand-to-Hand values.
        if self.gearset["main"]["Skill Type"] == "Hand-to-Hand":
            self.stats["Attack2"] = self.stats["Attack1"] - 0.5*self.stats["STR"]*(1+self.stats.get("Attack%",0))*0 # The off-hand H2H Attack might use STR/2 like normal weapons. This is ignored in the main code where I simply set attack1 = attack2 before calculating H2H damage.
            self.stats["Accuracy2"] = self.stats["Accuracy1"]
            self.gearset["sub"]["Skill Type"] = self.gearset["main"]["Skill Type"]
            base_dmg = 3 + int((self.stats.get("Hand-to-Hand Skill",0) + self.stats.get("main Hand-to-Hand Skill",0))*0.11)
            self.stats["DMG1"] = base_dmg + self.stats["DMG1"]
            self.stats["DMG2"] = self.stats["DMG1"]
            if self.abilities.get("Footwork",False):
                self.stats["Kick DMG"] = self.stats["DMG1"] + self.stats.get("Kick Attacks DMG",0) # Main-hand DMG adds to Kick DMG during Footwork. See [https://www.ffxiah.com/forum/topic/55864/new-monk-questions/#3600604] [https://www.ffxiah.com/forum/topic/36705/iipunch-monk-guide/213/#3368961] and [https://forum.square-enix.com/ffxi/threads/52969-August.-3-2017-%28JST%29-Version-Update]
            else:
                self.stats["Kick DMG"] = base_dmg + self.stats.get("Kick Attacks DMG",0) # Without Footwork, Kick DMG is your base damage (from Skill) and any gear with "Kick Attacks Attack" on it.
            base_delay0 = 480
            self.stats["Delay1"] += base_delay0
            self.stats["Delay2"] = self.stats["Delay1"]
            base_delay = self.stats["Delay1"]
            reduced_delay = (base_delay - self.stats.get("Martial Arts",0)) * (1 - total_haste)
            self.stats["Dual Wield"] = 0 # We set a "dual_wield" variable to True in the main code to trigger off-hand hits, but we need to make sure that the "Dual Wield" stat is zero, otherwise it will stack with Martial Arts for H2H weapons.
        else:
            base_delay = (self.stats["Delay1"] + self.stats["Delay2"])/2 
            reduced_delay = base_delay * ((1 - self.stats.get("Dual Wield",0)/100)) * (1 - total_haste)
            self.stats["Kick DMG"] = 0

        self.stats["Delay Reduction"] = (1 - reduced_delay/base_delay) if (1 - reduced_delay/base_delay) < 0.8 else 0.8

    def add_buffs(self,) -> None:
        #
        # Add stats from buffs and job abilities.
        #
        # Buffs accumulate into self.stats and order matters, so the helpers below
        # must be called in exactly this sequence.
        two_handed = TWO_HANDED_SKILLS
        jobs = [self.main_job, self.sub_job]

        # Add buffs from Food, COR, BRD, GEO, and WHM first.
        self.add_party_buffs()

        # Add buffs from individual job abilities, in source order. The order matters
        # because buffs accumulate into self.stats.
        self.add_war_mnk_ability_buffs(jobs)
        self.add_caster_ability_buffs(jobs)
        self.add_thf_pld_drk_ability_buffs(jobs, two_handed)
        self.add_rng_sam_nin_cor_ability_buffs(jobs, two_handed)
        self.add_dnc_sch_geo_run_bst_ability_buffs(jobs)
        self.add_pup_gear_bonus()

        # Add trait-based and weapon-based bonuses (Smite, Fencer, aftermath).
        self.add_smite_and_fencer(two_handed)
        self.add_aftermath_buffs()

        # Add buffs accessible to all jobs from assumed party members.
        self.add_universal_party_member_buffs()

    def add_party_buffs(self,) -> None:
        #
        # Add buffs from Food, COR, BRD, GEO, and WHM (everything in self.buffs).
        #
        ignore_stats = GEAR_METADATA_KEYS
        for source in self.buffs:
            for stat in self.buffs[source]:
                if stat not in ignore_stats:
                    self.stats[stat] = self.stats.get(stat,0) + self.buffs[source][stat]

    def add_war_mnk_ability_buffs(self, jobs: list[str]) -> None:
        #
        # Add buffs from Warrior and Monk job abilities.
        #
        # ===========================================================================
        # ===========================================================================
        # Warrior abilities
        if "war" in jobs:
            if self.abilities.get("Berserk",False):
                self.add_stat("Attack%", 0.25 + 0.085*(self.gearset["main"]["Name"]=="Conqueror") + (100./1024)*(self.main_job=="war")) # Warrior main gets +10% more attack with berserk.
                self.add_stat("Attack", 40*(self.main_job=="war"))
                self.add_stat("Crit Rate", 14*(self.gearset["main"]["Name"]=="Conqueror"))
            if self.abilities.get("Aggressor",False):
                self.add_stat("Accuracy", 25 + 20*(self.main_job=="war"))
            if self.main_job=="war":
                if self.abilities.get("Mighty Strikes",False):
                    self.stats["Crit Rate"] = 100
                    self.add_stat("Accuracy", 40)
        # ===========================================================================
        # ===========================================================================
        # Monk abilities
        if "mnk" in jobs:
            if self.main_job=="mnk":
                if self.abilities.get("Focus",False):
                    self.add_stat("Crit Rate", 20)
                    self.add_stat("Accuracy", 100 + 20)
                if self.abilities.get("Footwork",False):
                    self.add_stat("Kick Attacks", 20)
                    self.stats["Kick Attacks Attack%"] = self.stats.get("Kick Attacks Attack%",0) + 100./1024 + (130./1024 if "Bhikku Gaiters +2"==self.gearset["feet"]["Name"] else 160./1024)
                    self.add_stat("Kick Attacks DMG", 20 + 20) # Activating footwork increases Kick DMG by 20, with an additional 20 from job points
                if self.abilities.get("Impetus",False):
                    impetus_potency = 0.9
                    self.add_stat("Crit Rate", 50*impetus_potency)
                    self.add_stat("Attack", (100+40)*impetus_potency)
                    self.add_stat("Crit Damage", 50*impetus_potency*("Bhikku Cyclas" in self.gearset["body"]["Name"]))
                    self.add_stat("Accuracy", 100*impetus_potency*("Bhikku Cyclas" in self.gearset["body"]["Name"]))
            if self.sub_job=="mnk":
                if self.abilities.get("Focus",False):
                    self.add_stat("Crit Rate", 20*(1 - (99 - (self.sub_job_level))/100))
                    self.add_stat("Accuracy", 100*(1 - (99 - (self.sub_job_level))/100))

    def add_caster_ability_buffs(self, jobs: list[str]) -> None:
        #
        # Add buffs from Black Mage and Red Mage job abilities.
        #
        # ===========================================================================
        # ===========================================================================
        # Black Mage abilities
        if "blm" in jobs:
            if self.main_job=="blm":
                if self.abilities.get("Manafont",False):
                    self.add_stat("Magic Damage", 60)
                if self.abilities.get("Manawell",False):
                    self.add_stat("Magic Damage", 20)
        # ===========================================================================
        # ===========================================================================
        # Red Mage abilities
        if "rdm" in jobs:
            if self.main_job=="rdm":
                if self.abilities.get("Chainspell",False):
                    self.add_stat("Magic Damage", 40)
                if self.abilities.get("Composure",False):
                    self.add_stat("Accuracy", 20 + 50) # +50 from Lv99 base and +20 from JP
                    self.add_stat("EnSpell Damage%", 200) # +200% EnSpell damage from Composure
                    self.stats["EnSpell Damage"] = self.stats.get("EnSpell Damage",0)

    def add_thf_pld_drk_ability_buffs(self, jobs: list[str], two_handed: list[str]) -> None:
        #
        # Add buffs from Thief, Paladin, and Dark Knight job abilities.
        #
        # ===========================================================================
        # ===========================================================================
        # Thief abilities
        if "thf" in jobs:
            if self.main_job=="thf":
                if self.abilities.get("Conspirator",False): # Assuming 6 players on the enmity list.
                    self.add_stat("Accuracy", 25 + 20)
                    self.add_stat("Subtle Blow", 50)
                    self.add_stat("Attack", 25*("Skulker's Vest" in self.gearset["body"]["Name"])) # Must be equipped for the extra Attack
        # ===========================================================================
        # ===========================================================================
        # Paladin abilities
        if "pld" in jobs:
            if self.main_job=="pld":
                if self.abilities.get("Divine Emblem",False):
                    self.add_stat("Magic Damage", 40)
                if self.abilities.get("Enlight II",False):
                    divine_skill = self.abilities.get("Enhancing Skill",0)
                    divine_skill = 0 if divine_skill < 0 else divine_skill
                    if divine_skill <=500:
                        enlight_acc = 2*((divine_skill + 85)/13) + ((divine_skill + 85)/26)
                    else:
                        enlight_acc = 2*((divine_skill + 400)/20) + ((divine_skill + 400)/40)

                    enlight_potency = 0.80
                    self.add_stat("Accuracy", enlight_acc*enlight_potency + 20) # +20 from JP.  +120 accuracy at 600 skill
        # ===========================================================================
        # ===========================================================================
        # Dark Knight abilities
        if "drk" in jobs:
            if self.abilities.get("Last Resort",False):
                self.add_stat("Attack%", 256./1024 + 100./1024*(self.main_job=="drk"))
                self.add_stat("Attack", 40*(self.main_job=="drk"))
                self.stats["JA Haste"] = self.stats.get("JA Haste",0) + 15 + 10*(self.main_job=="drk") if self.gearset["main"]["Skill Type"] in two_handed else self.stats.get("JA Haste",0)
            if self.main_job=="drk":
                if self.abilities.get("Endark II",False): # https://ffxiclopedia.fandom.com/wiki/Endark_II
                    endark_potency = 0.80
                    dark_magic_skill = self.abilities.get("Enhancing Skill",0)
                    self.add_stat("Accuracy", 20)
                    self.add_stat("Attack", (((dark_magic_skill + 20)/13 + 5)*2.5) * endark_potency + 20) # +125 attack at 600 skill

    def add_rng_sam_nin_cor_ability_buffs(self, jobs: list[str], two_handed: list[str]) -> None:
        #
        # Add buffs from Ranger, Samurai, Ninja, and Corsair job abilities.
        #
        # # ===========================================================================
        # # ===========================================================================
        # # Bard abilities
        # if self.main_job=="brd":
        #     if self.buffs["brd"].get("Attack",0)>0:
        #         self.stats["Attack"] = self.stats.get("Attack",0) + 20 # Job Points give +20 extra attack if you have a Minuet up.
        # ===========================================================================
        # ===========================================================================
        # Ranger abilities
        if "rng" in jobs:
            if self.abilities.get("Sharpshot",False):
                self.add_stat("Ranged Accuracy", 40)
                self.add_stat("Ranged Attack", 40*(self.main_job=="rng"))
            if self.main_job == "rng":
                if self.abilities.get("Barrage",False):
                    self.add_stat("Ranged Attack", 60)
                if self.abilities.get("Velocity Shot",False):
                    self.add_stat("Ranged Attack", 40)
                    self.stats["JA Haste"] = self.stats.get("JA Haste",0) - 15
                    self.add_stat("Ranged Attack%", 152./1024 + 112./1024*("Amini Caban +3"==self.gearset["body"]["Name"]) + 92./1024*("Amini Caban +2"==self.gearset["body"]["Name"]) + 20./1024*("Belenus" in self.gearset["back"]["Name"]))
                if self.abilities.get("Double Shot",False):
                    self.add_stat("Double Shot", 40 + 5*("Arcadian Jerkin" in self.gearset["body"]["Name"]))
                    if "Arcadian Jerkin" in self.gearset["body"]["Name"]: # Half of your Double Shot becomes Triple Shot with the relic body equipped. This ratio is assumed from the Triple>Quad ratio for COR linked below.
                        self.stats["Triple Shot"] = self.stats.get("Double Shot",0)/2 # The way this is written will overwrite "Triple Shot" from Oshosi. This is intentional since I believe that RNG can't proc Triple Shot on Oshosi and COR can't proc Double Shot on Oshosi
                        self.stats["Double Shot"] = self.stats.get("Double Shot",0)/2
                else:
                    self.stats["Double Shot"] = 0 + 5*("Arcadian Jerkin" in self.gearset["body"]["Name"])
                    self.stats["Triple Shot"] = 0
                if self.abilities.get("Hover Shot",False): # We double damage dealt with Hover Shot enabled in the main code.
                    self.add_stat("Ranged Accuracy", 100)
                    # self.stats["Magic Accuracy"] = self.stats.get("Magic Accuracy",0) + 100 # Hover Shot Magic accuracy applies only to magic WS. We add +100 Macc in the actions.py file only if using a ranged attack
        # ===========================================================================
        # ===========================================================================
        # Samurai abilities
        if "sam" in jobs:
            if self.abilities.get("Hasso",False) and (self.gearset["main"]["Skill Type"] in two_handed):
                if self.main_job=="sam":
                    self.add_stat("STR", 14 + 20)
                    self.stats["Zanshin"] = 100 if self.stats.get("Zanshin",0) > 100 else self.stats.get("Zanshin",0)
                    self.stats["Zanhasso"] = self.stats.get("Zanshin",0)*0.35
                else:
                    self.add_stat("STR", int(self.sub_job_level/7))

                self.add_stat("JA Haste", 10 + self.stats.get("Hasso+ JA Haste",0))
                self.add_stat("Accuracy", 10)
        # ===========================================================================
        # ===========================================================================
        # Ninja abilities
        if self.main_job=="nin":
            if self.abilities.get("Sange",False) and self.gearset["ammo"]["Type"]=="Shuriken":
                self.add_stat("Ranged Accuracy", 100) # Assume 5/5 Sange Merits
                self.stats["Daken"] = 100
            if self.abilities.get("Innin",False):
                innin_potency = 0.7 # Innin ninjutsu damage bonus is handled in the actions.py code when calculating ninjutsu damage dealt
                self.add_stat("Accuracy", 20)
                self.add_stat("Skillchain Bonus", 5)
                self.add_stat("Magic Burst Damage Trait", 5)
                self.add_stat("Crit Rate", (innin_potency*(30-10) + 10))
                self.add_stat("Evasion", (innin_potency*(-30 - -10) + -10))
                self.add_stat("DA", self.stats.get("Innin DA%",0))
            if self.abilities.get("Futae",False):
                self.add_stat("Ninjutsu Magic Damage", 100)
        # ===========================================================================
        # ===========================================================================
        # Corsair abilities
        if self.main_job == "cor":
            if self.abilities.get("Triple Shot",False):
                self.add_stat("Triple Shot", 40)
                if "Lanun Gants" in self.gearset["hands"]["Name"]: # Half of your Triple Shot becomes Quad Shot with Relic Hands. See: (https://www.ffxiah.com/forum/topic/31312/the-pirates-lair-a-guide-to-corsair/154/#3323623) and (http://wiki.ffo.jp/html/30818.html)
                    self.stats["Quad Shot"] = self.stats.get("Triple Shot",0)/2 
                    self.stats["Triple Shot"] = self.stats.get("Triple Shot",0)/2
            else:
                self.stats["Triple Shot"] = 0
                self.stats["Quad Shot"] = 0

    def add_dnc_sch_geo_run_bst_ability_buffs(self, jobs: list[str]) -> None:
        #
        # Add buffs from Dancer, Scholar, Geomancer, Rune Fencer, and Beastmaster abilities.
        #
        # ===========================================================================
        # ===========================================================================
        # Dancer abilities. We already assumed Haste Samba is always active in the "job-specific stats from spells and abilities" section above.
        if self.main_job=="dnc":
            if self.abilities.get("Building Flourish",False):
                self.add_stat("Crit Rate", 10)
                self.add_stat("Accuracy", 40)
                self.add_stat("Attack%", 0.25)
                self.add_stat("Weapon Skill Damage", self.stats.get("Building Flourish WSD",0))
            # We deal with Climactic, Striking, and Ternary Flourish in the main code since they are special cases that apply some stuff only to the first hit.
            if self.abilities.get("Saber Dance",False):
                self.add_stat("DA", 25) # Assume minimum potency Saber Dance since it decays quickly.
            if self.abilities.get("Closed Position", False):
                self.add_stat("Store TP", (3*5)*("Horos Toe Shoes +3"==self.gearset["feet"]["Name"] or "Horos Toe Shoes +4"==self.gearset["feet"]["Name"])) # DNC Relic+3 feet provide +3 Store TP for each merit into Closed Position

        # ===========================================================================
        # ===========================================================================
        # Scholar abilities.
        if self.main_job=="sch":
            if self.abilities.get("Ebullience",False):
                self.stats["Magic damage"] = self.stats.get("Magic Damage",0) + 40 # We deal with the damage bonus from Ebullience in the main code.
            if self.abilities.get("Enlightenment",False):
                self.add_stat("INT", 20)
                self.add_stat("MND", 20)
        if "sch" in [self.main_job, self.sub_job]:
            if self.abilities.get("Klimaform",False): 
                self.add_stat("Magic Accuracy", 15) # SCH Empy+3 feet provide damage+ with klimaform active. We deal with that in the main code.

        # ===========================================================================
        # ===========================================================================
        # Geomancer abilities.
        if self.main_job=="geo":
            if self.abilities.get("Theurgic Focus",False):
                self.add_stat("-ra Magic Attack", 50)
                self.stats["-ra Magic damage"] = self.stats.get("-ra Magic Damage",0) + 60
        # ===========================================================================
        # ===========================================================================
        # Rune Fencer abilities.
        if "run" in jobs:
            if self.abilities.get("Swordplay",False):
                swordplay_potency = 0.9 # Assume 90% potency
                self.add_stat("Accuracy", 60*swordplay_potency)
                self.add_stat("Evasion", 60*swordplay_potency)
        # ===========================================================================
        # ===========================================================================
        # Corsair abilities
        if self.main_job=="bst":
            if self.abilities.get("Rage",False):
                self.add_stat("Attack%", 0.50)
            if self.abilities.get("Frenzied Rage",False):
                self.add_stat("Attack%", 0.25)

    def add_pup_gear_bonus(self,) -> None:
        #
        # Add the Puppetmaster exclusive gear bonus.
        #
        # ===========================================================================
        # ===========================================================================






        # ===========================================================================
        # ===========================================================================
        # Puppetmaster exclusive gear bonus
        if self.main_job=="pup":
            if self.gearset["main"]["Name"] == "Dragon Fangs":
                self.add_stat("Kick Attacks", 14)
        # ===========================================================================
        # ===========================================================================

    def add_smite_and_fencer(self, two_handed: list[str]) -> None:
        #
        # Add Smite (two-handed/H2H Attack%) and Fencer (TP Bonus/Crit Rate).
        #
        # Add Smite.
        if self.gearset["main"]["Skill Type"] in (two_handed+["Hand-to-Hand"]):
            smite_level = int(self.stats.get("Smite",0))
            self.add_stat("Attack%", SMITE_ATTACK_PCT[smite_level])
        # Add Fencer.
        if (self.gearset["sub"]["Type"] in ["Shield","None"]) and (self.gearset["main"]["Skill Type"]!="Hand-to-Hand") and (self.gearset["main"]["Skill Type"] not in two_handed):
            fencer_level = 8 if self.stats.get("Fencer",0) > 8 else int(self.stats.get("Fencer",0))
            fencer_tp_bonus, fencer_crit_rate = FENCER_BONUSES[fencer_level]
            self.add_stat("TP Bonus", fencer_tp_bonus + self.stats.get("Fencer TP Bonus",0))
            self.add_stat("Crit Rate", fencer_crit_rate)

    def add_aftermath_buffs(self,) -> None:
        #
        # Add Relic, Mythic, and Prime weapon aftermath stats.
        #
        aftermath_level = self.abilities.get("Aftermath",0)
        if aftermath_level > 0:
            main_wpn_name = self.gearset["main"]["Name"]
            main_wpn_name2 = self.gearset["main"]["Name2"]
            ranged_wpn_name = self.gearset["ranged"]["Name"]
            self.add_relic_aftermath(aftermath_level, main_wpn_name, ranged_wpn_name)
            self.add_mythic_aftermath(aftermath_level, main_wpn_name, ranged_wpn_name)
            self.add_prime_aftermath(aftermath_level, main_wpn_name, main_wpn_name2)

            # Empyrean Aftermath is entirely handled in the main code when calculating damage.

    def add_relic_aftermath(self, aftermath_level: int, main_wpn_name: str, ranged_wpn_name: str) -> None:
        #
        # Add Relic weapon aftermath stats. Only one relic aftermath applies:
        # the main-hand weapon takes priority over the ranged weapon.
        #
        if main_wpn_name in RELIC_MAIN_AFTERMATH:
            for stat, value in RELIC_MAIN_AFTERMATH[main_wpn_name]:
                self.add_stat(stat, value)
        elif ranged_wpn_name in RELIC_RANGED_AFTERMATH:
            for stat, value in RELIC_RANGED_AFTERMATH[ranged_wpn_name]:
                self.add_stat(stat, value)

    def add_mythic_aftermath(self, aftermath_level: int, main_wpn_name: str, ranged_wpn_name: str) -> None:
        #
        # Add Mythic weapon aftermath stats (melee and ranged).
        #
        if main_wpn_name in MYTHIC_AFTERMATH: # Melee Mythic aftermath check. We add other forms of OAX later with similar formats.
            if aftermath_level==3:
                self.add_stat("OA2 main", 40)
                self.add_stat("OA3 main", 20)
            else:
                stat, value = MYTHIC_AFTERMATH[main_wpn_name][aftermath_level-1]
                self.add_stat(stat, value)
        if ranged_wpn_name in MYTHIC_AFTERMATH and aftermath_level in [1,2]: # Ranged Mythic aftermath check. We deal with Lv2 later in the main code when calculating damage.
            stat, value = MYTHIC_AFTERMATH[ranged_wpn_name][aftermath_level-1]
            self.add_stat(stat, value)

    def add_prime_aftermath(self, aftermath_level: int, main_wpn_name: str, main_wpn_name2: str) -> None:
        #
        # Add Prime weapon aftermath effects (PDL, Magic Damage, Magic Attack).
        # Lv1/Lv2 aftermath values are interpolated between the tier endpoints at
        # the assumed potency; Lv3 grants the full top-tier value.
        #
        if main_wpn_name not in PRIME_AFTERMATH_STATS:
            return

        stage = main_wpn_name2.split()[-1] # "IV" or "V"

        def prime_value(tiers: tuple[float, float, float]) -> float:
            if aftermath_level == 1:
                return (tiers[1] - tiers[0])*PRIME_AFTERMATH_POTENCY + tiers[0]
            if aftermath_level == 2:
                return (tiers[2] - tiers[1])*PRIME_AFTERMATH_POTENCY + tiers[1]
            return tiers[2]

        for stat in PRIME_AFTERMATH_STATS[main_wpn_name]:
            self.add_stat(stat, prime_value(PRIME_AFTERMATH_TIERS[stat][stage]))

    def add_universal_party_member_buffs(self,) -> None:
        #
        # Add buffs accessible to all jobs from assumed party members (SMN, BLU, BST, WAR, etc).
        #
        if self.abilities.get("Blood Rage",False):
            self.add_stat("Crit Rate", 20)
            if self.main_job=="war":
                self.add_stat("Crit Rate", 20)

        if self.abilities.get("Warcry", False):
            self.add_stat("Attack%", int(99/4 + 4.75)/256)
            self.add_stat("TP Bonus", 500+200)
            if self.main_job=="war":
                self.add_stat("Attack", 60)
        
        if self.abilities.get("Warcry (sub)", False):
            self.add_stat("Attack%", int(self.sub_job_level/4 + 4.75)/256)

        if self.abilities.get("Crimson Howl", False):
            self.add_stat("Attack%", int(99/4 + 4.75)/256)

        if self.abilities.get("Crystal Blessing", False):
            self.add_stat("TP Bonus", 250)

        if self.abilities.get("Ifrit's Favor", False):
            self.add_stat("DA", 25)

        if self.abilities.get("Shiva's Favor", False):
            self.add_stat("Magic Attack", 39)

        if self.abilities.get("Ramuh's Favor", False):
            self.add_stat("Crit Rate", 23)

        if self.abilities.get("Haste Samba", False):
            self.add_stat("JA Haste", 10.1)
        elif self.abilities.get("Haste Samba (sub)", False):
            self.add_stat("JA Haste", 5.1)

        if self.abilities.get("Nature's Meditation",False): # https://www.bg-wiki.com/ffxi/Nat._Meditation
            self.add_stat("Attack%", 52./256)

        if self.abilities.get("Mighty Guard",False): # https://www.bg-wiki.com/ffxi/Nat._Meditation
            self.add_stat("Magic Haste", 0.15)
            self.add_stat("Magic Defense", 15)
            


    def add_gear_stats(self,) -> None:
        #
        # Add stats from the equipped gear, including set bonuses at the end.
        #
        self.accumulate_gear_stats()
        self.add_set_bonuses()

    def accumulate_gear_stats(self,) -> None:
        #
        # Accumulate the individual stats from each equipped gear piece into self.stats.
        #
        # A list of stats to not include in <stats>. These do not affect player stats. We will use DMG and Delay in the main code later to calculate damage, though.
        ignore_stats = GEAR_METADATA_KEYS
        for slot in self.gearset:
            for stat in self.gearset[slot]:
                if stat=="Triple Shot" and self.main_job=="rng": # Skip Triple Shot bonuses on Oshosi for RNG
                    continue
                if stat=="Double Shot" and self.main_job=="cor": # Skip Double Shot bonuses on Oshosi for COR
                    continue
                if stat not in ignore_stats:
                    if not (slot in ["main","sub"] and stat in MAIN_SUB_ONLY_SKILLS):
                        if stat in ["FUA","OA8","OA7","OA6","OA5","OA4","OA3","OA2","EnSpell Damage","EnSpell Damage%"] and slot in ["main", "sub"]: # OAX stats apply only to the weapon they are attached to.
                            self.add_stat(f"{stat} {slot}", self.gearset[slot][stat])
                        elif stat=="WSC":
                            name, coeff = self.gearset[slot][stat]
                            self.wsc.append((name, coeff))
                        else:
                            self.add_stat(stat, self.gearset[slot][stat])
                    else:
                        self.add_stat(f"{slot} {stat}", self.gearset[slot][stat])

    def add_set_bonuses(self,) -> None:
        #
        # Count equipped pieces of each gear set and apply the set bonuses.
        # Stats are accumulated even when a bonus is zero so the stat keys
        # always exist afterwards.
        #
        for set_bonus in ARMOR_SET_BONUSES:
            count = 0
            for slot in self.gearset:
                name = self.gearset[slot]["Name"]
                if set_bonus.name_substring not in name:
                    continue
                if set_bonus.requires_plus1 and "+1" not in name:
                    continue
                if set_bonus.slots is not None and slot not in set_bonus.slots:
                    continue
                count += 1
            count = min(count, set_bonus.max_pieces)
            pieces = count if set_bonus.counts_first_piece else count - 1
            bonus = set_bonus.per_piece*pieces if count >= 2 else 0
            for stat in set_bonus.stats:
                self.add_stat(stat, bonus)

        # Regal Ring / Regal Earring give Accuracy/Ranged Accuracy/Magic Accuracy
        # per equipped AF+3 armor piece (max 5 pieces each).
        af_prefix = AF_ARMOR_PREFIX[self.main_job]
        af_count = sum(af_prefix in self.gearset[slot]["Name"].lower() for slot in ["head","body","hands","legs","feet"])
        af_count = min(af_count, 5)
        regal_count = 0
        if "Regal Ring" in [self.gearset["ring1"]["Name"], self.gearset["ring2"]["Name"]]:
            regal_count += af_count
        if "Regal Earring" in [self.gearset["ear1"]["Name"], self.gearset["ear2"]["Name"]]:
            regal_count += af_count
        self.add_stat("Accuracy", regal_count*15)
        self.add_stat("Ranged Accuracy", regal_count*15)
        self.add_stat("Magic Accuracy", regal_count*15)


    def add_base_stats(self,) -> None:
        #
        # Include base stats (without any gear or buffs) to the <stats> dictionary.
        #
        # ===========================================================================
        # ===========================================================================
        # Add base parameters for Lv99 <main_job> and <sub_job> with <master_level> master levels.
        for stat in JOB_BASE_PARAMETERS[self.main_job]:
            self.stats[stat] = self.stats.get(stat,0) + JOB_BASE_PARAMETERS[self.main_job][stat] + SUBJOB_BASE_PARAMETERS[self.sub_job][stat] + self.master_level 

        # Traits are setup with the following format:  "job1": [[level1, tier1],[level2,tier2],[etc], "job2":[[level1,tier1],[level2,tier2],[etc]],"job3":etc.. With JOB_TRAITS in decending order.
        for trait in JOB_TRAITS:
            # Add <main_job> JOB_TRAITS first so that <sub_job> JOB_TRAITS do not overwrite them and they do not stack.
            if self.main_job in JOB_TRAITS[trait]:
                for k in JOB_TRAITS[trait][self.main_job]: # Loop through the tiers in decending order. Stop on the highest tier below or equal to your main job level.
                    if self.main_job_level >= k[0]:
                        self.stats[trait] = self.stats.get(trait,0) + k[1] # Directly add the <main_job> trait to player stats.
                        break
            # Add <sub_job> JOB_TRAITS now.
             # Since we add JOB_TRAITS immediately after base parameters, all stats should be 0 except those from <main_job> JOB_TRAITS. This ordering ensures that we can cleanly compare <main_job> and <sub_job> trait tiers and keep only the highest without stacking them.
            if self.sub_job in JOB_TRAITS[trait]:
                for k in JOB_TRAITS[trait][self.sub_job]:
                    if trait=="DA" and self.main_job=="dnc" and self.sub_job=="war" and self.abilities.get("Saber Dance",False): # Saber Dance does not stack with DA JOB_TRAITS apparently.
                        continue
                    if self.sub_job_level >= k[0]:
                        self.stats[trait] = max(self.stats.get(trait,0), k[1]) # Only keep the highest tier trait between your <main_job> and <sub_job>.
                        break

        # BST gets +50 Accuracy from Tandem Strike JOB_TRAITS when the pet is attacking the same target.
        if self.main_job=="bst":
            self.add_stat("Accuracy", 50)

        self.add_stat("Ranged Attack", 0)

        # ===========================================================================
        # ===========================================================================
        # Add combat skills for a level 99 <main_job>. We add merits later.
        for stat in JOB_COMBAT_SKILLS[self.main_job]:
            self.stats[stat] = self.stats.get(stat,0) + JOB_COMBAT_SKILLS[self.main_job][stat] + self.master_level

        # ===========================================================================
        # ===========================================================================
        # Add stats from job merits.

        for stat in JOB_MERIT_STATS[self.main_job]:
            self.add_stat(stat, JOB_MERIT_STATS[self.main_job][stat])

        # Merit bonuses gated behind ability toggles. The stat keys are still
        # created when the toggle is off (the added bonus is just zero).
        if self.main_job == "sam":
            # Overwhelm is treated as a checkbox toggle and only applies when enabled. Currently applies to Ranged WSs as well, which is incorrect.
            self.add_stat("Weapon Skill Damage", 19 * self.abilities.get("Overwhelm", False))
        if self.main_job == "dnc":
            # +15 Accuracy and Evasion when Closed Position is enabled. DNC Relic+3 feet handled later.
            self.add_stat("Accuracy", 15 * self.abilities.get("Closed Position", False))
            self.add_stat("Evasion", 15 * self.abilities.get("Closed Position", False))

        # Add +5% crit rate from merits and +5% and an additional 5% from the base crit chance
        self.add_stat("Crit Rate", 5 + 5)

        # Add +16 to all combat stats from merits.
        for stat in ALL_COMBAT_SKILLS:
            self.add_stat(stat, 16)

        # ===========================================================================
        # ===========================================================================
        # Add stats from job mastery, including job gifts and job point bonuses.
        # There could easily be typos here. These were all added manually by using ctrl+F on each job page for specific stat names within the Job Points sections.

        for stat in JOB_MASTERY_STATS[self.main_job]:
            self.stats[stat] = self.stats.get(stat,0) + JOB_MASTERY_STATS[self.main_job].get(stat,0)

        # ===========================================================================
        # ===========================================================================
        # Add job-specific stats from spells and abilities which are assumed to be active full-time.

        if self.main_job == "rdm": 
            temper2_ta = min(40, int((self.abilities.get("Enhancing Skill",0)-300)/10) if int((self.abilities.get("Enhancing Skill",0)-300)/10) > 0 else 0)  # Temper2 based on Enhancing Magic Skill https://www.bg-wiki.com/ffxi/Temper_II
            self.add_stat("TA", temper2_ta*self.abilities.get("Temper II", False))

        if self.main_job == "run":
            temper1_da = int((self.abilities.get("Enhancing Skill",0)-300)/10) if int((self.abilities.get("Enhancing Skill",0)-300)/10) > 0 else 0  # Temper1 based on Enhancing Magic Skill https://www.bg-wiki.com/ffxi/Temper
            temper1_da = 5 if temper1_da < 5 else temper1_da
            self.add_stat("DA", temper1_da*self.abilities.get("Temper",False))
        
        if "sch" in [self.main_job, self.sub_job]:
            self.stats["Elemental Magic Skill"] = max(self.stats.get("Elemental Magic Skill",0), 404+16) # Dark Arts enhances elemental magic skill to B+ rank, plus 16 from merits
            self.stats["Dark Magic Skill"] = max(self.stats.get("Dark Magic Skill",0), 404+16) # Dark Arts enhances dark magic skill to B+ rank, plus 16 from merits
            self.stats["Divine Magic Skill"] = max(self.stats.get("Divine Magic Skill",0), 404+16) # Light Arts enhances divine magic skill to B+ rank, plus 16 from merits

        if self.main_job == "blu": # "Zahak Reborn" spell set.
            self.add_stat("Accuracy", 48)
            self.add_stat("Ranged Accuracy", 48)
            self.add_stat("Magic Accuracy", 36)
            self.add_stat("Store TP", 30)
            self.add_stat("Dual Wield", 25)
            self.add_stat("TA", 5)
            self.add_stat("Crit Damage", 11)
            self.add_stat("Skillchain Bonus", 16)
            self.add_stat("STR", 5+4+3+2-3)
            self.add_stat("DEX", 8+6+2+4+4+1+8+4)
            self.add_stat("VIT", 4+7+4)
            self.add_stat("AGI", 5+2+1)
            self.add_stat("MND", 4+2)
            self.add_stat("INT", 4-1)
            self.add_stat("CHR", 1+5-2)

        if self.main_job == "drg": # Bonus stats for having a fully leveled Wyvern pet:
            self.wyvern_bonus_attack = True # This represents the +20% attack that will be applied later for having a wyvern out. Additive bonus with smite, berserk, chaos roll, etc.
            self.add_stat("Weapon Skill Damage Trait", 10)
            self.add_stat("JA Haste", 10)
            self.add_stat("DA", 15)
            self.add_stat("Attack", 40) # +40 attack for having a fully leveled Wyvern pet with 20/20 Job Points.

        if self.main_job == "nin":
            self.add_stat("Store TP", 10) # Kakka: Ichi
            self.add_stat("Subtle Blow", 10) # Myoshu: Ichi
            




if __name__ == "__main__":
    #
    #
    #
    import sys
    from gear import *
    main_job = sys.argv[1]
    sub_job = sys.argv[2]
    master_level = int(sys.argv[3])
    gearset: Gearset = { "main" : Heishi,
                "sub" : Kraken_Club,
                "ranged" : Empty,
                "ammo" : Date,
                "head" : Blistering_Sallet,
                "body" : Mpaca_Doublet30,
                "hands" : Hachiya_Tekko,
                "legs" : Jokushu_Haidate,
                "feet" : Mochizuki_Kyahan,
                "neck" : Ninja_Nodowa,
                "waist" : Fotia_Belt,
                "ear1" : Odr_Earring,
                "ear2" : Lugra_Earring_Aug,
                "ring1" : Regal_Ring,
                "ring2" : Gere_Ring,
                "back" : Empty}

    # Mirrors aggregate_buffs() output: metadata keys ("Name", "Type") are stripped, leaving numeric stats.
    Grape_Daifuku2 = {"STR":3, "VIT":4, "Food Attack":55, "Food Ranged Attack":55, "Accuracy":85, "Ranged Accuracy":85, "Magic Attack":4}
    buffs = {"food": Grape_Daifuku2,
             "brd": {"Attack": 0, "Accuracy": 0, "Ranged Accuracy": 0,"STR":0,"DEX":0, "VIT":0, "AGI":0, "INT":0, "MND":0, "CHR":0,},
             "cor": {"Attack%": 0., "Ranged Attack%":0., "Store TP": 0, "Accuracy": 0, "Magic Attack": 0, "DA":0, "Crit Rate": 0},
             "geo": {"Attack%": 0, "Ranged Attack%": 0, "Accuracy": 0, "Ranged Accuracy":0, "STR":0,"DEX":0, "VIT":0, "AGI":0, "INT":0, "MND":0, "CHR":0,},
             "whm": {"MDT":-29,"Magic Haste": 307./1024, "STR":0,"DEX":0, "VIT":0, "AGI":0, "INT":0, "MND":0, "CHR":0}, # WHM buffs like boost-STR.
             }
    abilities = {"Ebullience":False,
                        "Futae":False,
                        "Sneak Attack":False,
                        "Trick Attack":False,
                        "Footwork":False,
                        "Impetus":False,
                        "Building Flourish":False,
                        "Climactic Flourish":False,
                        "Striking Flourish":False,
                        "Ternary Flourish":False,
                        "Velocity Shot":False,
                        "Blood Rage":False,
                        "Last Resort":False}


    player = create_player(main_job, sub_job, master_level, gearset, buffs, abilities)
    

    print(f"\n\nPlayer stats list ({player.main_job_level}{player.main_job.upper()}/{player.sub_job_level}{player.sub_job.upper()})\n\n",player.stats)
