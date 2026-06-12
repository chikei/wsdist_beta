'''
File containing calculations for physical damage dealt.
    
Author: Kastra (Asura server)
'''
from typed_numba import njit

@njit
def get_phys_damage(wpn_dmg: float, fstr_wpn: float, wsc: float, pdif: float, ftp: float, crit: bool, crit_dmg: float, wsd: float, ws_bonus: float, ws_trait: float, n: int, sneak_attack_bonus: float = 0, trick_attack_bonus: float = 0, climactic_flourish_bonus: float = 0, striking_flourish_bonus: float = 0, ternary_flourish_bonus: float = 0) -> float:
    #
    # Calculate physical damage dealt for a single attack.
    # https://www.bg-wiki.com/ffxi/Weapon_Skill_Damage
    #
    # "crit" is a True/False based on a random number check earlier in the code. This determines whether or not to add crit_dmg bonuses.
    # "wsd" is the typical "Weapon Skill Damage +" stat on gear. It only applies to the first main-hand hit of a weapon skill, so we multiply by (n==0) to ensure that the "wsd" term becomes zero if not checking the first main-hand hit.
    # "wsd_trait" comes from "Weapon Skill Damage" traits, specifically from DRG. This bonus applies to all hits of the weapon skill.
    # "wsd_bonus" comes from specific damage boosts to weapon skills. Sources include REMA hidden damage boosts, REMA augments, and Ambuscade weapons. This applies to all hits of the weapon skill.
    #
    phys = int(  ((wpn_dmg + fstr_wpn + wsc)*ftp*(1+wsd*(n==0))*(1+ws_bonus)*(1+ws_trait)) + sneak_attack_bonus*(n==0) + trick_attack_bonus*(n==0) + climactic_flourish_bonus*(n==0) + striking_flourish_bonus*(n==0) + ternary_flourish_bonus*(n==0)) * pdif * (1 + crit*min(crit_dmg,1.0)) # "crit" = True/False
    return(phys)

@njit
def get_avg_phys_damage(wpn_dmg: float, fstr_wpn: float, wsc: float, pdif: float, ftp: float, crit_rate: float, crit_dmg: float, wsd: float, ws_bonus: float, ws_trait: float, sneak_attack_bonus: float = 0, trick_attack_bonus: float = 0, climactic_flourish_bonus: float = 0, striking_flourish_bonus: float = 0, ternary_flourish_bonus: float = 0) -> float:
    #
    # Calculate average physical damage dealt for a single attack.
    # https://www.bg-wiki.com/ffxi/Weapon_Skill_Damage
    #
    phys = int(  ((wpn_dmg + fstr_wpn + wsc)*ftp  * (1+wsd)*(1+ws_bonus)*(1+ws_trait)) + sneak_attack_bonus + trick_attack_bonus + climactic_flourish_bonus + striking_flourish_bonus + ternary_flourish_bonus) * pdif * (1 + min(crit_rate,1.0)*min(crit_dmg,1.0))
    return(phys)
