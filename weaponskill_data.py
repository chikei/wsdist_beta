'''
Data-driven weapon skill table backing weaponskill_info().

Author: Kastra (Asura server)

Each weapon skill is described by a dictionary of properties (see WS_TABLE
below).  Static properties (nhits, ftp, element, ...) are stored directly.
Anything that depends on TP, the player, or the enemy is expressed as a small
callable "hook" so the dynamic logic from the original if/elif chain is
preserved exactly.

The shared anchor points used for TP interpolation.
'''
import numpy as np
from get_dex_crit import get_dex_crit

base_tp = [1000, 2000, 3000] # TP anchor points used for interpolation.


class WSContext:
    '''Mutable bag of the per-weapon-skill values that the original function
    built up inside its giant if/elif chain.  Hooks read/write these fields and
    the final scaling dict is assembled from them.'''

    def __init__(self, tp, player, enemy):
        self.tp = tp
        self.player = player
        self.enemy = enemy

        self.player_str = player.stats.get("STR", 0)
        self.player_dex = player.stats.get("DEX", 0)
        self.player_vit = player.stats.get("VIT", 0)
        self.player_agi = player.stats.get("AGI", 0)
        self.player_int = player.stats.get("INT", 0)
        self.player_mnd = player.stats.get("MND", 0)
        self.player_chr = player.stats.get("CHR", 0)

        self.player_accuracy1 = player.stats.get("Accuracy1", 0)
        self.player_accuracy2 = player.stats.get("Accuracy2", 0)
        self.player_rangedaccuracy = player.stats.get("Ranged Accuracy", 0)

        self.player_attack1 = player.stats.get("Attack1", 0)
        self.player_attack2 = player.stats.get("Attack2", 0)
        self.player_rangedattack = player.stats.get("Ranged Attack", 0)

        self.enemy_basedef = enemy.stats["Base Defense"]
        self.enemy_def = enemy.stats["Defense"]
        self.enemy_vit = enemy.stats["VIT"]
        self.enemy_agi = enemy.stats["AGI"]
        self.enemy_int = enemy.stats["INT"]
        self.enemy_mnd = enemy.stats["MND"]
        self.enemy_chr = enemy.stats["MND"]

        self.crit_rate = 0 # Start from zero crit rate. We add crit rate if the weapon skill is a crit weapon skill and/or if Shining One is equipped.

        # Set default values. We will modify these for special cases.
        self.crit_ws = False # Used to ensure that we do not double-count dDEX on crit WSs when using Shining One.
        self.hybrid = False
        self.magical = False
        self.dSTAT = 0 # Magical WS dINT or dMND
        self.element = "None" # Magical/Hybrid WS element
        self.ftp_hybrid = 0 # Hybrid WSs have unique FTP rules.

        # Populated from the WS table for each weapon skill.
        self.ftp = 0
        self.ftp_rep = False
        self.wsc = 0
        self.nhits = 0


def interp(tp, points):
    '''Interpolate a 3-point [1k, 2k, 3k] table at the current TP.'''
    return np.interp(tp, base_tp, points)


def crit_hook(crit_boost):
    '''Build a hook for the standard "critical hit" weapon skills (melee).

    These add gear crit rate, a TP-scaled crit bonus, and dDEX-based crit rate.
    '''
    def apply(ctx):
        ctx.crit_ws = True
        ctx.crit_rate += ctx.player.stats.get("Crit Rate", 0)/100
        ctx.crit_rate += interp(ctx.tp, crit_boost)
        ctx.crit_rate += get_dex_crit(ctx.player_dex, ctx.enemy_agi)
    return apply


def ranged_crit_hook(crit_boost):
    '''Build a hook for ranged critical hit weapon skills.

    Ranged attacks gain crit rate from AGI rather than DEX.
    '''
    def apply(ctx):
        ctx.crit_ws = True
        ctx.crit_rate += ctx.player.stats.get("Crit Rate", 0)/100
        ctx.crit_rate += interp(ctx.tp, crit_boost)
        ctx.crit_rate += ((ctx.player_agi - ctx.enemy_agi)/10)/100
    return apply


def acc_hook(acc_boost, both_hands=True):
    '''Build a hook that adds a TP-scaled accuracy bonus.'''
    def apply(ctx):
        acc_bonus = interp(ctx.tp, acc_boost)
        ctx.player_accuracy1 += acc_bonus
        if both_hands:
            ctx.player_accuracy2 += acc_bonus
    return apply


def ranged_acc_hook(acc_boost):
    '''Build a hook that adds a TP-scaled ranged accuracy bonus.'''
    def apply(ctx):
        ctx.player_rangedaccuracy += interp(ctx.tp, acc_boost)
    return apply


def _scale_attack(player, attack, modifier):
    '''Apply a weapon-skill attack% modifier, preserving food handling.

    Certain weapon skills increase/decrease attack by some percentage. We remove
    attack from food, multiply by the ratio of the new/old attack% buff, and then
    re-add attack from food.
    '''
    food = player.stats.get("Food Attack", 0)
    attack -= food
    attack *= (1 + player.stats.get("Attack%", 0) + modifier) / (1 + player.stats.get("Attack%", 0))
    attack += food
    return attack


def atk_hook(modifier, both_hands=True):
    '''Build a hook that applies a fixed attack% modifier.

    When both_hands is True the off-hand is scaled too, but only if it has a
    nonzero attack value (matching the original `player_attack2 > 0` guard).
    '''
    def apply(ctx):
        ctx.player_attack1 = _scale_attack(ctx.player, ctx.player_attack1, modifier)
        if both_hands and ctx.player_attack2 > 0:
            ctx.player_attack2 = _scale_attack(ctx.player, ctx.player_attack2, modifier)
    return apply


def atk_hook_unguarded(modifier):
    '''Like atk_hook but always scales the off-hand (Hand-to-Hand weapon skills,
    which scale player_attack2 without checking that it is positive).'''
    def apply(ctx):
        ctx.player_attack1 = _scale_attack(ctx.player, ctx.player_attack1, modifier)
        ctx.player_attack2 = _scale_attack(ctx.player, ctx.player_attack2, modifier)
    return apply


