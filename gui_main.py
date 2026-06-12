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

from PIL import Image

from PySide6 import QtCore, QtGui, QtWidgets

import pickle

import importlib

# Import other code related to this project.
import enemies as enemies_pyfile
import gear as gear_pyfile
import create_player as create_player_pyfile
import actions as actions_pyfile
import buffs as buffs_pyfile
import wsdist as wsdist_pyfile
import fancy_plot as fancy_plot_pyfile
from lumo_scrollablelabelframe import ScrollableLabelFrame # TODO: Replace with ChatGPT's virtual_frames
from gpt_manage_defaults import *

from virtual_frames import VirtualCheckboxFrame, VirtualRadioFrame


class WheelIntLineEdit(QtWidgets.QLineEdit):
    '''Integer entry that increments/decrements on mouse wheel, clamped to [lo, hi].'''

    def __init__(self, value=0, lo=-50, hi=100, step=1, parent=None):
        super().__init__(str(value), parent)
        self._lo = lo
        self._hi = hi
        self._step = step
        self.setValidator(QtGui.QIntValidator(lo, hi, self))
        self.setAlignment(QtCore.Qt.AlignmentFlag.AlignHCenter)

    def value(self):
        try:
            return int(self.text())
        except ValueError:
            return 0

    def wheelEvent(self, event):
        step = self._step if event.angleDelta().y() > 0 else -self._step
        self.setText(str(max(self._lo, min(self._hi, self.value() + step))))
        event.accept()


