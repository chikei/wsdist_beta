'''
File containing code to build the GUI

    TODO:
        Finish Automaton Tab (merge with quicklook frame?)
        Add tooltips throughout.
    
Author: Kastra (Asura server)
'''

import numpy as np
import os, sys
sys.path.append(os.path.dirname(sys.executable))

from collections.abc import Iterable
from typing import Any, cast

from PySide6 import QtGui, QtWidgets

import json
import pickle
import random

import importlib

# Import other code related to this project.
import gear as gear_pyfile
import create_player as create_player_pyfile
import actions as actions_pyfile
import buffs as buffs_pyfile
from gpt_manage_defaults import *

from app_state import AppState
from wsdist_types import GearPiece
from tabs.stats_tab import StatsTab
from tabs.automaton_tab import AutomatonTab
from tabs.optimize_tab import OptimizeTab
from tabs.simulate_tab import SimulateTab
from tabs.quicklook_tab import QuicklookTab


class application(QtWidgets.QMainWindow):

    states: dict[str, dict[str, Any]]

    def reload_gear_pyfile(self,):
        '''
        Reloads gear.py and enemies.py to allow seeing live changes to them with the GUI open.
        '''
        importlib.reload(gear_pyfile)

        # Automatically re-equip gear to see changes immediately.
        for slot in self.equipment_button_positions:
            self.quicklook_tab.update_quicklook_equipment((slot, self.quicklook_equipped_dict[slot]["item"]["Name2"], "quicklook"))
            self.quicklook_tab.update_quicklook_equipment((slot, self.tp_quicklook_equipped_dict[slot]["item"]["Name2"], "tp"))
            self.quicklook_tab.update_quicklook_equipment((slot, self.ws_quicklook_equipped_dict[slot]["item"]["Name2"], "ws"))

        # Automatically reset main and subjobs to trigger refreshing the gear selection lists.
        self.quicklook_tab.update_job("main static")
        self.quicklook_tab.update_job("sub")

    def save_defaults(self,):
        '''
        When clicking the "Save Defaults" button.
        Save relevant GUI selections to an output JSON file to be read in later.
        The outfile file is a dictionary with keys being jobs and values being dictionaries containing the relevant GUI parameters to be loaded for each job.
        '''
        main_job = self.main_job_combobox.currentText()

        state = {}
        for w in walk_widgets(self): # Get a list of ALL widgets in the GUI
            
            key = getattr(w, "save_name", w.objectName()) # Use save_name attribute if present, otherwise use the widget's name
            if "defaults_" in key: # Only save widgets for items manually named "defaults_{stuff}"
                value = get_widget_state(w)
                state[key] = value # key and value are both string representations of the widget and its value.

        for slot in self.quicklook_equipped_dict:
            state[f"quicklook_{slot}_item"] = self.quicklook_equipped_dict[slot]["item"]["Name2"]
            state[f"tp_quicklook_{slot}_item"] = self.tp_quicklook_equipped_dict[slot]["item"]["Name2"]
            state[f"ws_quicklook_{slot}_item"] = self.ws_quicklook_equipped_dict[slot]["item"]["Name2"]

        self.states["default"] = state
        self.states[main_job] = state

        with open("defaults.json", "w") as f:
            json.dump(self.states, f, indent=2, sort_keys=True)
        print(f"File updated: defaults.json (default, {main_job})")

    def read_states(self) -> dict[str, dict[str, Any]]:
        '''
        Load the saved job profiles, preferring defaults.json.

        Falls back to the legacy defaults.pkl when no JSON file exists yet, then
        writes defaults.json so the next read uses the new format. The pickle path
        runs arbitrary code on load, so only the user's own local file is trusted.
        '''
        if os.path.isfile("defaults.json"):
            with open("defaults.json") as f:
                return json.load(f)

        with open("defaults.pkl", "rb") as f:
            states = pickle.load(f)
        with open("defaults.json", "w") as f:
            json.dump(states, f, indent=2, sort_keys=True)
        print("Migrated legacy defaults.pkl -> defaults.json")
        return states

    def load_defaults(self, type: str = "default") -> None:
        '''
        When clicking the "Load Defaults" button or when changing main jobs.
        Read the defaults.json file and load the state saved in it for the currently selected main job.
        '''
        self.states = self.read_states()
        selection = "default"
        try:
            main_job = self.main_job_combobox.currentText()

            selection = "default" if type=="default" else main_job
            if len(self.states[selection]) == 0:
                print(f"No defaults profile found for {selection}")
                return

            state = self.states[selection]

            # Check all of the widgets in the GUI and update those with saved states.
            for w in walk_widgets(self):
                key = getattr(w, "save_name", w.objectName())
                if key in state:
                    set_widget_state(w, state[key])

            # Build the quicklook equipment.
            for slot in self.quicklook_equipped_dict:
                try:
                    saved_item = gear_pyfile.all_gear[state[f"quicklook_{slot}_item"]]
                    self.quicklook_equipped_dict[slot]["item"] = saved_item
                    self.quicklook_equipped_dict[slot]["icon"] = self.get_equipment_icon(saved_item["Name"])
                    self.set_button_icon(self.quicklook_equipped_dict[slot]["button"], self.quicklook_equipped_dict[slot]["icon"])
                    self.quicklook_equipped_dict[slot]["button"].setToolTip(self.format_tooltip_stats(saved_item))
                except Exception as err:
                    print(err)

                try:
                    saved_item = gear_pyfile.all_gear[state[f"tp_quicklook_{slot}_item"]]
                    self.tp_quicklook_equipped_dict[slot]["item"] = saved_item
                    self.tp_quicklook_equipped_dict[slot]["icon"] = self.get_equipment_icon(saved_item["Name"])
                    self.set_button_icon(self.tp_quicklook_equipped_dict[slot]["button"], self.tp_quicklook_equipped_dict[slot]["icon"])
                    self.tp_quicklook_equipped_dict[slot]["button"].setToolTip(self.format_tooltip_stats(saved_item))
                except Exception as err:
                    print(err)

                try:
                    saved_item = gear_pyfile.all_gear[state[f"ws_quicklook_{slot}_item"]]
                    self.ws_quicklook_equipped_dict[slot]["item"] = saved_item
                    self.ws_quicklook_equipped_dict[slot]["icon"] = self.get_equipment_icon(saved_item["Name"])
                    self.set_button_icon(self.ws_quicklook_equipped_dict[slot]["button"], self.ws_quicklook_equipped_dict[slot]["icon"])
                    self.ws_quicklook_equipped_dict[slot]["button"].setToolTip(self.format_tooltip_stats(saved_item))
                except Exception as err:
                    print(err)
                    
            # Call the "update" functions to ensure everything is up to date.
            for input in ("main", "sub", "master_level"):
                if type != input: # Do not run update_job("main") if load_defaults was called from "main", since this would be an infinite loop.
                    self.quicklook_tab.update_job(input) 
            for input in [f"set {k}" for k in ["dia", "haste", "boost", "storm", "Indi-", "Geo-", "Entrust-", "food"]+[f"Song{i+1}" for i in range(4)]+[f"Roll{i+1}" for i in range(4)]]:
                self.quicklook_tab.update_buffs(input)
            for slot in self.equipment_button_positions:
                self.quicklook_tab.update_quicklook_equipment((slot, self.quicklook_equipped_dict[slot]["item"]["Name2"], "quicklook"))
                self.quicklook_tab.update_quicklook_equipment((slot, self.tp_quicklook_equipped_dict[slot]["item"]["Name2"], "tp"))
                self.quicklook_tab.update_quicklook_equipment((slot, self.ws_quicklook_equipped_dict[slot]["item"]["Name2"], "ws"))
            self.simulate_tab.quicklook("show stats quicklook")
        except Exception as err:
            print(err)
            print(f"Failed to load default values for {selection}")
            return

    def format_tooltip_stats(self, item: GearPiece) -> str:
        '''
        Given a dictionary containing an item's stats, create a string to display with that item's icon as a tooltip.
        Returns a string.
        '''
        ignore_stats = ["Jobs","Name","Name2","Type","Skill Type","Rank"] # Do not include these stats in the tooltip
        wpn_stats = ["DMG","Delay"] # DMG and Delay show up first if available
        base_stats = ["STR", "DEX", "VIT", "AGI", "INT", "MND", "CHR"] # Base parameters show up on their own line.
        main_stats = ["Accuracy","Attack","Ranged Accuracy","Ranged Attack","Magic Accuracy","Magic Damage","Magic Attack"]
        def_stats = ["Evasion","Magic Evasion", "Magic Defense","DT","MDT","PDT","MDT2","PDT2","Subtle Blow","Subtle Blow II",]

        tooltip = f"{item['Name2' if 'Name2' in item else 'Name']}\n" # Start with the item's unique name

        nl = False # nl = NL = New Line: insert a new line to force a line break
        for k in wpn_stats:
            if item.get(k,False):
                tooltip += f"{k}:{item[k]},"
                nl = True
            if k=="Delay" and nl:
                tooltip += "\n"

        nl = False
        for k in base_stats:
            if item.get(k,False):
                tooltip += f"{k}:{item[k]},"
                nl = True
            if nl and k=="CHR":
                tooltip += "\n"

        nl = False
        for k in main_stats:
            if item.get(k,False):
                tooltip += f"{k}:{item[k]},"
                nl = True
            if "Attack" in k and nl:
                tooltip += "\n"
                nl = False
        for k in item:
            if k in base_stats or k in ignore_stats or k in main_stats or k in wpn_stats or k in def_stats:
                continue
            tooltip += f"{k}:{item[k]}\n"

        nl = False
        for k in def_stats:
            if item.get(k,False):
                tooltip += f"{k}:{item[k]},"
                nl = True
            if "Def" in k and nl:
                tooltip += "\n"
                nl = False

        return tooltip.strip()

    def get_equipment_icon(self, item_name: str = "Empty") -> QtGui.QPixmap:
        try:
            item_id = self.item_id_dict["id"][self.item_id_dict["name"]==item_name.lower()][0]
            icon = QtGui.QPixmap(f"icons32/{item_id}.png")
            if icon.isNull():
                raise FileNotFoundError(f"icons32/{item_id}.png")
        except Exception:
            print(f"Missing icon image file for \"{item_name}\"")
            icon = self.get_equipment_icon(np.random.choice(["fire", "earth", "water", "wind", "ice", "thunder", "light", "dark"]) + " attachment")
        return icon

    def set_button_icon(self, button: QtWidgets.QAbstractButton, icon: QtGui.QPixmap) -> None:
        '''Set an equipment-slot button's icon from a QPixmap.'''
        button.setIcon(QtGui.QIcon(icon))
        button.setIconSize(icon.size())

    def _set_combo_values(self, combo: QtWidgets.QComboBox, values: Iterable[Any]) -> None:
        '''Replace a QComboBox's items, preserving the current selection if still valid.'''
        current = combo.currentText()
        combo.blockSignals(True)
        combo.clear()
        combo.addItems([str(v) for v in values])
        if current and combo.findText(current) >= 0:
            combo.setCurrentText(current)
        combo.blockSignals(False)


    def set_visible_frame(self, frame: QtWidgets.QWidget) -> None:
        '''Raise a scrollframe to the top of its QStackedLayout parent.'''
        parent = frame.parentWidget()
        assert parent is not None, "scrollframe has no parent widget"
        stack = cast(QtWidgets.QStackedLayout, parent.layout())
        stack.setCurrentWidget(frame)


    def test_gui(self) -> None:
        '''
        Run a series of generic tests to ensure there are no errors
        TODO: Determine proper tests to implement...
        '''
        
        main_job = self.jobs_dict[self.main_job_combobox.currentText()]
        sub_job = self.jobs_dict.get(self.sub_job_selection_combobox.currentText(), "None")
        master_level = int(self.master_level_combobox.currentText())

        ws_name = self.ws_selection_combobox.currentText()
        ws_type = "ranged" if ws_name in (self.ws_dict["Marksmanship"]+self.ws_dict["Archery"]) else "melee"
        spell_name = self.spell_selection_combobox.currentText()

        tp_entry_value = int(self.tp_entry_box.text() or 0)

        special_toggles_dict = {k:self.all_special_toggles_dict[k]["checkbox"].isChecked() for k in self.all_special_toggles_dict if k not in buffs_pyfile.misc_debuffs}
        special_toggles_dict["Enhancing Skill"] = int(self.enhancing_skill_entry.text() or 0)
        special_toggles_dict["Aftermath"] = int(self.aftermath_combobox.currentText())
        special_toggles_dict["Storm spell"] = self.whm_selections_dict["Storm"]
        special_toggles_dict["Enemy Resist Rank"] = self.resist_rank_combobox.currentText()
        special_toggles_dict["99999"] = True

        active_buffs, active_debuffs = self.quicklook_tab.aggregate_buffs()

        # Build a player character with one piece of gear equipped, checking all possible items one at a time.
        for slot in gear_pyfile.gear_dict:
            for item in gear_pyfile.gear_dict[slot]:
                equipped_gearset: dict[str, GearPiece] = {slot0:gear_pyfile.Empty for slot0 in gear_pyfile.gear_dict}
                equipped_gearset[slot] = item
                player = create_player_pyfile.create_player(main_job, sub_job, master_level, gearset=equipped_gearset, buffs=active_buffs, abilities=special_toggles_dict,)

        random_stats = ["Magic Haste", "Attack%", "Ranged Attack%"]
        for slot in gear_pyfile.gear_dict:
            if slot not in ["main", "sub", "ranged", "ammo"]:
                for item in gear_pyfile.gear_dict[slot]:
                    for stat in item:
                        if stat not in random_stats and stat not in ["Name", "Name2", "Jobs"]:
                            random_stats.append(stat)

        spell_list = [k for job in self.spells_dict for k in self.spells_dict[job]]

        # Build N random ML, job, buffs, toggles, ws, spell, combinations and return WS damage
        for _ in range(10000):
            special_toggles_dict = {k:self.all_special_toggles_dict[k]["checkbox"].isChecked() for k in self.all_special_toggles_dict if k not in buffs_pyfile.misc_debuffs}
            special_toggles_dict["99999"] = True
            special_toggles_dict_random: dict[str, Any] = {}
            for ability_name in special_toggles_dict:
                if isinstance(special_toggles_dict[ability_name], bool):
                    special_toggles_dict_random[ability_name] = np.random.uniform() < 0.5
            
                special_toggles_dict_random["Enhancing Skill"] = np.random.randint(0, 700)
                special_toggles_dict_random["Aftermath"] = np.random.choice([0,1,2,3])
                special_toggles_dict_random["Storm spell"] = np.random.choice(self.storm_spell_list)
                special_toggles_dict_random["Enemy Resist Rank"] = np.random.choice(self.enemy_resist_ranks_list)
                

            main_job = np.random.choice(list(self.jobs_dict.values()))
            sub_job = np.random.choice(list(self.jobs_dict.values()) + ["None"])
            master_level = np.random.randint(0, 51)

            equipped_gearset = {slot:random.choice(gear_pyfile.gear_dict[slot]) for slot in gear_pyfile.gear_dict}
            while equipped_gearset["main"]["Name"] == "Empty":
                equipped_gearset["main"] = random.choice(gear_pyfile.gear_dict["main"])

            main_skill_type = equipped_gearset["main"]["Skill Type"]
            ranged_skill_type = equipped_gearset["ranged"].get("Skill Type", "None")

            ws_list = list(self.ws_dict[main_skill_type])
            if ranged_skill_type in self.ws_dict and ranged_skill_type != "None":
                ws_list = ws_list + self.ws_dict[ranged_skill_type]

                ranged_type = equipped_gearset["ranged"].get("Type", "None")
                ammo_type = "None"
                if ranged_type == "Crossbow":
                    ammo_type = "Bolt"
                elif ranged_type == "Gun":
                    ammo_type = "Bullet"
                elif ranged_type == "Bow":
                    ammo_type = "Arrow"

                forced_ammo = random.choice([k for k in gear_pyfile.ammos if k.get("Type", "None")==ammo_type])
                equipped_gearset["ammo"] = forced_ammo

            ws_name = np.random.choice(ws_list)

            ws_type = "ranged" if ws_name in (self.ws_dict["Marksmanship"]+self.ws_dict["Archery"]) else "melee"

            tp_entry_value = np.random.randint(1000,3001)

            player = create_player_pyfile.create_player(main_job, sub_job, master_level, gearset=equipped_gearset, buffs=active_buffs, abilities=special_toggles_dict_random,)

            useful_enemy_stats = {stat:int(self.enemy_input_obj[stat].text() or 0) for stat in self.enemy_input_obj}
            enemy = create_player_pyfile.create_enemy(useful_enemy_stats)
            for stat in active_debuffs:
                if "requirement" in stat.lower():
                    continue
                if stat == "Defense":
                    enemy.stats["Defense"] *= (1 - active_debuffs.get("Defense", 0))
                else:
                    enemy.stats[stat] -= active_debuffs[stat]
            enemy.stats["Base Defense"] = useful_enemy_stats["Defense"]
            enemy.stats["Magic Defense"] = max(-50, enemy.stats.get("Magic Defense", 0)) # Enemy Magic Defense can not be brought lower than -50 (magic damage taken x2)
            enemy.stats["Magic Damage Taken"] = enemy.stats.pop("Magic DT%")

            active_buffs: dict[str, dict[str, Any]] = {"cor":{}, "brd":{}, "whm":{}}
            for job in active_buffs:
                if np.random.uniform() < 0.1:
                    continue
                for stat in random_stats:
                    if np.random.uniform() < 0.8:
                        continue
                    if stat in ["Crit Rate", "Crit Damage", "ftp"] or "%" in stat or "haste" in stat.lower():
                        active_buffs[job][stat] = np.random.uniform(0, 0.5)
                    elif stat in ["DA", "TA", "QA", *[f"OA{x} {j}" for x in [2,3,4,5,6,7,8] for j in ["main", "sub"]]]:
                        active_buffs[job][stat] = np.random.uniform(0, 50)
                    elif stat == "Fencer":
                        active_buffs[job][stat] = np.random.randint(0,9)
                    else:
                        active_buffs[job][stat] = np.random.uniform(0, 200)

                                             
            spell_name = np.random.choice(np.unique(spell_list))
            while spell_name in ["Barrage", "Ranged Attack"] and equipped_gearset["ranged"]["Skill Type"] not in ["Marksmanship", "Archery"]:
                spell_name = np.random.choice(np.unique(spell_list))

            if "ton: " in spell_name.lower():
                spell_type = "Ninjutsu"
            elif spell_name.split()[-1].lower() == "shot":
                spell_type = "Quick Draw"
            elif "Banish" in spell_name or "Holy" in spell_name:
                spell_type = "Divine Magic"
            elif spell_name in ["Ranged Attack", "Barrage"]:
                spell_type = "Ranged Attack"
            else:
                spell_type = "Elemental Magic"

            actions_pyfile.average_ws(player, enemy, ws_name, tp_entry_value, ws_type, "Damage dealt")
            actions_pyfile.cast_spell(player, enemy, spell_name, spell_type, "Damage dealt")
            actions_pyfile.average_attack_round(player, enemy, 0, tp_entry_value, "Time to WS")

    def __init__(self):
        super().__init__()

        # Bind the "q" key to close the application.
        QtGui.QShortcut(QtGui.QKeySequence("q"), self).activated.connect(self.close)

        self.setWindowTitle("Kastra FFXI Damage Simulator  (2026 May 21a)") # pyinstaller --exclude-module gear --exclude-module enemies --clean --onefile --icon=icons32/23937.ico gui_main.py
        # self.setFixedSize(700, 850)
        self.setWindowIcon(QtGui.QIcon("icons32/23937.png")) # hat

        # Define the GUI tabs as a notebook of pages.
        self.notebook = QtWidgets.QTabWidget()
        self.setCentralWidget(self.notebook)


        # Top-level application menus.
        self.menu_bar = self.menuBar()
        self.file_menu = self.menu_bar.addMenu("File")
        self.file_menu.addAction("Save Defaults", self.save_defaults)
        self.file_menu.addSeparator()
        self.file_menu.addAction("Reload gear.py", self.reload_gear_pyfile)
        self.file_menu.addSeparator()
        self.file_menu.addAction("Close GUI (q)", self.close)

        self.settings_menu = self.menu_bar.addMenu("Settings")

        self.damage_limit99999 = self.settings_menu.addAction("Damage Limit (DPS)")
        self.damage_limit99999.setCheckable(True)

        self.verbose_swaps = self.settings_menu.addAction("Verbose Swaps")
        self.verbose_swaps.setCheckable(True)


        '''
        ===============================================
            Define re-usable lists and dictionaries
        ===============================================
        '''

        # Shared read-only reference data lives in AppState. Alias the hot lookups onto
        # the controller so existing handlers keep using self.<name> unchanged.
        self.state = AppState()
        self.item_id_dict = self.state.item_id_dict
        self.all_equipment_dict = self.state.all_equipment_dict
        self.ws_dict = self.state.ws_dict
        self.jobs_dict = self.state.jobs_dict
        self.rema_weapons = self.state.rema_weapons
        self.spells_dict = self.state.spells_dict
        self.equipment_button_positions = self.state.equipment_button_positions
        self.tvr_rings = self.state.tvr_rings
        self.soa_rings = self.state.soa_rings

        self.quicklook_tab = QuicklookTab(self)
        self.notebook.addTab(self.quicklook_tab, "Quicklook")
        QtGui.QShortcut(QtGui.QKeySequence("Ctrl+1"), self).activated.connect(lambda: self.notebook.setCurrentWidget(self.quicklook_tab))
        # Alias quicklook-tab widgets read by controller-kept methods
        # (save_defaults/load_defaults/reload_gear_pyfile/test_gui/bootstrap).
        self.quicklook_equipped_dict          = self.quicklook_tab.quicklook_equipped_dict
        self.quicklook_scrollframes           = self.quicklook_tab.quicklook_scrollframes
        self.main_job_combobox                = self.quicklook_tab.main_job_combobox
        self.sub_job_selection_combobox       = self.quicklook_tab.sub_job_selection_combobox
        self.master_level_combobox            = self.quicklook_tab.master_level_combobox
        self.ws_selection_combobox            = self.quicklook_tab.ws_selection_combobox
        self.spell_selection_combobox         = self.quicklook_tab.spell_selection_combobox
        self.tp_entry_box                     = self.quicklook_tab.tp_entry_box
        self.all_special_toggles_dict         = self.quicklook_tab.all_special_toggles_dict
        self.enhancing_skill_entry            = self.quicklook_tab.enhancing_skill_entry
        self.aftermath_combobox               = self.quicklook_tab.aftermath_combobox
        self.resist_rank_combobox             = self.quicklook_tab.resist_rank_combobox
        self.whm_selections_dict              = self.quicklook_tab.whm_selections_dict
        self.enemy_input_obj                  = self.quicklook_tab.enemy_input_obj
        self.storm_spell_list                 = self.quicklook_tab.storm_spell_list
        self.enemy_resist_ranks_list          = self.quicklook_tab.enemy_resist_ranks_list


        self.optimize_tab = OptimizeTab(self)
        self.notebook.addTab(self.optimize_tab, "Optimize")
        QtGui.QShortcut(QtGui.QKeySequence("Ctrl+2"), self).activated.connect(lambda: self.notebook.setCurrentWidget(self.optimize_tab))

        # Cross-tab couplings via Qt signals (loose coupling; tabs never poke each other directly).
        self.quicklook_tab.slotsRefiltered.connect(self.optimize_tab.apply_slot_refilter)
        self.optimize_tab.bestSetReady.connect(self.quicklook_tab.equip_set)


        self.simulate_tab = SimulateTab(self)
        self.notebook.addTab(self.simulate_tab, "Simulate")
        QtGui.QShortcut(QtGui.QKeySequence("Ctrl+3"), self).activated.connect(lambda: self.notebook.setCurrentWidget(self.simulate_tab))
        # Alias TP/WS gear dicts and scrollframes so reload_gear_pyfile/load_defaults/bootstrap
        # keep reaching them as controller attributes.
        self.tp_quicklook_equipped_dict = self.simulate_tab.tp_quicklook_equipped_dict
        self.ws_quicklook_equipped_dict = self.simulate_tab.ws_quicklook_equipped_dict
        self.tp_quicklook_scrollframes = self.simulate_tab.tp_quicklook_scrollframes
        self.ws_quicklook_scrollframes = self.simulate_tab.ws_quicklook_scrollframes



        '''
        ==============================================================================================
         Build the "show stats" frame
        ==============================================================================================
        '''

        self.stats_tab = StatsTab(self)
        self.notebook.addTab(self.stats_tab, "Player Stats")
        QtGui.QShortcut(QtGui.QKeySequence("Ctrl+4"), self).activated.connect(lambda: self.notebook.setCurrentWidget(self.stats_tab))


        '''
        ==============================================================================================
            Build the Automaton tab.
            Work in progress. Still need details on automaton stats anyway.
        ==============================================================================================
        '''
        if False:
            self.automaton_tab = AutomatonTab(self)
            self.notebook.addTab(self.automaton_tab, "Automaton")
            QtGui.QShortcut(QtGui.QKeySequence("Ctrl+5"), self).activated.connect(lambda: self.notebook.setCurrentWidget(self.automaton_tab))
            self.automaton_equipped_dict = self.automaton_tab.automaton_equipped_dict
            self.notebook.setCurrentWidget(self.automaton_tab)
        '''
        ==============================================================================================
         The GUI has been built at this point.
         Call the update functions to set good values in all entries.
        ==============================================================================================
        '''
        if os.path.isfile("defaults.json") or os.path.isfile("defaults.pkl"):
            self.load_defaults("default")
        else:
            # Dictionary containing the job profiles for saving/loading defaults by main job selection.
            # Saved to defaults.json when using "Save Defaults" button.
            self.states = {"default":{}, **{job:{} for job in self.jobs_dict.keys()}}

            self.quicklook_tab.update_job("main")
            self.quicklook_tab.update_job("sub")
            for slot in self.quicklook_equipped_dict:
                self.quicklook_tab.update_quicklook_equipment((slot, self.quicklook_equipped_dict[slot]["item"]["Name2"], "quicklook"))
                self.quicklook_tab.update_quicklook_equipment((slot, self.tp_quicklook_equipped_dict[slot]["item"]["Name2"], "tp"))
                self.quicklook_tab.update_quicklook_equipment((slot, self.ws_quicklook_equipped_dict[slot]["item"]["Name2"], "ws"))

        self.set_visible_frame(self.quicklook_scrollframes["main"])
        self.set_visible_frame(self.tp_quicklook_scrollframes["main"])
        self.set_visible_frame(self.ws_quicklook_scrollframes["main"])

if __name__ == "__main__":

    qt_app = QtWidgets.QApplication(sys.argv)
    window = application()
    window.show()
    qt_app.exec()
