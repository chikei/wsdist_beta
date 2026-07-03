'''
File containing algorithm to automatically build and test gear sets for set finding.

Uses a partially-exhaustive search of all possible combinations of gear involving at most 2 swaps at a time.

Each WS set typically has one deep global minima that this algorithm finds relatively well.

Critical hit weapon skills (and those with Shining One equipped) can have two minima (one Crit build, one WS damage build). 
This algorithm may get caught in a crit build if starting from a crit build, but this only affects crit WSs.
    
Author: Kastra (Asura server)
'''
import os
import sys
from datetime import datetime # For timestamping new sets to put on BG Wiki
from dataclasses import dataclass
from typing import Any, cast

from create_player import *
from actions import *

from wsdist_types import Buffs, GearPiece, Gearset

# Use an external gear.py file
# https://stackoverflow.com/questions/47350078/importing-external-module-in-single-file-exe-created-with-pyinstaller
sys.path.append(os.path.dirname(sys.executable))
from gear import *

# Imported last so the numpy alias is not shadowed by the wildcard imports above.
import numpy as np


@dataclass(frozen=True)
class OptimizerOptions:
    iterations: int = 10
    max_swap_slots: int = 2
    restart_count: int = 1
    seed: int | None = None
    dt_step: int = 1


def _normalize_optimizer_options(options: OptimizerOptions | None) -> OptimizerOptions:
    if options is None:
        options = OptimizerOptions()
    return OptimizerOptions(
        iterations=max(1, options.iterations),
        max_swap_slots=1 if options.max_swap_slots <= 1 else 2,
        restart_count=max(1, options.restart_count),
        seed=options.seed,
        dt_step=max(1, options.dt_step),
    )


def _jse_ear_names() -> list[str]:
    names = ["Hattori", "Heathen's", "Lethargy", "Ebers", "Wicce", "Peltast's", "Boii", "Bhikku", "Skulker's", "Chevalier's", "Nukumi", "Fili", "Amini", "Kasuga", "Beckoner's", "Hashishin", "Chasseur's", "Karagoz", "Maculele", "Arbatel", "Azimuth", "Erilaz"]
    return [k + " Earring +1" for k in names] + [k + " Earring +2" for k in names]


def _copy_gearset(gearset: Gearset) -> Gearset:
    return {slot: piece for slot, piece in gearset.items()}


def _prefilter_check_gear(check_gear: dict[str, Any], main_job: str) -> dict[str, list[GearPiece]]:
    return {
        slot: [
            item
            for item in cast("list[GearPiece]", items)
            if main_job in item.jobs
        ]
        for slot, items in check_gear.items()
    }


def _fast_dt_for_gearset(gearset: Gearset, buffs: Buffs) -> tuple[float, float]:
    pdt = 0.0
    mdt = 0.0
    dt = 0.0
    pdt2 = 0.0
    mdt2 = 0.0
    dt2 = 0.0

    for piece in gearset.values():
        pdt += float(piece.stats.get("PDT", 0))
        mdt += float(piece.stats.get("MDT", 0))
        dt += float(piece.stats.get("DT", 0))
        pdt2 += float(piece.stats.get("PDT2", 0))
        mdt2 += float(piece.stats.get("MDT2", 0))
        dt2 += float(piece.stats.get("DT2", 0))

    for buff_stats in buffs.values():
        pdt += float(buff_stats.get("PDT", 0))
        mdt += float(buff_stats.get("MDT", 0))
        dt += float(buff_stats.get("DT", 0))
        pdt2 += float(buff_stats.get("PDT2", 0))
        mdt2 += float(buff_stats.get("MDT2", 0))
        dt2 += float(buff_stats.get("DT2", 0))

    total_pdt = max(-50.0, pdt + dt) + pdt2 + dt2
    total_mdt = max(-50.0, mdt + dt) + mdt2 + dt2
    return total_pdt, total_mdt


def _dt_for_player(player: "create_player") -> tuple[float, float]:
    pdt = player.stats.get("PDT", 0) + player.stats.get("DT", 0)
    mdt = player.stats.get("MDT", 0) + player.stats.get("DT", 0)
    pdt = -50 if pdt < -50 else pdt
    mdt = -50 if mdt < -50 else mdt
    pdt += player.stats.get("PDT2", 0) + player.stats.get("DT2", 0)
    mdt += player.stats.get("MDT2", 0) + player.stats.get("DT2", 0)
    return pdt, mdt


def _can_use_fast_dt_gate(abilities: dict[str, Any]) -> bool:
    return int(abilities.get("Aftermath", 0) or 0) == 0


def _slot_pairs(check_slots: list[str], max_swap_slots: int) -> list[tuple[int, str, int, str]]:
    pairs: list[tuple[int, str, int, str]] = []
    for i1, slot1 in enumerate(check_slots):
        for i2, slot2 in enumerate(check_slots):
            if i2 < i1:
                continue
            if max_swap_slots == 1 and i2 != i1:
                continue
            pairs.append((i1, slot1, i2, slot2))
    return pairs


