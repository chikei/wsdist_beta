import unittest

import gear
from create_player import create_player
from wsdist import (
    OptimizerOptions,
    _dt_for_player,
    _fast_dt_for_gearset,
    _is_valid_gearset,
    _jse_ear_names,
    _normalize_optimizer_options,
)


def base_set() -> dict[str, dict[str, object]]:
    return {
        "main": gear.Heishi,
        "sub": gear.Empty,
        "ranged": gear.Empty,
        "ammo": gear.Empty,
        "head": gear.Empty,
        "neck": gear.Empty,
        "ear1": gear.Empty,
        "ear2": gear.Empty,
        "body": gear.Empty,
        "hands": gear.Empty,
        "ring1": gear.Empty,
        "ring2": gear.Empty,
        "back": gear.Empty,
        "waist": gear.Empty,
        "legs": gear.Empty,
        "feet": gear.Empty,
    }


class OptimizerHelperTest(unittest.TestCase):
    def valid(self, gearset: dict[str, dict[str, object]], action_type: str = "attack round", ws_name: str = "", spell_name: str = "") -> bool:
        return _is_valid_gearset(
            gearset,
            "nin",
            "war",
            action_type,
            ws_name,
            spell_name,
            {"Katana": ["Blade: Shun"], "Archery": ["Namas Arrow"], "Marksmanship": ["Last Stand"]},
            {"Blade: Metsu": "Kikoku"},
            _jse_ear_names(),
        )

    def test_rejects_duplicate_unique_jewelry(self) -> None:
        gearset = base_set()
        gearset["ring1"] = gear.Epona_Ring
        gearset["ring2"] = gear.Epona_Ring

        self.assertFalse(self.valid(gearset))

    def test_rejects_invalid_offhand_weapon_for_non_dual_wield_job(self) -> None:
        gearset = base_set()
        gearset["sub"] = gear.Crepuscular_Knife

        self.assertFalse(
            _is_valid_gearset(
                gearset,
                "war",
                "drg",
                "attack round",
                "",
                "",
                {},
                {},
                _jse_ear_names(),
            )
        )

    def test_rejects_ranged_ammo_mismatch(self) -> None:
        gearset = base_set()
        gearset["ranged"] = gear.Donar_Gun
        gearset["ammo"] = gear.Yoichi_Arrow

        self.assertFalse(self.valid(gearset))

    def test_rejects_impact_without_required_cloak(self) -> None:
        gearset = base_set()

        self.assertFalse(self.valid(gearset, action_type="spell cast", spell_name="Impact"))

    def test_accepts_impact_with_required_cloak_and_empty_head(self) -> None:
        gearset = base_set()
        gearset["body"] = gear.Twilight_Cloak

        self.assertTrue(self.valid(gearset, action_type="spell cast", spell_name="Impact"))

    def test_rejects_restricted_weapon_skill_without_required_weapon(self) -> None:
        gearset = base_set()

        self.assertFalse(self.valid(gearset, action_type="weapon skill", ws_name="Blade: Metsu"))

    def test_fast_dt_matches_create_player_for_plain_gear_and_buffs(self) -> None:
        gearset = base_set()
        gearset["ring1"] = gear.Defending_Ring
        gearset["ring2"] = gear.Gelatinous_Ring
        gearset["body"] = gear.Malignance_Tabard
        buffs = {"whm": {"MDT": -29.0}}

        player = create_player("nin", "war", 50, gearset, buffs, {})

        self.assertEqual(_fast_dt_for_gearset(gearset, buffs), _dt_for_player(player))

    def test_optimizer_options_are_bounded_and_seeded(self) -> None:
        options = _normalize_optimizer_options(
            OptimizerOptions(
                iterations=0,
                max_swap_slots=9,
                restart_count=0,
                seed=123,
                dt_step=0,
            )
        )

        self.assertEqual(options.iterations, 1)
        self.assertEqual(options.max_swap_slots, 2)
        self.assertEqual(options.restart_count, 1)
        self.assertEqual(options.seed, 123)
        self.assertEqual(options.dt_step, 1)


if __name__ == "__main__":
    unittest.main()