def interp_atk_hook(atk_boost, both_hands=True):
    '''Build a hook applying a TP-scaled attack% modifier (modifier = value-1).'''
    def apply(ctx):
        modifier = interp(ctx.tp, atk_boost) - 1.0
        ctx.player_attack1 = _scale_attack(ctx.player, ctx.player_attack1, modifier)
        if both_hands and ctx.player_attack2 > 0:
            ctx.player_attack2 = _scale_attack(ctx.player, ctx.player_attack2, modifier)
    return apply


def _scale_ranged_attack(player, attack, modifier):
    '''Apply a weapon-skill ranged attack% modifier, preserving food handling.'''
    food = player.stats.get("Food Ranged Attack", 0)
    attack -= food
    attack *= (1 + player.stats.get("Ranged Attack%", 0) + modifier) / (1 + player.stats.get("Ranged Attack%", 0))
    attack += food
    return attack


def ranged_atk_hook(modifier):
    '''Build a hook applying a fixed ranged attack% modifier.'''
    def apply(ctx):
        ctx.player_rangedattack = _scale_ranged_attack(ctx.player, ctx.player_rangedattack, modifier)
    return apply


def kick_atk_hook():
    '''Build a hook for kick weapon skills (Dragon Kick, Tornado Kick).

    These may benefit from Footwork. We re-calculate player attack using the
    "Kick Attacks Attack%" stat, which is 0% without Footwork, or ~26% with
    Footwork.
    '''
    def _recalc(player, attack, modifier):
        food = player.stats.get("Food Attack", 0)
        attack -= food # Remove food bonuses
        attack /= (1 + player.stats.get("Attack%", 0)) # Remove multiplicative bonuses
        attack += player.stats.get("Kick Attacks Attack", 0) if player.abilities.get("Footwork", False) else 0 # Add "Kick Attacks Attack" stat if Footwork is active
        attack *= (1 + player.stats.get("Attack%", 0) + modifier) # Reapply the multiplicative buffs, but include the +26% from Footwork.
        attack += food # Reapply food
        return attack

    def apply(ctx):
        modifier = ctx.player.stats.get("Kick Attacks Attack%", 0)
        ctx.player_attack1 = _recalc(ctx.player, ctx.player_attack1, modifier)
        ctx.player_attack2 = _recalc(ctx.player, ctx.player_attack2, modifier)
    return apply


def def_down_hook(scaling):
    '''Build a hook that lowers enemy defense by a TP-scaled fraction of base
    defense.'''
    def apply(ctx):
        ctx.enemy_def -= ctx.enemy_basedef * interp(ctx.tp, scaling)
    return apply


# dSTAT helpers --------------------------------------------------------------
# Magical WSs compute dSTAT (the stat delta feeding the magic damage formula)
# in a handful of different ways.  These callables reproduce each formula.

def dstat_int_capped(ctx):
    '''Standard elemental WS: (dINT)/2 + 8, capped at 32.'''
    value = (ctx.player_int - ctx.enemy_int)/2 + 8
    return 32 if value > 32 else value


def dstat_int_double(ctx):
    '''(player INT - enemy INT) * 2, uncapped.'''
    return (ctx.player_int - ctx.enemy_int)*2


def dstat_agi_double(ctx):
    '''(player AGI - enemy INT) * 2, uncapped.'''
    return (ctx.player_agi - ctx.enemy_int)*2


def dstat_mnd_double(ctx):
    '''(player MND - enemy MND) * 2, uncapped.'''
    return (ctx.player_mnd - ctx.enemy_mnd)*2


def dstat_primal_rend(ctx):
    '''Primal Rend: (player CHR - enemy INT) * 1.5, capped at 651.'''
    value = (ctx.player_chr - ctx.enemy_int)*1.5
    return 651 if value > 651 else value


def dstat_wildfire(ctx):
    '''Wildfire: (player AGI - enemy INT) * 2, capped at 1276.'''
    value = (ctx.player_agi - ctx.enemy_int)*2
    return 1276 if value > 1276 else value


# wsc helpers ----------------------------------------------------------------
# WSC (the weapon-skill stat contribution) is always a linear combination of
# player stats.  We store it as a callable taking the context.

def wsc(*terms):
    '''Build a WSC callable from (coefficient, stat_name) terms.

    Example: wsc((0.4, "STR"), (0.4, "DEX")) -> 0.4*STR + 0.4*DEX.
    '''
    attr = {
        "STR": "player_str", "DEX": "player_dex", "VIT": "player_vit",
        "AGI": "player_agi", "INT": "player_int", "MND": "player_mnd",
        "CHR": "player_chr",
    }
    pairs = [(coeff, attr[stat]) for coeff, stat in terms]

    def compute(ctx):
        return sum(coeff * getattr(ctx, name) for coeff, name in pairs)
    return compute