def _is_valid_gearset(test_set: Gearset, main_job: str, sub_job: str, action_type: str, ws_name: str, spell_name: str, ws_dict: dict[str, list[str]], restricted_ws: dict[str, str], jse_ears: list[str]) -> bool:
    if (test_set["ring1"] == test_set["ring2"]) and (test_set["ring1"].name != "Empty"):
        return False
    if (test_set["ear1"] == test_set["ear2"]) and (test_set["ear1"].name != "Empty"):
        return False
    if (test_set["main"] == test_set["sub"]) and (test_set["main"].name != "Empty"):
        return False

    one_handed = ["Axe", "Club", "Dagger", "Sword", "Katana"]
    if (test_set["main"].skill_type in one_handed) and (test_set["sub"].type == "Grip"):
        return False

    two_handed = ["Great Sword", "Great Katana", "Great Axe", "Polearm", "Scythe", "Staff"]
    if (test_set["main"].skill_type in two_handed) and (test_set["sub"].type == "Weapon" or test_set["sub"].type == "Shield"):
        return False

    if (test_set["main"].skill_type == "Hand-to-Hand") and (test_set["sub"].name != "Empty"):
        return False

    archery = ["Empyreal Arrow", "Flaming Arrow", "Namas Arrow", "Jishnu's Radiance", "Apex Arrow", "Refulgent Arrow", "Sidewinder", "Blast Arrow", "Piercing Arrow"]
    marksmanship = ["Last Stand", "Hot Shot", "Leaden Salute", "Wildfire", "Coronach", "Trueflight", "Detonator", "Blast Shot", "Slug Shot", "Split Shot"]
    if (action_type == "weapon skill") and (ws_name in archery + marksmanship):
        if (ws_name in archery) and (test_set["ranged"].skill_type != "Archery" or test_set["ammo"].type != "Arrow"):
            return False
        if (ws_name in marksmanship) and (test_set["ranged"].skill_type != "Marksmanship" or test_set["ammo"].type not in ["Bolt", "Bullet"]):
            return False
        if (test_set["ranged"].type == "Crossbow") and (test_set["ammo"].type != "Bolt"):
            return False
        if (test_set["ranged"].type == "Gun") and (test_set["ammo"].type != "Bullet"):
            return False

    if (action_type == "spell cast") and (spell_name == "Ranged Attack"):
        if (test_set["ranged"].type not in ["Gun", "Bow", "Crossbow"]) or (test_set["ammo"].type not in ["Bullet", "Arrow", "Bolt"]):
            return False

    if (test_set["ranged"].type == "Gun") and (test_set["ammo"].type not in ["Bullet", "None"]):
        return False
    if (test_set["ranged"].type == "Bow") and (test_set["ammo"].type not in ["Arrow", "None"]):
        return False
    if (test_set["ranged"].type == "Crossbow") and (test_set["ammo"].type not in ["Bolt", "None"]):
        return False

    if (test_set["ammo"].type == "Bullet") and (test_set["ranged"].type != "Gun"):
        return False
    if (test_set["ammo"].type == "Arrow") and (test_set["ranged"].type != "Bow"):
        return False
    if (test_set["ammo"].type == "Bolt") and (test_set["ranged"].type != "Crossbow"):
        return False

    if (test_set["ranged"].type == "Instrument") and (test_set["ammo"].type != "None"):
        return False

    if (main_job not in ["nin", "dnc", "thf", "blu"] and sub_job not in ["nin", "dnc"]) and (test_set["sub"].type == "Weapon"):
        return False

    if (test_set["ear1"].name in jse_ears) and (test_set["ear2"].name == "Balder Earring +1"):
        return False
    if (test_set["ear2"].name in jse_ears) and (test_set["ear1"].name == "Balder Earring +1"):
        return False

    if (test_set["body"].name in ["Cohort Cloak", "Cohort Cloak +1", "Crepuscular Cloak", "Twilight Cloak"]) and (test_set["head"].name != "Empty"):
        return False

    if action_type == "spell cast":
        if (spell_name == "Impact") and (test_set["body"].name not in ["Crepuscular Cloak", "Twilight Cloak"]):
            return False

    if action_type == "weapon skill":
        if ws_name in restricted_ws:
            if (restricted_ws[ws_name] != test_set["main"].name) and (restricted_ws[ws_name] != test_set["ranged"].name):
                return False

        ws_on_main = ws_name in ws_dict.get(test_set["main"].skill_type, [])
        ws_on_ranged = ws_name in ws_dict.get(test_set["ranged"].skill_type, [])
        if (not ws_on_main) and (not ws_on_ranged):
            return False

    return True