class application(QtWidgets.QMainWindow):

    def reload_gear_pyfile(self,):
        '''
        Reloads gear.py and enemies.py to allow seeing live changes to them with the GUI open.
        '''
        importlib.reload(gear_pyfile)

        # Automatically re-equip gear to see changes immediately.
        for slot in self.equipment_button_positions:
            self.update_quicklook_equipment((slot, self.quicklook_equipped_dict[slot]["item"]["Name2"], "quicklook"))
            self.update_quicklook_equipment((slot, self.tp_quicklook_equipped_dict[slot]["item"]["Name2"], "tp"))
            self.update_quicklook_equipment((slot, self.ws_quicklook_equipped_dict[slot]["item"]["Name2"], "ws"))

        # Automatically reset main and subjobs to trigger refreshing the gear selection lists.
        self.update_job("main static")
        self.update_job("sub")

    def save_defaults(self,):
        '''
        When clicking the "Save Defaults" button.
        Save relevant GUI selections to an output pickle file to be read in later.
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

        with open("defaults.pkl", "wb") as f:
            pickle.dump(self.states, f)
        print(f"File updated: defaults.pkl (default, {main_job})")

    def load_defaults(self, type="default"):
        '''
        When clicking the "Load Defaults" button or when changing main jobs.
        Read the defaults.pkl file and load the state saved in it for the currently selected main job.
        '''
        with open("defaults.pkl", "rb") as f:
            self.states = pickle.load(f)
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
                    self.update_job(input) 
            for input in [f"set {k}" for k in ["dia", "haste", "boost", "storm", "Indi-", "Geo-", "Entrust-", "food"]+[f"Song{i+1}" for i in range(4)]+[f"Roll{i+1}" for i in range(4)]]:
                self.update_buffs(input)
            for slot in self.equipment_button_positions:
                self.update_quicklook_equipment((slot, self.quicklook_equipped_dict[slot]["item"]["Name2"], "quicklook"))
                self.update_quicklook_equipment((slot, self.tp_quicklook_equipped_dict[slot]["item"]["Name2"], "tp"))
                self.update_quicklook_equipment((slot, self.ws_quicklook_equipped_dict[slot]["item"]["Name2"], "ws"))
            self.quicklook("show stats quicklook")
        except Exception as err:
            print(err)
            print(f"Failed to load default values for {selection}")
            return

    def format_tooltip_stats(self, item):
        '''
        Given a dictionary containing an item's stats, create a string to display with that item's icon as a tooltip.
        Returns a string.
        '''
        ignore_stats = ["Jobs","Name","Name2","Type","Skill Type","Rank"] # Do not include these stats in the tooltip
        wpn_stats = ["DMG","Delay"] # DMG and Delay show up first if available
        base_stats = ["STR", "DEX", "VIT", "AGI", "INT", "MND", "CHR"] # Base parameters show up on their own line.
        main_stats = ["Accuracy","Attack","Ranged Accuracy","Ranged Attack","Magic Accuracy","Magic Damage","Magic Attack"]
        all_stats = ["Striking Crit Rate","Climactic Crit Damage","Klimaform Damage%","Ebullience Bonus","Occult Acumen","Futae Bonus","WSC","Zanshin OA2","Recycle","Double Shot Damage%","Triple Shot Damage%","Ranged Crit Damage","Blood Pact Damage","Rank", "Kick Attacks", "Kick Attacks DMG", "Martial Arts", "Sneak Attack Bonus", "Trick Attack Bonus", "Double Shot", "True Shot","Zanshin", "Hasso", "Quick Draw Damage", "Quick Draw Magic Accuracy", "Quick Draw Damage%", "Triple Shot","Magic Crit Rate II","Magic Burst Accuracy","Fencer","JA Haste","Accuracy", "AGI", "Attack", "Axe Skill", "CHR", "Club Skill", "Crit Damage", "Crit Rate", "DA", "DA Damage%", "Dagger Skill", "Daken", "Dark Affinity", "Dark Elemental Bonus", "Delay", "DEX", "DMG", "Dual Wield", "Earth Affinity", "Earth Elemental Bonus", "Elemental Bonus", "Elemental Magic Skill", "Fire Affinity", "Fire Elemental Bonus", "ftp", "Gear Haste", "Great Axe Skill", "Great Katana Skill", "Great Sword Skill", "Hand-to-Hand Skill", "Ice Affinity", "Ice Elemental Bonus", "INT", "Jobs", "Katana Skill", "Light Affinity", "Light Elemental Bonus", "Magic Accuracy Skill", "Magic Accuracy", "Magic Attack", "Magic Burst Damage II", "Magic Burst Damage", "Magic Damage", "MND", "Name", "Name2", "Ninjutsu Damage%", "Ninjutsu Magic Attack","Ninjutsu Magic Accuracy", "Ninjutsu Skill", "OA2", "OA3", "OA4", "OA5", "OA6", "OA7", "OA8", "PDL", "Polearm Skill", "QA", "Ranged Accuracy", "Ranged Attack", "Scythe Skill", "Skill Type", "Skillchain Bonus", "Staff Skill", "Store TP", "STR", "Sword Skill", "TA", "TA Damage%", "Throwing Skill", "Thunder Affinity", "Thunder Elemental Bonus", "TP Bonus", "Type", "VIT", "Water Affinity", "Water Elemental Bonus", "Weapon Skill Accuracy", "Weapon Skill Damage", "Weather", "Wind Affinity", "Wind Elemental Bonus","Polearm Skill","Marksmanship Skill","Archery Skill"]
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

    def get_equipment_icon(self, item_name="Empty"):
        try:
            item_id = self.item_id_dict["id"][self.item_id_dict["name"]==item_name.lower()][0]
            icon = QtGui.QPixmap(f"icons32/{item_id}.png")
            if icon.isNull():
                raise FileNotFoundError(f"icons32/{item_id}.png")
        except Exception:
            print(f"Missing icon image file for \"{item_name}\"")
            icon = self.get_equipment_icon(np.random.choice(["fire", "earth", "water", "wind", "ice", "thunder", "light", "dark"]) + " attachment")
        return icon

    def set_button_icon(self, button, icon):
        '''Set an equipment-slot button's icon from a QPixmap.'''
        button.setIcon(QtGui.QIcon(icon))
        button.setIconSize(icon.size())

    def _set_combo_values(self, combo, values):
        '''Replace a QComboBox's items, preserving the current selection if still valid.'''
        current = combo.currentText()
        combo.blockSignals(True)
        combo.clear()
        combo.addItems([str(v) for v in values])
        if current and combo.findText(current) >= 0:
            combo.setCurrentText(current)
        combo.blockSignals(False)

    def validate_tp_value(self, event=None):
        '''
        After the user clicks off the TP entry box, force the final value to be between 1000 and 3000.
        If the box is empty, use 1000 by default.
        '''
        try:
            tp_value = int(self.tp_entry_box.text())
            if tp_value < 1000:
                self.tp_entry_box.setText("1000")
            elif tp_value > 3000:
                self.tp_entry_box.setText("3000")
        except Exception as err:
            self.tp_entry_box.setText("1000")

    def select_enemy(self,):
        '''
        When selecting a new enemy from the enemy input combobox
        Loop through the stats and update the user-input values based on the preset_enemies dictionaries from enemies.py
        '''
        self.selected_enemy = enemies_pyfile.preset_enemies[self.selected_enemy_combobox.currentText()] 
        for i,stat in enumerate(self.enemy_stats_list1 + self.enemy_stats_list2):
            self.enemy_input_obj[stat].setText(str(self.selected_enemy[stat]))

        self.enemy_level_location_label.setText(f"{self.selected_enemy['Location']} (Lv.{self.selected_enemy['Level']})")

    def update_job(self, trigger):
        '''
        When selecting a new main or sub job
        Update the available spell list, special ability list, and equipment lists.
        Enact various other restrictions throughout the GUI based on job selections.
        '''

        main_job_shorthand = self.jobs_dict[self.main_job_combobox.currentText()]
        sub_job_shorthand = self.jobs_dict.get(self.sub_job_selection_combobox.currentText(), "None")
        dual_wield = (main_job_shorthand in ["nin", "dnc", "thf", "blu"] or sub_job_shorthand in ["nin", "dnc"])

        if trigger=="master level":
            self.sub_job_level = 49 + int(self.master_level_combobox.currentText()//5)

        elif "main" in trigger.lower():

            # Update the spell list options.
            if main_job_shorthand in self.spells_dict:
                new_spell_list = self.spells_dict[main_job_shorthand]
                self._set_combo_values(self.spell_selection_combobox, new_spell_list)
                if self.spell_selection_combobox.currentText() not in new_spell_list:
                    self.spell_selection_combobox.setCurrentText(new_spell_list[0])
            else:
                self._set_combo_values(self.spell_selection_combobox, ["None"])
                self.spell_selection_combobox.setCurrentText("None")

            # Update the subjob selections
            if self.main_job_combobox.currentText() == self.sub_job_selection_combobox.currentText():
                # Remove the subjob if the new main job selection matches the current sub job.
                self.sub_job_selection_combobox.setCurrentText("None")

            # Hide the newly selected main job from the sub job list.
            new_subjob_options = [k for k in sorted(self.jobs_dict) if k != self.main_job_combobox.currentText()] + ["None"]
            self._set_combo_values(self.sub_job_selection_combobox, new_subjob_options)
            # Update the virtual scrollframes to show radiobuttons and checkbuttons for items equippable by the selected job.
            for slot in self.all_equipment_dict:
                if slot == "sub":
                    allowed_subtypes = ["Shield", "Grip", "None"]
                    if dual_wield:
                        allowed_subtypes += ["Weapon"]
                    filtered_equipment_list = sorted([k["Name2" if "Name2" in k else "Name"] for k in self.all_equipment_dict["sub"] if (main_job_shorthand in k["Jobs"]) and (k["Type"] in allowed_subtypes)])
                else:
                    filtered_equipment_list = sorted([k["Name2" if "Name2" in k else "Name"] for k in self.all_equipment_dict[slot] if main_job_shorthand.lower() in k["Jobs"]])
                self.quicklook_scrollframes[slot].set_visible_data(filtered_equipment_list)
                self.tp_quicklook_scrollframes[slot].set_visible_data(filtered_equipment_list)
                self.ws_quicklook_scrollframes[slot].set_visible_data(filtered_equipment_list)
                self.optimize_scrollframes[slot].set_visible_data(filtered_equipment_list)
                self.optimize_scrollframes[slot].deselect("all")


            # Red Mage uses Boost-STAT instead of Gain-STAT. Update the corresponding buff drop-down menu and selection here.
            old_boost_stat = "None" if self.boost_combobox.currentText()=="None" else self.boost_combobox.currentText().split("-")[-1]
            
            if main_job_shorthand.lower() == "rdm":
                if old_boost_stat != "None":
                    self.boost_combobox.setCurrentText(f"Gain-{old_boost_stat}")
                self._set_combo_values(self.boost_combobox, ["Gain-"+k for k in ["STR", "DEX", "VIT", "AGI", "MND", "INT", "CHR"]] + ["None"])
                self.enhancing_skill_label.setText("Enhancing Skill:")
                self.enhancing_skill_entry.setText(str(650))

            else:
                if old_boost_stat != "None":
                    self.boost_combobox.setCurrentText(f"Boost-{old_boost_stat}")
                self._set_combo_values(self.boost_combobox, ["Boost-"+k for k in ["STR", "DEX", "VIT", "AGI", "MND", "INT", "CHR"]] + ["None"])
                if main_job_shorthand.lower() == "run":
                    self.enhancing_skill_entry.setText(str(570))
                elif main_job_shorthand.lower() == "drk":
                    self.enhancing_skill_entry.setText(str(600))
                    self.enhancing_skill_label.setText("Dark Skill:")
                elif main_job_shorthand.lower() == "pld":
                    self.enhancing_skill_entry.setText(str(600))
                    self.enhancing_skill_label.setText("Divine Skill:")
                else:
                    self.enhancing_skill_label.setText("Enhancing Skill:")
                    self.enhancing_skill_entry.setText(str(500))
        
            if os.path.isfile("defaults.pkl") and "static" not in trigger:
                self.load_defaults("main") # Update GUI to reflect the new main job
            
        elif trigger=="sub":
            # Hide weapons in off-hand slot unless new main+sub combo allows dual wielding.
            allowed_subtypes = ["Shield", "Grip", "None"]
            if dual_wield:
                allowed_subtypes += ["Weapon"]
            else:
                restricted_items = [k["Name2" if "Name2" in k else "Name"] for k in self.all_equipment_dict["sub"] if (main_job_shorthand in k["Jobs"]) and (k["Type"] == "Weapon")]
            new_off_hand_equipment_list = sorted([k["Name2" if "Name2" in k else "Name"] for k in self.all_equipment_dict["sub"] if (main_job_shorthand in k["Jobs"]) and (k["Type"] in allowed_subtypes)])
            

            self.quicklook_scrollframes["sub"].set_visible_data(new_off_hand_equipment_list)
            self.tp_quicklook_scrollframes["sub"].set_visible_data(new_off_hand_equipment_list)
            self.ws_quicklook_scrollframes["sub"].set_visible_data(new_off_hand_equipment_list)
            self.optimize_scrollframes["sub"].set_visible_data(new_off_hand_equipment_list)
            if not dual_wield:
                self.optimize_scrollframes["sub"].deselect(restricted_items)

            # Unequip off-hand weapon if not able to dual-wield.
            if not dual_wield and self.quicklook_equipped_dict["sub"]["item"]["Type"]=="Weapon":
                new_sub_item = gear_pyfile.all_gear["Empty"]
                self.quicklook_equipped_dict["sub"]["item"] = new_sub_item
                self.quicklook_equipped_dict["sub"]["icon"] = self.get_equipment_icon(new_sub_item["Name"])
                self.set_button_icon(self.quicklook_equipped_dict["sub"]["button"], self.quicklook_equipped_dict["sub"]["icon"])
                self.quicklook_equipped_dict["sub"]["button"].setToolTip(self.format_tooltip_stats(new_sub_item))

        # Hide abilities not accessible to the selected main/sub/ML combo.
        main_job = self.jobs_dict[self.main_job_combobox.currentText()]
        sub_job = self.jobs_dict[self.sub_job_selection_combobox.currentText()] if self.sub_job_selection_combobox.currentText() != "None" else "None"
        for ability_name in self.all_special_toggles_dict:

            self.all_special_toggles_dict[ability_name]["checkbox"].setVisible(False) # Hide all abilities so we can show them again in order and retain alphabetical ordering.

            if (main_job.lower() in self.all_special_toggles_dict[ability_name]["job requirement"]) or (self.sub_job_level >= self.all_special_toggles_dict[ability_name]["level requirement"] and sub_job.lower() in self.all_special_toggles_dict[ability_name]["job requirement"]):
                if (main_job.lower()=="dnc" and ability_name.lower() in ["haste samba (sub)", "box step (sub)"]) or (main_job.lower()=="war" and ability_name.lower()=="warcry (sub)"):
                    continue
                self.all_special_toggles_dict[ability_name]["checkbox"].setVisible(True)
            else:
                self.all_special_toggles_dict[ability_name]["checkbox"].setChecked(False)

    def select_gear_opt(self, event):
        '''
        When clicking one of the "Select X" buttons in the optimize tab.
        Selects or unselects all equipment based on input.
        '''
        tvr_ring_names = [k.lower()+" ring" for k in self.tvr_rings]
        soa_ring_names = [k.lower()+" ring +1" for k in self.soa_rings]

        empyrean_names = ["Hattori", "Heathen's", "Wicce", "Lethargy", "Peltast's", "Ebers", "Kasuga", "Arbatel", "Boii", "Chasseur's", "Fili", "Skulker's", "Bhikku", "Maculele", "Nukumi", "Azimuth", "Chevalier's", "Amini", "Hashishin", "Erilaz", "Karagoz", "Beckoner's"]
        relic_names = ["Pedagogy", "Hesychast", "Vitiation", "Mochizuki", "Fallen", "Horos", "Pitre", "Luhlaza", "Plunderer", "Bagua", "Archmage", "Piety", "Agoge", "Caballarius", "Wakido", "Ankusa", "Bihu", "Glyphic", "Lanun", "Arcadian", "Pteroslaver", "Futhark"]
        af_names = ["Academic", "Anchorite", "Atrophy", "Hachiya", "Ignominy", "Maxixi", "Foire", "Assimilator", "Pillager", "Geomancy", "Spaekona", "Theophany", "Pummeler", "Reverence", "Sakonji", "Totemic", "Brioso", "Convoker", "Laksamana", "Orion", "Vishap", "Runeist"]


        input_items_full = [] # Full item names
        if event == "select all file":
            # Select items from a "//gs export all" file. Items must be in the item_list.csv file.
            # TODO: Redo the item_list.csv file to include NQ items. item_list.csv currently only includes items in the gear.py file.
            # TODO: Re-add NQ item icons.
            filename, _ = QtWidgets.QFileDialog.getOpenFileName(self, 'Select file', './')

            if len(filename) > 0:
                with open(filename, "r") as ifile:
                    for line in ifile:
                        try:
                            item_name_abbreviated = line.split('"')[1]
                            item_index = np.flatnonzero(np.char.lower(self.item_id_dict["name2"]) == item_name_abbreviated.lower())
                            if len(item_index) > 0:
                                input_items_full.append(str(self.item_id_dict["name"][item_index[0]]))
                            else:
                                continue
                        except Exception as err:
                            # print(f"Failed to include item   {line}\n{err}")
                            continue

        for slot in self.quicklook_equipped_dict:

            # Only consider the selected slot when using slot-specific buttons.
            if event in ["select all slot", "unselect all slot"] and self.visible_optimize_frame_slot != slot:
                continue

            # Start by removing all selections.
            self.optimize_scrollframes[slot].deselect("all")

            # Enable all selections if using "select all" buttons.
            if event in ["select all slot", "select all"]:
                self.optimize_scrollframes[slot].select("visible")

            # Adjust specific item selections based on filters.
            for item_name in self.optimize_scrollframes[slot].visible_data:

                # Create the full-stats item dictionary for reference
                item = gear_pyfile.all_gear[item_name]

                if event == "select all file":                        
                    if (item["Name"].lower() in input_items_full): # Select direct matches first
                        self.optimize_scrollframes[slot].select(item_name)
                    elif (item["Name"].lower().split(" +")[0] in input_items_full): # Only select close match if direct match not found.
                        self.optimize_scrollframes[slot].select(item_name)


                # Deselect if the item's Odyssey rank does not match your selected Odyssey Rank.
                if str(item.get("Rank", self.ody_rank_combobox.currentText())) != self.ody_rank_combobox.currentText():
                    self.optimize_scrollframes[slot].deselect(item_name)

                # Swap Nyame R30B for R25B if specific checkbox is enabled. Deselect Nyame Paths "not B"
                if "nyame" in item_name.lower():
                    if "B" != item_name[-1]:
                        self.optimize_scrollframes[slot].deselect(item_name)
                    elif self.ody_rank_combobox.currentText()=="30" and self.nyame25_checkbox.isChecked():
                        if "30B" in item_name:
                            self.optimize_scrollframes[slot].deselect(item_name)
                        elif "25B" in item_name and event in ["select all", "select all slot"]:
                            self.optimize_scrollframes[slot].select(item_name)

                if slot in ["main", "sub", "ranged"]:
                    if item_name.split()[0] in self.rema_weapons and "R15" not in item_name:
                        self.optimize_scrollframes[slot].deselect(item_name)
                    if item_name.split()[-1] == "V":
                        self.optimize_scrollframes[slot].deselect(item_name)
                    if "kraken" in item_name.lower():
                        self.optimize_scrollframes[slot].deselect(item_name)

                if slot in ["ammo"]:
                    if "Hoxne" in item_name or "Antitail" in item_name:
                        self.optimize_scrollframes[slot].deselect(item_name)

                if slot in ["head", "body", "hands", "legs", "feet"]:
                    if event != "select all file":
                        if item_name.split()[0] in relic_names+af_names and "+4" not in item_name:
                            self.optimize_scrollframes[slot].deselect(item_name)
                        if item_name.split()[0] in empyrean_names and "+3" not in item_name:
                            self.optimize_scrollframes[slot].deselect(item_name)
                    for limbus_set_name in ["hope", "perfection", "revelation", "trust", "prestige", "sworn", "bravery", "intrepid", "indomitable", "justice", "magnificent", "duty", "mercy", "grace", "clemency"]:
                        if limbus_set_name in item_name.lower() and "R30" in item_name: # Only select R0 versions of the limbus equipment (at least for now)
                            self.optimize_scrollframes[slot].deselect(item_name)

                if slot in ["neck"]:
                    if event != "select all file":
                        if "R20" in item_name and "+1" in item_name:
                            self.optimize_scrollframes[slot].deselect(item_name)

                if slot in ["ear1", "ear2"]:
                    if item_name.split()[0] in empyrean_names and "+2" in item_name:
                        self.optimize_scrollframes[slot].deselect(item_name)
                    if "Hoxne" in item_name and self.mastery_rank_combobox.currentText().lower() not in item_name.lower():
                        self.optimize_scrollframes[slot].deselect(item_name)
                    if "Balder" in item_name:
                        self.optimize_scrollframes[slot].deselect(item_name)
                    if "(night)" in item_name.lower():
                        self.optimize_scrollframes[slot].deselect(item_name)

                if slot in ["ring1", "ring2"]:
                    if item_name.lower() in tvr_ring_names and item_name.lower() != self.tvr_ring_combobox.currentText().lower() + " ring":
                        self.optimize_scrollframes[slot].deselect(item_name)

                    if item_name.lower() in soa_ring_names and item_name.lower() != self.soa_ring_combobox.currentText().lower() + " ring +1":
                        self.optimize_scrollframes[slot].deselect(item_name)

                # if "Murky" in item_name or "Alabaster" in item_name:
                #     self.optimize_scrollframes[slot].deselect(item_name)

    def equip_best_set(self):
        '''
        When clicking the "Equip best set" button.
        Take the best gear set from an optimize run and equip it to the quicklook gear.
        '''
        for slot in self.best_player.gearset:
            self.quicklook_equipped_dict[slot]["item"] = self.best_player.gearset[slot]
            self.quicklook_equipped_dict[slot]["icon"] = self.get_equipment_icon(self.quicklook_equipped_dict[slot]["item"]["Name"])
            self.set_button_icon(self.quicklook_equipped_dict[slot]["button"], self.quicklook_equipped_dict[slot]["icon"])
            self.quicklook_equipped_dict[slot]["button"].setToolTip(self.format_tooltip_stats(self.quicklook_equipped_dict[slot]["item"]))

            best_item_name = self.best_player.gearset[slot]["Name2"]
            self.quicklook_scrollframes[slot].set_selected(best_item_name)

            self.notebook.setCurrentIndex(0)



    def copy_gearset_dict(self, event):
        '''
        When clicking the Copy to TP/WS/Quickook buttons
        Copy the gearset displayed at the source to the destination.
        '''
        if event=="quicklook to tp":
            source_dict = self.quicklook_equipped_dict
            destination_dict = self.tp_quicklook_equipped_dict
            destination_tab = "2"
            destination_scrollframe = self.tp_quicklook_scrollframes
        elif event=="quicklook to ws":
            source_dict = self.quicklook_equipped_dict
            destination_dict = self.ws_quicklook_equipped_dict
            destination_tab = "2"
            destination_scrollframe = self.ws_quicklook_scrollframes
        elif event=="tp to quicklook":
            source_dict = self.tp_quicklook_equipped_dict
            destination_dict = self.quicklook_equipped_dict
            destination_scrollframe = self.quicklook_scrollframes
            destination_tab = "0"
        elif event=="ws to quicklook":
            source_dict = self.ws_quicklook_equipped_dict
            destination_dict = self.quicklook_equipped_dict
            destination_scrollframe = self.quicklook_scrollframes
            destination_tab = "0"


        for slot in destination_dict:
            destination_dict[slot]["item"] = source_dict[slot]["item"]
            destination_dict[slot]["icon"] = self.get_equipment_icon(destination_dict[slot]["item"]["Name"])
            self.set_button_icon(destination_dict[slot]["button"], destination_dict[slot]["icon"])
            destination_dict[slot]["button"].setToolTip(self.format_tooltip_stats(destination_dict[slot]["item"]))

            source_item_name = source_dict[slot]["item"]["Name2"]
            destination_scrollframe[slot].set_selected(source_item_name)


        self.notebook.setCurrentIndex(int(destination_tab))


    def copy_to_clipboard(self, type):
        '''
        When clicking the "Copy to Clipboard" button.
        Build a set that can be copy-pasted into a gearswap lua (ignoring augments)
        '''
        if type=="quicklook":
            equipped_gear_dict = self.quicklook_equipped_dict
        elif type=="tp":
            equipped_gear_dict = self.tp_quicklook_equipped_dict
        elif type=="ws":
            equipped_gear_dict = self.quicklook_equipped_dict

        output_string = "new_set = {\n"
        for gear_slot in self.quicklook_equipped_dict:
            item_name = self.item_id_dict["name2"][self.item_id_dict["name"]==equipped_gear_dict[gear_slot]['item']['Name'].lower()][0]
            item_name = " ".join(k.capitalize() for k in item_name.split())
            output_string = output_string + f"    {gear_slot}=\"{item_name}\",\n"
        output_string = output_string + "}"

        app.clipboard_clear()
        app.clipboard_append(output_string)

    def update_quicklook_equipment(self, selection):
        '''
        When selecting a new equipment piece from the radio button lists in the quicklook frame.
        Update the item stored in the quicklook_equipped_dict object
        Update the icon shown on the 4x4 button grid.
        Update the icon HoverTip to show the new selection's stats
        Remove equipment in other slots if the new combination is not possible.


        selection is a 3-tuple:
        slot:                equipment slot under consideration
        new_item_name:       The name of the radio button clicked to trigger this function
        source:              Which caller ("quicklook"/"tp"/"ws") — selects the equipment
                             dict to modify and avoids TP/WS sets affecting the quicklook tab.
        '''
        slot, new_item_name, source = selection

        if source == "quicklook":
            equipped_items_dict = self.quicklook_equipped_dict
            scrollframe = self.quicklook_scrollframes
        elif source == "tp":
            equipped_items_dict = self.tp_quicklook_equipped_dict
            scrollframe = self.tp_quicklook_scrollframes
        elif source == "ws":
            equipped_items_dict = self.ws_quicklook_equipped_dict
            scrollframe = self.ws_quicklook_scrollframes

        new_item = gear_pyfile.all_gear[new_item_name] # New item (dictionary of stats)
        old_item = equipped_items_dict[slot]["item"]   # Old item (dictionary of stats)

        if slot in ["ring1", "ring2", "ear1", "ear2"]: # Prepare for swapping rings/earrings later for convenience.
            ring1_item_before = equipped_items_dict["ring1"]["item"]
            ring2_item_before = equipped_items_dict["ring2"]["item"]
            ear1_item_before = equipped_items_dict["ear1"]["item"]
            ear2_item_before = equipped_items_dict["ear2"]["item"]

        # Equip the new item now.
        equipped_items_dict[slot]["item"] = new_item
        equipped_items_dict[slot]["icon"] = self.get_equipment_icon(new_item["Name"])
        self.set_button_icon(equipped_items_dict[slot]["button"], equipped_items_dict[slot]["icon"])
        equipped_items_dict[slot]["button"].setToolTip(self.format_tooltip_stats(new_item))

        # Remove equipment in other slots if the new combination is not possible
        main_skill_type = equipped_items_dict["main"]["item"]["Skill Type"]
        sub_item_type = equipped_items_dict["sub"]["item"]["Type"]
        if slot == "main":
            main_job_shorthand = self.jobs_dict[self.main_job_combobox.currentText()]
            sub_job_shorthand = self.jobs_dict.get(self.sub_job_selection_combobox.currentText(), "None")
            dual_wield = (main_job_shorthand in ["nin", "dnc", "thf", "blu"] or sub_job_shorthand in ["nin", "dnc"])
            if (main_skill_type in ["Great Sword", "Great Katana", "Great Axe", "Polearm", "Scythe", "Staff"] and sub_item_type not in ["Grip", "None"]) or (main_skill_type=="Hand-to-Hand") or (main_skill_type in ["Axe", "Club", "Dagger", "Sword", "Katana"] and (sub_item_type not in ["Shield", "None"]) and not dual_wield):
                new_sub_item = gear_pyfile.all_gear["Empty"]
                equipped_items_dict["sub"]["item"] = new_sub_item
                equipped_items_dict["sub"]["icon"] = self.get_equipment_icon(new_sub_item["Name"])
                self.set_button_icon(equipped_items_dict["sub"]["button"], equipped_items_dict["sub"]["icon"])
                equipped_items_dict["sub"]["button"].setToolTip(self.format_tooltip_stats(new_sub_item))
                scrollframe["sub"].set_selected("Empty")


            if source=="quicklook":
                self.wpn_type_main = equipped_items_dict["main"]["item"]["Skill Type"]
                self._set_combo_values(self.ws_selection_combobox, self.ws_dict[self.wpn_type_main] + (self.ws_dict[self.wpn_type_ranged] if self.wpn_type_ranged not in ["None", "Instrument"] else []))
                if old_item["Skill Type"] != new_item["Skill Type"]:
                    if self.ws_selection_combobox.currentText() not in [self.ws_selection_combobox.itemText(i) for i in range(self.ws_selection_combobox.count())]:
                        self.ws_selection_combobox.setCurrentText(self.ws_dict[self.wpn_type_main][0])

        if slot == "sub":
            if (sub_item_type=="Grip" and main_skill_type not in ["Great Sword", "Great Katana", "Great Axe", "Polearm", "Scythe", "Staff"]) or (sub_item_type=="Shield" and main_skill_type not in ["None", "Axe", "Club", "Dagger", "Sword", "Katana"]):
                new_main_item = gear_pyfile.all_gear["Empty"]
                equipped_items_dict["main"]["item"] = new_main_item
                equipped_items_dict["main"]["icon"] = self.get_equipment_icon(new_main_item["Name"])
                self.set_button_icon(equipped_items_dict["main"]["button"], equipped_items_dict["main"]["icon"])
                equipped_items_dict["main"]["button"].setToolTip(self.format_tooltip_stats(new_main_item))
                scrollframe["main"].set_selected("Empty")

        ranged_item_type = equipped_items_dict["ranged"]["item"]["Type"]
        ammo_item_type = equipped_items_dict["ammo"]["item"]["Type"]
        if slot == "ranged":
            if (ranged_item_type=="Gun" and ammo_item_type != "Bullet") or (ranged_item_type=="Bow" and ammo_item_type != "Arrow") or (ranged_item_type=="Crossbow" and ammo_item_type != "Bolt") or (ranged_item_type in ["Instrument", "Equipment"]):
                new_ammo_item = gear_pyfile.all_gear["Empty"]
                equipped_items_dict["ammo"]["item"] = new_ammo_item
                equipped_items_dict["ammo"]["icon"] = self.get_equipment_icon(new_ammo_item["Name"])
                self.set_button_icon(equipped_items_dict["ammo"]["button"], equipped_items_dict["ammo"]["icon"])
                equipped_items_dict["ammo"]["button"].setToolTip(self.format_tooltip_stats(new_ammo_item))
                scrollframe["ammo"].set_selected("Empty")

            if source=="quicklook":
                self.wpn_type_ranged = equipped_items_dict["ranged"]["item"]["Skill Type"]
                self._set_combo_values(self.ws_selection_combobox, self.ws_dict[self.wpn_type_main] + (self.ws_dict[self.wpn_type_ranged] if self.wpn_type_ranged not in ["None", "Instrument"] else []))
                if (old_item["Skill Type"] != new_item["Skill Type"]) and (new_item["Skill Type"] not in ["Instrument"]):
                    if self.ws_selection_combobox.currentText() not in self.ws_dict[self.wpn_type_main] + self.ws_dict[self.wpn_type_ranged]:
                        self.ws_selection_combobox.setCurrentText(self.ws_dict[self.wpn_type_main][0])

        if slot == "ammo":
            if (ammo_item_type=="Bullet" and ranged_item_type != "Gun") or (ammo_item_type=="Arrow" and ranged_item_type != "Bow") or (ammo_item_type=="Bolt" and ranged_item_type != "Crossbow") or (ammo_item_type in ["Equipment", "Shuriken"]):
                new_ammo_item = gear_pyfile.all_gear["Empty"]
                equipped_items_dict["ranged"]["item"] = new_ammo_item
                equipped_items_dict["ranged"]["icon"] = self.get_equipment_icon(new_ammo_item["Name"])
                self.set_button_icon(equipped_items_dict["ranged"]["button"], equipped_items_dict["ranged"]["icon"])
                equipped_items_dict["ranged"]["button"].setToolTip(self.format_tooltip_stats(new_ammo_item))
                scrollframe["ranged"].set_selected("Empty")

        # Swap the rings if selecting ring1/ring2 to be the item in ring2/ring1 slot.
        if ((slot == "ring1" and (new_item == ring2_item_before)) or (slot == "ring2" and (new_item == ring1_item_before))) and (new_item["Name"] != "Empty"):
            equipped_items_dict["ring1"]["item"] = ring2_item_before
            equipped_items_dict["ring1"]["icon"] = self.get_equipment_icon(ring2_item_before["Name"])
            self.set_button_icon(equipped_items_dict["ring1"]["button"], equipped_items_dict["ring1"]["icon"])
            equipped_items_dict["ring1"]["button"].setToolTip(self.format_tooltip_stats(ring2_item_before))
            scrollframe["ring1"].set_selected(ring2_item_before["Name2"])

            equipped_items_dict["ring2"]["item"] = ring1_item_before
            equipped_items_dict["ring2"]["icon"] = self.get_equipment_icon(ring1_item_before["Name"])
            self.set_button_icon(equipped_items_dict["ring2"]["button"], equipped_items_dict["ring2"]["icon"])
            equipped_items_dict["ring2"]["button"].setToolTip(self.format_tooltip_stats(ring1_item_before))
            scrollframe["ring2"].set_selected(ring1_item_before["Name2"])

        # Swap the earrings if selecting ear1/ear2 to be the item in ear2/ear1 slot.
        if ((slot == "ear1" and (new_item == ear2_item_before)) or (slot == "ear2" and (new_item == ear1_item_before))) and (new_item["Name"] != "Empty"):
            equipped_items_dict["ear1"]["item"] = ear2_item_before
            equipped_items_dict["ear1"]["icon"] = self.get_equipment_icon(ear2_item_before["Name"])
            self.set_button_icon(equipped_items_dict["ear1"]["button"], equipped_items_dict["ear1"]["icon"])
            equipped_items_dict["ear1"]["button"].setToolTip(self.format_tooltip_stats(ear2_item_before))
            scrollframe["ear1"].set_selected(ear2_item_before["Name2"])

            equipped_items_dict["ear2"]["item"] = ear1_item_before
            equipped_items_dict["ear2"]["icon"] = self.get_equipment_icon(ear1_item_before["Name"])
            self.set_button_icon(equipped_items_dict["ear2"]["button"], equipped_items_dict["ear2"]["icon"])
            equipped_items_dict["ear2"]["button"].setToolTip(self.format_tooltip_stats(ear1_item_before))
            scrollframe["ear2"].set_selected(ear1_item_before["Name2"])

        # Can't equip a cloak with a hat
        if slot == "body":
            if ("cloak" in new_item_name.lower()):
                equipped_items_dict["head"]["item"] = gear_pyfile.all_gear["Empty"]
                equipped_items_dict["head"]["icon"] = self.get_equipment_icon(gear_pyfile.all_gear["Empty"]["Name"])
                self.set_button_icon(equipped_items_dict["head"]["button"], equipped_items_dict["head"]["icon"])
                equipped_items_dict["head"]["button"].setToolTip(self.format_tooltip_stats(gear_pyfile.all_gear["Empty"]))
                scrollframe["head"].set_selected("Empty")

        if slot == "head":
            if ("cloak" in equipped_items_dict["body"]["item"]["Name"].lower()):
                equipped_items_dict["body"]["item"] = gear_pyfile.all_gear["Empty"]
                equipped_items_dict["body"]["icon"] = self.get_equipment_icon(gear_pyfile.all_gear["Empty"]["Name"])
                self.set_button_icon(equipped_items_dict["body"]["button"], equipped_items_dict["body"]["icon"])
                equipped_items_dict["body"]["button"].setToolTip(self.format_tooltip_stats(gear_pyfile.all_gear["Empty"]))
                scrollframe["body"].set_selected("Empty")

        # Update the radio button selections based on the newly equipped gear.
        scrollframe[slot].set_selected(new_item_name)


    def update_buffs(self, event):
        '''
        When enabling/disabling/selecting buffs from WHM, COR, BRD, GEO selections.
        Ensure that no two buffs are identical (do not allow double chaos roll)
        Ensure that Bolster is not enabled with BoG.

        Inputs:
            set {dia, haste, boost, storm}
            set {Song1, Song2, Song3, Song4}
            set {Roll1, Roll2, Roll3, Roll4}
            set {Indi-, Geo-, Entrust-}
        '''

        if event == "soul voice":
            if self.soul_voice_checkbox.isChecked() == True:
                self.marcato_checkbox.setChecked(False)

        elif event == "marcato":
            if self.marcato_checkbox.isChecked() == True:
                self.soul_voice_checkbox.setChecked(False)

        elif event == "bolster":
            if self.bolster_checkbox.isChecked() == True:
                self.bog_checkbox.setChecked(False)
                
        elif event == "blaze of glory":
            if self.bog_checkbox.isChecked() == True:
                self.bolster_checkbox.setChecked(False)

        elif "set song" in event.lower():
            new_song_slot = event.split()[-1]
            new_song_name = self.song_selections_dict[new_song_slot]["combobox"].currentText()
            for song_slot in self.song_selections_dict:
                other_song_name_in_use = self.song_selections_dict[song_slot]["combobox"].currentText()
                if (new_song_slot != song_slot) and (other_song_name_in_use == new_song_name) and (other_song_name_in_use != "None"):
                    self.song_selections_dict[song_slot]["combobox"].setCurrentText("None")

        elif "set roll" in event.lower():
            new_roll_slot = event.split()[-1]
            new_roll_name = self.roll_selections_dict[new_roll_slot]["name combobox"].currentText()
            for roll_slot in self.roll_selections_dict:
                other_roll_name_in_use = self.roll_selections_dict[roll_slot]["name combobox"].currentText()
                if (new_roll_slot != roll_slot) and (other_roll_name_in_use == new_roll_name) and (other_roll_name_in_use != "None"):
                    self.roll_selections_dict[roll_slot]["name combobox"].setCurrentText("None")

        elif event.lower() in ["set indi-", "set geo-", "set entrust-"]:
            new_bubble_slot = event.split()[-1]
            new_bubble_name = self.bubble_selections_dict[new_bubble_slot]["combobox"].currentText().split("-")[-1]
            for bubble_slot in self.bubble_selections_dict:
                other_bubble_name_in_use = self.bubble_selections_dict[bubble_slot]["combobox"].currentText().split("-")[-1]
                if (new_bubble_slot != bubble_slot) and (other_bubble_name_in_use == new_bubble_name) and (other_bubble_name_in_use != "None"):
                    self.bubble_selections_dict[bubble_slot]["combobox"].setCurrentText("None")

        elif event == "set dia":
            self.whm_selections_dict["Dia"] = self.dia_combobox.currentText()
        elif event == "set haste":
            self.whm_selections_dict["Haste"] = self.haste_combobox.currentText()
        elif event == "set boost":
            self.whm_selections_dict["Boost"] = self.boost_combobox.currentText()
        elif event == "set storm":
            self.whm_selections_dict["Storm"] = self.storm_combobox.currentText()

        elif event == "set food":
            self.food_selections_dict["item"] = gear_pyfile.all_food[self.food_selections_dict["combobox"].currentText()]
            self.food_selections_dict["combobox"].setToolTip(self.format_tooltip_stats(self.food_selections_dict["item"]))

        elif "haste samba" in event.lower():
            if "sub" in event.lower() and self.all_special_toggles_dict["Haste Samba (sub)"]["checkbox"].isChecked():
                self.all_special_toggles_dict["Haste Samba"]["checkbox"].setChecked(False)
            if "sub" not in event.lower() and self.all_special_toggles_dict["Haste Samba"]["checkbox"].isChecked():
                self.all_special_toggles_dict["Haste Samba (sub)"]["checkbox"].setChecked(False)

        elif "warcry" in event.lower():
            if "sub" in event.lower() and self.all_special_toggles_dict["Warcry (sub)"]["checkbox"].isChecked():
                self.all_special_toggles_dict["Warcry"]["checkbox"].setChecked(False)
            if "sub" not in event.lower() and self.all_special_toggles_dict["Warcry"]["checkbox"].isChecked():
                self.all_special_toggles_dict["Warcry (sub)"]["checkbox"].setChecked(False)

        elif "box step" in event.lower():
            if "sub" in event.lower() and self.all_special_toggles_dict["Box Step (sub)"]["checkbox"].isChecked():
                self.all_special_toggles_dict["Box Step"]["checkbox"].setChecked(False)
            if "sub" not in event.lower() and self.all_special_toggles_dict["Box Step"]["checkbox"].isChecked():
                self.all_special_toggles_dict["Box Step (sub)"]["checkbox"].setChecked(False)

    def aggregate_buffs(self,):
        '''
        Called by quicklook function
        Reads GUI values to determine active buffs and debuffs.
        Returns dictionary containing the sum of enabled buffs.
        '''
        buffs = {"brd":{}, "cor":{}, "geo":{}, "whm":{}, "food":{}}
        debuffs = {"cor":{}, "geo":{}, "whm":{}, "other":{}}

        # BRD buffs
        if self.brd_checkbox.isChecked() == True:
            for song_slot in self.song_selections_dict:
                song_name = self.song_selections_dict[song_slot]["combobox"].currentText()
                if song_name in buffs_pyfile.brd:
                    song_bonus = int(self.song_bonus_combobox.currentText().split("+")[-1]) # "Songs +7" etc
                    song_bonus_limit = buffs_pyfile.brd_song_limits[song_name]   # Some songs are limited to Songs+X due to instrument requirements or other gear limitations.
                    soul_voice = 1.0 + 1.0*self.soul_voice_checkbox.isChecked() # Soul Voice affects all songs. 
                    marcato = 1.0 + 0.5*self.marcato_checkbox.isChecked() if song_slot in ["Song1"] else 1.0 # Marcato only affects song in slot 1
                    for stat in buffs_pyfile.brd[song_name]:
                        values = buffs_pyfile.brd[song_name][stat]
                        buffs["brd"][stat] = buffs["brd"].get(stat, 0) + soul_voice * marcato * (values[0] + min(song_bonus_limit, song_bonus)*values[1]) + 20*("minuet" in song_name.lower() and stat.lower() in ["attack", "ranged attack"]) # +20 Attack to Minuets from Job Point gifts (assuming it applies to all players, not just the BRD)

        buffs["brd"]["Attack"] = int(buffs["brd"].get("Attack",0))
        buffs["brd"]["Ranged Attack"] = int(buffs["brd"].get("Ranged Attack",0))

        # COR buffs
        if self.cor_checkbox.isChecked() == True:
            for roll_slot in self.roll_selections_dict:
                roll_name = self.roll_selections_dict[roll_slot]["name combobox"].currentText().split()[0]
                if roll_name in buffs_pyfile.cor:
                    roll_bonus = int(self.roll_bonus_combobox.currentText().split("+")[-1]) # "Rolls +7" etc
                    roll_value = self.roll_selections_dict[roll_slot]["potency combobox"].currentText() # I, II, III, IV, etc
                    crooked_cards = 1.0 + 0.2*self.crooked_checkbox.isChecked() if roll_slot in ["Roll1", "Roll3"] else 1.0 # Crooked Cards only affects rolls 1 and 3 here. 
                    for stat in buffs_pyfile.cor[roll_name]:
                        values = buffs_pyfile.cor[roll_name][stat]
                        job_bonus = self.job_bonus_checkbox.isChecked() * (values[2])
                        buffs["cor"][stat] = buffs["cor"].get(stat, 0) + crooked_cards * (values[0][roll_value] + roll_bonus*values[1] + job_bonus)

        # COR debuffs
        if self.light_shot_checkbox.isChecked() and self.whm_checkbox.isChecked() and ("dia" in self.whm_selections_dict["Dia"].lower()):
            for stat in buffs_pyfile.cor_debuffs["Light Shot"]:
                debuffs["cor"][stat] = debuffs["cor"].get(stat, 0) + buffs_pyfile.cor_debuffs["Light Shot"][stat]

        # GEO buffs and debuffs
        if self.geo_checkbox.isChecked() == True:
            for bubble_slot in self.bubble_selections_dict:
                bubble_name = self.bubble_selections_dict[bubble_slot]["combobox"].currentText().split("-")[-1]

                bubble_bonus = int(self.bubble_bonus_combobox.currentText().split("+")[-1]) if (bubble_slot in ["Indi-", "Geo-"]) else 0 # "Bubbles +7" etc. Entrust spells do not benefit from Bubbles+ gear.
                bolster = 1.0 + 1.0*self.bolster_checkbox.isChecked() if (bubble_slot in ["Indi-", "Geo-"]) else 1.0 # Bolster only affects "Indi-" and "Geo-" spells.
                bog = 1.0 + 0.5*self.bog_checkbox.isChecked() if (bubble_slot in ["Geo-"]) else 1.0 # Blaze of Glory (bog) only affects "Geo-" bubbles
                
                # GEO Buffs
                if bubble_name in buffs_pyfile.geo:
                    for stat in buffs_pyfile.geo[bubble_name]:
                        values = buffs_pyfile.geo[bubble_name][stat]
                        buffs["geo"][stat] = buffs["geo"].get(stat, 0) + bolster * bog * (values[0] + bubble_bonus*values[1]) 

                # GEO Debuffs
                if bubble_name in buffs_pyfile.geo_debuffs:

                    # Debuffing bubbles are frequently reduced to 10~70% of their original potency.
                    bubble_potency = max(0, int(self.bubble_potency_entry.text() or 0)/100)

                    for stat in buffs_pyfile.geo_debuffs[bubble_name]:
                        values = buffs_pyfile.geo_debuffs[bubble_name][stat]
                        debuffs["geo"][stat] = debuffs["geo"].get(stat, 0) + bolster * bog * (values[0] + bubble_bonus*values[1]) * bubble_potency

        # WHM buffs and debuffs
        if self.whm_checkbox.isChecked() == True:
            for spell_slot in self.whm_selections_dict:
                spell_name = self.whm_selections_dict[spell_slot]

                if spell_name in buffs_pyfile.whm:
                    for stat in buffs_pyfile.whm[spell_name]:
                        buffs["whm"][stat] = buffs["whm"].get(stat, 0) + buffs_pyfile.whm[spell_name][stat]

                # WHM Debuffs
                if spell_name in buffs_pyfile.whm_debuffs:
                    for stat in buffs_pyfile.whm_debuffs[spell_name]:
                        debuffs["whm"][stat] = debuffs["whm"].get(stat, 0) + buffs_pyfile.whm_debuffs[spell_name][stat]

        # WHM Shell V
        if self.shell5_checkbox.isChecked():
            for stat in buffs_pyfile.whm["Shell V"]:
                buffs["whm"][stat] = buffs["whm"].get(stat, 0) + buffs_pyfile.whm["Shell V"][stat]

        # Food buffs
        if self.food_selections_dict["combobox"].currentText() in gear_pyfile.all_food:
            active_food = gear_pyfile.all_food[self.food_selections_dict["combobox"].currentText()] # Dictionary of stats.
            for stat in active_food:
                if stat not in ["Name", "Name2", "Type"]:
                    # Attack from food is added after Attack% from COR/Berserk/etc, so it needs a different name here.
                    buffs["food"][stat] = buffs["food"].get(stat, 0) + active_food[stat]
        if "Attack" in buffs["food"]:
            buffs["food"]["Food Attack"] = buffs["food"].pop("Attack")
        if "Ranged Attack" in buffs["food"]:
            buffs["food"]["Food Ranged Attack"] = buffs["food"].pop("Ranged Attack")

        # Other debuffs from the special toggles checkboxes
        for debuff_name in self.all_special_toggles_dict:
            if debuff_name in buffs_pyfile.misc_debuffs:
                for stat in self.all_special_toggles_dict[debuff_name]:
                    if not any(k in stat.lower() for k in ["requirement", "checkbox", "boolvar"]) and (self.all_special_toggles_dict[debuff_name]["checkbox"].isChecked()):
                        debuffs["other"][stat] = debuffs["other"].get(stat, 0) + self.all_special_toggles_dict[debuff_name][stat]

        # Combine debuffs into a simpler dictionary
        combined_debuffs = {}
        for source in debuffs:
            for stat in debuffs[source]:
                combined_debuffs[stat] = combined_debuffs.get(stat, 0) + debuffs[source][stat]
        # print(buffs)
        return buffs, combined_debuffs

    def quicklook(self, trigger):
        '''
        When clicking the "Quicklook WS" button.
        Compile the player stats from selected buffs and equipment.
        Run create_player() to build a player character with complete stats.
        Run average_ws() with the given player and selected WS parameters.
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
        special_toggles_dict["99999"] = self.damage_limit99999.isChecked()

        active_buffs, active_debuffs = self.aggregate_buffs()

        equipped_gearset = {slot:self.quicklook_equipped_dict[slot]["item"] for slot in self.quicklook_equipped_dict}
        player = create_player_pyfile.create_player(main_job, sub_job, master_level, gearset=equipped_gearset, buffs=active_buffs, abilities=special_toggles_dict,)

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
        enemy.stats["Defense"] = max(1, enemy.stats.get("Defense", 1)) # Enemy Defense can not be brought lower than 1
        enemy.stats["Magic Defense"] = max(-50, enemy.stats.get("Magic Defense", 0)) # Enemy Magic Defense can not be brought lower than -50 (magic damage taken x2)
        enemy.stats["Magic Damage Taken"] = enemy.stats.pop("Magic DT%")


        if trigger=="run one ws":

            print()
            print()
            actions_pyfile.average_ws(player, enemy, ws_name, tp_entry_value, ws_type, "Damage dealt", simulation=True, single=True, verbose=True)
            print()
            print()
            return

        elif trigger=="ws":
            output = actions_pyfile.average_ws(player, enemy, self.ws_selection_combobox.currentText(), tp_entry_value, ws_type, "Damage dealt")
            self.quicklook_results_damage_label1.setText("Average Damage =")
            self.quicklook_results_tp_label1.setText("Average TP =")
            self.quicklook_damage_value = output[1][0]
            self.quicklook_results_damage_label2.setText(f"{self.quicklook_damage_value:.0f}")
            self.quicklook_tp_value = output[1][1]
            self.quicklook_results_tp_label2.setText(f"{self.quicklook_tp_value:.1f}")

        elif trigger=="spell":

            assert spell_name != "None", "No spell selected."
                

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

            output = actions_pyfile.cast_spell(player, enemy, spell_name, spell_type, "Damage dealt")
            self.quicklook_results_damage_label1.setText("Average Damage =")
            self.quicklook_results_tp_label1.setText("Average TP =")
            self.quicklook_damage_value = output[1][0]
            self.quicklook_results_damage_label2.setText(f"{self.quicklook_damage_value:.0f}")
            self.quicklook_tp_value = output[1][1]
            self.quicklook_results_tp_label2.setText(f"{self.quicklook_tp_value:.1f}")

        elif trigger=="tp":
            output = actions_pyfile.average_attack_round(player, enemy, 0, tp_entry_value, "Time to WS")
            self.quicklook_results_damage_label1.setText("Time per WS =")
            self.quicklook_results_tp_label1.setText("TP/round =")
            self.quicklook_damage_value = output[0]
            self.quicklook_results_damage_label2.setText(f"{self.quicklook_damage_value:.3f}")
            self.quicklook_tp_value = output[1][1]
            self.quicklook_results_tp_label2.setText(f"{self.quicklook_tp_value:.1f}")


        elif "optimize" in trigger:

            # Build the list of equipment to check based on the checkbox selections in each slot.
            # check_gear_dict is a dictionary containing lists of full gear.py item dictionaries
            
            check_gear_dict = {slot:[gear_pyfile.all_gear[item_name] for item_name in self.optimize_scrollframes[slot].get_selected()] for slot in self.quicklook_equipped_dict}

            # Print what is being considered in each slot.
            if False:
                for slot in check_gear_dict:
                    for item in check_gear_dict[slot]:
                        print(slot, item["Name2"])

            assert any([len(check_gear_dict[k])>1 for k in check_gear_dict]), "At least two items must be selected in at least one slot to find the best set."

            if "ws" in trigger:
                if not ws_name or ws_name=="None":
                    print("No weapon skill selected.")
                    return
                
            if "spell" in trigger:
                if not spell_name or spell_name=="None":
                    print("No spell selected.")
                    return

            special_toggles_dict["Verbose Swaps"] = self.verbose_swaps.isChecked()
            starting_gearset = {slot:self.quicklook_equipped_dict[slot]["item"] for slot in self.quicklook_equipped_dict}

            actions = {
                        "optimize ws":    ["weapon skill", self.ws_metric_combobox.currentText()],
                        "optimize spell": ["spell cast", self.spell_metric_combobox.currentText()],
                        "optimize tp":    ["attack round", self.tp_metric_combobox.currentText()],
                    }

            self.best_player, _ = wsdist_pyfile.build_set(main_job, sub_job, master_level, active_buffs, special_toggles_dict, enemy, ws_name, spell_name, actions[trigger][0],
                                                            tp_entry_value, check_gear_dict, starting_gearset, 
                                                            self.pdt_requirements_entry.value(), self.mdt_requirements_entry.value(), actions[trigger][1],
                                                            self.show_similar_results_checkbox.isChecked(), int(self.show_similar_results_entry.text() or 0),
                                                        )
            self.equip_best_set_button.setEnabled(True)

        elif trigger == "run dps simulations":

            equipped_tp_gearset = {slot:self.tp_quicklook_equipped_dict[slot]["item"] for slot in self.tp_quicklook_equipped_dict}
            tp_player = create_player_pyfile.create_player(main_job, sub_job, master_level, gearset=equipped_tp_gearset, buffs=active_buffs, abilities=special_toggles_dict,)

            equipped_ws_gearset = {slot:self.ws_quicklook_equipped_dict[slot]["item"] for slot in self.ws_quicklook_equipped_dict}
            ws_player = create_player_pyfile.create_player(main_job, sub_job, master_level, gearset=equipped_ws_gearset, buffs=active_buffs, abilities=special_toggles_dict,)

            actions_pyfile.run_simulation(tp_player, ws_player, enemy, tp_entry_value, ws_name, ws_type, self.plot_dps_checkbox.isChecked())

        elif trigger == "build distribution":

            equipped_ws_gearset = {slot:self.ws_quicklook_equipped_dict[slot]["item"] for slot in self.ws_quicklook_equipped_dict}
            ws_player = create_player_pyfile.create_player(main_job, sub_job, master_level, gearset=equipped_ws_gearset, buffs=active_buffs, abilities=special_toggles_dict,)

            damage_list = []
            tp_list = []
            for k in range(20000): # Sample 20,000 WSs with a fixed TP value
                outputs = actions_pyfile.average_ws(ws_player, enemy, ws_name, tp_entry_value, ws_type, "Damage dealt", simulation=True, single=True, verbose=False)
                damage_list.append(outputs[0])
                tp_list.append(outputs[1])

            fancy_plot_pyfile.plot_final(damage_list, ws_player, tp_entry_value, ws_name)

        elif trigger == "compare tp ws stats":
            # Build player stats
            equipped_tp_gearset = {slot:self.tp_quicklook_equipped_dict[slot]["item"] for slot in self.tp_quicklook_equipped_dict}
            tp_player = create_player_pyfile.create_player(main_job, sub_job, master_level, gearset=equipped_tp_gearset, buffs=active_buffs, abilities=special_toggles_dict,)

            equipped_ws_gearset = {slot:self.ws_quicklook_equipped_dict[slot]["item"] for slot in self.ws_quicklook_equipped_dict}
            ws_player = create_player_pyfile.create_player(main_job, sub_job, master_level, gearset=equipped_ws_gearset, buffs=active_buffs, abilities=special_toggles_dict,)

            # Calculte damage dealt by selected WS
            ws_output_tp = actions_pyfile.average_ws(tp_player, enemy, self.ws_selection_combobox.currentText(), tp_entry_value, ws_type, "Damage dealt")
            tp_player_wsdmg = int(ws_output_tp[0])
            ws_output_ws = actions_pyfile.average_ws(ws_player, enemy, self.ws_selection_combobox.currentText(), tp_entry_value, ws_type, "Damage dealt")
            ws_player_wsdmg = int(ws_output_ws[0])

            # Calculate time per WS
            tp_output_tp = actions_pyfile.average_attack_round(tp_player, enemy, 0, tp_entry_value, "Time to WS")
            tp_player_time = np.round(tp_output_tp[0], 3)
            tp_output_ws = actions_pyfile.average_attack_round(ws_player, enemy, 0, tp_entry_value, "Time to WS")
            ws_player_time = np.round(tp_output_ws[0], 3)

            if spell_name != "None":
                # Calculate damage dealt by selected spell
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
                spell_output_tp = actions_pyfile.cast_spell(tp_player, enemy, spell_name, spell_type, "Damage dealt")
                tp_player_spell = spell_output_tp[0]
                spell_output_ws = actions_pyfile.cast_spell(ws_player, enemy, spell_name, spell_type, "Damage dealt")
                ws_player_spell = spell_output_ws[0]

            max_stat_name_length = 0
            all_stats = ["Regain"]
            for stat in tp_player.stats:
                if stat not in all_stats:
                    all_stats.append(stat)
                if len(stat) > max_stat_name_length:
                    max_stat_name_length = len(stat)

            for stat in ws_player.stats:
                if stat not in all_stats:
                    all_stats.append(stat)
                if len(stat) > max_stat_name_length:
                    max_stat_name_length = len(stat)


            all_stats = sorted(all_stats)
            all_stats.append(f"Time per WS")
            all_stats.append(f"{ws_name} Damage")


            tp_player.stats[f"{ws_name} Damage"] = tp_player_wsdmg
            ws_player.stats[f"{ws_name} Damage"] = ws_player_wsdmg
            tp_player.stats["Time per WS"] = tp_player_time
            ws_player.stats["Time per WS"] = ws_player_time
            if spell_name != "None":
                all_stats.append(f"{spell_name} Damage")
                tp_player.stats[f"{spell_name} Damage"] = tp_player_spell
                ws_player.stats[f"{spell_name} Damage"] = ws_player_spell

            print("================================================")
            for stat in all_stats:

                tp_stat = tp_player.stats.get(stat, 0)
                ws_stat = ws_player.stats.get(stat, 0)
                
                # TODO: Move Gokotai regain to create_player.py and remove it from everywhere else.
                if stat.lower() == "regain":
                    tp_stat += tp_player.stats.get("Dual Wield",0)*(tp_player.gearset["main"]["Name"]=="Gokotai")
                    ws_stat += ws_player.stats.get("Dual Wield",0)*(ws_player.gearset["main"]["Name"]=="Gokotai")


                pet_stat = "pet:"==stat.lower()[:4]
                if pet_stat:
                    stat = stat[4:]

                if "haste" in stat.lower() or "delay reduction"==stat.lower() or "attack%" in stat.lower():
                    tp_stat = f"{tp_stat*100:.1f}%"
                    ws_stat = f"{ws_stat*100:.1f}%"
                
                elif "elemental bonus" in stat.lower() or "crit" in stat.lower() or stat.lower() in ["zanshin", "zanshin oa2", "daken", "kick attacks", "pdt", "mdt", "dt", "da", "ta", "qa", "double shot", "triple shot", "quad shot", *[f"oa{k} {j}" for j in ["main", "sub"] for k in range(2,9)]] or "pdl" in stat.lower() or "magic burst" in stat.lower() or "%" in stat.lower() or "weapon skill damage" in stat.lower() or "skillchain" in stat.lower():
                    tp_stat = f"{tp_stat:.0f}%"
                    ws_stat = f"{ws_stat:.0f}%"
                
                elif "attack" in stat.lower() or "accuracy" in stat.lower() or "evasion" in stat.lower() or "store tp"==stat.lower():
                    tp_stat = f"{tp_stat:.0f}"
                    ws_stat = f"{ws_stat:.0f}"

                if pet_stat:
                    stat = "Pet:" + stat

                if stat.lower() == "wsc":
                
                    if isinstance(tp_stat, list):
                        param, tp_stat = tp_stat[0]
                    else:
                        tp_stat = 0
                
                    if isinstance(ws_stat, list):
                        param, ws_stat = ws_stat[0]
                    else:
                        ws_stat = 0
                    tp_stat = f"{tp_stat:.0f}%"
                    ws_stat = f"{ws_stat:.0f}%"

                    stat = f"{stat}:{param}"

                print(f"{stat:>{max_stat_name_length}}   {tp_stat:>10}   {ws_stat:<10}")

        if "show stats" in trigger:

            if "quicklook" in trigger:
                equipped_gearset = {slot:self.quicklook_equipped_dict[slot]["item"] for slot in self.quicklook_equipped_dict}

            elif "tp" in trigger:
                equipped_gearset = {slot:self.tp_quicklook_equipped_dict[slot]["item"] for slot in self.tp_quicklook_equipped_dict}

            elif "ws" in trigger:
                equipped_gearset = {slot:self.ws_quicklook_equipped_dict[slot]["item"] for slot in self.ws_quicklook_equipped_dict}

            player = create_player_pyfile.create_player(main_job, sub_job, master_level, gearset=equipped_gearset, buffs=active_buffs, abilities=special_toggles_dict,)

            for stat in self.stats_dict:

                value = player.stats.get(stat, 0)
                if "haste" in stat.lower() or "delay reduction"==stat.lower():
                    if stat.lower() in ["gear haste", "ja haste"]:
                        value = 256./1024 if value > 256./1024 else value
                    elif stat.lower() == "magic haste":
                        value = 448./1024 if value > 448./1024 else value

                    value = f"{value*100:.1f}%"
                
                elif "crit" in stat.lower() or stat.lower() in ["zanshin", "zanshin oa2", "daken", "kick attacks", "pdt", "mdt", "dt", "da", "ta", "qa", "double shot", "triple shot", "quad shot", *[f"oa{k} {j}" for j in ["main", "sub"] for k in range(2,9)]] or "pdl" in stat.lower() or "magic burst" in stat.lower() or "%" in stat.lower() or "weapon skill damage" in stat.lower() or "skillchain" in stat.lower():
                    value = f"{value:.0f}%"
                
                elif "attack" in stat.lower() or "accuracy" in stat.lower() or "evasion" in stat.lower() or "store tp"==stat.lower():
                    value = f"{value:.0f}"

                if stat.lower() == "regain":
                    value += player.stats.get("Dual Wield",0)*(player.gearset["main"]["Name"]=="Gokotai")

                self.stats_dict[stat]["label2"].setText(str(value))


    def set_visible_frame(self, frame):
        '''Raise a scrollframe to the top of its QStackedLayout parent.'''
        frame.parentWidget().layout().setCurrentWidget(frame)

    def update_visible_quicklook_frame(self, slot):
        '''
        When clicking the quicklook equipment icons.
        Raise the selected slot's frame to the top and make it visible.
        Update the self.visible_quicklook_frame_slot variable
        '''
        self.set_visible_frame(self.quicklook_scrollframes[slot])
        self.visible_quicklook_frame_slot = slot

    def update_visible_quicklook_frame_tp(self, slot):
        '''
        When clicking the quicklook equipment icons.
        Raise the selected slot's frame to the top and make it visible.
        Update the self.visible_quicklook_frame_slot variable
        '''
        self.set_visible_frame(self.tp_quicklook_scrollframes[slot])
        self.tp_visible_quicklook_frame_slot = slot

    def update_visible_quicklook_frame_ws(self, slot):
        '''
        When clicking the quicklook equipment icons.
        Raise the selected slot's frame to the top and make it visible.
        Update the self.visible_quicklook_frame_slot variable
        '''
        self.set_visible_frame(self.ws_quicklook_scrollframes[slot])
        self.ws_visible_quicklook_frame_slot = slot

    def update_visible_optimize_frame(self, slot):
        '''
        When clicking the slot buttons in the optimize tab.
        Raise the selected slot's frame to the top and make it visible.
        Update the self.visible_optimize_frame_slot variable
        '''
        self.set_visible_frame(self.optimize_scrollframes[slot])
        self.visible_optimize_frame_slot = slot


    def test_gui(self,):
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

        active_buffs, active_debuffs = self.aggregate_buffs()

        # Build a player character with one piece of gear equipped, checking all possible items one at a time.
        for slot in gear_pyfile.gear_dict:
            for item in gear_pyfile.gear_dict[slot]:
                equipped_gearset = {slot0:gear_pyfile.Empty for slot0 in gear_pyfile.gear_dict}
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
        for i in range(10000):
            special_toggles_dict = {k:self.all_special_toggles_dict[k]["checkbox"].isChecked() for k in self.all_special_toggles_dict if k not in buffs_pyfile.misc_debuffs}
            special_toggles_dict["99999"] = True
            special_toggles_dict_random = {}
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

            equipped_gearset = {slot:np.random.choice(gear_pyfile.gear_dict[slot]) for slot in gear_pyfile.gear_dict}
            while equipped_gearset["main"]["Name"] == "Empty":
                equipped_gearset["main"] = np.random.choice(gear_pyfile.gear_dict["main"])

            main_skill_type = equipped_gearset["main"]["Skill Type"]
            ranged_skill_type = equipped_gearset["ranged"].get("Skill Type", "None")

            ws_list = list(self.ws_dict[main_skill_type])
            if ranged_skill_type in self.ws_dict and ranged_skill_type != "None":
                ws_list = ws_list + self.ws_dict[ranged_skill_type]

                ranged_type = equipped_gearset["ranged"].get("Type", "None")
                if ranged_type == "Crossbow":
                    ammo_type = "Bolt"
                elif ranged_type == "Gun":
                    ammo_type = "Bullet"
                elif ranged_type == "Bow":
                    ammo_type = "Arrow"
                
                forced_ammo = np.random.choice([k for k in gear_pyfile.ammos if k.get("Type", "None")==ammo_type])
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

            active_buffs = {"cor":{}, "brd":{}, "whm":{}}
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

            output_ws = actions_pyfile.average_ws(player, enemy, ws_name, tp_entry_value, ws_type, "Damage dealt")
            output_spell = actions_pyfile.cast_spell(player, enemy, spell_name, spell_type, "Damage dealt")
            output_tp = actions_pyfile.average_attack_round(player, enemy, 0, tp_entry_value, "Time to WS")

            # print(main_job, sub_job, ws_name, spell_name, output_tp[0], output_ws[0], output_spell[0])

    def __init__(self):
        super().__init__()

        # Bind the "q" key to close the application.
        QtGui.QShortcut(QtGui.QKeySequence("q"), self).activated.connect(self.close)

        self.setWindowTitle("Kastra FFXI Damage Simulator  (2026 May 21a)") # pyinstaller --exclude-module gear --exclude-module enemies --clean --onefile --icon=icons32/23937.ico gui_main.py
        self.setFixedSize(700, 850)
        self.setWindowIcon(QtGui.QIcon("icons32/23937.png")) # hat

        # Define the GUI tabs as a notebook of pages.
        self.notebook = QtWidgets.QTabWidget()
        self.setCentralWidget(self.notebook)

        inputs_tab = QtWidgets.QWidget()
        self.notebook.addTab(inputs_tab, "Quicklook")
        inputs_tab_layout = QtWidgets.QGridLayout(inputs_tab)
        inputs_tab_layout.setContentsMargins(0, 0, 0, 0)
        inputs_tab_layout.setSpacing(2)
        inputs_tab_layout.setColumnStretch(0, 1)
        QtGui.QShortcut(QtGui.QKeySequence("Ctrl+1"), self).activated.connect(lambda: self.notebook.setCurrentWidget(inputs_tab))

        optimize_tab = QtWidgets.QWidget()
        self.notebook.addTab(optimize_tab, "Optimize")
        QtGui.QShortcut(QtGui.QKeySequence("Ctrl+2"), self).activated.connect(lambda: self.notebook.setCurrentWidget(optimize_tab))
        optimize_tab_layout = QtWidgets.QGridLayout(optimize_tab)
        optimize_tab_layout.setContentsMargins(0, 0, 0, 0)
        optimize_tab_layout.setSpacing(2)
        optimize_tab_layout.setRowStretch(0, 1)
        optimize_tab_layout.setColumnStretch(0, 1)

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

        item_tmp = np.loadtxt("item_list.csv", delimiter=";", skiprows=1, dtype=str, unpack=True)
        self.item_id_dict = {"id":item_tmp[0], "name":item_tmp[1], "name2":item_tmp[2]}

        self.all_equipment_dict = {
            "main":gear_pyfile.mains,
            "sub":gear_pyfile.subs + gear_pyfile.grips,
            "ranged":gear_pyfile.ranged,
            "ammo":gear_pyfile.ammos,
            "head":gear_pyfile.heads,
            "neck":gear_pyfile.necks,
            "ear1":gear_pyfile.ears,
            "ear2":gear_pyfile.ears2,
            "body":gear_pyfile.bodies,
            "hands":gear_pyfile.hands,
            "ring1":gear_pyfile.rings,
            "ring2":gear_pyfile.rings2,
            "back":gear_pyfile.capes,
            "waist":gear_pyfile.waists,
            "legs":gear_pyfile.legs,
            "feet":gear_pyfile.feet,
            }

        self.ws_dict = {
            "Katana": ["Blade: Retsu", "Blade: Teki", "Blade: To", "Blade: Chi", "Blade: Ei", "Blade: Jin", "Blade: Ten", "Blade: Ku", "Blade: Yu", "Blade: Metsu", "Blade: Kamu", "Blade: Hi", "Blade: Shun", "Zesho Meppo",],
            "Great Katana": ["Tachi: Enpi", "Tachi: Goten", "Tachi: Kagero", "Tachi: Jinpu", "Tachi: Koki", "Tachi: Yukikaze", "Tachi: Gekko", "Tachi: Kasha", "Tachi: Ageha", "Tachi: Kaiten", "Tachi: Rana", "Tachi: Fudo", "Tachi: Shoha", "Tachi: Mumei",],
            "Dagger": [ "Viper Bite", "Dancing Edge", "Shark Bite", "Evisceration", "Aeolian Edge", "Mercy Stroke", "Mandalic Stab", "Mordant Rime", "Pyrrhic Kleos", "Rudra's Storm", "Exenterator", "Ruthless Stroke",],
            "Sword": ["Fast Blade", "Fast Blade II", "Burning Blade", "Red Lotus Blade", "Seraph Blade", "Circle Blade", "Swift Blade", "Savage Blade", "Sanguine Blade", "Knights of Round", "Death Blossom", "Expiacion", "Chant du Cygne", "Requiescat", "Imperator",],
            "Scythe": ["Slice", "Dark Harvest", "Shadow of Death", "Nightmare Scythe", "Spinning Scythe", "Guillotine", "Cross Reaper", "Spiral Hell", "Infernal Scythe", "Catastrophe", "Quietus", "Insurgency", "Entropy", "Origin",], 
            "Great Sword": ["Hard Slash", "Freezebite", "Shockwave", "Sickle Moon", "Spinning Slash", "Ground Strike", "Herculean Slash", "Resolution", "Scourge", "Dimidiation", "Torcleaver", "Fimbulvetr",], 
            "Club": ["Shining Strike", "Seraph Strike", "Skullbreaker", "True Strike", "Judgment", "Hexa Strike", "Black Halo", "Randgrith", "Exudation", "Mystic Boon", "Realmrazer", "Dagda",], 
            "Polearm": ["Double Thrust", "Thunder Thrust", "Raiden Thrust", "Penta Thrust", "Wheeling Thrust", "Impulse Drive", "Sonic Thrust", "Geirskogul", "Drakesbane", "Camlann's Torment", "Stardiver", "Diarmuid",], 
            "Staff": ["Heavy Swing", "Rock Crusher", "Earth Crusher", "Starburst", "Sunburst", "Shell Crusher", "Full Swing", "Cataclysm", "Retribution", "Gate of Tartarus", "Omniscience", "Vidohunir", "Garland of Bliss", "Shattersoul", "Oshala",], 
            "Great Axe": ["Iron Tempest", "Shield Break", "Armor Break", "Weapon Break", "Raging Rush", "Full Break", "Steel Cyclone", "Fell Cleave", "Metatron Torment", "King's Justice", "Ukko's Fury", "Upheaval", "Disaster",], 
            "Axe": ["Raging Axe", "Spinning Axe", "Rampage", "Calamity", "Mistral Axe", "Decimation", "Bora Axe", "Onslaught", "Primal Rend", "Cloudsplitter", "Ruinator", "Blitz",], 
            "Archery": ["Flaming Arrow", "Piercing Arrow", "Dulling Arrow", "Sidewinder", "Blast Arrow", "Empyreal Arrow", "Refulgent Arrow", "Namas Arrow", "Jishnu's Radiance", "Apex Arrow", "Sarv",], 
            "Marksmanship": ["Hot Shot", "Split Shot", "Sniper Shot", "Slug Shot", "Blast Shot", "Detonator", "Coronach", "Leaden Salute", "Trueflight", "Wildfire", "Last Stand", "Terminus",], 
            "Hand-to-Hand": ["Combo", "One Inch Punch", "Raging Fists", "Spinning Attack", "Howling Fist", "Dragon Kick", "Asuran Fists", "Tornado Kick", "Ascetic's Fury", "Stringing Pummel", "Final Heaven", "Victory Smite", "Shijin Spiral", "Maru Kala", "Dragon Blow",],
            "None": ["None"],
            }

        self.jobs_dict = {"Ninja":"nin", "Dark Knight":"drk", "Scholar":"sch", "Red Mage":"rdm", "Black Mage":"blm", "Samurai":"sam", "Dragoon":"drg", "White Mage":"whm", "Warrior":"war", "Corsair":"cor", "Bard":"brd", "Thief":"thf", "Monk":"mnk", "Dancer":"dnc", "Beastmaster":"bst", "Rune Fencer":"run", "Ranger":"rng", "Puppetmaster":"pup", "Blue Mage":"blu", "Geomancer":"geo", "Paladin":"pld", "Summoner":"smn"}

        self.rema_weapons = [
                        "Amanomurakumo", "Annihilator", "Apocalypse", "Bravura", "Excalibur", "Gungnir", "Guttler", "Kikoku", "Mandau", "Mjollnir", "Ragnarok", "Spharai", "Yoichinoyumi",
                        "Almace", "Armageddon", "Caladbolg", "Farsha", "Gandiva", "Kannagi", "Masamune", "Redemption", "Rhongomiant", "Twashtar", "Ukonvasara", "Verethragna", "Hvergelmir",
                        "Aymur", "Burtgang", "Carnwenhan", "Conqueror", "Death Penalty", "Gastraphetes", "Glanzfaust", "Kenkonken", "Kogarasumaru", "Laevateinn", "Liberator", "Murgleis", "Nagi", "Ryunohige", "Terpsichore", "Tizona", "Tupsimati", "Nirvana", "Vajra", "Yagrush", 
                        "Epeolatry", "Idris",
                        "Aeneas", "Anguta", "Chango", "Dojikiri Yasutsuna", "Fail-not", "Fomalhaut", "Godhands", "Heishi Shorinken", "Khatvanga", "Lionheart", "Sequence", "Tishtrya", "Tri-edge", "Trishula",
                        ]



        self.spells_dict = {
                    "nin":["Doton: Ichi", "Doton: Ni", "Doton: San",
                          "Suiton: Ichi", "Suiton: Ni", "Suiton: San",
                          "Huton: Ichi", "Huton: Ni", "Huton: San",
                          "Katon: Ichi", "Katon: Ni", "Katon: San",
                          "Hyoton: Ichi", "Hyoton: Ni", "Hyoton: San",
                          "Raiton: Ichi", "Raiton: Ni", "Raiton: San",
                          "Ranged Attack"],
                    "blm":["Stone", "Stone II", "Stone III", "Stone IV", "Stone V", "Stone VI", "Stoneja",
                           "Water", "Water II", "Water III", "Water IV", "Water V", "Water VI", "Waterja",
                           "Aero", "Aero II", "Aero III", "Aero IV", "Aero V", "Aero VI", "Aeroja",
                           "Fire", "Fire II", "Fire III", "Fire IV", "Fire V", "Fire VI", "Firaja",
                           "Blizzard", "Blizzard II", "Blizzard III", "Blizzard IV", "Blizzard V", "Blizzard VI", "Blizzaja",
                           "Thunder", "Thunder II", "Thunder III", "Thunder IV", "Thunder V", "Thunder VI", "Thundaja", "Impact",
                           "Ranged Attack"],
                    "rdm":["EnSpell", 
                           "Stone", "Stone II", "Stone III", "Stone IV", "Stone V",
                           "Water", "Water II", "Water III", "Water IV", "Water V",
                           "Aero", "Aero II", "Aero III", "Aero IV", "Aero V",
                           "Fire", "Fire II", "Fire III", "Fire IV", "Fire V",
                           "Blizzard", "Blizzard II", "Blizzard III", "Blizzard IV", "Blizzard V",
                           "Thunder", "Thunder II", "Thunder III", "Thunder IV", "Thunder V", "Impact", "Ranged Attack"],
                    "geo":["Stone", "Stone II", "Stone III", "Stone IV", "Stone V",
                           "Water", "Water II", "Water III", "Water IV", "Water V",
                           "Aero", "Aero II", "Aero III", "Aero IV", "Aero V",
                           "Fire", "Fire II", "Fire III", "Fire IV", "Fire V",
                           "Blizzard", "Blizzard II", "Blizzard III", "Blizzard IV", "Blizzard V",
                           "Thunder", "Thunder II", "Thunder III", "Thunder IV", "Thunder V", "Impact"],
                    "sch":["Stone", "Stone II", "Stone III", "Stone IV", "Stone V", "Geohelix II",
                           "Water", "Water II", "Water III", "Water IV", "Water V", "Hydrohelix II",
                           "Aero", "Aero II", "Aero III", "Aero IV", "Aero V", "Anemohelix II",
                           "Fire", "Fire II", "Fire III", "Fire IV", "Fire V", "Pyrohelix II",
                           "Blizzard", "Blizzard II", "Blizzard III", "Blizzard IV", "Blizzard V", "Cryohelix II",
                           "Thunder", "Thunder II", "Thunder III", "Thunder IV", "Thunder V", "Ionohelix II",
                           "Luminohelix II", "Noctohelix II", "Kaustra", "Impact",],
                    "drk":["Stone", "Stone II", "Stone III",
                           "Water", "Water II", "Water III",
                           "Aero", "Aero II", "Aero III",
                           "Fire", "Fire II", "Fire III",
                           "Blizzard", "Blizzard II", "Blizzard III",
                           "Thunder", "Thunder II", "Thunder III", "Impact"],
                    "cor":["Ranged Attack", "Earth Shot", "Water Shot", "Wind Shot", "Fire Shot", "Ice Shot", "Thunder Shot"],
                    "rng":["Ranged Attack"],
                    "sam":["Ranged Attack"],
                    "thf":["Ranged Attack"],
                    }

        self.equipment_button_positions = {
            "main":  [0,0],
            "sub":   [0,1],
            "ranged":[0,2],
            "ammo":  [0,3],
            "head":  [1,0],
            "neck":  [1,1],
            "ear1":  [1,2],
            "ear2":  [1,3],
            "body":  [2,0],
            "hands": [2,1],
            "ring1": [2,2],
            "ring2": [2,3],
            "back":  [3,0],
            "waist": [3,1],
            "legs":  [3,2],
            "feet":  [3,3]
        }

        '''
        ==============================================================================================
            Build the frame containing player inputs
        ==============================================================================================
        '''
        self.numeric_validator = QtGui.QRegularExpressionValidator(QtCore.QRegularExpression(r"^-?\d{0,4}$"))

        def make_combo(values, default, object_name, width_chars=18, on_selected=None):
            combo = QtWidgets.QComboBox()
            items = [str(v) for v in values]
            combo.addItems(items)
            if str(default) and str(default) not in items:
                combo.addItem(str(default))
            combo.setCurrentText(str(default))
            combo.setObjectName(object_name)
            combo.setMinimumWidth(width_chars * 8)
            if on_selected is not None:
                combo.textActivated.connect(on_selected)
            return combo

        inputs_frame = QtWidgets.QWidget()
        inputs_frame_layout = QtWidgets.QGridLayout(inputs_frame)
        inputs_frame_layout.setContentsMargins(0, 0, 0, 0)
        inputs_frame_layout.setSpacing(2)
        inputs_tab_layout.addWidget(inputs_frame, 0, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        basic_inputs_frame = QtWidgets.QGroupBox("  Basic Inputs  ")
        basic_inputs_frame.setAlignment(QtCore.Qt.AlignmentFlag.AlignLeft)
        basic_inputs_layout = QtWidgets.QGridLayout(basic_inputs_frame)
        basic_inputs_layout.setContentsMargins(5, 5, 5, 5)
        basic_inputs_layout.setSpacing(2)
        inputs_frame_layout.addWidget(basic_inputs_frame, 0, 0, QtCore.Qt.AlignmentFlag.AlignTop)

        def add_basic_row(row, label_text, widget):
            label = QtWidgets.QLabel(label_text)
            label.setMinimumWidth(100)
            basic_inputs_layout.addWidget(label, row, 0, QtCore.Qt.AlignmentFlag.AlignRight)
            basic_inputs_layout.addWidget(widget, row, 1)

        self.master_level_combobox = make_combo(tuple(np.arange(50, -1, -1)), 30, "defaults_ml_combobox",
                                                on_selected=lambda text: self.update_job("master level"))
        add_basic_row(0, "Master Lv: ", self.master_level_combobox)

        self.main_job_combobox = make_combo(sorted(self.jobs_dict), "Scholar", "defaults_mainjob_combobox",
                                            on_selected=lambda text: self.update_job("main"))
        add_basic_row(1, "Main Job: ", self.main_job_combobox)

        self.sub_job_level = 49 + int(self.master_level_combobox.currentText()) // 5
        self.sub_job_selection_combobox = make_combo(sorted(self.jobs_dict) + ["None"], "Red Mage", "defaults_subjob_combobox",
                                                     on_selected=lambda text: self.update_job("sub"))
        add_basic_row(2, "Sub Job: ", self.sub_job_selection_combobox)

        self.aftermath_combobox = make_combo([3, 2, 1, 0], 0, "defaults_aftermath_combobox")
        add_basic_row(3, "Aftermath Lv: ", self.aftermath_combobox)

        spell_list = list(self.spells_dict[self.jobs_dict[self.main_job_combobox.currentText()]])
        self.spell_selection_combobox = make_combo(spell_list, "", "defaults_spell_combobox")
        add_basic_row(4, "Spell: ", self.spell_selection_combobox)

        self.wpn_type_main = "None"
        self.wpn_type_ranged = "None"
        self.ws_selection_combobox = make_combo(self.ws_dict[self.wpn_type_main] + self.ws_dict[self.wpn_type_ranged], "", "defaults_ws_combobox")
        add_basic_row(5, "Weapon Skill: ", self.ws_selection_combobox)

        self.tp_entry_box = WheelIntLineEdit(value=1900, lo=1000, hi=3000, step=100)
        self.tp_entry_box.setObjectName("defaults_tp_entry")
        self.tp_entry_box.setFixedWidth(60)
        self.tp_entry_box.editingFinished.connect(self.validate_tp_value)
        add_basic_row(6, "TP Value: ", self.tp_entry_box)

        '''
        ===============================================
          Build the frame containing player abilities
        ===============================================
        '''
        sf = ScrollableLabelFrame(inputs_frame, text="  Special Toggles  ")
        sf.setFixedSize(200, 210)
        inputs_frame_layout.addWidget(sf, 0, 1, QtCore.Qt.AlignmentFlag.AlignTop)
        toggles_layout = sf.interior.layout()

        # Be careful here. The buff names here must match exactly what is presented in the "buffs.py" file under "misc_buffs" dict. # TODO: move this to buffs.py with debuffs
        self.all_special_toggles_dict = dict(sorted({
            "Aggressor":          {"level requirement":45, "job requirement":["war"]}, 
            "Barrage":            {"level requirement":30, "job requirement":["rng"]}, 
            "Berserk":            {"level requirement":15, "job requirement":["war"]}, 
            "Blood Rage":         {"level requirement":87, "job requirement":["war"]}, 
            "Building Flourish":  {"level requirement":50, "job requirement":["dnc"]}, 
            "Chainspell":         {"level requirement":99, "job requirement":["rdm"]}, 
            "Climactic Flourish": {"level requirement":80, "job requirement":["dnc"]}, 
            "Closed Position":    {"level requirement":99, "job requirement":["dnc"]}, 
            "Composure":          {"level requirement":99, "job requirement":["rdm"]}, 
            "Conspirator":        {"level requirement":0,  "job requirement":list(self.jobs_dict.values())}, 
            "Crimson Howl":       {"level requirement":0,  "job requirement":list(self.jobs_dict.values())}, 
            "Crystal Blessing":   {"level requirement":0,  "job requirement":list(self.jobs_dict.values())}, 
            "Divine Emblem":      {"level requirement":78, "job requirement":["pld"]}, 
            "Double Shot":        {"level requirement":79, "job requirement":["rng"]}, 
            "Ebullience":         {"level requirement":55, "job requirement":["sch"]}, 
            "EnSpell":            {"level requirement":27, "job requirement":["rdm"]}, 
            "Endark II":          {"level requirement":99, "job requirement":["drk"]}, 
            "Enlight II":         {"level requirement":99, "job requirement":["pld"]}, 
            "Enlightenment":      {"level requirement":99, "job requirement":["sch"]}, 
            "Focus":              {"level requirement":25, "job requirement":["mnk"]}, 
            "Footwork":           {"level requirement":65, "job requirement":["mnk"]}, 
            "Frenzied Rage":      {"level requirement":99, "job requirement":["bst"]}, 
            "Futae":              {"level requirement":99, "job requirement":["nin"]}, 
            "Hasso":              {"level requirement":25, "job requirement":["sam"]}, 
            "Haste Samba":        {"level requirement":0,  "job requirement":list(self.jobs_dict.values())}, 
            "Haste Samba (sub)":  {"level requirement":45, "job requirement":["dnc"]}, 
            "Hover Shot":         {"level requirement":99, "job requirement":["rng"]}, 
            "Ifrit's Favor":      {"level requirement":0,  "job requirement":list(self.jobs_dict.values())}, 
            "Impetus":            {"level requirement":88, "job requirement":["mnk"]}, 
            "Innin":              {"level requirement":99, "job requirement":["nin"]}, 
            "Klimaform":          {"level requirement":46, "job requirement":["sch"]}, 
            "Last Resort":        {"level requirement":15, "job requirement":["drk"]}, 
            "Magic Burst":        {"level requirement":0,  "job requirement":list(self.jobs_dict.values())}, 
            "Manafont":           {"level requirement":99, "job requirement":["blm"]}, 
            "Manawell":           {"level requirement":99, "job requirement":["blm"]}, 
            "Mighty Guard":       {"level requirement":0,  "job requirement":list(self.jobs_dict.values())}, 
            "Mighty Strikes":     {"level requirement":99, "job requirement":["war"]}, 
            "Nature's Meditation":{"level requirement":0,  "job requirement":list(self.jobs_dict.values())}, 
            "Overwhelm":          {"level requirement":99, "job requirement":["sam"]}, 
            "Rage":               {"level requirement":99, "job requirement":["bst"]}, 
            "Ramuh's Favor":      {"level requirement":0,  "job requirement":list(self.jobs_dict.values())}, 
            "Saber Dance":        {"level requirement":99, "job requirement":["dnc"]}, 
            "Sange":              {"level requirement":99, "job requirement":["nin"]}, 
            "Sharpshot":          {"level requirement":1,  "job requirement":["rng"]}, 
            "Shiva's Favor":      {"level requirement":0,  "job requirement":list(self.jobs_dict.values())}, 
            "Sneak Attack":       {"level requirement":15, "job requirement":["thf"]}, 
            "Striking Flourish":  {"level requirement":89, "job requirement":["dnc"]}, 
            "Swordplay":          {"level requirement":20, "job requirement":["run"]}, 
            "Temper":             {"level requirement":99, "job requirement":["run"]}, 
            "Temper II":          {"level requirement":99, "job requirement":["rdm"]}, 
            "Ternary Flourish":   {"level requirement":93, "job requirement":["dnc"]}, 
            "Theurgic Focus":     {"level requirement":80, "job requirement":["geo"]}, 
            "Trick Attack":       {"level requirement":30, "job requirement":["thf"]}, 
            "Triple Shot":        {"level requirement":87, "job requirement":["cor"]}, 
            "True Shot":          {"level requirement":0,  "job requirement":["rng", "cor"]}, 
            "Velocity Shot":      {"level requirement":99, "job requirement":["rng"]}, 
            "Warcry":             {"level requirement":0,  "job requirement":list(self.jobs_dict.values())}, 
            "Warcry (sub)":       {"level requirement":35, "job requirement":["war"]}, 
            **buffs_pyfile.misc_debuffs,
        }.items()))
            
        for ability_name in self.all_special_toggles_dict:
            checkbox = QtWidgets.QCheckBox(ability_name)
            checkbox.setObjectName(f"defaults_{ability_name}_checkbox")
            toggles_layout.addWidget(checkbox)
            self.all_special_toggles_dict[ability_name]["checkbox"] = checkbox

        # Disallow mainjob and subjob versions to be selected at the same time.
        self.all_special_toggles_dict["Haste Samba"]["checkbox"].clicked.connect(lambda checked=False: self.update_buffs("haste samba"))
        self.all_special_toggles_dict["Haste Samba (sub)"]["checkbox"].clicked.connect(lambda checked=False: self.update_buffs("haste samba (sub)"))

        self.all_special_toggles_dict["Warcry"]["checkbox"].clicked.connect(lambda checked=False: self.update_buffs("warcry"))
        self.all_special_toggles_dict["Warcry (sub)"]["checkbox"].clicked.connect(lambda checked=False: self.update_buffs("warcry (sub)"))

        self.all_special_toggles_dict["Box Step"]["checkbox"].clicked.connect(lambda checked=False: self.update_buffs("box step"))
        self.all_special_toggles_dict["Box Step (sub)"]["checkbox"].clicked.connect(lambda checked=False: self.update_buffs("box step (sub)"))

        '''
        ===============================================
            Build the frame containing enemy stats
        ===============================================
        '''
        enemy_inputs_frame = QtWidgets.QGroupBox("  Enemy Inputs  ")
        enemy_inputs_frame.setFixedSize(230, 210)
        enemy_inputs_layout = QtWidgets.QGridLayout(enemy_inputs_frame)
        enemy_inputs_layout.setContentsMargins(2, 2, 2, 2)
        enemy_inputs_layout.setSpacing(2)
        inputs_frame_layout.addWidget(enemy_inputs_frame, 0, 2, QtCore.Qt.AlignmentFlag.AlignTop)

        # Subframe for selecting a preset enemy.
        enemy_selection_frame = QtWidgets.QWidget()
        enemy_selection_layout = QtWidgets.QVBoxLayout(enemy_selection_frame)
        enemy_selection_layout.setContentsMargins(0, 0, 0, 0)
        enemy_selection_layout.setSpacing(1)
        enemy_inputs_layout.addWidget(enemy_selection_frame, 0, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        enemy_names_list = [k for k in enemies_pyfile.preset_enemies.keys()] # Read directly from the enemies.py file
        self.selected_enemy_combobox = make_combo(enemy_names_list, enemy_names_list[-3], "defaults_enemy_combobox",
                                                  width_chars=30, on_selected=lambda text: self.select_enemy())
        enemy_selection_layout.addWidget(self.selected_enemy_combobox)

        self.selected_enemy_dict = enemies_pyfile.preset_enemies[self.selected_enemy_combobox.currentText()]

        enemy_level_location_text = f"{self.selected_enemy_dict['Location']} (Lv.{self.selected_enemy_dict['Level']})"
        self.enemy_level_location_label = QtWidgets.QLabel(enemy_level_location_text)
        self.enemy_level_location_label.setObjectName("defaults_enemy_level_location_label")
        self.enemy_level_location_label.setAlignment(QtCore.Qt.AlignmentFlag.AlignHCenter)
        enemy_selection_layout.addWidget(self.enemy_level_location_label)

        # Subframe for manually inputting enemy stats, split into two columns.
        enemy_stats_frame = QtWidgets.QWidget()
        enemy_stats_layout = QtWidgets.QGridLayout(enemy_stats_frame)
        enemy_stats_layout.setContentsMargins(0, 0, 0, 0)
        enemy_stats_layout.setSpacing(2)
        enemy_inputs_layout.addWidget(enemy_stats_frame, 1, 0)

        self.enemy_stats_subframe1 = QtWidgets.QWidget()
        subframe1_layout = QtWidgets.QGridLayout(self.enemy_stats_subframe1)
        subframe1_layout.setContentsMargins(0, 0, 0, 0)
        subframe1_layout.setSpacing(1)
        enemy_stats_layout.addWidget(self.enemy_stats_subframe1, 0, 0)
        self.enemy_stats_subframe2 = QtWidgets.QWidget()
        subframe2_layout = QtWidgets.QGridLayout(self.enemy_stats_subframe2)
        subframe2_layout.setContentsMargins(0, 0, 0, 0)
        subframe2_layout.setSpacing(1)
        enemy_stats_layout.addWidget(self.enemy_stats_subframe2, 0, 1)

        def add_enemy_entry(layout, row, stat, label_width, field_width):
            label = QtWidgets.QLabel(stat + ":")
            label.setMinimumWidth(label_width)
            layout.addWidget(label, row, 0, QtCore.Qt.AlignmentFlag.AlignRight)
            entry = QtWidgets.QLineEdit(str(self.selected_enemy_dict[stat]))
            entry.setObjectName(f"defaults_enemy_{stat}_entry")
            entry.setValidator(self.numeric_validator)
            entry.setFixedWidth(field_width)
            layout.addWidget(entry, row, 1)
            self.enemy_input_obj[stat] = entry

        self.enemy_input_obj = {}
        self.enemy_stats_list1 = ["Evasion", "Defense", "Magic Evasion", "Magic Defense", "Magic DT%"]
        for i, stat in enumerate(self.enemy_stats_list1):
            add_enemy_entry(subframe1_layout, i, stat, 110, 64)

        self.enemy_stats_list2 = ["VIT", "AGI", "INT", "MND", "CHR"]
        for i, stat in enumerate(self.enemy_stats_list2):
            add_enemy_entry(subframe2_layout, i, stat, 40, 40)

        self.enemy_resist_ranks_list = ["150%", "130%", "115%", "100%", "85%", "70%", "60%", "50%", "40%", "30%", "25%", "20%", "15%", "10%", "5%"]
        resist_rank_label = QtWidgets.QLabel("Resist Rank:")
        resist_rank_label.setMinimumWidth(110)
        subframe1_layout.addWidget(resist_rank_label, len(self.enemy_stats_list1), 0, QtCore.Qt.AlignmentFlag.AlignRight)
        self.resist_rank_combobox = make_combo(self.enemy_resist_ranks_list, "100%", "defaults_enemy_resist_rank_combobox", width_chars=6)
        subframe1_layout.addWidget(self.resist_rank_combobox, len(self.enemy_stats_list1), 1)

        '''
        ===============================================
            Build the frame containing player buffs
        ===============================================
        '''
        player_buffs_frame = QtWidgets.QGroupBox("Active Buffs")
        player_buffs_layout = QtWidgets.QGridLayout(player_buffs_frame)
        player_buffs_layout.setContentsMargins(2, 2, 2, 2)
        player_buffs_layout.setSpacing(1)
        inputs_tab_layout.addWidget(player_buffs_frame, 1, 0)

        def make_buff_checkbox(text, object_name, checked, on_click=None):
            checkbox = QtWidgets.QCheckBox(text)
            checkbox.setObjectName(object_name)
            checkbox.setChecked(checked)
            if on_click is not None:
                checkbox.clicked.connect(on_click)
            return checkbox

        def new_buff_subframe(column):
            frame = QtWidgets.QWidget()
            frame.setFixedSize(166, 210)
            layout = QtWidgets.QVBoxLayout(frame)
            layout.setContentsMargins(2, 2, 2, 2)
            layout.setSpacing(1)
            player_buffs_layout.addWidget(frame, 0, column, QtCore.Qt.AlignmentFlag.AlignTop)
            return layout

        '''
            ===============================================
                Build the White Magic and Food subframe
            ===============================================
        '''
        whm_layout = new_buff_subframe(0)
        self.whm_checkbox = make_buff_checkbox("White Magic", "defaults_whm_checkbox", True)
        whm_layout.addWidget(self.whm_checkbox, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        self.whm_selections_dict = {}

        self.shell5_checkbox = make_buff_checkbox("Shell V", "defaults_shell5_checkbox", True)
        whm_layout.addWidget(self.shell5_checkbox, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        self.dia_combobox = make_combo(["Dia III", "Dia II", "Dia", "None"], "Dia II", "defaults_dia_combobox",
                                       on_selected=lambda text: self.update_buffs("set dia"))
        whm_layout.addWidget(self.dia_combobox)
        self.whm_selections_dict["Dia"] = self.dia_combobox.currentText()

        self.haste_combobox = make_combo(["Haste II", "Haste", "None"], "Haste", "defaults_haste_combobox",
                                         on_selected=lambda text: self.update_buffs("set haste"))
        whm_layout.addWidget(self.haste_combobox)
        self.whm_selections_dict["Haste"] = self.haste_combobox.currentText()

        boost_spell_list = ["Boost-"+k for k in ["STR", "DEX", "VIT", "AGI", "MND", "INT", "CHR"]] + ["None"]
        self.boost_combobox = make_combo(boost_spell_list, "Boost-STR", "defaults_boost_combobox",
                                         on_selected=lambda text: self.update_buffs("set boost"))
        whm_layout.addWidget(self.boost_combobox)
        self.whm_selections_dict["Boost"] = self.boost_combobox.currentText()

        self.storm_spell_list = [spell_name for sublist in [[k + "storm II", k+"storm"] for k in ["Aurora", "Void", "Fire", "Sand", "Rain", "Wind", "Hail", "Thunder"]] for spell_name in sublist] + ["None"]
        self.storm_combobox = make_combo(self.storm_spell_list, "Aurorastorm", "defaults_storm_combobox",
                                         on_selected=lambda text: self.update_buffs("set storm"))
        whm_layout.addWidget(self.storm_combobox)
        self.whm_selections_dict["Storm"] = self.storm_combobox.currentText()

        enhancing_row = QtWidgets.QWidget()
        enhancing_row_layout = QtWidgets.QHBoxLayout(enhancing_row)
        enhancing_row_layout.setContentsMargins(0, 0, 0, 0)
        enhancing_row_layout.setSpacing(2)
        self.enhancing_skill_label = QtWidgets.QLabel("Enhancing Skill:")
        enhancing_row_layout.addWidget(self.enhancing_skill_label)
        self.enhancing_skill_entry = QtWidgets.QLineEdit("500")
        self.enhancing_skill_entry.setObjectName("defaults_enhancing_skill_entry")
        self.enhancing_skill_entry.setValidator(self.numeric_validator)
        self.enhancing_skill_entry.setFixedWidth(64)
        enhancing_row_layout.addWidget(self.enhancing_skill_entry)
        whm_layout.addWidget(enhancing_row)

        '''
            ===============================================
                  Build a separate subframe for food
            ===============================================
        '''
        food_subframe = QtWidgets.QGroupBox("  Active Food  ")
        food_subframe.setAlignment(QtCore.Qt.AlignmentFlag.AlignHCenter)
        food_subframe_layout = QtWidgets.QVBoxLayout(food_subframe)
        food_subframe_layout.setContentsMargins(2, 2, 2, 2)
        whm_layout.addStretch(1)
        whm_layout.addWidget(food_subframe)

        food_list = [k for k in gear_pyfile.all_food] + ["None"]
        self.food_selections_dict = {}
        self.food_selections_dict["item"] = gear_pyfile.all_food["Grape Daifuku"]
        self.food_selections_dict["combobox"] = make_combo(food_list, "Grape Daifuku", "defaults_food_combobox", width_chars=0,
                                                           on_selected=lambda text: self.update_buffs("set food"))
        self.food_selections_dict["combobox"].setToolTip(self.format_tooltip_stats(self.food_selections_dict["item"]))
        food_subframe_layout.addWidget(self.food_selections_dict["combobox"])

        '''
            ===============================================
                     Build the Bard Songs subframe
            ===============================================
        '''
        brd_layout = new_buff_subframe(1)
        self.brd_checkbox = make_buff_checkbox("Bard Songs", "defaults_brd_checkbox", True)
        brd_layout.addWidget(self.brd_checkbox, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        self.song_bonus_combobox = make_combo(["Songs +"+str(i) for i in range(9, -1, -1)], "Songs +7", "defaults_song_bonus_combobox")
        brd_layout.addWidget(self.song_bonus_combobox)

        song_list = [song_name for song_name in buffs_pyfile.brd] + ["None"]
        number_of_songs = 5
        self.song_selections_dict = {f"Song{i+1}":{} for i in range(number_of_songs)}
        for song_slot in self.song_selections_dict:
            combo = make_combo(song_list, "None", f"defaults_{song_slot}_combobox",
                               on_selected=lambda text, s=song_slot: self.update_buffs(f"set {s}"))
            self.song_selections_dict[song_slot]["combobox"] = combo
            brd_layout.addWidget(combo)

        brd_layout.addStretch(1)
        self.marcato_checkbox = make_buff_checkbox("Marcato", "defaults_marcato_checkbox", True,
                                                   on_click=lambda checked=False: self.update_buffs("marcato"))
        brd_layout.addWidget(self.marcato_checkbox, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)
        self.soul_voice_checkbox = make_buff_checkbox("Soul Voice", "defaults_soul_voice_checkbox", False,
                                                      on_click=lambda checked=False: self.update_buffs("soul voice"))
        brd_layout.addWidget(self.soul_voice_checkbox, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        '''
            ===============================================
                     Build the Corsair Rolls subframe
            ===============================================
        '''
        cor_layout = new_buff_subframe(2)
        self.cor_checkbox = make_buff_checkbox("Corsair Rolls", "defaults_cor_checkbox", True)
        cor_layout.addWidget(self.cor_checkbox, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        self.roll_bonus_combobox = make_combo(["Rolls +"+str(i) for i in range(8, -1, -1)], "Rolls +7", "defaults_roll_bonus_combobox")
        cor_layout.addWidget(self.roll_bonus_combobox)

        roll_list = [roll_name for roll_name in buffs_pyfile.cor]
        roll_potency_list = ["XI", "X", "IX", "VIII", "VII", "VI", "V", "IV", "III", "II", "I"]
        number_of_rolls = 4
        self.roll_selections_dict = {f"Roll{i+1}":{} for i in range(number_of_rolls)}
        for roll_slot in self.roll_selections_dict:
            roll_row = QtWidgets.QWidget()
            roll_row_layout = QtWidgets.QHBoxLayout(roll_row)
            roll_row_layout.setContentsMargins(0, 0, 0, 0)
            roll_row_layout.setSpacing(2)
            potency_combo = make_combo(roll_potency_list, "IX", f"defaults_{roll_slot}_potency_combobox", width_chars=4)
            self.roll_selections_dict[roll_slot]["potency combobox"] = potency_combo
            roll_row_layout.addWidget(potency_combo)
            name_combo = make_combo([k + " Roll" for k in roll_list] + ["None"], "None", f"defaults_{roll_slot}_name_combobox", width_chars=15,
                                    on_selected=lambda text, s=roll_slot: self.update_buffs(f"set {s}"))
            self.roll_selections_dict[roll_slot]["name combobox"] = name_combo
            roll_row_layout.addWidget(name_combo)
            cor_layout.addWidget(roll_row)

        cor_layout.addStretch(1)
        self.crooked_checkbox = make_buff_checkbox("Crooked Cards", "defaults_crooked_cards_checkbox", True)
        cor_layout.addWidget(self.crooked_checkbox, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)
        self.job_bonus_checkbox = make_buff_checkbox("Job Bonus", "defaults_cor_job_bonus_checkbox", True)
        cor_layout.addWidget(self.job_bonus_checkbox, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)
        self.light_shot_checkbox = make_buff_checkbox("Light Shot", "defaults_light_shot_checkbox", True)
        cor_layout.addWidget(self.light_shot_checkbox, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        '''
            ===============================================
                     Build the Geomancy Bubbles subframe
            ===============================================
        '''
        geo_layout = new_buff_subframe(3)
        self.geo_checkbox = make_buff_checkbox("Geomancy Bubbles", "defaults_geo_checkbox", False)
        geo_layout.addWidget(self.geo_checkbox, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        self.bubble_bonus_combobox = make_combo(["Geomancy +"+str(i) for i in range(10, -1, -1)], "Geomancy +10", "defaults_bubble_bonus_checkbox")
        geo_layout.addWidget(self.bubble_bonus_combobox)

        bubble_list = [bubble_name for bubble_name in sorted(list(buffs_pyfile.geo.keys()) + list(buffs_pyfile.geo_debuffs.keys()))]
        bubble_prefix_list = ["Indi-", "Geo-", "Entrust-"]
        self.bubble_selections_dict = {f"{k}":{} for k in bubble_prefix_list}
        for i, bubble_slot in enumerate(self.bubble_selections_dict):
            combo = make_combo([bubble_prefix_list[i]+k for k in bubble_list] + ["None"], "None", f"defaults_{bubble_slot}_combobox",
                               on_selected=lambda text, s=bubble_slot: self.update_buffs(f"set {s}"))
            self.bubble_selections_dict[bubble_slot]["combobox"] = combo
            geo_layout.addWidget(combo)

        bubble_potency_row = QtWidgets.QWidget()
        bubble_potency_row_layout = QtWidgets.QHBoxLayout(bubble_potency_row)
        bubble_potency_row_layout.setContentsMargins(0, 0, 0, 0)
        bubble_potency_row_layout.setSpacing(2)
        bubble_potency_row_layout.addWidget(QtWidgets.QLabel("Bubble Potency:"))
        self.bubble_potency_entry = QtWidgets.QLineEdit("50")
        self.bubble_potency_entry.setObjectName("defaults_bubble_potency_entry")
        self.bubble_potency_entry.setValidator(self.numeric_validator)
        self.bubble_potency_entry.setFixedWidth(48)
        bubble_potency_row_layout.addWidget(self.bubble_potency_entry)
        geo_layout.addWidget(bubble_potency_row)

        geo_layout.addStretch(1)
        self.bog_checkbox = make_buff_checkbox("Blaze of Glory", "defaults_bog_checkbox", True,
                                               on_click=lambda checked=False: self.update_buffs("blaze of glory"))
        geo_layout.addWidget(self.bog_checkbox, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)
        self.bolster_checkbox = make_buff_checkbox("Bolster", "defaults_bolster_checkbox", False,
                                                   on_click=lambda checked=False: self.update_buffs("bolster"))
        geo_layout.addWidget(self.bolster_checkbox, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        '''
        ===============================================
                  Build the Quicklook subframe
        ===============================================
        '''
        quicklook_frame = QtWidgets.QWidget()
        quicklook_frame_layout = QtWidgets.QGridLayout(quicklook_frame)
        quicklook_frame_layout.setContentsMargins(0, 0, 0, 0)
        quicklook_frame_layout.setSpacing(2)
        quicklook_frame_layout.setColumnStretch(0, 1)
        quicklook_frame_layout.setColumnStretch(1, 2)
        inputs_tab_layout.addWidget(quicklook_frame, 2, 0)

        quicklook_subframe_left = QtWidgets.QGroupBox("  Equipped Items  ")
        quicklook_subframe_left.setAlignment(QtCore.Qt.AlignmentFlag.AlignHCenter)
        quicklook_left_layout = QtWidgets.QGridLayout(quicklook_subframe_left)
        quicklook_left_layout.setContentsMargins(2, 2, 2, 2)
        quicklook_left_layout.setSpacing(2)
        quicklook_frame_layout.addWidget(quicklook_subframe_left, 0, 0, QtCore.Qt.AlignmentFlag.AlignTop)

        copy_set_frame = QtWidgets.QWidget()
        copy_set_layout = QtWidgets.QGridLayout(copy_set_frame)
        copy_set_layout.setContentsMargins(0, 0, 0, 0)
        copy_set_layout.setSpacing(1)
        quicklook_left_layout.addWidget(copy_set_frame, 0, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        copy2tp_button = QtWidgets.QPushButton("Copy to TP Set")
        copy2tp_button.setMinimumWidth(120)
        copy2tp_button.clicked.connect(lambda checked=False: self.copy_gearset_dict("quicklook to tp"))
        copy_set_layout.addWidget(copy2tp_button, 0, 0)
        copy2ws_button = QtWidgets.QPushButton("Copy to WS Set")
        copy2ws_button.setMinimumWidth(120)
        copy2ws_button.clicked.connect(lambda checked=False: self.copy_gearset_dict("quicklook to ws"))
        copy_set_layout.addWidget(copy2ws_button, 0, 1)
        copy2clipboard_button = QtWidgets.QPushButton("Copy to Clipboard")
        copy2clipboard_button.clicked.connect(lambda checked=False: self.copy_to_clipboard("quicklook"))
        copy_set_layout.addWidget(copy2clipboard_button, 1, 0, 1, 2)

        quicklook_gear_frame = QtWidgets.QWidget()
        quicklook_gear_layout = QtWidgets.QGridLayout(quicklook_gear_frame)
        quicklook_gear_layout.setContentsMargins(0, 0, 0, 0)
        quicklook_gear_layout.setSpacing(1)
        quicklook_left_layout.addWidget(quicklook_gear_frame, 1, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        self.quicklook_equipped_dict = {slot: {"icon":self.get_equipment_icon(), "item":gear_pyfile.Empty} for slot in self.all_equipment_dict}

        for slot in self.all_equipment_dict:
            button = QtWidgets.QPushButton()
            button.clicked.connect(lambda checked=False, event=slot: self.update_visible_quicklook_frame(event))
            self.set_button_icon(button, self.quicklook_equipped_dict[slot]["icon"])
            self.quicklook_equipped_dict[slot]["button"] = button
            button.setToolTip(self.format_tooltip_stats(self.quicklook_equipped_dict[slot]["item"]))
            row, col = self.equipment_button_positions[slot]
            quicklook_gear_layout.addWidget(button, row, col)

        quicklook_buttons_frame = QtWidgets.QWidget()
        quicklook_buttons_layout = QtWidgets.QGridLayout(quicklook_buttons_frame)
        quicklook_buttons_layout.setContentsMargins(0, 0, 0, 0)
        quicklook_buttons_layout.setSpacing(1)
        quicklook_left_layout.addWidget(quicklook_buttons_frame, 2, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        for label, event, pos in [
            ("Quicklook TP", "tp", (0, 0)),
            ("Quicklook WS", "ws", (0, 1)),
            ("Quicklook Spell", "spell", (1, 0)),
            ("Simulate WS", "run one ws", (1, 1)),
        ]:
            button = QtWidgets.QPushButton(label)
            button.setMinimumWidth(120)
            button.clicked.connect(lambda checked=False, e=event: self.quicklook(e))
            quicklook_buttons_layout.addWidget(button, pos[0], pos[1])

        quicklook_results_frame = QtWidgets.QWidget()
        quicklook_results_layout = QtWidgets.QGridLayout(quicklook_results_frame)
        quicklook_results_layout.setContentsMargins(0, 0, 0, 0)
        quicklook_results_layout.setSpacing(2)
        quicklook_left_layout.addWidget(quicklook_results_frame, 3, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        courier_font = QtGui.QFont("Courier")
        self.quicklook_damage_value = 0
        self.quicklook_tp_value = 0
        self.quicklook_results_damage_label1 = QtWidgets.QLabel("Average Damage =")
        self.quicklook_results_damage_label1.setFont(courier_font)
        quicklook_results_layout.addWidget(self.quicklook_results_damage_label1, 0, 0, QtCore.Qt.AlignmentFlag.AlignRight)
        self.quicklook_results_damage_label2 = QtWidgets.QLabel(f"{self.quicklook_damage_value:.0f}")
        self.quicklook_results_damage_label2.setFont(courier_font)
        quicklook_results_layout.addWidget(self.quicklook_results_damage_label2, 0, 1, QtCore.Qt.AlignmentFlag.AlignRight)
        self.quicklook_results_tp_label1 = QtWidgets.QLabel("Average TP =")
        self.quicklook_results_tp_label1.setFont(courier_font)
        quicklook_results_layout.addWidget(self.quicklook_results_tp_label1, 1, 0, QtCore.Qt.AlignmentFlag.AlignRight)
        self.quicklook_results_tp_label2 = QtWidgets.QLabel(f"{self.quicklook_tp_value:.1f}")
        self.quicklook_results_tp_label2.setFont(courier_font)
        quicklook_results_layout.addWidget(self.quicklook_results_tp_label2, 1, 1, QtCore.Qt.AlignmentFlag.AlignRight)

        quicklook_subframe_right = QtWidgets.QWidget()
        quicklook_subframe_right.setFixedSize(410, 200)
        quicklook_frame_layout.addWidget(quicklook_subframe_right, 0, 1)

        quicklook_subframe_stack = QtWidgets.QStackedLayout(quicklook_subframe_right)
        self.quicklook_scrollframes = {}
        for slot in self.all_equipment_dict:
            equipment_list = sorted([k["Name2" if "Name2" in k else "Name"] for k in self.all_equipment_dict[slot]])
            self.quicklook_scrollframes[slot] = VirtualRadioFrame(quicklook_subframe_right, text=f"  Select {slot.capitalize()}  ", equipment_slot=slot, selection_type="quicklook", command=self.update_quicklook_equipment, master_data=equipment_list, N=16)
            quicklook_subframe_stack.addWidget(self.quicklook_scrollframes[slot])


        '''
        ==============================================================================================
            Build the frame containing gear selection checkboxes.
        ==============================================================================================
        '''
        def build_combo(values, default, object_name=None, width_chars=18):
            combo = QtWidgets.QComboBox()
            items = [str(v) for v in values]
            combo.addItems(items)
            if str(default) not in items:
                combo.addItem(str(default))
            combo.setCurrentText(str(default))
            if object_name:
                combo.setObjectName(object_name)
            combo.setMinimumWidth(width_chars * 8)
            return combo

        align_left = QtCore.Qt.AlignmentFlag.AlignLeft
        align_right = QtCore.Qt.AlignmentFlag.AlignRight
        align_top = QtCore.Qt.AlignmentFlag.AlignTop
        align_hcenter = QtCore.Qt.AlignmentFlag.AlignHCenter

        optimize_frame_top = QtWidgets.QWidget()
        optimize_frame_top.setFixedSize(630, 480)
        optimize_frame_top_layout = QtWidgets.QGridLayout(optimize_frame_top)
        optimize_frame_top_layout.setContentsMargins(0, 0, 0, 0)
        optimize_frame_top_layout.setSpacing(2)
        optimize_frame_top_layout.setColumnStretch(0, 1)
        optimize_tab_layout.addWidget(optimize_frame_top, 0, 0, align_top)

        '''
        ===============================================
        Build the frame holding the 4x4 grid of buttons
            and the "select all" buttons
            and the conditional select buttons.
        ===============================================
        '''
        optimize_frame_topleft = QtWidgets.QWidget()
        optimize_frame_topleft.setFixedSize(300, 400)
        optimize_frame_topleft_layout = QtWidgets.QGridLayout(optimize_frame_topleft)
        optimize_frame_topleft_layout.setContentsMargins(2, 2, 2, 2)
        optimize_frame_topleft_layout.setSpacing(2)
        optimize_frame_top_layout.addWidget(optimize_frame_topleft, 0, 0, align_top)

        '''
        ===============================================
                Build the 4x4 grid of buttons
        ===============================================
        '''
        buttons_grid_frame1 = QtWidgets.QWidget()
        buttons_grid_layout = QtWidgets.QGridLayout(buttons_grid_frame1)
        buttons_grid_layout.setContentsMargins(0, 0, 0, 0)
        buttons_grid_layout.setSpacing(1)
        optimize_frame_topleft_layout.addWidget(buttons_grid_frame1, 0, 0, align_hcenter)

        button_size = 48
        select_buttons_dict = {slot: {} for slot in self.equipment_button_positions}
        for slot in self.equipment_button_positions:
            button = QtWidgets.QPushButton(slot)
            button.setFixedSize(button_size, button_size)
            button.clicked.connect(lambda checked=False, event=slot: self.update_visible_optimize_frame(event))
            select_buttons_dict[slot]["button"] = button
            row, col = self.equipment_button_positions[slot]
            buttons_grid_layout.addWidget(button, row, col)

        '''
        ===============================================
                Add in the Select all buttons.
        ===============================================
        '''
        buttons_grid_frame2 = QtWidgets.QWidget()
        buttons_grid2_layout = QtWidgets.QGridLayout(buttons_grid_frame2)
        buttons_grid2_layout.setContentsMargins(0, 0, 0, 0)
        buttons_grid2_layout.setSpacing(1)
        optimize_frame_topleft_layout.addWidget(buttons_grid_frame2, 1, 0, align_hcenter)

        for label, event, tip, pos in [
            ("Select all", "select all slot", "Select all items in the currently displayed list.", (0, 0)),
            ("Select ALL", "select all", "Select all items in all equipment lists.", (1, 0)),
            ("Unselect all", "unselect all slot", "Unselect all items in the currently displayed list.", (0, 1)),
            ("Select all File", "select all file", "Select all items in all equipment lists if the item appears in an input file.\nInput file must use format from Windower command \"//gs export all\"", (1, 1)),
        ]:
            button = QtWidgets.QPushButton(label)
            button.setFixedSize(100, 30)
            button.setToolTip(tip)
            button.clicked.connect(lambda checked=False, e=event: self.select_gear_opt(e))
            buttons_grid2_layout.addWidget(button, pos[0], pos[1])

        '''
        ===============================================
                Add in the conditional entries.
        ===============================================
        '''
        select_conditionals_frame = QtWidgets.QWidget()
        select_conditionals_layout = QtWidgets.QGridLayout(select_conditionals_frame)
        select_conditionals_layout.setContentsMargins(0, 0, 0, 0)
        select_conditionals_layout.setSpacing(2)
        select_conditionals_layout.setColumnStretch(1, 1)
        optimize_frame_topleft_layout.addWidget(select_conditionals_frame, 2, 0, align_top)

        self.tvr_rings = ["Cornelia's", "Ephramad's", "Fickblix's", "Gurebu-Ogurebu's", "Lehko Habhoka's", "Medada's", "Ragelise's", "None"]
        self.soa_rings = ["Weatherspoon", "Karieyh", "Vocane", "None"]
        ody_selections = ["30", "25", "20", "15", "0", "None"]
        mastery_rank_selections = ["MR10", "MR09", "MR08", "MR07", "MR06", "MR05"]

        for row, (text, values, default, object_name, attr) in enumerate([
            ("Odyssey Rank:", ody_selections, "30", "defaults_ody_rank_combobox", "ody_rank_combobox"),
            ("SoA Ring:", self.soa_rings, "Weatherspoon", "defaults_soa_ring_combobox", "soa_ring_combobox"),
            ("TVR Ring:", self.tvr_rings, "Lehko Habhoka's", "defaults_tvr_ring_combobox", "tvr_ring_combobox"),
            ("Mastery Rank:", mastery_rank_selections, "MR07", "defaults_mastery_rank_combobox", "mastery_rank_combobox"),
        ]):
            label = QtWidgets.QLabel(text)
            label.setMinimumWidth(160)
            select_conditionals_layout.addWidget(label, row, 0, align_left)
            combo = build_combo(values, default, object_name=object_name)
            tooltips = {
                "ody_rank_combobox": "Only select Odyssey equipment with this rank when using the Select buttons.",
                "soa_ring_combobox": "Only select this SoA ring when using the Select buttons.",
                "tvr_ring_combobox": "Only select this TVR ring when using the Select buttons.",
                "mastery_rank_combobox": "Only select the Hoxne Earring with this mastery rank when using the Select buttons.",
            }
            combo.setToolTip(tooltips[attr])
            select_conditionals_layout.addWidget(combo, row, 1, align_right)
            setattr(self, attr, combo)

        self.nyame25_checkbox = QtWidgets.QCheckBox("Max R25 Nyame?")
        self.nyame25_checkbox.setObjectName("defaults_nyame25_checkbox")
        self.nyame25_checkbox.setChecked(True)
        self.nyame25_checkbox.setToolTip("Select R25B Nyame when Odyssey Rank selection is 30.")
        select_conditionals_layout.addWidget(self.nyame25_checkbox, 4, 1, align_right)

        '''
        ===============================================
          Build the 16 scrollframes of gear checkboxes
        ===============================================
        '''
        opt_scrollframe_relative_frame = QtWidgets.QWidget()
        opt_scrollframe_relative_frame.setFixedSize(370, 300)
        optimize_frame_top_layout.addWidget(opt_scrollframe_relative_frame, 0, 1)

        opt_scrollframe_stack = QtWidgets.QStackedLayout(opt_scrollframe_relative_frame)
        self.optimize_scrollframes = {}
        for slot in self.all_equipment_dict:
            equipment_list = sorted([k["Name2" if "Name2" in k else "Name"] for k in self.all_equipment_dict[slot]])
            self.optimize_scrollframes[slot] = VirtualCheckboxFrame(opt_scrollframe_relative_frame,
                                                                    text=f"  Select {slot.capitalize()}  ",
                                                                    master_data=equipment_list,
                                                                    N=22,
                                                                    )
            opt_scrollframe_stack.addWidget(self.optimize_scrollframes[slot])

        '''
        ==============================================================================================
          Build the bottom part of the optimize tab.
          Contains PDT/MDT requirements, metrics, and the buttons to run optimizations
        ==============================================================================================
        '''
        optimize_frame_bottom = QtWidgets.QWidget()
        optimize_frame_bottom.setFixedSize(630, 320)
        optimize_frame_bottom_layout = QtWidgets.QGridLayout(optimize_frame_bottom)
        optimize_frame_bottom_layout.setContentsMargins(0, 0, 0, 0)
        optimize_frame_bottom_layout.setSpacing(2)
        optimize_frame_bottom_layout.setColumnStretch(0, 1)
        optimize_frame_bottom_layout.setColumnStretch(1, 1)
        optimize_tab_layout.addWidget(optimize_frame_bottom, 1, 0, align_top)

        '''
        ===============================================
        Build the frame containing PDT, MDT, and metrics
        ===============================================
        '''
        optimize_frame_bottomleft = QtWidgets.QWidget()
        optimize_frame_bottomleft.setFixedSize(250, 230)
        optimize_frame_bottomleft_layout = QtWidgets.QGridLayout(optimize_frame_bottomleft)
        optimize_frame_bottomleft_layout.setContentsMargins(0, 0, 0, 0)
        optimize_frame_bottomleft_layout.setSpacing(2)
        optimize_frame_bottomleft_layout.setColumnStretch(0, 1)
        optimize_frame_bottom_layout.addWidget(optimize_frame_bottomleft, 0, 0, align_top)

        tp_metrics = ["Time to WS", "Damage dealt", "TP return", "DPS"]
        spell_metrics = ["Damage dealt", "TP return"]
        ws_metrics = ["Damage dealt", "TP return", "Magic Accuracy"]

        pdt_requirements_text = QtWidgets.QLabel("Required PDT %")
        pdt_requirements_text.setMinimumWidth(160)
        optimize_frame_bottomleft_layout.addWidget(pdt_requirements_text, 0, 0, align_left)
        self.pdt_requirements_entry = WheelIntLineEdit(value=0, lo=-50, hi=100)
        self.pdt_requirements_entry.setFixedWidth(80)
        optimize_frame_bottomleft_layout.addWidget(self.pdt_requirements_entry, 0, 1)

        mdt_requirements_text = QtWidgets.QLabel("Required MDT %")
        mdt_requirements_text.setMinimumWidth(160)
        optimize_frame_bottomleft_layout.addWidget(mdt_requirements_text, 1, 0, align_left)
        self.mdt_requirements_entry = WheelIntLineEdit(value=0, lo=-50, hi=100)
        self.mdt_requirements_entry.setFixedWidth(80)
        optimize_frame_bottomleft_layout.addWidget(self.mdt_requirements_entry, 1, 1)

        for row, (text, values, default, attr) in enumerate([
            ("TP set metric", tp_metrics, "Time to WS", "tp_metric_combobox"),
            ("WS set metric", ws_metrics, "Damage Dealt", "ws_metric_combobox"),
            ("Spell set metric", spell_metrics, "Damage Dealt", "spell_metric_combobox"),
        ], start=2):
            label = QtWidgets.QLabel(text)
            label.setMinimumWidth(160)
            optimize_frame_bottomleft_layout.addWidget(label, row, 0, align_left)
            combo = build_combo(values, default, width_chars=15)
            optimize_frame_bottomleft_layout.addWidget(combo, row, 1)
            setattr(self, attr, combo)

        '''
        ===============================================
          Build the frame containing optimize buttons
        ===============================================
        '''
        optimize_frame_bottomright = QtWidgets.QWidget()
        optimize_frame_bottomright.setFixedSize(300, 230)
        optimize_frame_bottomright_layout = QtWidgets.QGridLayout(optimize_frame_bottomright)
        optimize_frame_bottomright_layout.setContentsMargins(0, 0, 0, 0)
        optimize_frame_bottomright_layout.setSpacing(2)
        optimize_frame_bottomright_layout.setColumnStretch(0, 1)
        optimize_frame_bottom_layout.addWidget(optimize_frame_bottomright, 0, 1, align_top)

        show_similar_results_frame = QtWidgets.QWidget()
        show_similar_results_layout = QtWidgets.QGridLayout(show_similar_results_frame)
        show_similar_results_layout.setContentsMargins(0, 0, 0, 0)
        show_similar_results_layout.setSpacing(2)
        optimize_frame_bottomright_layout.addWidget(show_similar_results_frame, 0, 0, align_hcenter)

        self.show_similar_results_checkbox = QtWidgets.QCheckBox("Print equipment with similar results?")
        self.show_similar_results_checkbox.setToolTip("Print equipment that is within x% of the best set, using the Entry box to enter x.")
        show_similar_results_layout.addWidget(self.show_similar_results_checkbox, 0, 0)

        self.show_similar_results_entry = QtWidgets.QLineEdit("2")
        self.show_similar_results_entry.setValidator(QtGui.QIntValidator(0, 100, self.show_similar_results_entry))
        self.show_similar_results_entry.setAlignment(align_hcenter)
        self.show_similar_results_entry.setFixedWidth(30)
        self.show_similar_results_entry.setToolTip("Print equipment that is within x% of the best set. Enter x here.")
        show_similar_results_layout.addWidget(self.show_similar_results_entry, 0, 1)

        optimize_buttons_frame = QtWidgets.QWidget()
        optimize_buttons_layout = QtWidgets.QGridLayout(optimize_buttons_frame)
        optimize_buttons_layout.setContentsMargins(0, 0, 0, 0)
        optimize_buttons_layout.setSpacing(1)
        optimize_frame_bottomright_layout.addWidget(optimize_buttons_frame, 1, 0, align_hcenter)

        for label, event, pos in [
            ("Optimize WS", "optimize ws", (0, 0)),
            ("Optimize TP", "optimize tp", (1, 0)),
            ("Optimize Spell", "optimize spell", (0, 1)),
        ]:
            button = QtWidgets.QPushButton(label)
            button.setFixedSize(100, 30)
            button.clicked.connect(lambda checked=False, e=event: self.quicklook(e))
            optimize_buttons_layout.addWidget(button, pos[0], pos[1])

        self.equip_best_set_button = QtWidgets.QPushButton("Equip best set")
        self.equip_best_set_button.setFixedSize(100, 30)
        self.equip_best_set_button.setEnabled(False)
        self.equip_best_set_button.clicked.connect(lambda checked=False: self.equip_best_set())
        optimize_buttons_layout.addWidget(self.equip_best_set_button, 1, 1)





        '''
        ==============================================================================================
            Build the simulations tab.
        ==============================================================================================
        '''
        simulate_tab = QtWidgets.QWidget()
        self.notebook.addTab(simulate_tab, "Simulate")
        QtGui.QShortcut(QtGui.QKeySequence("Ctrl+3"), self).activated.connect(lambda: self.notebook.setCurrentWidget(simulate_tab))
        simulate_tab_layout = QtWidgets.QGridLayout(simulate_tab)
        simulate_tab_layout.setContentsMargins(0, 0, 0, 0)
        simulate_tab_layout.setSpacing(5)
        simulate_tab_layout.setColumnStretch(0, 1)

        def build_simulation_set(set_type, group_title, equipped_dict, scrollframes, visible_cb,
                                 copy_inputs_event, copy_clip_event):
            '''Build one Equipped-set frame (TP or WS): copy buttons, gear grid, slot pickers.'''
            outer = QtWidgets.QWidget()
            outer.setFixedSize(650, 300)
            outer_layout = QtWidgets.QGridLayout(outer)
            outer_layout.setContentsMargins(0, 0, 0, 0)
            outer_layout.setSpacing(2)
            outer_layout.setColumnStretch(0, 1)

            group = QtWidgets.QGroupBox(group_title)
            group.setAlignment(QtCore.Qt.AlignmentFlag.AlignLeft)
            group_layout = QtWidgets.QVBoxLayout(group)
            group_layout.setContentsMargins(0, 0, 0, 0)
            group_layout.addStretch(1)

            center = QtWidgets.QWidget()
            center_layout = QtWidgets.QGridLayout(center)
            center_layout.setContentsMargins(0, 0, 0, 0)
            center_layout.setSpacing(5)
            group_layout.addWidget(center, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)
            group_layout.addStretch(1)
            outer_layout.addWidget(group, 0, 0)

            copy_frame = QtWidgets.QWidget()
            copy_layout = QtWidgets.QVBoxLayout(copy_frame)
            copy_layout.setContentsMargins(0, 0, 0, 0)
            copy_layout.setSpacing(2)
            for label, handler in [
                ("Copy to Quicklook", lambda e=copy_inputs_event: self.copy_gearset_dict(e)),
                ("Copy to Clipboard", lambda e=copy_clip_event: self.copy_to_clipboard(e)),
            ]:
                button = QtWidgets.QPushButton(label)
                button.setMinimumWidth(200)
                button.clicked.connect(lambda checked=False, h=handler: h())
                copy_layout.addWidget(button)
            center_layout.addWidget(copy_frame, 0, 0)

            gear_frame = QtWidgets.QWidget()
            gear_layout = QtWidgets.QGridLayout(gear_frame)
            gear_layout.setContentsMargins(0, 0, 0, 0)
            gear_layout.setSpacing(2)
            center_layout.addWidget(gear_frame, 1, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

            for slot in equipped_dict:
                button = QtWidgets.QPushButton()
                button.clicked.connect(lambda checked=False, e=slot: visible_cb(e))
                self.set_button_icon(button, equipped_dict[slot]["icon"])
                equipped_dict[slot]["button"] = button
                button.setToolTip(self.format_tooltip_stats(equipped_dict[slot]["item"]))
                row, col = self.equipment_button_positions[slot]
                gear_layout.addWidget(button, row, col)

            radio_frame = QtWidgets.QWidget()
            radio_frame.setFixedSize(400, 350)
            outer_layout.addWidget(radio_frame, 0, 1, QtCore.Qt.AlignmentFlag.AlignRight)
            radio_stack = QtWidgets.QStackedLayout(radio_frame)
            for slot in self.all_equipment_dict:
                equipment_list = sorted([k["Name2" if "Name2" in k else "Name"] for k in self.all_equipment_dict[slot]])
                scrollframes[slot] = VirtualRadioFrame(radio_frame, text=f"  Select {slot.capitalize()}  ", equipment_slot=slot, selection_type=set_type, command=self.update_quicklook_equipment, master_data=equipment_list, N=13)
                radio_stack.addWidget(scrollframes[slot])
            return outer

        '''
        ===============================================
          Build the top frame (holding TP set stuff)
        ===============================================
        '''
        self.tp_quicklook_equipped_dict = {slot: {"icon":self.get_equipment_icon(), "item":gear_pyfile.Empty} for slot in self.all_equipment_dict}
        self.tp_quicklook_scrollframes = {}
        simulations_tp_frame = build_simulation_set("tp", "Equipped TP set", self.tp_quicklook_equipped_dict, self.tp_quicklook_scrollframes, self.update_visible_quicklook_frame_tp, "tp to quicklook", "tp")
        simulate_tab_layout.addWidget(simulations_tp_frame, 0, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        '''
        ===============================================
         Build the bottom frame (holding WS set stuff)
        ===============================================
        '''
        self.ws_quicklook_equipped_dict = {slot: {"icon":self.get_equipment_icon(), "item":gear_pyfile.Empty} for slot in self.all_equipment_dict}
        self.ws_quicklook_scrollframes = {}
        simulations_ws_frame = build_simulation_set("ws", "Equipped WS set", self.ws_quicklook_equipped_dict, self.ws_quicklook_scrollframes, self.update_visible_quicklook_frame_ws, "ws to quicklook", "ws")
        simulate_tab_layout.addWidget(simulations_ws_frame, 1, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        '''
        ===============================================
                 Build the simulation buttons
        ===============================================
        '''
        simulation_button_frame = QtWidgets.QWidget()
        simulation_button_layout = QtWidgets.QGridLayout(simulation_button_frame)
        simulation_button_layout.setContentsMargins(0, 0, 0, 0)
        simulation_button_layout.setSpacing(5)
        for col in (0, 1, 2):
            simulation_button_layout.setColumnStretch(col, 1)
        simulate_tab_layout.addWidget(simulation_button_frame, 2, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        dps_simulation_button = QtWidgets.QPushButton("Run DPS simulations")
        dps_simulation_button.setFixedSize(150, 30)
        dps_simulation_button.clicked.connect(lambda checked=False: self.quicklook("run dps simulations"))
        simulation_button_layout.addWidget(dps_simulation_button, 0, 0)

        self.plot_dps_checkbox = QtWidgets.QCheckBox("Plot DPS")
        simulation_button_layout.addWidget(self.plot_dps_checkbox, 1, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        ws_distribution_button = QtWidgets.QPushButton("Create WS damage\ndistribution plot")
        ws_distribution_button.setFixedSize(150, 30)
        ws_distribution_button.clicked.connect(lambda checked=False: self.quicklook("build distribution"))
        simulation_button_layout.addWidget(ws_distribution_button, 0, 1)

        compare_sets = QtWidgets.QPushButton("Compare TP & WS stats")
        compare_sets.setFixedSize(150, 30)
        compare_sets.clicked.connect(lambda checked=False: self.quicklook("compare tp ws stats"))
        simulation_button_layout.addWidget(compare_sets, 0, 2)



        '''
        ==============================================================================================
         Build the "show stats" frame
        ==============================================================================================
        '''

        stats_tab = QtWidgets.QWidget()
        self.notebook.addTab(stats_tab, "Player Stats")
        QtGui.QShortcut(QtGui.QKeySequence("Ctrl+4"), self).activated.connect(lambda: self.notebook.setCurrentWidget(stats_tab))
        stats_tab_layout = QtWidgets.QGridLayout(stats_tab)
        stats_tab_layout.setContentsMargins(0, 0, 0, 0)
        stats_tab_layout.setSpacing(2)
        stats_tab_layout.setColumnStretch(0, 1)

        align_top = QtCore.Qt.AlignmentFlag.AlignTop
        align_hcenter = QtCore.Qt.AlignmentFlag.AlignHCenter

        stats_buttons_frame = QtWidgets.QWidget()
        stats_buttons_frame.setFixedSize(600, 50)
        stats_buttons_layout = QtWidgets.QGridLayout(stats_buttons_frame)
        stats_buttons_layout.setContentsMargins(0, 0, 0, 0)
        stats_buttons_layout.setSpacing(2)
        for col in (0, 1, 2):
            stats_buttons_layout.setColumnStretch(col, 1)
        stats_tab_layout.addWidget(stats_buttons_frame, 0, 0, align_hcenter | align_top)

        for col, (label, event) in enumerate([
            ("Quicklook Gear Stats", "show stats quicklook"),
            ("TP Gear Stats", "show stats tp"),
            ("WS Gear Stats", "show stats ws"),
        ]):
            button = QtWidgets.QPushButton(label)
            button.setFixedSize(150, 30)
            button.clicked.connect(lambda checked=False, e=event: self.quicklook(e))
            stats_buttons_layout.addWidget(button, 0, col)

        stats_frame = QtWidgets.QFrame()
        stats_frame.setFrameShape(QtWidgets.QFrame.Shape.Box)
        stats_frame.setLineWidth(2)
        stats_frame.setFixedSize(675, 750)
        stats_frame_layout = QtWidgets.QGridLayout(stats_frame)
        stats_frame_layout.setContentsMargins(0, 0, 0, 0)
        stats_frame_layout.setSpacing(2)
        stats_tab_layout.addWidget(stats_frame, 1, 0)

        stats_subframes = []
        for sub_row in range(3):
            sub = QtWidgets.QWidget()
            sub_layout = QtWidgets.QGridLayout(sub)
            sub_layout.setContentsMargins(0, 0, 0, 0)
            sub_layout.setSpacing(2)
            stats_frame_layout.addWidget(sub, sub_row, 0, QtCore.Qt.AlignmentFlag.AlignLeft)
            stats_subframes.append(sub_layout)
        stats_frame1_layout, stats_frame2_layout, stats_frame3_layout = stats_subframes

        useful_stats =  [
                        ["STR", "DEX", "VIT", "AGI", "INT", "MND", "CHR"],
                        ["Accuracy1", "Accuracy2", "Attack1", "Attack2", "Ranged Accuracy", "Ranged Attack",],
                        ["Magic Accuracy", "Magic Attack", "Magic Damage", "Magic Burst Damage", "Magic Burst Damage II", "Magic Burst Damage Trait",],
                        ["Daken", "Zanshin", "Kick Attacks", "DA", "TA", "QA", "Double Shot", "Triple Shot", "Quad Shot",],
                        ["Dual Wield", "Martial Arts", "Gear Haste", "JA Haste", "Magic Haste", "Delay Reduction",],
                        ["PDT", "MDT", "DT", "Evasion", "Magic Evasion", "Magic Defense", "Subtle Blow", "Subtle Blow II", ],
                        ["Regain", "Store TP", "Crit Rate", "Crit Damage", "Ranged Crit Damage", "Weapon Skill Damage", "Weapon Skill Damage Trait", "Skillchain Bonus", "PDL", "PDL Trait", "TP Bonus", ]
                        ]
        self.stats_dict = {stat:{} for k in useful_stats for stat in k }

        stat_font = QtGui.QFont("Courier", 10)

        def build_stat_group(parent_layout, title, stats_list, size, title_align, grid_pos, sticky=None):
            group = QtWidgets.QGroupBox(title)
            group.setAlignment(title_align)
            group.setFixedSize(*size)
            group_layout = QtWidgets.QGridLayout(group)
            group_layout.setContentsMargins(0, 0, 0, 0)
            group_layout.setSpacing(2)
            group_layout.setColumnStretch(1, 1)
            for i, stat in enumerate(stats_list):
                label1 = QtWidgets.QLabel(stat)
                label1.setFont(stat_font)
                group_layout.addWidget(label1, i, 0, QtCore.Qt.AlignmentFlag.AlignRight)
                label2 = QtWidgets.QLabel("")
                label2.setFont(stat_font)
                group_layout.addWidget(label2, i, 2, QtCore.Qt.AlignmentFlag.AlignRight)
                self.stats_dict[stat]["label1"] = label1
                self.stats_dict[stat]["label2"] = label2
            cell_align = align_top if sticky == "n" else QtCore.Qt.AlignmentFlag(0)
            parent_layout.addWidget(group, *grid_pos, cell_align)
            return group

        build_stat_group(stats_frame1_layout, "Base Parameters", useful_stats[0], (120, 170), align_hcenter, (0, 0))
        build_stat_group(stats_frame1_layout, "Physical", useful_stats[1], (260, 170), align_hcenter, (0, 1), sticky="n")
        build_stat_group(stats_frame1_layout, "Magical", useful_stats[2], (260, 170), align_hcenter, (0, 2), sticky="n")
        build_stat_group(stats_frame2_layout, "Multi-Attack", useful_stats[3], (200, 240), align_hcenter, (1, 0))
        build_stat_group(stats_frame2_layout, "Attack Speed", useful_stats[4], (250, 240), align_hcenter, (1, 1), sticky="n")
        build_stat_group(stats_frame3_layout, "Defensive", useful_stats[5], (210, 290), QtCore.Qt.AlignmentFlag.AlignLeft, (2, 0), sticky="n")
        build_stat_group(stats_frame3_layout, "Other Stats", useful_stats[6], (270, 290), QtCore.Qt.AlignmentFlag.AlignLeft, (2, 1), sticky="n")


        '''
        ==============================================================================================
            Build the Automaton tab.
            Work in progress. Still need details on automaton stats anyway.
        ==============================================================================================
        '''
        if False:
            automaton_tab = QtWidgets.QWidget()
            self.notebook.addTab(automaton_tab, "Automaton")
            QtGui.QShortcut(QtGui.QKeySequence("Ctrl+5"), self).activated.connect(lambda: self.notebook.setCurrentWidget(automaton_tab))
            automaton_tab_layout = QtWidgets.QGridLayout(automaton_tab)

            self.automaton_equipped_dict = {f"slot{i}":{} for i in range(21)} # 16 attachments, 3 maneuvers, head, frame

            container = QtWidgets.QFrame()
            container.setFrameShape(QtWidgets.QFrame.Shape.Box)
            container.setFixedSize(350, 350)
            container_layout = QtWidgets.QGridLayout(container)
            automaton_tab_layout.addWidget(container, 0, 0, QtCore.Qt.AlignmentFlag.AlignCenter)

            head_frame_frame = QtWidgets.QFrame()
            head_frame_frame.setFrameShape(QtWidgets.QFrame.Shape.Box)
            head_frame_frame.setFixedSize(120, 50)
            head_frame_layout = QtWidgets.QGridLayout(head_frame_frame)
            container_layout.addWidget(head_frame_frame, 0, 1, QtCore.Qt.AlignmentFlag.AlignLeft)
            for i in range(2):
                random_pet = [
                            np.random.choice(["Harlequin Head", "Valoredge Head", "Stormwaker Head", "Soulsoother Head", "Spiritreaver Head"]),
                            np.random.choice(["Harlequin Frame", "Valoredge Frame", "Stormwaker Frame"]),
                            ]
                self.automaton_equipped_dict[f"slot{i}"]["icon"] = self.get_equipment_icon(random_pet[i])
                button = QtWidgets.QPushButton()
                self.set_button_icon(button, self.automaton_equipped_dict[f"slot{i}"]["icon"])
                button.clicked.connect(lambda checked=False, event=f"slot{i}": print(event))
                self.automaton_equipped_dict[f"slot{i}"]["button"] = button
                head_frame_layout.addWidget(button, 0, i)


            capacity_frame = QtWidgets.QFrame()
            capacity_frame.setFrameShape(QtWidgets.QFrame.Shape.Box)
            capacity_frame.setFixedSize(120, 170)
            container_layout.addWidget(capacity_frame, 1, 0)

            gear_frame = QtWidgets.QFrame()
            gear_frame.setFrameShape(QtWidgets.QFrame.Shape.Box)
            gear_layout = QtWidgets.QGridLayout(gear_frame)
            container_layout.addWidget(gear_frame, 1, 1)

            for i,slot in enumerate(self.equipment_button_positions):
                random_attachment = np.random.choice(["Fire Attachment", "Ice Attachment", "Thunder Attachment", "Earth Attachment", "Light Attachment", "Dark Attachment", "Water Attachment", "Wind Attachment"])
                self.automaton_equipped_dict[f"slot{i+2}"]["icon"] = self.get_equipment_icon(random_attachment)
                button = QtWidgets.QPushButton()
                self.set_button_icon(button, self.automaton_equipped_dict[f"slot{i+2}"]["icon"])
                button.clicked.connect(lambda checked=False, event=f"slot{i+2}": print(event))
                self.automaton_equipped_dict[f"slot{i+2}"]["button"] = button
                row, col = self.equipment_button_positions[slot]
                gear_layout.addWidget(button, row, col)

            maneuver_frame = QtWidgets.QFrame()
            maneuver_frame.setFrameShape(QtWidgets.QFrame.Shape.Box)
            maneuver_layout = QtWidgets.QGridLayout(maneuver_frame)
            container_layout.addWidget(maneuver_frame, 1, 2, QtCore.Qt.AlignmentFlag.AlignTop)

            for i in range(3):
                random_maneuvers = [
                            np.random.choice([element + " Maneuver" for element in ["Fire", "Earth", "Water", "Wind", "Ice", "Thunder", "Light", "Dark"]]),
                            np.random.choice([element + " Maneuver" for element in ["Fire", "Earth", "Water", "Wind", "Ice", "Thunder", "Light", "Dark"]]),
                            np.random.choice([element + " Maneuver" for element in ["Fire", "Earth", "Water", "Wind", "Ice", "Thunder", "Light", "Dark"]]),
                            ]
                self.automaton_equipped_dict[f"slot{i+18}"]["icon"] = self.get_equipment_icon(random_maneuvers[i])
                button = QtWidgets.QPushButton()
                self.set_button_icon(button, self.automaton_equipped_dict[f"slot{i+18}"]["icon"])
                button.clicked.connect(lambda checked=False, event=f"slot{i+18}": print(event))
                self.automaton_equipped_dict[f"slot{i+18}"]["button"] = button
                maneuver_layout.addWidget(button, i, 0)


            self.notebook.setCurrentWidget(automaton_tab)
        '''
        ==============================================================================================
         The GUI has been built at this point.
         Call the update functions to set good values in all entries.
        ==============================================================================================
        '''
        if os.path.isfile("defaults.pkl"):
            self.load_defaults("default")
        else:
            # Dictionary containing the job profiles for saving/loading defaults by main job selection.
            # Saved to defaults.pkl when using "Save Defaults" button.
            self.states = {"default":{}, **{job:{} for job in self.jobs_dict.keys()}}

            self.update_job("main")
            self.update_job("sub")
            for slot in self.quicklook_equipped_dict:
                self.update_quicklook_equipment((slot, self.quicklook_equipped_dict[slot]["item"]["Name2"], "quicklook"))
                self.update_quicklook_equipment((slot, self.tp_quicklook_equipped_dict[slot]["item"]["Name2"], "tp"))
                self.update_quicklook_equipment((slot, self.ws_quicklook_equipped_dict[slot]["item"]["Name2"], "ws"))

        self.set_visible_frame(self.quicklook_scrollframes["main"])
        self.set_visible_frame(self.tp_quicklook_scrollframes["main"])
        self.set_visible_frame(self.ws_quicklook_scrollframes["main"])
        self.set_visible_frame(self.optimize_scrollframes["main"])
        self.visible_quicklook_frame_slot = "main"
        self.visible_optimize_frame_slot = "main"

if __name__ == "__main__":

    qt_app = QtWidgets.QApplication(sys.argv)
    window = application()
    window.show()
    qt_app.exec()
