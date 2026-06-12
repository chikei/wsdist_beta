'''
File containing available weapon skills and their properties

Author: Kastra (Asura server)
'''
from typing import TYPE_CHECKING, Any, cast

import numpy as np
from get_dex_crit import *
from weaponskill_data import WS_TABLE, WSContext, interp

if TYPE_CHECKING:
    from create_player import create_enemy, create_player


def _apply_naegling(ctx: WSContext) -> None:
    # Naegling provides +1% attack per active buff. For now I assume this is +13% (protect, shell, haste, songx4, rollx2, signet). TODO: update later by counting buffs from the GUI
    player = ctx.player
    ws_atk_modifier = 0.13
    ctx.player_attack1 -= player.stats.get("Food Attack", 0)
    ctx.player_attack1 *= (1+player.stats.get("Attack%", 0) + ws_atk_modifier) / (1+player.stats.get("Attack%", 0))
    ctx.player_attack1 += player.stats.get("Food Attack", 0)
    if ctx.player_attack2 > 0:
        ctx.player_attack2 -= player.stats.get("Food Attack", 0)
        ctx.player_attack2 *= (1+player.stats.get("Attack%", 0) + ws_atk_modifier) / (1+player.stats.get("Attack%", 0))
        ctx.player_attack2 += player.stats.get("Food Attack", 0)


def _apply_weapon_setup(ctx: WSContext) -> None:
    # Some main weapons modify attack or enemy defense before the WS-specific logic.
    main_name = ctx.player.gearset["main"]["Name"]
    if main_name == "Naegling":
        _apply_naegling(ctx)
    elif main_name == "Nandaka":
        # Nandaka lowers enemy defense by 1% per debuff present on weapon skills. I assume this is only -3% (dia, slow, paralyze, )
        ctx.enemy_def -= 0.03*ctx.enemy_basedef


def _apply_ws_table(ctx: WSContext, ws_name: str) -> None:
    # Populate the context from the data-driven weapon skill table.
    spec = WS_TABLE[ws_name]

    ctx.nhits = spec["nhits"]
    ctx.wsc = spec["wsc"](ctx)
    ctx.ftp_rep = spec.get("ftp_rep", False)

    if "ftp" in spec:
        ctx.ftp = spec["ftp"]
    else:
        ctx.ftp = interp(ctx.tp, spec["base_ftp"])

    if spec.get("magical", False):
        ctx.magical = True
        ctx.element = spec["element"]
        dstat = spec["dSTAT"]
        ctx.dSTAT = cast(float, dstat(ctx)) if callable(dstat) else dstat

    if spec.get("hybrid", False):
        ctx.hybrid = True
        ctx.element = spec["element"]
        ctx.ftp_hybrid = interp(ctx.tp, spec["base_ftp_hybrid"])

    if "crit_rate" in spec:
        ctx.crit_ws = True
        ctx.crit_rate = spec["crit_rate"]
    elif "crit" in spec:
        spec["crit"](ctx)

    for hook in spec.get("hooks", []):
        hook(ctx)


def _apply_shining_one(ctx: WSContext, ws_name: str) -> None:
    # Shining One allows most weapon skills to crit. https://www.bg-wiki.com/ffxi/Shining_One
    ranged_ws = ["Flaming Arrow", "Namas Arrow", "Apex Arrow", "Refulgent Arrow", "Empyreal Arrow", "Sidewinder", "Piercing Arrow", "Jishnu's Radiance", "Blast Arrow", "Hot Shot", "Coronach", "Last Stand", "Detonator", "Blast Shot", "Slug Shot", "Split Shot", ]
    if ctx.player.gearset["main"]["Name"] != "Shining One" or ws_name in ranged_ws:
        return

    if not ctx.crit_ws:
        # Add gear and dDEX crit rate if the selected WS did not already do it.
        ctx.crit_rate += ctx.player.stats.get('Crit Rate', 0)/100
        ctx.crit_rate += get_dex_crit(ctx.player.stats.get('DEX', 0), ctx.enemy_agi)

    crit_boost = [0.05, 0.10, 0.15] # Crit rate bonuses based on TP for wielding Shining One
    ctx.crit_rate += np.interp(ctx.tp, [1000, 2000, 3000], crit_boost)


def weaponskill_info(ws_name: str, tp: float, player: "create_player", enemy: "create_enemy", wsc_bonus: list[list[Any]], dual_wield: bool) -> dict[str, Any]:
    #
    # Setup weaponskill statistics (TP scaling, # of hits, ftp replication, WSC, etc)
    #
    # wsc_bonus is the WSC bonus from Utu Grip and Crepuscular Knife. The format is wsc_bonus = [["DEX", 0.10],["CHR",0.03],] etc
    #
    #
    ctx = WSContext(tp, player, enemy)

    _apply_weapon_setup(ctx)
    _apply_ws_table(ctx, ws_name)
    _apply_shining_one(ctx, ws_name)

    ctx.crit_rate = 1.0 if player.abilities.get("Mighty Strikes", False) else ctx.crit_rate
    ctx.enemy_def = 1 if ctx.enemy_def < 1 else ctx.enemy_def

    ctx.wsc += sum([player.stats[k[0]] * k[1]/100 for k in wsc_bonus]) # Add WSC bonuses from things like Utu Grip and Crepuscular Knife, which use the "WSC" stat in their gear entry.

    scaling = {"hybrid": ctx.hybrid,
               "magical": ctx.magical,
               "dSTAT": ctx.dSTAT,
               "wsc": ctx.wsc,
               "nhits": ctx.nhits,
               "element": ctx.element,
               "ftp": ctx.ftp,
               "ftp_rep": ctx.ftp_rep,
               "player_attack1": ctx.player_attack1,
               "player_attack2": ctx.player_attack2,
               "player_accuracy1": ctx.player_accuracy1,
               "player_accuracy2": ctx.player_accuracy2,
               "player_rangedattack": ctx.player_rangedattack,
               "player_rangedaccuracy": ctx.player_rangedaccuracy,
               "enemy_def": ctx.enemy_def,
               "crit_rate": ctx.crit_rate,
               "ftp_hybrid": ctx.ftp_hybrid,
               }
    return(scaling)