def _evaluate_metric(player: "create_player", enemy: "create_enemy", ws_name: str, spell_name: str, action_type: str, min_tp: float, ws_type: str, spell_type: str, input_metric: str) -> tuple[float, list[Any], int, int, int]:
    if action_type == "weapon skill":
        decimals = 1
        nondecimals = 8
        metric_base, output = average_ws(player, enemy, ws_name, min_tp, ws_type, input_metric)
        invert = output[-1]
        metric = metric_base**invert
    elif action_type == "spell cast":
        decimals = 1
        nondecimals = 8
        metric_base, output = cast_spell(player, enemy, spell_name, spell_type, input_metric)
        invert = output[-1]
        metric = metric_base**invert
    elif action_type == "attack round":
        decimals = 3
        nondecimals = 8
        metric_base, output, _ = average_attack_round(player, enemy, 0, min_tp, input_metric)
        invert = output[-1]
        metric = metric_base**invert
    else:
        raise ValueError(f"Unknown action_type ({action_type})")

    metric = 0.0001 if metric <= 0 else metric
    return metric, output, invert, decimals, nondecimals


def _single_start_options(options: OptimizerOptions, restart_index: int) -> OptimizerOptions:
    return OptimizerOptions(
        iterations=options.iterations,
        max_swap_slots=options.max_swap_slots,
        restart_count=1,
        seed=None if options.seed is None else options.seed + restart_index,
        dt_step=options.dt_step,
    )


def _copy_check_gear(check_gear: dict[str, Any]) -> dict[str, list[GearPiece]]:
    return {slot: list(cast("list[GearPiece]", items)) for slot, items in check_gear.items()}