# ---------------------------------------------------------------------------
# Weapon skill table.
#
# Per-entry keys (all optional unless noted):
#   "wsc"        : callable(ctx) -> WSC value (required).
#   "nhits"      : number of hits (required).
#   "ftp"        : static fTP value.
#   "base_ftp"   : 3-point [1k, 2k, 3k] fTP table interpolated at the current TP.
#                  Exactly one of "ftp"/"base_ftp" is given (hybrid WSs use "ftp"
#                  for the physical portion and "base_ftp_hybrid" for the hybrid
#                  portion).
#   "ftp_rep"    : fTP replication flag (default False).
#   "magical"    : True for fully magical WSs (default False).
#   "hybrid"     : True for hybrid WSs (default False).
#   "base_ftp_hybrid" : 3-point hybrid fTP table (hybrid WSs only).
#   "element"    : damage element string (default "None").
#   "dSTAT"      : magical stat delta; constant or callable(ctx) -> value.
#   "crit"       : callable(ctx) hook installing crit handling.
#   "crit_rate"  : fixed crit rate override (True Strike only).
#   "hooks"      : list of callable(ctx) applied in order (attack/accuracy/def).
# ---------------------------------------------------------------------------
WS_TABLE = {
    # Sword weapon skills
    "Fast Blade": {"base_ftp": [1.0, 1.5, 2.0], "wsc": wsc((0.4, "STR"), (0.4, "DEX")), "nhits": 2},
    "Burning Blade": {"base_ftp": [1.0, 2.09765625, 3.3984375], "wsc": wsc((0.4, "STR"), (0.4, "INT")), "nhits": 1, "magical": True, "element": "Fire", "dSTAT": dstat_int_capped},
    "Red Lotus Blade": {"base_ftp": [1.0, 2.3828125, 3.75], "wsc": wsc((0.4, "STR"), (0.4, "INT")), "nhits": 1, "magical": True, "element": "Fire", "dSTAT": dstat_int_capped},
    "Shining Blade": {"base_ftp": [1.125, 2.22265625, 3.5234375], "wsc": wsc((0.4, "STR"), (0.4, "MND")), "nhits": 1, "magical": True, "element": "Light", "dSTAT": 0},
    "Seraph Blade": {"base_ftp": [1.125, 2.625, 4.125], "wsc": wsc((0.4, "STR"), (0.4, "MND")), "nhits": 1, "magical": True, "element": "Light", "dSTAT": 0},
    "Circle Blade": {"ftp": 1.0, "wsc": wsc((1.0, "STR")), "nhits": 1},
    "Swift Blade": {"ftp": 1.5, "ftp_rep": True, "wsc": wsc((0.5, "STR"), (0.5, "MND")), "nhits": 3, "hooks": [acc_hook([0, 20, 40])]},
    "Savage Blade": {"base_ftp": [4.0, 10.25, 13.75], "wsc": wsc((0.5, "STR"), (0.5, "MND")), "nhits": 2},
    "Sanguine Blade": {"ftp": 2.75, "wsc": wsc((0.3, "STR"), (0.5, "MND")), "nhits": 1, "magical": True, "element": "Dark", "dSTAT": dstat_int_double},
    "Requiescat": {"ftp": 1.0, "ftp_rep": True, "wsc": wsc((0.85, "MND")), "nhits": 5, "hooks": [interp_atk_hook([0.8, 0.9, 1.0])]},
    "Knights of Round": {"ftp": 5.0, "wsc": wsc((0.4, "STR"), (0.4, "MND")), "nhits": 1},
    "Chant du Cygne": {"ftp": 1.6328125, "ftp_rep": True, "wsc": wsc((0.8, "DEX")), "nhits": 3, "crit": crit_hook([0.15, 0.25, 0.40])},
    "Death Blossom": {"ftp": 4.0, "wsc": wsc((0.3, "STR"), (0.5, "MND")), "nhits": 3},
    "Expiacion": {"base_ftp": [3.796875, 9.390625, 12.1875], "wsc": wsc((0.3, "STR"), (0.2, "DEX"), (0.3, "INT")), "nhits": 2},
    "Fast Blade II": {"base_ftp": [1.8, 3.5, 5.0], "ftp_rep": True, "wsc": wsc((0.8, "DEX")), "nhits": 2},
    "Imperator": {"base_ftp": [3.75, 7.5, 11.75], "wsc": wsc((0.7, "DEX"), (0.7, "MND")), "nhits": 1},

    # Katana weapon skills
    "Blade: Retsu": {"ftp": 1.0, "wsc": wsc((0.2, "STR"), (0.6, "DEX")), "nhits": 2},
    "Blade: Teki": {"ftp": 1.0, "hybrid": True, "base_ftp_hybrid": [0.5, 1.375, 2.25], "wsc": wsc((0.3, "STR"), (0.3, "INT")), "nhits": 1, "element": "Water"},
    "Blade: To": {"ftp": 1.0, "hybrid": True, "base_ftp_hybrid": [0.5, 1.5, 2.5], "wsc": wsc((0.4, "STR"), (0.4, "INT")), "nhits": 1, "element": "Ice"},
    "Blade: Chi": {"ftp": 1.0, "hybrid": True, "base_ftp_hybrid": [0.5, 1.375, 2.25], "wsc": wsc((0.3, "STR"), (0.3, "INT")), "nhits": 2, "element": "Earth"},
    "Blade: Ei": {"base_ftp": [1.0, 3.0, 5.0], "wsc": wsc((0.4, "STR"), (0.4, "INT")), "nhits": 1, "magical": True, "element": "Dark", "dSTAT": dstat_int_capped},
    "Blade: Jin": {"ftp": 1.375, "ftp_rep": True, "wsc": wsc((0.3, "STR"), (0.3, "DEX")), "nhits": 3, "crit": crit_hook([0.1, 0.25, 0.5])},
    "Blade: Ten": {"base_ftp": [4.5, 11.5, 15.5], "wsc": wsc((0.3, "STR"), (0.3, "DEX")), "nhits": 1},
    "Blade: Ku": {"ftp": 1.25, "ftp_rep": True, "wsc": wsc((0.3, "STR"), (0.3, "DEX")), "nhits": 5, "hooks": [acc_hook([0, 20, 40])]},
    "Blade: Yu": {"ftp": 3.0, "wsc": wsc((0.4, "DEX"), (0.4, "INT")), "nhits": 1, "magical": True, "element": "Water", "dSTAT": 0},
    "Blade: Kamu": {"ftp": 1.0, "wsc": wsc((0.6, "STR"), (0.6, "INT")), "nhits": 1, "hooks": [atk_hook(1.25), def_down_hook([0.25, 0.25, 0.25])]},
    "Blade: Shun": {"ftp": 1.0, "ftp_rep": True, "wsc": wsc((0.85, "DEX")), "nhits": 5, "hooks": [interp_atk_hook([1.0, 2.0, 3.0])]},
    "Blade: Metsu": {"ftp": 5.0, "wsc": wsc((0.8, "DEX")), "nhits": 1},
    "Blade: Hi": {"ftp": 5.0, "wsc": wsc((0.8, "AGI")), "nhits": 1, "crit": crit_hook([0.15, 0.2, 0.25])},
    "Zesho Meppo": {"base_ftp": [4.0, 10.0, 18.715], "wsc": wsc((0.25, "DEX"), (0.25, "AGI")), "nhits": 4},

    # Dagger weapon skills
    "Viper Bite": {"ftp": 1.0, "wsc": wsc((1.0, "DEX")), "nhits": 2, "hooks": [atk_hook(1.0)]},
    "Dancing Edge": {"ftp": 1.1875, "wsc": wsc((0.4, "DEX"), (0.4, "CHR")), "nhits": 5, "hooks": [acc_hook([0, 20, 40])]},
    "Shark Bite": {"base_ftp": [4.5, 6.8, 8.5], "wsc": wsc((0.4, "DEX"), (0.4, "AGI")), "nhits": 2},
    "Evisceration": {"ftp": 1.25, "ftp_rep": True, "wsc": wsc((0.5, "DEX")), "nhits": 5, "crit": crit_hook([0.1, 0.25, 0.5])},
    "Aeolian Edge": {"base_ftp": [2.0, 3.0, 4.5], "wsc": wsc((0.4, "DEX"), (0.4, "INT")), "nhits": 1, "magical": True, "element": "Wind", "dSTAT": dstat_int_capped},
    "Exenterator": {"ftp": 1.0, "ftp_rep": True, "wsc": wsc((0.85, "AGI")), "nhits": 4},
    "Mercy Stroke": {"ftp": 5.0, "wsc": wsc((0.8, "STR")), "nhits": 1},
    "Rudra's Storm": {"base_ftp": [5.0, 10.19, 13.0], "wsc": wsc((0.8, "DEX")), "nhits": 1},
    "Mandalic Stab": {"base_ftp": [4.0, 6.09, 8.5], "wsc": wsc((0.6, "DEX")), "nhits": 1, "hooks": [atk_hook(0.75)]},
    "Mordant Rime": {"ftp": 5.0, "wsc": wsc((0.3, "DEX"), (0.7, "CHR")), "nhits": 2, "hooks": [acc_hook([0, 20, 40])]},
    "Pyrrhic Kleos": {"ftp": 1.75, "ftp_rep": True, "wsc": wsc((0.4, "STR"), (0.4, "DEX")), "nhits": 4},
    "Ruthless Stroke": {"base_ftp": [5.375, 14.0, 23.0], "wsc": wsc((0.25, "DEX"), (0.25, "AGI")), "nhits": 4},

    # Polearm weapon skills
    "Double Thrust": {"base_ftp": [1.0, 1.5, 2.0], "wsc": wsc((0.3, "STR"), (0.3, "DEX")), "nhits": 2},
    "Thunder Thrust": {"base_ftp": [1.5, 2.0, 2.5], "wsc": wsc((0.4, "STR"), (0.4, "INT")), "nhits": 1, "magical": True, "element": "Thunder", "dSTAT": dstat_int_capped},
    "Raiden Thrust": {"base_ftp": [1.0, 2.0, 3.0], "wsc": wsc((0.4, "STR"), (0.4, "INT")), "nhits": 1, "magical": True, "element": "Thunder", "dSTAT": dstat_int_capped},
    "Penta Thrust": {"ftp": 1.0, "wsc": wsc((0.2, "STR"), (0.2, "DEX")), "nhits": 5, "hooks": [acc_hook([0, 20, 40], both_hands=False), atk_hook(-0.125, both_hands=False)]},
    "Wheeling Thrust": {"ftp": 1.75, "wsc": wsc((0.8, "STR")), "nhits": 1, "hooks": [def_down_hook([0.50, 0.625, 0.75])]},
    "Impulse Drive": {"base_ftp": [1.0, 3.0, 5.5], "wsc": wsc((1.0, "STR")), "nhits": 2},
    "Sonic Thrust": {"base_ftp": [3.0, 3.7, 4.5], "wsc": wsc((0.4, "STR"), (0.4, "DEX")), "nhits": 1},
    "Stardiver": {"base_ftp": [0.75, 1.25, 1.75], "ftp_rep": True, "wsc": wsc((0.85, "STR")), "nhits": 4},
    "Geirskogul": {"ftp": 3.0, "wsc": wsc((0.8, "DEX")), "nhits": 1},
    "Camlann's Torment": {"ftp": 3.0, "wsc": wsc((0.6, "STR"), (0.6, "VIT")), "nhits": 1, "hooks": [def_down_hook([0.125, 0.375, 0.625])]},
    "Drakesbane": {"ftp": 1.0, "wsc": wsc((0.5, "STR")), "nhits": 4, "crit": crit_hook([0.1, 0.25, 0.40]), "hooks": [atk_hook(0.8125 - 1.0, both_hands=False)]},
    "Diarmuid": {"base_ftp": [2.17, 5.36, 8.55], "wsc": wsc((0.55, "STR"), (0.55, "VIT")), "nhits": 2},

    # Great Katana weapon skills
    "Tachi: Enpi": {"base_ftp": [1.0, 1.5, 2.0], "wsc": wsc((0.6, "STR")), "nhits": 2},
    "Tachi: Goten": {"ftp": 1.0, "hybrid": True, "base_ftp_hybrid": [0.5, 1.5, 2.5], "wsc": wsc((0.6, "STR")), "nhits": 1, "element": "Thunder"},
    "Tachi: Kagero": {"ftp": 1.0, "hybrid": True, "base_ftp_hybrid": [0.5, 1.5, 2.5], "wsc": wsc((0.75, "STR")), "nhits": 1, "element": "Fire"},
    "Tachi: Koki": {"ftp": 1.0, "hybrid": True, "base_ftp_hybrid": [0.5, 1.5, 2.5], "wsc": wsc((0.5, "STR"), (0.3, "MND")), "nhits": 1, "element": "Light"},
    "Tachi: Jinpu": {"ftp": 1.0, "hybrid": True, "base_ftp_hybrid": [0.5, 1.5, 2.5], "wsc": wsc((0.3, "STR")), "nhits": 2, "element": "Wind"},
    "Tachi: Yukikaze": {"base_ftp": [1.5625, 2.6875, 4.125], "wsc": wsc((0.75, "STR")), "nhits": 1, "hooks": [atk_hook(0.5, both_hands=False)]},
    "Tachi: Gekko": {"base_ftp": [1.5625, 2.6875, 4.125], "wsc": wsc((0.75, "STR")), "nhits": 1, "hooks": [atk_hook(1.0, both_hands=False)]},
    "Tachi: Kasha": {"base_ftp": [1.5625, 2.6875, 4.125], "wsc": wsc((0.75, "STR")), "nhits": 1, "hooks": [atk_hook(0.65, both_hands=False)]},
    "Tachi: Ageha": {"ftp": 2.625, "wsc": wsc((0.4, "STR"), (0.6, "CHR")), "nhits": 1},
    "Tachi: Shoha": {"base_ftp": [1.375, 2.1875, 2.6875], "wsc": wsc((0.85, "STR")), "nhits": 2, "hooks": [atk_hook(1.375, both_hands=False)]},
    "Tachi: Kaiten": {"ftp": 3.0, "wsc": wsc((0.8, "STR")), "nhits": 1},
    "Tachi: Fudo": {"base_ftp": [3.75, 5.75, 8.0], "wsc": wsc((0.8, "STR")), "nhits": 1},
    "Tachi: Rana": {"ftp": 1.0, "wsc": wsc((0.5, "STR")), "nhits": 3, "hooks": [acc_hook([0, 20, 40], both_hands=False)]},
    "Tachi: Mumei": {"base_ftp": [3.66, 7.33, 11.0], "wsc": wsc((0.5, "STR"), (0.5, "DEX")), "nhits": 1},

    # Scythe weapon skills
    "Slice": {"base_ftp": [1.5, 1.75, 2.0], "wsc": wsc((1.0, "STR")), "nhits": 1},
    "Dark Harvest": {"base_ftp": [1.0, 2.0, 2.5], "wsc": wsc((0.4, "STR"), (0.4, "INT")), "nhits": 1, "magical": True, "element": "Dark", "dSTAT": dstat_int_capped},
    "Shadow of Death": {"base_ftp": [1.0, 4.17, 8.6], "wsc": wsc((0.4, "STR"), (0.4, "INT")), "nhits": 1, "magical": True, "element": "Dark", "dSTAT": dstat_int_capped},
    "Nightmare Scythe": {"ftp": 1.0, "wsc": wsc((0.6, "STR"), (0.6, "MND")), "nhits": 1},
    "Spinning Scythe": {"ftp": 1.0, "wsc": wsc((1.0, "STR")), "nhits": 1},
    "Cross Reaper": {"base_ftp": [2.0, 4.0, 7.0], "wsc": wsc((0.6, "STR"), (0.6, "MND")), "nhits": 2},
    "Guillotine": {"ftp": 0.875, "wsc": wsc((0.3, "STR"), (0.5, "MND")), "nhits": 4},
    "Spiral Hell": {"base_ftp": [1.375, 2.75, 4.75], "wsc": wsc((0.5, "STR"), (0.5, "INT")), "nhits": 1},
    "Infernal Scythe": {"ftp": 3.5, "wsc": wsc((0.3, "STR"), (0.7, "INT")), "nhits": 1, "magical": True, "element": "Dark", "dSTAT": 0},
    "Entropy": {"base_ftp": [0.75, 1.25, 2.0], "ftp_rep": True, "wsc": wsc((0.85, "INT")), "nhits": 4},
    "Catastrophe": {"ftp": 2.75, "wsc": wsc((0.4, "STR"), (0.4, "INT")), "nhits": 1},
    "Quietus": {"ftp": 3.0, "wsc": wsc((0.6, "STR"), (0.6, "MND")), "nhits": 1, "hooks": [def_down_hook([0.10, 0.30, 0.50])]},
    "Insurgency": {"base_ftp": [0.5, 3.25, 6.0], "wsc": wsc((0.2, "STR"), (0.2, "INT")), "nhits": 4},
    "Origin": {"base_ftp": [3.0, 6.25, 9.5], "wsc": wsc((0.6, "STR"), (0.6, "INT")), "nhits": 1},

    # Great Sword weapon skills
    "Hard Slash": {"base_ftp": [1.5, 1.75, 2.0], "wsc": wsc((0.4, "STR"), (0.4, "AGI")), "nhits": 2},
    "Freezebite": {"base_ftp": [1.5, 3.5, 6.0], "wsc": wsc((0.4, "STR"), (0.4, "INT")), "nhits": 1, "magical": True, "element": "Ice", "dSTAT": dstat_int_capped},
    "Shockwave": {"ftp": 1.0, "wsc": wsc((0.3, "STR"), (0.3, "MND")), "nhits": 1},
    "Sickle Moon": {"base_ftp": [1.5, 2.0, 2.75], "wsc": wsc((0.4, "STR"), (0.4, "AGI")), "nhits": 1},
    "Spinning Slash": {"base_ftp": [2.5, 3.0, 3.5], "wsc": wsc((0.3, "STR"), (0.3, "INT")), "nhits": 1, "hooks": [atk_hook(0.5, both_hands=False)]},
    "Ground Strike": {"base_ftp": [1.5, 1.75, 3.0], "wsc": wsc((0.5, "STR"), (0.5, "INT")), "nhits": 1, "hooks": [atk_hook(0.75, both_hands=False)]},
    "Herculean Slash": {"ftp": 3.5, "wsc": wsc((0.8, "VIT")), "nhits": 1, "magical": True, "element": "Ice", "dSTAT": 0},
    "Resolution": {"base_ftp": [0.71875, 1.5, 2.25], "ftp_rep": True, "wsc": wsc((0.85, "STR")), "nhits": 5, "hooks": [atk_hook(-0.15, both_hands=False)]},
    "Scourge": {"ftp": 3.0, "wsc": wsc((0.4, "STR"), (0.4, "VIT")), "nhits": 1},
    "Torcleaver": {"base_ftp": [4.75, 7.5, 9.765625], "wsc": wsc((0.8, "VIT")), "nhits": 1},
    "Dimidiation": {"base_ftp": [2.25, 4.5, 6.75], "wsc": wsc((0.8, "DEX")), "nhits": 2, "hooks": [atk_hook(0.25, both_hands=False)]},
    "Fimbulvetr": {"base_ftp": [3.3, 6.6, 9.9], "wsc": wsc((0.6, "STR"), (0.6, "VIT")), "nhits": 1},

    # Club weapon skills
    "Shining Strike": {"base_ftp": [1.625, 3.0, 4.625], "wsc": wsc((0.4, "STR"), (0.4, "MND")), "nhits": 1, "magical": True, "element": "Light", "dSTAT": 0},
    "Seraph Strike": {"base_ftp": [2.125, 3.675, 6.125], "wsc": wsc((0.4, "STR"), (0.4, "MND")), "nhits": 1, "magical": True, "element": "Light", "dSTAT": 0},
    "Skullbreaker": {"ftp": 1.0, "wsc": wsc((1.0, "STR")), "nhits": 1},
    "True Strike": {"ftp": 1.0, "wsc": wsc((1.0, "STR")), "nhits": 1, "crit_rate": 1.0, "hooks": [acc_hook([-60, -30, 0]), atk_hook(1.0)]},
    "Judgment": {"base_ftp": [3.5, 8.75, 12.0], "wsc": wsc((0.5, "STR"), (0.5, "MND")), "nhits": 1},
    "Hexa Strike": {"ftp": 1.125, "ftp_rep": True, "wsc": wsc((0.3, "STR"), (0.3, "MND")), "nhits": 6, "crit": crit_hook([0.1, 0.175, 0.25])},
    "Black Halo": {"base_ftp": [3.0, 7.25, 9.75], "wsc": wsc((0.3, "STR"), (0.7, "MND")), "nhits": 2},
    "Realmrazer": {"ftp": 0.9, "ftp_rep": True, "wsc": wsc((0.85, "MND")), "nhits": 7, "hooks": [acc_hook([0, 20, 40])]},
    "Randgrith": {"ftp": 4.25, "wsc": wsc((0.4, "STR"), (0.4, "MND")), "nhits": 1},
    "Mystic Boon": {"base_ftp": [2.5, 4.0, 7.0], "wsc": wsc((0.3, "STR"), (0.7, "MND")), "nhits": 1},
    "Exudation": {"ftp": 2.8, "wsc": wsc((0.5, "INT"), (0.5, "MND")), "nhits": 1, "hooks": [interp_atk_hook([1.5, 3.625, 4.750])]},
    "Dagda": {"base_ftp": [1, 2, 3], "wsc": wsc((0.0, "DEX")), "nhits": 2},

    # Great Axe weapon skills
    "Shield Break": {"ftp": 1.0, "wsc": wsc((0.6, "STR"), (0.6, "VIT")), "nhits": 1},
    "Iron Tempest": {"ftp": 1.0, "wsc": wsc((0.6, "STR")), "nhits": 1, "hooks": [interp_atk_hook([1.0, 1.2, 1.5], both_hands=False)]},
    "Armor Break": {"ftp": 1.0, "wsc": wsc((0.6, "STR"), (0.6, "VIT")), "nhits": 1},
    "Weapon Break": {"ftp": 1.0, "wsc": wsc((0.6, "STR"), (0.6, "VIT")), "nhits": 1},
    "Raging Rush": {"ftp": 1.0, "wsc": wsc((0.5, "STR")), "nhits": 3, "crit": crit_hook([0.15, 0.30, 0.50])},
    "Full Break": {"ftp": 1.0, "wsc": wsc((0.5, "STR"), (0.5, "VIT")), "nhits": 1},
    "Steel Cyclone": {"base_ftp": [1.5, 2.5, 4.0], "wsc": wsc((0.6, "STR"), (0.6, "VIT")), "nhits": 1, "hooks": [atk_hook(0.5, both_hands=False)]},
    "Fell Cleave": {"ftp": 2.75, "wsc": wsc((0.6, "STR")), "nhits": 1},
    "Upheaval": {"base_ftp": [1.0, 3.5, 6.5], "wsc": wsc((0.85, "VIT")), "nhits": 4},
    "Metatron Torment": {"ftp": 2.75, "wsc": wsc((0.8, "STR")), "nhits": 1},
    "Ukko's Fury": {"ftp": 2.0, "wsc": wsc((0.8, "STR")), "nhits": 2, "crit": crit_hook([0.2, 0.35, 0.55])},
    "King's Justice": {"base_ftp": [1.0, 3.0, 5.0], "wsc": wsc((0.5, "STR")), "nhits": 3},
    "Disaster": {"base_ftp": [3.05, 6.10, 9.15], "wsc": wsc((0.6, "STR"), (0.6, "VIT")), "nhits": 1},

    # Axe weapon skills
    "Raging Axe": {"base_ftp": [1.0, 1.5, 2.0], "wsc": wsc((0.6, "STR")), "nhits": 2},
    "Spinning Axe": {"base_ftp": [2.0, 2.5, 3.0], "wsc": wsc((0.6, "STR")), "nhits": 2},
    "Rampage": {"ftp": 1.0, "ftp_rep": True, "wsc": wsc((0.5, "STR")), "nhits": 5, "crit": crit_hook([0.0, 0.20, 0.40])},
    "Calamity": {"base_ftp": [2.5, 6.5, 10.375], "wsc": wsc((0.5, "STR"), (0.5, "VIT")), "nhits": 1},
    "Mistral Axe": {"base_ftp": [4.0, 10.5, 13.625], "wsc": wsc((0.5, "STR")), "nhits": 1},
    "Decimation": {"ftp": 1.75, "ftp_rep": True, "wsc": wsc((0.5, "STR")), "nhits": 3, "hooks": [acc_hook([0, 20, 40])]},
    "Bora Axe": {"ftp": 4.5, "wsc": wsc((1.0, "DEX")), "nhits": 1, "hooks": [acc_hook([0, 20, 40])]},
    "Ruinator": {"ftp": 1.0, "ftp_rep": True, "wsc": wsc((0.85, "STR")), "nhits": 4, "hooks": [acc_hook([0, 20, 40]), atk_hook(0.1)]},
    "Onslaught": {"ftp": 2.75, "wsc": wsc((0.8, "DEX")), "nhits": 1},
    "Cloudsplitter": {"base_ftp": [3.75, 6.69921875, 8.5], "wsc": wsc((0.4, "STR"), (0.4, "MND")), "nhits": 1, "magical": True, "element": "Thunder", "dSTAT": 0},
    "Primal Rend": {"base_ftp": [3.0625, 5.8359375, 7.5625], "wsc": wsc((0.3, "DEX"), (0.6, "CHR")), "nhits": 1, "magical": True, "element": "Light", "dSTAT": dstat_primal_rend},
    "Blitz": {"base_ftp": [1.5, 7.0, 12.5], "wsc": wsc((0.32, "STR"), (0.32, "DEX")), "nhits": 5},

    # Archery weapon skills
    "Flaming Arrow": {"ftp": 1.0, "hybrid": True, "base_ftp_hybrid": [0.5, 1.55, 2.1], "wsc": wsc((0.2, "STR"), (0.5, "AGI")), "nhits": 1, "element": "Fire"},
    "Piercing Arrow": {"ftp": 1.0, "ftp_rep": True, "wsc": wsc((0.2, "STR"), (0.5, "AGI")), "nhits": 1, "hooks": [def_down_hook([0.0, 0.35, 0.50])]},
    "Dulling Arrow": {"ftp": 1.0, "wsc": wsc((0.2, "STR"), (0.5, "AGI")), "nhits": 1, "crit": ranged_crit_hook([0.10, 0.20, 0.25])},
    "Sidewinder": {"ftp": 5.0, "wsc": wsc((0.2, "STR"), (0.5, "AGI")), "nhits": 1, "hooks": [ranged_acc_hook([-50, -20, 0])]},
    "Blast Arrow": {"ftp": 2.0, "wsc": wsc((0.2, "STR"), (0.5, "AGI")), "nhits": 1, "hooks": [ranged_acc_hook([0, 20, 40])]},
    "Empyreal Arrow": {"base_ftp": [1.5, 2.5, 5.0], "wsc": wsc((0.2, "STR"), (0.5, "AGI")), "nhits": 1, "hooks": [ranged_atk_hook(1.0)]},
    "Refulgent Arrow": {"base_ftp": [3.0, 4.25, 7.0], "wsc": wsc((0.6, "STR")), "nhits": 1},
    "Apex Arrow": {"ftp": 3.0, "wsc": wsc((0.85, "AGI")), "nhits": 1, "hooks": [def_down_hook([0.15, 0.30, 0.45])]},
    "Namas Arrow": {"ftp": 2.75, "wsc": wsc((0.4, "STR"), (0.4, "AGI")), "nhits": 1},
    "Jishnu's Radiance": {"ftp": 1.75, "ftp_rep": True, "wsc": wsc((0.8, "DEX")), "nhits": 3, "crit": ranged_crit_hook([0.15, 0.2, 0.25])},
    "Sarv": {"base_ftp": [2.75, 5.5, 8.25], "wsc": wsc((0.65, "STR"), (0.65, "AGI")), "nhits": 1},

    # Marksmanship weapon skills
    "Hot Shot": {"ftp": 1.0, "hybrid": True, "base_ftp_hybrid": [0.5, 1.55, 2.1], "wsc": wsc((0.7, "AGI")), "nhits": 1, "element": "Fire"},
    "Split Shot": {"ftp": 1.0, "wsc": wsc((0.7, "AGI")), "nhits": 1, "hooks": [def_down_hook([0.0, 0.35, 0.50])]},
    "Sniper Shot": {"ftp": 1.0, "wsc": wsc((0.7, "AGI")), "nhits": 1, "crit": ranged_crit_hook([0.10, 0.20, 0.25])},
    "Slug Shot": {"ftp": 5.0, "wsc": wsc((0.7, "AGI")), "nhits": 1, "hooks": [ranged_acc_hook([-50, -20, 0])]},
    "Blast Shot": {"ftp": 2.0, "wsc": wsc((0.7, "AGI")), "nhits": 1, "hooks": [ranged_acc_hook([0, 20, 40])]},
    "Detonator": {"base_ftp": [1.5, 2.5, 5.0], "wsc": wsc((0.7, "AGI")), "nhits": 1, "hooks": [ranged_atk_hook(1.0)]},
    "Last Stand": {"base_ftp": [2.0, 3.0, 4.0], "ftp_rep": True, "wsc": wsc((0.85, "AGI")), "nhits": 2},
    "Coronach": {"ftp": 3.0, "wsc": wsc((0.4, "DEX"), (0.4, "AGI")), "nhits": 1},
    "Wildfire": {"ftp": 5.5, "wsc": wsc((0.6, "AGI")), "nhits": 1, "magical": True, "element": "Fire", "dSTAT": dstat_wildfire},
    "Trueflight": {"base_ftp": [3.890625, 6.4921875, 9.671875], "wsc": wsc((1.0, "AGI")), "nhits": 1, "magical": True, "element": "Light", "dSTAT": dstat_agi_double},
    "Leaden Salute": {"base_ftp": [4.0, 6.7, 10.0], "wsc": wsc((1.0, "AGI")), "nhits": 1, "magical": True, "element": "Dark", "dSTAT": dstat_agi_double},
    "Terminus": {"base_ftp": [2.5, 5.0, 7.5], "wsc": wsc((0.7, "DEX"), (0.7, "AGI")), "nhits": 1},

    # Staff weapon skills
    "Heavy Swing": {"base_ftp": [1.0, 1.25, 2.25], "wsc": wsc((1.0, "STR")), "nhits": 1},
    "Rock Crusher": {"base_ftp": [1.0, 2.0, 2.5], "wsc": wsc((0.4, "STR"), (0.4, "INT")), "nhits": 1, "magical": True, "element": "Earth", "dSTAT": dstat_int_capped},
    "Earth Crusher": {"base_ftp": [1.0, 2.3125, 3.625], "wsc": wsc((0.4, "STR"), (0.4, "INT")), "nhits": 1, "magical": True, "element": "Earth", "dSTAT": dstat_int_capped},
    "Starburst": {"base_ftp": [1.0, 2.0, 2.5], "wsc": wsc((0.4, "STR"), (0.4, "MND")), "nhits": 1, "magical": True, "element": "Light", "dSTAT": dstat_int_capped},
    "Sunburst": {"base_ftp": [1.0, 2.5, 4.0], "wsc": wsc((0.4, "STR"), (0.4, "MND")), "nhits": 1, "magical": True, "element": "Light", "dSTAT": dstat_int_capped},
    "Shell Crusher": {"ftp": 1.0, "wsc": wsc((1.0, "STR")), "nhits": 1},
    "Full Swing": {"base_ftp": [1.0, 3.0, 5.0], "wsc": wsc((0.5, "STR")), "nhits": 1},
    "Retribution": {"base_ftp": [2.0, 2.5, 3.0], "wsc": wsc((0.3, "STR"), (0.5, "MND")), "nhits": 1, "hooks": [atk_hook(0.5, both_hands=False)]},
    "Cataclysm": {"base_ftp": [2.75, 4.0, 5.0], "wsc": wsc((0.3, "STR"), (0.3, "INT")), "nhits": 1, "magical": True, "element": "Dark", "dSTAT": dstat_int_capped},
    "Shattersoul": {"ftp": 1.375, "wsc": wsc((0.85, "INT")), "nhits": 3},
    "Vidohunir": {"ftp": 1.75, "wsc": wsc((0.8, "INT")), "nhits": 1, "magical": True, "element": "Dark", "dSTAT": dstat_int_double},
    "Omniscience": {"ftp": 2.0, "wsc": wsc((0.8, "MND")), "nhits": 1, "magical": True, "element": "Dark", "dSTAT": dstat_mnd_double},
    "Garland of Bliss": {"ftp": 2.25, "wsc": wsc((0.3, "STR"), (0.7, "MND")), "nhits": 1, "magical": True, "element": "Light", "dSTAT": dstat_mnd_double},
    "Gate of Tartarus": {"ftp": 3.0, "wsc": wsc((0.8, "INT")), "nhits": 1},
    "Oshala": {"base_ftp": [3.945, 7.894, 11.839], "wsc": wsc((0.45, "INT"), (0.45, "MND")), "nhits": 1},

    # Hand-to-Hand weapon skills
    # Notice that each weapon skill has "-1" nhits.
    # This is to allow us to fit in an "off-hand" attack to get full TP, as observed in game.
    "Combo": {"base_ftp": [1.0, 2.4, 3.4], "ftp_rep": True, "wsc": wsc((0.3, "STR"), (0.3, "DEX")), "nhits": 3 - 1},
    "One Inch Punch": {"ftp": 1.0, "ftp_rep": True, "wsc": wsc((1.0, "VIT")), "nhits": 2 - 1, "hooks": [def_down_hook([0.0, 0.25, 0.50])]},
    "Raging Fists": {"base_ftp": [1.0, 2.1875, 3.75], "ftp_rep": True, "wsc": wsc((0.3, "STR"), (0.3, "DEX")), "nhits": 5 - 1},
    "Spinning Attack": {"ftp": 1.0, "ftp_rep": True, "wsc": wsc((1.0, "STR")), "nhits": 2 - 1},
    "Howling Fist": {"base_ftp": [2.05, 3.55, 5.75], "ftp_rep": True, "wsc": wsc((0.2, "STR"), (0.5, "VIT")), "nhits": 2 - 1, "hooks": [atk_hook_unguarded(0.5)]},
    "Dragon Kick": {"base_ftp": [1.7, 3.0, 5.0], "ftp_rep": True, "wsc": wsc((0.5, "STR"), (0.5, "VIT")), "nhits": 2 - 1, "hooks": [kick_atk_hook()]},
    "Asuran Fists": {"ftp": 1.25, "ftp_rep": True, "wsc": wsc((0.15, "STR"), (0.15, "VIT")), "nhits": 8 - 1, "hooks": [acc_hook([0, 20, 40])]},
    "Tornado Kick": {"base_ftp": [1.7, 2.8, 4.5], "ftp_rep": True, "wsc": wsc((0.4, "STR"), (0.4, "VIT")), "nhits": 3 - 1, "hooks": [kick_atk_hook()]},
    "Shijin Spiral": {"ftp": 1.5, "ftp_rep": True, "wsc": wsc((0.85, "DEX")), "nhits": 5 - 1, "hooks": [acc_hook([0, 20, 40]), atk_hook_unguarded(0.05)]},
    "Final Heaven": {"ftp": 3.0, "wsc": wsc((0.8, "VIT")), "nhits": 2 - 1},
    "Victory Smite": {"ftp": 1.5, "ftp_rep": True, "wsc": wsc((0.8, "STR")), "nhits": 4 - 1, "crit": crit_hook([0.10, 0.25, 0.45])},
    "Ascetic's Fury": {"ftp": 1.0, "ftp_rep": True, "wsc": wsc((0.5, "STR"), (0.5, "VIT")), "nhits": 2 - 1, "crit": crit_hook([0.20, 0.30, 0.50]), "hooks": [atk_hook_unguarded(1.0)]},
    "Stringing Pummel": {"ftp": 1.0, "ftp_rep": True, "wsc": wsc((0.32, "STR"), (0.32, "VIT")), "nhits": 6 - 1, "crit": crit_hook([0.15, 0.30, 0.45])},
    "Maru Kala": {"base_ftp": [3.0, 7.0, 11.5], "wsc": wsc((0.4, "STR"), (0.4, "DEX")), "nhits": 2},
    "Dragon Blow": {"base_ftp": [3.675, 7.0, 10.4375], "wsc": wsc((0.85, "DEX")), "nhits": 2, "hooks": [atk_hook_unguarded(1.5)]},
}