def format_bgwiki(ws_name: str, tp: float, player: "create_player", best_metric: Any) -> None:
    #
    # Input: A player class containing job and gear info.
    # Output: None
    #
    # Prints to the terminal the player gearset in BG Wiki format, ignoring augments.
    #
    buffs = "High"


    # Certain items have shortened names on BG Wiki. Use the item_list.txt file to find and replace these names for BG Wiki.
    item_list = np.loadtxt("item_list.csv", unpack=False, dtype=str, delimiter=';', usecols=(1,2), skiprows=1)
    name_map = {k[0].lower():k[1] for k in item_list}

    backaugs: list[str] = []
    for stat in player.gearset["back"].stats:
        if stat.lower() in ["str","dex","vit","agi","int","mnd","chr","da","store tp","dual wield","crit rate","weapon skill damage", "magic attack"]:
            backaugs.append(stat)

    linosaugs: list[str] = []
    for stat in player.gearset["ranged"].stats:
        if stat.lower() in ["str","dex","vit","agi","int","mnd","chr","da","store tp","dual wield","crit rate","weapon skill damage", "magic attack","qa","da","ta"]:
            linosaugs.append(stat)

    # Moonshade natually looks best in the left ear slot.
    if "moonshade" in player.gearset["ear2"].name.lower():
        ear2 = player.gearset["ear2"]
        ear1 = player.gearset["ear1"]
        player.gearset["ear1"] = ear2
        player.gearset["ear2"] = ear1

    # JSE earrings work in the right ear slot
    jse_ears1 = [k + " Earring +1" for k in ["Hattori", "Heathen's", "Lethargy", "Ebers", "Wicce", "Peltast's", "Boii", "Bhikku", "Skulker's", "Chevalier's", "Nukumi", "Fili", "Amini", "Kasuga", "Beckoner's", "Hashishin", "Chasseur's", "Karagoz", "Maculele", "Arbatel", "Azimuth", "Erilaz"]]
    jse_ears2 = [k + " Earring +2" for k in ["Hattori", "Heathen's", "Lethargy", "Ebers", "Wicce", "Peltast's", "Boii", "Bhikku", "Skulker's", "Chevalier's", "Nukumi", "Fili", "Amini", "Kasuga", "Beckoner's", "Hashishin", "Chasseur's", "Karagoz", "Maculele", "Arbatel", "Azimuth", "Erilaz"]]
    if player.gearset["ear1"].name2 in jse_ears1 or player.gearset["ear1"].name2 in jse_ears2:
        ear2 = player.gearset["ear2"]
        ear1 = player.gearset["ear1"]
        player.gearset["ear1"] = ear2
        player.gearset["ear2"] = ear1

    # Do it again because the above doesn't always work??
    empy = ["Hattori", "Heathen", "Lethargy", "Eber", "Wicce", "Peltast", "Boii", "Bhikku", "Skulker", "Chevalier", "Nukumi", "Fili", "Amini", "Kasuga", "Beckoner", "Hashishin", "Chasseur", "Karagoz", "Maculele", "Arbatel", "Azimuth", "Erilaz"]
    for name in empy:
        if name.lower() in player.gearset["ear1"].name.lower():
            ear2 = player.gearset["ear2"]
            ear1 = player.gearset["ear1"]
            player.gearset["ear1"] = ear2
            player.gearset["ear2"] = ear1

    # Epami looks best in the left ring slot, but only if sroda is not also equipped.
    if "epami" in player.gearset["ring2"].name.lower():
        if "sroda" not in player.gearset["ring1"].name.lower():
            ring2 = player.gearset["ring2"]
            ring1 = player.gearset["ring1"]
            player.gearset["ring1"] = ring2
            player.gearset["ring2"] = ring1

    # Sroda looks best in the left ring slot.
    if "sroda" in player.gearset["ring2"].name.lower():
            ring2 = player.gearset["ring2"]
            ring1 = player.gearset["ring1"]
            player.gearset["ring1"] = ring2
            player.gearset["ring2"] = ring1

    # Niqmaddu and Regal look best in the right ring slot
    if ("niqmaddu" in player.gearset["ring1"].name.lower() and "regal" not in player.gearset["ring2"].name.lower()) or ("regal" in player.gearset["ring1"].name.lower() and "niqmaddu" not in player.gearset["ring2"].name.lower()):
            ring2 = player.gearset["ring2"]
            ring1 = player.gearset["ring1"]
            player.gearset["ring1"] = ring2
            player.gearset["ring2"] = ring1

    # player.gearset[slot]["Name"] = name_map[player.gearset[slot]["Name"].lower()]

    hardcode_gearset = {slot:name_map[player.gearset[slot].name.lower()] for slot in player.gearset}
    for slot in hardcode_gearset:
        hardcode_gearset[slot] = "" if hardcode_gearset[slot].lower()=="empty" else hardcode_gearset[slot]


            # |RangeAug = {", ".join(linosaugs)}
    bgwiki_text = f"""
    {'{'}{'{'}
        Guide Equipment Set
        |Set Name Background=#604028
        |Set Name Text Color=
        |Set Name Text Shadow=#000080
        |Set Name= {ws_name}[[{ws_name}|*]]
        |Set Border Color=#51414F
        |Equipment Set=
        {'{'}{'{'}
            Equipment Set
            |CaptionTop = {buffs} buff
            |CaptionBottom = {best_metric:.0f} damage
            |Main = {' '.join(k.capitalize() for k in hardcode_gearset["main"].split())} (Level 119 III)
            |Sub = {' '.join(k.capitalize() for k in hardcode_gearset["sub"].split())}
            |Range = {' '.join(k.capitalize() for k in hardcode_gearset["ranged"].split())}
            |Ammo = {' '.join(k.capitalize() for k in hardcode_gearset["ammo"].split())}
            |Head = {' '.join(k.capitalize() for k in hardcode_gearset["head"].split())}
            |Neck = {' '.join(k.capitalize() for k in hardcode_gearset["neck"].split())}
            |Ear1 = {' '.join(k.capitalize() for k in hardcode_gearset["ear1"].split())}
            |Ear2 = {' '.join(k.capitalize() for k in hardcode_gearset["ear2"].split())}
            |Body = {' '.join(k.capitalize() for k in hardcode_gearset["body"].split())}
            |Hands = {' '.join(k.capitalize() for k in hardcode_gearset["hands"].split())}
            |Ring1 = {' '.join(k.capitalize() for k in hardcode_gearset["ring1"].split())}
            |Ring2 = {' '.join(k.capitalize() for k in hardcode_gearset["ring2"].split())}
            |Back = {' '.join(k.capitalize() for k in hardcode_gearset["back"].split())}
            |BackAug = {", ".join(backaugs)}
            |Waist = {' '.join(k.capitalize() for k in hardcode_gearset["waist"].split())}
            |Legs = {' '.join(k.capitalize() for k in hardcode_gearset["legs"].split())}
            |Feet = {' '.join(k.capitalize() for k in hardcode_gearset["feet"].split())}
            |List = Y
            |Background =
        {'}'}{'}'}
        |Equipment Set Notes=ML{player.master_level} {player.main_job.upper()}/{player.sub_job.upper()}: {int(tp)} TP
        Updated {datetime.now().strftime("%Y %b. %d")}
    {'}'}{'}'}\n
    """
    print(bgwiki_text)

def build_set(main_job: str, sub_job: str, master_level: int, buffs: Buffs, abilities: dict[str, Any], enemy: "create_enemy", ws_name: str, spell_name: str, action_type: str, min_tp: float, check_gear: dict[str, Any], starting_gearset: Gearset, pdt_requirement: float, mdt_requirement: float, input_metric: str, print_swaps: bool, next_best_percent: float, optimizer_options: OptimizerOptions | None = None, ) -> tuple[Any, Any]:
    #
    # Build a valid gear set, test it, and return the best set found.
    #
    # action_type = "ranged attack", "weapon skill", "tp round", "spell cast"
    #
    main_job = main_job.lower()
    sub_job = sub_job.lower()
    options = _normalize_optimizer_options(optimizer_options)
    if options.seed is not None:
        np.random.seed(options.seed)
    n_iter = options.iterations

    # Defaults for values otherwise only set inside the optimization loop / branches.
    best_metric = 0.0001
    best_output: list[Any] = [0.0, 0.0, 0.0]
    invert = 1
    decimals = 1
    nondecimals = 8
    swaps: dict[str, list[Any]] = {}

    verbose_swaps = abilities.get("Verbose Swaps", False)
    use_fast_dt_gate = _can_use_fast_dt_gate(abilities)

    ws_dict = {"Katana": ["Blade: Retsu", "Blade: Teki", "Blade: To", "Blade: Chi", "Blade: Ei", "Blade: Jin", "Blade: Ten", "Blade: Ku", "Blade: Yu", "Blade: Metsu", "Blade: Kamu", "Blade: Hi", "Blade: Shun", "Zesho Meppo",],
        "Great Katana": ["Tachi: Enpi", "Tachi: Goten", "Tachi: Kagero", "Tachi: Jinpu", "Tachi: Koki","Tachi: Yukikaze", "Tachi: Gekko", "Tachi: Kasha", "Tachi: Ageha","Tachi: Kaiten", "Tachi: Rana", "Tachi: Fudo", "Tachi: Shoha", "Tachi: Mumei"],
        "Dagger": [ "Viper Bite", "Dancing Edge", "Shark Bite", "Evisceration", "Aeolian Edge", "Mercy Stroke", "Mandalic Stab", "Mordant Rime", "Pyrrhic Kleos", "Rudra's Storm", "Exenterator", "Ruthless Stroke"],
        "Sword": ["Fast Blade", "Fast Blade II", "Burning Blade", "Red Lotus Blade", "Seraph Blade", "Circle Blade", "Swift Blade", "Savage Blade", "Sanguine Blade", "Knights of Round", "Death Blossom", "Expiacion", "Chant du Cygne", "Requiescat", "Imperator"],
        "Scythe": ["Slice", "Dark Harvest", "Shadow of Death", "Nightmare Scythe", "Spinning Scythe", "Guillotine", "Cross Reaper", "Spiral Hell", "Infernal Scythe", "Catastrophe", "Quietus", "Insurgency", "Entropy", "Origin", ], 
        "Great Sword":["Hard Slash", "Freezebite", "Shockwave", "Sickle Moon", "Spinning Slash", "Ground Strike", "Herculean Slash", "Resolution", "Scourge", "Dimidiation", "Torcleaver", "Fimbulvetr", ], 
        "Club":["Shining Strike", "Seraph Strike", "Skullbreaker", "True Strike", "Judgment", "Hexa Strike", "Black Halo", "Randgrith", "Exudation", "Mystic Boon", "Realmrazer", "Dagda"], 
        "Polearm":["Double Thrust", "Thunder Thrust", "Raiden Thrust", "Penta Thrust", "Wheeling Thrust", "Impulse Drive", "Sonic Thrust", "Geirskogul", "Drakesbane", "Camlann's Torment", "Stardiver", "Diarmuid", ], 
        "Staff":["Heavy Swing", "Rock Crusher", "Earth Crusher", "Starburst", "Sunburst", "Shell Crusher", "Full Swing", "Cataclysm", "Retribution", "Gate of Tartarus", "Omniscience", "Vidohunir", "Garland of Bliss", "Shattersoul", "Oshala"], 
        "Great Axe":["Iron Tempest", "Shield Break", "Armor Break", "Weapon Break", "Raging Rush", "Full Break", "Steel Cyclone", "Fell Cleave", "Metatron Torment", "King's Justice", "Ukko's Fury", "Upheaval", "Disaster"], 
        "Axe":["Raging Axe", "Spinning Axe", "Rampage", "Calamity", "Mistral Axe", "Decimation", "Bora Axe", "Onslaught", "Primal Rend", "Cloudsplitter", "Ruinator", "Blitz", ], 
        "Archery":["Flaming Arrow", "Piercing Arrow", "Dulling Arrow", "Sidewinder", "Blast Arrow", "Empyreal Arrow", "Refulgent Arrow", "Namas Arrow", "Jishnu's Radiance", "Apex Arrow", "Sarv"], 
        "Marksmanship":["Hot Shot", "Split Shot", "Sniper Shot", "Slug Shot", "Blast Shot", "Detonator", "Coronach", "Leaden Salute", "Trueflight", "Wildfire", "Last Stand", "Terminus", ], 
        "Hand-to-Hand":["Combo","One Inch Punch","Raging Fists","Spinning Attack","Howling Fist","Dragon Kick","Asuran Fists","Tornado Kick","Ascetic's Fury","Stringing Pummel","Final Heaven","Victory Smite","Shijin Spiral","Maru Kala","Dragon Blow",],
        }

    melee_ws = [ws for skill in ws_dict if skill not in ["Archery","Marksmanship"] for ws in ws_dict[skill]]
    ranged_ws = [ws for skill in ws_dict if skill in ["Archery","Marksmanship"] for ws in ws_dict[skill]]
    
    ws_type = "melee" if ws_name in melee_ws else "ranged" if ws_name in ranged_ws else "None"
    
    if " Shot" in spell_name:
        spell_type = "Quick Draw"
    elif spell_name=="Ranged Attack":
        spell_type="Ranged Attack"
    elif (": Ichi" in spell_name) or (": Ni" in spell_name) or (": San" in spell_name):
        spell_type = "Ninjutsu"
    else:
        spell_type = "Elemental Magic"

    if options.restart_count > 1:
        best_restart_player: Any = None
        best_restart_output: list[Any] = []
        best_restart_metric = 0.0001
        for restart_index in range(options.restart_count):
            restart_player, restart_output = build_set(
                main_job,
                sub_job,
                master_level,
                buffs,
                abilities,
                enemy,
                ws_name,
                spell_name,
                action_type,
                min_tp,
                _copy_check_gear(check_gear),
                _copy_gearset(starting_gearset),
                pdt_requirement,
                mdt_requirement,
                input_metric,
                print_swaps,
                next_best_percent,
                _single_start_options(options, restart_index),
            )
            restart_metric, _, _, _, _ = _evaluate_metric(restart_player, enemy, ws_name, spell_name, action_type, min_tp, ws_type, spell_type, input_metric)
            if restart_metric > best_restart_metric:
                best_restart_metric = restart_metric
                best_restart_player = restart_player
                best_restart_output = restart_output

        return best_restart_player, best_restart_output

    # List of weapon skills and their associated weapons.
    restricted_ws = {"Blade: Metsu":"Kikoku",
                    "Final Heaven":"Spharai",
                    "Mercy Stroke":"Mandau",
                    "Knights of Round":"Excalibur",
                    "Scourge":"Ragnarok",
                    "Onslaught":"Guttler",
                    "Metatron Torment":"Bravura",
                    "Catastrophe":"Apocalypse",
                    "Geirskogul":"Gungnir",
                    "Tachi: Kaiten":"Amanomurakumo",
                    "Randgrith":"Mjollnir",
                    "Gate of Tartarus":"Claustrum",
                    "Namas Arrow":"Yoichinoyumi",
                    "Coronach":"Annihilator",
                    "Fast Blade II":"Onion Sword III",
                    "Dragon Blow":"Dragon Fangs",
                    "Imperator":"Caliburnus",
                    "Zesho Meppo":"Dokoku",
                    "Terminus":"Earp",
                    "Origin":"Foenaria",
                    "Diarmuid":"Gae Buide",
                    "Fimbulvetr":"Helheim",
                    "Tachi: Mumei":"Kusanagi-no-Tsurugi",
                    "Disaster":"Laphria",
                    "Dagda":"Lorg Mor",
                    "Ruthless Stroke":"Mpu Gandring",
                    "Oshala":"Opashoro",
                    "Sarv":"Pinaka",
                    "Blitz":"Spalirisos",
                    "Maru Kala":"Varga Purnikawa",
                    }

    # If testing a melee WS, only check instruments in the "ranged" slot.
    # This does not apply to RNG or COR who might want savage blade sets to test gun/bow options
    if ws_type=="melee" and main_job not in ["rng", "cor"]:
        check_gear["ranged"] = [k for k in check_gear["ranged"] if k.type not in ["Crossbow", "Gun", "Bow"]]
        check_gear["ammo"] = [k for k in check_gear["ammo"] if k.type not in ["Bolt", "Bullet", "Arrow"] and "antitail" not in k.name2]

    check_gear = _prefilter_check_gear(check_gear, main_job.lower())

    # Rather than start with an empty slot, randomly build a set from the selected gear so we likely start with some accuracy+ and avoid getting stuck.
    # Do not adjust slots that are not being checked.
    for slot in starting_gearset:

        # Unequip gear you can't wear if it's already equipped, even if the slot is "frozen"
        if main_job.lower() not in starting_gearset[slot].jobs:
            starting_gearset[slot] = Empty

        frozen_slot = (len(check_gear[slot]) == 0)
        if not frozen_slot:
            starting_gearset[slot] = np.random.choice(check_gear[slot])
            
            # Avoid wearing two rare items in initial gearset to prevent "unphysical" sets.
            if slot == "ring2" and (starting_gearset["ring1"].name2 == starting_gearset["ring2"].name2):
                starting_gearset["ring2"] = Empty
            if slot == "ear2" and (starting_gearset["ear1"].name2 == starting_gearset["ear2"].name2):
                starting_gearset["ear2"] = Empty


    best_set =  starting_gearset.copy()

    # Define JSE earrings now. We'll use them later to prevent Balder's Earring+1 and a JSE+2 being equipped at the same time since we ignore right_ear requirement for testing.
    jse_ears = _jse_ear_names()

    pdt = 200 # How much PDT the set has
    mdt = 200

    conditional_converge_count = 0 # Break out of the loop if converged.
    pdt_old = 200 # Used to check if the automatic set finder gets stuck trying to find a set that doesn't exist. Compare this value to the old value. If no change in 3 consecutive iterations, then break out.
    mdt_old = 200

    pdt_thresh = pdt_requirement # How much PDT the final set is aiming for, taken from the user input.
    mdt_thresh = mdt_requirement

    pdt_thresh_temp = 200 # How much PDT the current new set must have to be accepted. The starting values are high to ensure that the code enters the loop to begin with.
    mdt_thresh_temp = 200


    while pdt > pdt_thresh or mdt > mdt_thresh:
        # print(f"\nChecking conditions: PDT:{pdt_thresh_temp},  MDT:{mdt_thresh_temp}")

        # Reset some variables between PDT/MDT iterations.

        best_metric = 0.0001 # Metric used to find the best set. This part of the code exclusively looks for "highest number".

        for z in range(n_iter):
            print(f"Current iteration: {z+1}")
            
            converged_set = best_set.copy() # The loop is considered converged if the set after a full iteration is the same as this set (no improvements were found).

            # A list of items in each slot that are within some % of the best item in that slot.
            swaps = {"ammo":[],"head":[],"neck":[],"ear1":[],"ear2":[],"body":[],"hands":[],"ring1":[],"ring2":[],"waist":[],"legs":[],"feet":[]}

            # Randomize the order that we check gear slots in
            check_slots: list[str] = list(check_gear)
            np.random.shuffle(check_slots)

            # For now, the code supports one- or two-slot swaps. Larger neighborhoods
            # need a different candidate strategy because the item-pair count grows fast.
            for _, slot1, _, slot2 in _slot_pairs(check_slots, options.max_swap_slots):
                # Randomize the order that the gear in each slot is checked.
                np.random.shuffle(check_gear[slot1])
                np.random.shuffle(check_gear[slot2])

                for item1 in check_gear[slot1]:
                    for item2 in check_gear[slot2]:
                        if (slot1 == slot2) and (item1 != item2): # Do not try to equip two different items in the same slot.
                            continue

                        if (item1 == best_set[slot1]) or (item2 == best_set[slot2]): # If an item is already equipped in one of the slots, then skip the iteration. I let the "item1==item2" cases handle single-item swaps.
                            continue

                        test_set = _copy_gearset(best_set)
                        test_set[slot1] = item1
                        test_set[slot2] = item2

                        if not _is_valid_gearset(test_set, main_job, sub_job, action_type, ws_name, spell_name, ws_dict, restricted_ws, jse_ears):
                            continue

                        if use_fast_dt_gate:
                            pdt, mdt = _fast_dt_for_gearset(test_set, buffs)
                            if pdt > pdt_thresh_temp or mdt > mdt_thresh_temp:
                                continue

                        player = create_player(main_job, sub_job, master_level, test_set, buffs, abilities)
                        pdt, mdt = _dt_for_player(player)
                        if pdt > pdt_thresh_temp or mdt > mdt_thresh_temp:
                            continue

                        metric, output, invert, decimals, nondecimals = _evaluate_metric(player, enemy, ws_name, spell_name, action_type, min_tp, ws_type, spell_type, input_metric)
                        if metric > best_metric:
                            if item1 == item2:
                                print(f"[{slot1:<15s}]: [{best_set[slot1].name2} ->  {item1.name2}   [{best_metric**invert:>{nondecimals}.{decimals}f} -> {metric**invert:>{nondecimals}.{decimals}f}]") if verbose_swaps else None
                                best_set[slot1] = item1
                            else:
                                print(f"[{slot1:<6s} & {slot2:<6s}]: [{best_set[slot1].name2} & {best_set[slot2].name2}] -> [{item1.name2} & {item2.name2}] [{best_metric**invert:>{nondecimals}.{decimals}f} -> {metric**invert:>{nondecimals}.{decimals}f}]") if verbose_swaps else None
                                best_set[slot1] = item1
                                best_set[slot2] = item2
                            best_metric = metric
                            best_output = output

                        elif item1 == item2:
                            try:
                                if (best_metric%metric / best_metric < (float(next_best_percent)/100)) and (slot1 not in ["main","sub","ranged","back"]):
                                    swaps[slot1].append([item1.name2, metric**invert])
                            except Exception:
                                # print(f"Error on \"{item1.name2}\" - Metric = {metric}  - Best Metric = {best_metric}")
                                pass

            if best_set==converged_set: # If no improvement is found after one full iteration.
                # best_player = create_player(main_job, sub_job, master_level, best_set, buffs, abilities)
                # for k in best_player.gearset:
                #     print(k,best_player.gearset[k]["Name2"])
                # print(best_output)
                break # Break out of the main loop and check PDT/MDT conditions.

        best_player0 = create_player(main_job, sub_job, master_level, best_set, buffs, abilities)
        pdt, mdt = _dt_for_player(best_player0)

        # Compare the pdt and mdt values from this iteration with the previous iteration.
        if pdt == pdt_old and mdt == mdt_old:
            conditional_converge_count += 1
            if conditional_converge_count >= 3:
                print("Unable to find a set which satisfies the conditions better than the current set. Exiting.")
                break
        else:
            conditional_converge_count = 0

        # Save the PDT and MDT values from this iteration to compare with the next iteration.
        pdt_old = pdt
        mdt_old = mdt

        # Update the temporary PDT and MDT requirements so that the next set is slightly closer to the true requirements.
        pdt_thresh_temp = pdt - options.dt_step if pdt-options.dt_step > pdt_thresh else pdt_thresh
        mdt_thresh_temp = mdt - options.dt_step if mdt-options.dt_step > mdt_thresh else mdt_thresh
        
        print(f"Current best set: PDT:{pdt},  MDT:{mdt}")


    # At this point, we've found the best conditional set.

    # Swap the earrings to make sure the "Right Ear:" effect earrings show up in the ear2 slot.
    if best_set["ear1"].name in jse_ears+["Balder Earring +1"]:
        best_set["ear1"],best_set["ear2"] = best_set["ear2"],best_set["ear1"]

    # Record the stats for the best gear set.
    best_player = create_player(main_job, sub_job, master_level, best_set, buffs, abilities)


    header = {"weapon skill":ws_name,"spell cast":spell_name,"attack round":"Melee TP set"}[action_type]
    # Print a fancy output.
    print("==============================================================")
    print(f"Best   \"{input_metric}\"   \"{header}\"   set")
    print("==============================================================")
    for k in best_player.gearset:
        print(f"{k:>10s}  {best_player.gearset[k].name2:<50s}")
    print()
    if action_type=="attack round":
        if input_metric=="Time to WS":
            print(f"Avg WS Time = {best_metric**invert:<{nondecimals}.{decimals}f} s")
            print(f"Avg TP per round = {best_output[1]:<5.1f} TP")
        else:
            print(f"Avg Damage per round = {best_output[0]:<{nondecimals}.{decimals}f} damage")
            print(f"Avg time per round = {best_output[2]:<5.1f} s")
            print(f"Avg TP per round = {best_output[1]:<5.1f} TP")
    else:
        print(f"Avg Damage = {best_output[0]:<{nondecimals}.{decimals}f} damage")
        print(f"Avg TP return = {best_output[1]:<5.1f} TP")
    print("==============================================================")
    print("==============================================================")

    if print_swaps:
        print(f"\nList of potential swaps within {next_best_percent}% of the best set ({float(best_metric)**invert:<{nondecimals}.{decimals}f}):")
        for slot in swaps:
            for swap in swaps[slot]:
                line = f"{slot:<6s} {swap[0]:<50s} {float(swap[1]):<{nondecimals}.{decimals}f} {best_metric%swap[1]**invert/best_metric*100:>5.1f}%"
                print(line)

    # Print additional output formatted for BG Wiki item sets.
    if False:
        format_bgwiki(ws_name, (min_tp), best_player, best_metric)

    return(best_player, best_output)

if __name__ == "__main__":

    main_job = sys.argv[1]
    sub_job = sys.argv[2]
    master_level = int(sys.argv[3])
    buffs: dict[str, Any] = {}
    abilities: dict[str, Any] = {}
    enemy = create_enemy(preset_enemies["Apex Toad"])
    ws_name = "Blade: Metsu"
    spell_name = "Waterja"
    action_type = "weapon skill"
    min_tp = 1000
    check_gear = gear_dict
    starting_gearset: Gearset = { "main" : Heishi,
                        'sub' : Crepuscular_Knife,
                        'ranged' : Empty,
                        'ammo' : Seki,
                        'head' : Malignance_Chapeau,
                        'body' : Tatenashi_Haramaki,
                        'hands' : Malignance_Gloves,
                        'legs' : Samnuha_Tights,
                        'feet' : Malignance_Boots,
                        'neck' : Ninja_Nodowa,
                        'waist' : Sailfi_Belt,
                        'ear1' : Dedition_Earring,
                        'ear2' : Telos_Earring,
                        'ring1' : Gere_Ring,
                        'ring2' : Epona_Ring,
                        'back' : np.random.choice(cast(Any, [k for k in capes if "nin" in k.jobs and "DEX Store TP" in k.name2 and "Ranged" not in k.stats]))}
    pdt_requirement = -50
    mdt_requirement = -21
    print_swaps = True
    next_best_percent = 1

    metric = "Damage Dealt"

    player, output = build_set(main_job, sub_job, master_level, buffs, abilities, enemy, ws_name, spell_name, action_type, min_tp, check_gear, starting_gearset, pdt_requirement, mdt_requirement, metric, print_swaps, next_best_percent)
    print(player.stats)


    # TODO: If hit rate is < 20% in initial set, then begin by finding and equipping the max accuracy piece in each slot before finding the best set.
