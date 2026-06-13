'''
Quicklook tab: player inputs (job/sub/ML, spell/WS, special toggles), enemy inputs,
active buffs (WHM/food/BRD/COR/GEO), and the quicklook equipped-gear grid + result labels.

Owns all quicklook input widgets and the handlers that read them. The sim driver
(`quicklook`) and shared helpers stay on the controller and are reached through `ctx`.
'''

import os
from typing import Any

import numpy as np
from PySide6 import QtCore, QtGui, QtWidgets

import enemies as enemies_pyfile
import gear as gear_pyfile
import buffs as buffs_pyfile
from widgets import WheelIntLineEdit, make_combo
from virtual_frames import VirtualRadioFrame
from wsdist_types import GearPiece, Gearset


class QuicklookTab(QtWidgets.QWidget):
    '''Player/enemy/buff inputs and the quicklook gear grid + results.'''

    slotsRefiltered = QtCore.Signal(dict)  # slot -> (filtered_list, deselect_spec); drives OptimizeTab on job change.

    def __init__(self, ctx: Any, parent: QtWidgets.QWidget | None = None) -> None:
        super().__init__(parent)
        self.ctx = ctx

        layout = QtWidgets.QGridLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(2)
        layout.setColumnStretch(0, 1)

        self.numeric_validator = QtGui.QRegularExpressionValidator(QtCore.QRegularExpression(r"^-?\d{0,4}$"))

        inputs_frame = QtWidgets.QWidget()
        inputs_frame_layout = QtWidgets.QGridLayout(inputs_frame)
        inputs_frame_layout.setContentsMargins(0, 0, 0, 0)
        inputs_frame_layout.setSpacing(2)
        layout.addWidget(inputs_frame, 0, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        basic_inputs_frame = QtWidgets.QGroupBox("  Basic Inputs  ")
        basic_inputs_frame.setAlignment(QtCore.Qt.AlignmentFlag.AlignLeft)
        basic_inputs_layout = QtWidgets.QGridLayout(basic_inputs_frame)
        basic_inputs_layout.setContentsMargins(5, 5, 5, 5)
        basic_inputs_layout.setSpacing(2)
        inputs_frame_layout.addWidget(basic_inputs_frame, 0, 0, QtCore.Qt.AlignmentFlag.AlignTop)

        def add_basic_row(row: int, label_text: str, widget: QtWidgets.QWidget) -> None:
            label = QtWidgets.QLabel(label_text)
            label.setMinimumWidth(100)
            basic_inputs_layout.addWidget(label, row, 0, QtCore.Qt.AlignmentFlag.AlignRight)
            basic_inputs_layout.addWidget(widget, row, 1)

        self.master_level_combobox = make_combo(tuple(np.arange(50, -1, -1)), 30, "defaults_ml_combobox",
                                                on_selected=lambda text: self.update_job("master level"))
        add_basic_row(0, "Master Lv: ", self.master_level_combobox)

        self.main_job_combobox = make_combo(sorted(self.ctx.state.jobs_dict), "Scholar", "defaults_mainjob_combobox",
                                            on_selected=lambda text: self.update_job("main"))
        add_basic_row(1, "Main Job: ", self.main_job_combobox)

        self.sub_job_level = 49 + int(self.master_level_combobox.currentText()) // 5
        self.sub_job_selection_combobox = make_combo(sorted(self.ctx.state.jobs_dict) + ["None"], "Red Mage", "defaults_subjob_combobox",
                                                     on_selected=lambda text: self.update_job("sub"))
        add_basic_row(2, "Sub Job: ", self.sub_job_selection_combobox)

        self.aftermath_combobox = make_combo([3, 2, 1, 0], 0, "defaults_aftermath_combobox")
        add_basic_row(3, "Aftermath Lv: ", self.aftermath_combobox)

        spell_list = list(self.ctx.state.spells_dict[self.ctx.state.jobs_dict[self.main_job_combobox.currentText()]])
        self.spell_selection_combobox = make_combo(spell_list, "", "defaults_spell_combobox")
        add_basic_row(4, "Spell: ", self.spell_selection_combobox)

        self.wpn_type_main: str = "None"
        self.wpn_type_ranged: str = "None"
        self.ws_selection_combobox = make_combo(self.ctx.state.ws_dict[self.wpn_type_main] + self.ctx.state.ws_dict[self.wpn_type_ranged], "", "defaults_ws_combobox")
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
        sf = QtWidgets.QGroupBox("  Special Toggles  ", inputs_frame)
        sf.setFixedSize(200, 210)
        inputs_frame_layout.addWidget(sf, 0, 1, QtCore.Qt.AlignmentFlag.AlignTop)

        sf_scroll = QtWidgets.QScrollArea(sf)
        sf_scroll.setWidgetResizable(True)
        sf_scroll.setFrameShape(QtWidgets.QFrame.Shape.NoFrame)
        sf_interior = QtWidgets.QFrame()
        toggles_layout = QtWidgets.QVBoxLayout(sf_interior)
        toggles_layout.setContentsMargins(0, 0, 0, 0)
        toggles_layout.setSpacing(2)
        sf_scroll.setWidget(sf_interior)
        sf_layout = QtWidgets.QGridLayout(sf)
        sf_layout.setContentsMargins(0, 0, 0, 0)
        sf_layout.addWidget(sf_scroll, 0, 0)

        # Be careful here. The buff names here must match exactly what is presented in the "buffs.py" file under "misc_buffs" dict. # TODO: move this to buffs.py with debuffs
        self.all_special_toggles_dict: dict[str, dict[str, Any]] = dict(sorted({
            "Aggressor":          {"level requirement":45, "job requirement":["war"]}, 
            "Barrage":            {"level requirement":30, "job requirement":["rng"]}, 
            "Berserk":            {"level requirement":15, "job requirement":["war"]}, 
            "Blood Rage":         {"level requirement":87, "job requirement":["war"]}, 
            "Building Flourish":  {"level requirement":50, "job requirement":["dnc"]}, 
            "Chainspell":         {"level requirement":99, "job requirement":["rdm"]}, 
            "Climactic Flourish": {"level requirement":80, "job requirement":["dnc"]}, 
            "Closed Position":    {"level requirement":99, "job requirement":["dnc"]}, 
            "Composure":          {"level requirement":99, "job requirement":["rdm"]}, 
            "Conspirator":        {"level requirement":0,  "job requirement":list(self.ctx.state.jobs_dict.values())}, 
            "Crimson Howl":       {"level requirement":0,  "job requirement":list(self.ctx.state.jobs_dict.values())}, 
            "Crystal Blessing":   {"level requirement":0,  "job requirement":list(self.ctx.state.jobs_dict.values())}, 
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
            "Haste Samba":        {"level requirement":0,  "job requirement":list(self.ctx.state.jobs_dict.values())}, 
            "Haste Samba (sub)":  {"level requirement":45, "job requirement":["dnc"]}, 
            "Hover Shot":         {"level requirement":99, "job requirement":["rng"]}, 
            "Ifrit's Favor":      {"level requirement":0,  "job requirement":list(self.ctx.state.jobs_dict.values())}, 
            "Impetus":            {"level requirement":88, "job requirement":["mnk"]}, 
            "Innin":              {"level requirement":99, "job requirement":["nin"]}, 
            "Klimaform":          {"level requirement":46, "job requirement":["sch"]}, 
            "Last Resort":        {"level requirement":15, "job requirement":["drk"]}, 
            "Magic Burst":        {"level requirement":0,  "job requirement":list(self.ctx.state.jobs_dict.values())}, 
            "Manafont":           {"level requirement":99, "job requirement":["blm"]}, 
            "Manawell":           {"level requirement":99, "job requirement":["blm"]}, 
            "Mighty Guard":       {"level requirement":0,  "job requirement":list(self.ctx.state.jobs_dict.values())}, 
            "Mighty Strikes":     {"level requirement":99, "job requirement":["war"]}, 
            "Nature's Meditation":{"level requirement":0,  "job requirement":list(self.ctx.state.jobs_dict.values())}, 
            "Overwhelm":          {"level requirement":99, "job requirement":["sam"]}, 
            "Rage":               {"level requirement":99, "job requirement":["bst"]}, 
            "Ramuh's Favor":      {"level requirement":0,  "job requirement":list(self.ctx.state.jobs_dict.values())}, 
            "Saber Dance":        {"level requirement":99, "job requirement":["dnc"]}, 
            "Sange":              {"level requirement":99, "job requirement":["nin"]}, 
            "Sharpshot":          {"level requirement":1,  "job requirement":["rng"]}, 
            "Shiva's Favor":      {"level requirement":0,  "job requirement":list(self.ctx.state.jobs_dict.values())}, 
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
            "Warcry":             {"level requirement":0,  "job requirement":list(self.ctx.state.jobs_dict.values())}, 
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

        def add_enemy_entry(layout: QtWidgets.QGridLayout, row: int, stat: str, label_width: int, field_width: int) -> None:
            label = QtWidgets.QLabel(stat + ":")
            label.setMinimumWidth(label_width)
            layout.addWidget(label, row, 0, QtCore.Qt.AlignmentFlag.AlignRight)
            entry = QtWidgets.QLineEdit(str(self.selected_enemy_dict[stat]))
            entry.setObjectName(f"defaults_enemy_{stat}_entry")
            entry.setValidator(self.numeric_validator)
            entry.setFixedWidth(field_width)
            layout.addWidget(entry, row, 1)
            self.enemy_input_obj[stat] = entry

        self.enemy_input_obj: dict[str, QtWidgets.QLineEdit] = {}
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
        layout.addWidget(player_buffs_frame, 1, 0)

        def make_buff_checkbox(text: str, object_name: str, checked: bool, on_click: Any = None) -> QtWidgets.QCheckBox:
            checkbox = QtWidgets.QCheckBox(text)
            checkbox.setObjectName(object_name)
            checkbox.setChecked(checked)
            if on_click is not None:
                checkbox.clicked.connect(on_click)
            return checkbox

        def new_buff_subframe(column: int) -> QtWidgets.QVBoxLayout:
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

        self.whm_selections_dict: dict[str, str] = {}

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
        self.food_selections_dict: dict[str, Any] = {}
        self.food_selections_dict["item"] = gear_pyfile.all_food["Grape Daifuku"]
        self.food_selections_dict["combobox"] = make_combo(food_list, "Grape Daifuku", "defaults_food_combobox", width_chars=0,
                                                           on_selected=lambda text: self.update_buffs("set food"))
        self.food_selections_dict["combobox"].setToolTip(self.ctx.format_tooltip_stats(self.food_selections_dict["item"]))
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
        self.song_selections_dict: dict[str, dict[str, Any]] = {f"Song{i+1}":{} for i in range(number_of_songs)}
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
        self.roll_selections_dict: dict[str, dict[str, Any]] = {f"Roll{i+1}":{} for i in range(number_of_rolls)}
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
        self.bubble_selections_dict: dict[str, dict[str, Any]] = {f"{k}":{} for k in bubble_prefix_list}
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
        layout.addWidget(quicklook_frame, 2, 0)

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
        copy2tp_button.clicked.connect(lambda checked=False: self.ctx.simulate_tab.copy_gearset_dict("quicklook to tp"))
        copy_set_layout.addWidget(copy2tp_button, 0, 0)
        copy2ws_button = QtWidgets.QPushButton("Copy to WS Set")
        copy2ws_button.setMinimumWidth(120)
        copy2ws_button.clicked.connect(lambda checked=False: self.ctx.simulate_tab.copy_gearset_dict("quicklook to ws"))
        copy_set_layout.addWidget(copy2ws_button, 0, 1)
        copy2clipboard_button = QtWidgets.QPushButton("Copy to Clipboard")
        copy2clipboard_button.clicked.connect(lambda checked=False: self.copy_to_clipboard("quicklook"))
        copy_set_layout.addWidget(copy2clipboard_button, 1, 0, 1, 2)

        quicklook_gear_frame = QtWidgets.QWidget()
        quicklook_gear_layout = QtWidgets.QGridLayout(quicklook_gear_frame)
        quicklook_gear_layout.setContentsMargins(0, 0, 0, 0)
        quicklook_gear_layout.setSpacing(1)
        quicklook_left_layout.addWidget(quicklook_gear_frame, 1, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        self.quicklook_equipped_dict: dict[str, dict[str, Any]] = {slot: {"icon":self.ctx.get_equipment_icon(), "item":gear_pyfile.Empty} for slot in self.ctx.state.all_equipment_dict}

        for slot in self.ctx.state.all_equipment_dict:
            button = QtWidgets.QPushButton()
            button.clicked.connect(lambda checked=False, event=slot: self.update_visible_quicklook_frame(event))
            self.ctx.set_button_icon(button, self.quicklook_equipped_dict[slot]["icon"])
            self.quicklook_equipped_dict[slot]["button"] = button
            button.setToolTip(self.ctx.format_tooltip_stats(self.quicklook_equipped_dict[slot]["item"]))
            row, col = self.ctx.state.equipment_button_positions[slot]
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
            button.clicked.connect(lambda checked=False, e=event: self.ctx.simulate_tab.quicklook(e))
            quicklook_buttons_layout.addWidget(button, pos[0], pos[1])

        quicklook_results_frame = QtWidgets.QWidget()
        quicklook_results_layout = QtWidgets.QGridLayout(quicklook_results_frame)
        quicklook_results_layout.setContentsMargins(0, 0, 0, 0)
        quicklook_results_layout.setSpacing(2)
        quicklook_left_layout.addWidget(quicklook_results_frame, 3, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        courier_font = QtGui.QFont("Courier")
        self.quicklook_damage_value: float = 0
        self.quicklook_tp_value: float = 0
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
        self.quicklook_scrollframes: dict[str, Any] = {}
        for slot in self.ctx.state.all_equipment_dict:
            equipment_list = sorted([k["Name2" if "Name2" in k else "Name"] for k in self.ctx.state.all_equipment_dict[slot]])
            self.quicklook_scrollframes[slot] = VirtualRadioFrame(quicklook_subframe_right, text=f"  Select {slot.capitalize()}  ", equipment_slot=slot, selection_type="quicklook", command=self.update_quicklook_equipment, master_data=equipment_list, N=16)
            quicklook_subframe_stack.addWidget(self.quicklook_scrollframes[slot])

        self.visible_quicklook_frame_slot = "main"

    def gather_player_inputs(self) -> dict[str, Any]:
        '''
        Collect every quicklook-tab input the sim driver needs into one dict.
        This is the single sanctioned cross-tab accessor: SimulateTab.quicklook()
        calls it instead of reaching into individual quicklook widgets.
        Buffs come from aggregate_buffs() separately. The menu-driven toggles
        ("99999"/"Verbose Swaps") are added by the caller (they live on the controller).
        '''
        main_job = self.ctx.state.jobs_dict[self.main_job_combobox.currentText()]
        sub_job = self.ctx.state.jobs_dict.get(self.sub_job_selection_combobox.currentText(), "None")
        master_level = int(self.master_level_combobox.currentText())

        ws_name = self.ws_selection_combobox.currentText()
        ws_type = "ranged" if ws_name in (self.ctx.state.ws_dict["Marksmanship"] + self.ctx.state.ws_dict["Archery"]) else "melee"
        spell_name = self.spell_selection_combobox.currentText()

        tp_value = int(self.tp_entry_box.text() or 0)

        special_toggles = {k: self.all_special_toggles_dict[k]["checkbox"].isChecked() for k in self.all_special_toggles_dict if k not in buffs_pyfile.misc_debuffs}
        special_toggles["Enhancing Skill"] = int(self.enhancing_skill_entry.text() or 0)
        special_toggles["Aftermath"] = int(self.aftermath_combobox.currentText())
        special_toggles["Storm spell"] = self.whm_selections_dict["Storm"]
        special_toggles["Enemy Resist Rank"] = self.resist_rank_combobox.currentText()

        enemy_stats = {stat: int(self.enemy_input_obj[stat].text() or 0) for stat in self.enemy_input_obj}

        return {
            "main_job": main_job, "sub_job": sub_job, "master_level": master_level,
            "ws_name": ws_name, "ws_type": ws_type, "spell_name": spell_name,
            "tp_value": tp_value, "special_toggles": special_toggles, "enemy_stats": enemy_stats,
        }

    def validate_tp_value(self, event: Any = None) -> None:
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
        except Exception:
            self.tp_entry_box.setText("1000")

    def select_enemy(self) -> None:
        '''
        When selecting a new enemy from the enemy input combobox
        Loop through the stats and update the user-input values based on the preset_enemies dictionaries from enemies.py
        '''
        self.selected_enemy = enemies_pyfile.preset_enemies[self.selected_enemy_combobox.currentText()]
        for stat in self.enemy_stats_list1 + self.enemy_stats_list2:
            self.enemy_input_obj[stat].setText(str(self.selected_enemy[stat]))

        self.enemy_level_location_label.setText(f"{self.selected_enemy['Location']} (Lv.{self.selected_enemy['Level']})")

    def update_job(self, trigger: str) -> None:
        '''
        When selecting a new main or sub job
        Update the available spell list, special ability list, and equipment lists.
        Enact various other restrictions throughout the GUI based on job selections.
        '''

        main_job_shorthand = self.ctx.state.jobs_dict[self.main_job_combobox.currentText()]
        sub_job_shorthand = self.ctx.state.jobs_dict.get(self.sub_job_selection_combobox.currentText(), "None")
        dual_wield = (main_job_shorthand in ["nin", "dnc", "thf", "blu"] or sub_job_shorthand in ["nin", "dnc"])

        if trigger=="master level":
            self.sub_job_level = 49 + int(self.master_level_combobox.currentText()) // 5

        elif "main" in trigger.lower():

            # Update the spell list options.
            if main_job_shorthand in self.ctx.state.spells_dict:
                new_spell_list = self.ctx.state.spells_dict[main_job_shorthand]
                self.ctx._set_combo_values(self.spell_selection_combobox, new_spell_list)
                if self.spell_selection_combobox.currentText() not in new_spell_list:
                    self.spell_selection_combobox.setCurrentText(new_spell_list[0])
            else:
                self.ctx._set_combo_values(self.spell_selection_combobox, ["None"])
                self.spell_selection_combobox.setCurrentText("None")

            # Update the subjob selections
            if self.main_job_combobox.currentText() == self.sub_job_selection_combobox.currentText():
                # Remove the subjob if the new main job selection matches the current sub job.
                self.sub_job_selection_combobox.setCurrentText("None")

            # Hide the newly selected main job from the sub job list.
            new_subjob_options = [k for k in sorted(self.ctx.state.jobs_dict) if k != self.main_job_combobox.currentText()] + ["None"]
            self.ctx._set_combo_values(self.sub_job_selection_combobox, new_subjob_options)
            # Update the virtual scrollframes to show radiobuttons and checkbuttons for items equippable by the selected job.
            optimize_updates: dict[str, Any] = {}
            for slot in self.ctx.state.all_equipment_dict:
                if slot == "sub":
                    allowed_subtypes = ["Shield", "Grip", "None"]
                    if dual_wield:
                        allowed_subtypes += ["Weapon"]
                    filtered_equipment_list = sorted([k["Name2" if "Name2" in k else "Name"] for k in self.ctx.state.all_equipment_dict["sub"] if (main_job_shorthand in k["Jobs"]) and (k["Type"] in allowed_subtypes)])
                else:
                    filtered_equipment_list = sorted([k["Name2" if "Name2" in k else "Name"] for k in self.ctx.state.all_equipment_dict[slot] if main_job_shorthand.lower() in k["Jobs"]])
                self.quicklook_scrollframes[slot].set_visible_data(filtered_equipment_list)
                self.ctx.tp_quicklook_scrollframes[slot].set_visible_data(filtered_equipment_list)
                self.ctx.ws_quicklook_scrollframes[slot].set_visible_data(filtered_equipment_list)
                optimize_updates[slot] = (filtered_equipment_list, "all")
            self.slotsRefiltered.emit(optimize_updates)


            # Red Mage uses Boost-STAT instead of Gain-STAT. Update the corresponding buff drop-down menu and selection here.
            old_boost_stat = "None" if self.boost_combobox.currentText()=="None" else self.boost_combobox.currentText().split("-")[-1]
            
            if main_job_shorthand.lower() == "rdm":
                if old_boost_stat != "None":
                    self.boost_combobox.setCurrentText(f"Gain-{old_boost_stat}")
                self.ctx._set_combo_values(self.boost_combobox, ["Gain-"+k for k in ["STR", "DEX", "VIT", "AGI", "MND", "INT", "CHR"]] + ["None"])
                self.enhancing_skill_label.setText("Enhancing Skill:")
                self.enhancing_skill_entry.setText(str(650))

            else:
                if old_boost_stat != "None":
                    self.boost_combobox.setCurrentText(f"Boost-{old_boost_stat}")
                self.ctx._set_combo_values(self.boost_combobox, ["Boost-"+k for k in ["STR", "DEX", "VIT", "AGI", "MND", "INT", "CHR"]] + ["None"])
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
                self.ctx.load_defaults("main") # Update GUI to reflect the new main job
            
        elif trigger=="sub":
            # Hide weapons in off-hand slot unless new main+sub combo allows dual wielding.
            allowed_subtypes = ["Shield", "Grip", "None"]
            restricted_items: list[str] = []
            if dual_wield:
                allowed_subtypes += ["Weapon"]
            else:
                restricted_items = [k["Name2" if "Name2" in k else "Name"] for k in self.ctx.state.all_equipment_dict["sub"] if (main_job_shorthand in k["Jobs"]) and (k["Type"] == "Weapon")]
            new_off_hand_equipment_list = sorted([k["Name2" if "Name2" in k else "Name"] for k in self.ctx.state.all_equipment_dict["sub"] if (main_job_shorthand in k["Jobs"]) and (k["Type"] in allowed_subtypes)])
            

            self.quicklook_scrollframes["sub"].set_visible_data(new_off_hand_equipment_list)
            self.ctx.tp_quicklook_scrollframes["sub"].set_visible_data(new_off_hand_equipment_list)
            self.ctx.ws_quicklook_scrollframes["sub"].set_visible_data(new_off_hand_equipment_list)
            self.slotsRefiltered.emit({"sub": (new_off_hand_equipment_list, None if dual_wield else restricted_items)})

            # Unequip off-hand weapon if not able to dual-wield.
            if not dual_wield and self.quicklook_equipped_dict["sub"]["item"]["Type"]=="Weapon":
                new_sub_item = gear_pyfile.all_gear["Empty"]
                self.quicklook_equipped_dict["sub"]["item"] = new_sub_item
                self.quicklook_equipped_dict["sub"]["icon"] = self.ctx.get_equipment_icon(new_sub_item["Name"])
                self.ctx.set_button_icon(self.quicklook_equipped_dict["sub"]["button"], self.quicklook_equipped_dict["sub"]["icon"])
                self.quicklook_equipped_dict["sub"]["button"].setToolTip(self.ctx.format_tooltip_stats(new_sub_item))

        # Hide abilities not accessible to the selected main/sub/ML combo.
        main_job = self.ctx.state.jobs_dict[self.main_job_combobox.currentText()]
        sub_job = self.ctx.state.jobs_dict[self.sub_job_selection_combobox.currentText()] if self.sub_job_selection_combobox.currentText() != "None" else "None"
        for ability_name in self.all_special_toggles_dict:

            self.all_special_toggles_dict[ability_name]["checkbox"].setVisible(False) # Hide all abilities so we can show them again in order and retain alphabetical ordering.

            if (main_job.lower() in self.all_special_toggles_dict[ability_name]["job requirement"]) or (self.sub_job_level >= self.all_special_toggles_dict[ability_name]["level requirement"] and sub_job.lower() in self.all_special_toggles_dict[ability_name]["job requirement"]):
                if (main_job.lower()=="dnc" and ability_name.lower() in ["haste samba (sub)", "box step (sub)"]) or (main_job.lower()=="war" and ability_name.lower()=="warcry (sub)"):
                    continue
                self.all_special_toggles_dict[ability_name]["checkbox"].setVisible(True)
            else:
                self.all_special_toggles_dict[ability_name]["checkbox"].setChecked(False)

    def copy_to_clipboard(self, type: str) -> None:
        '''
        When clicking the "Copy to Clipboard" button.
        Build a set that can be copy-pasted into a gearswap lua (ignoring augments)
        '''
        if type=="tp":
            equipped_gear_dict = self.ctx.tp_quicklook_equipped_dict
        # type == quicklook
        # type == ws
        else:
            equipped_gear_dict = self.quicklook_equipped_dict

        output_string = "new_set = {\n"
        for gear_slot in self.quicklook_equipped_dict:
            item_name = self.ctx.state.item_id_dict["name2"][self.ctx.state.item_id_dict["name"]==equipped_gear_dict[gear_slot]['item']['Name'].lower()][0]
            item_name = " ".join(k.capitalize() for k in item_name.split())
            output_string = output_string + f"    {gear_slot}=\"{item_name}\",\n"
        output_string = output_string + "}"

        QtWidgets.QApplication.clipboard().setText(output_string)

    def update_quicklook_equipment(self, selection: tuple[str, str, str]) -> None:
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

        if source == "tp":
            equipped_items_dict = self.ctx.tp_quicklook_equipped_dict
            scrollframe = self.ctx.tp_quicklook_scrollframes
        elif source == "ws":
            equipped_items_dict = self.ctx.ws_quicklook_equipped_dict
            scrollframe = self.ctx.ws_quicklook_scrollframes
        # source == quicklook
        else:
            equipped_items_dict = self.quicklook_equipped_dict
            scrollframe = self.quicklook_scrollframes

        new_item = gear_pyfile.all_gear[new_item_name] # New item (dictionary of stats)
        old_item = equipped_items_dict[slot]["item"]   # Old item (dictionary of stats)

        # Snapshot ring/earring slots for the swap-on-reselect convenience below.
        ring1_item_before: GearPiece = equipped_items_dict["ring1"]["item"]
        ring2_item_before: GearPiece = equipped_items_dict["ring2"]["item"]
        ear1_item_before: GearPiece = equipped_items_dict["ear1"]["item"]
        ear2_item_before: GearPiece = equipped_items_dict["ear2"]["item"]

        # Equip the new item now.
        equipped_items_dict[slot]["item"] = new_item
        equipped_items_dict[slot]["icon"] = self.ctx.get_equipment_icon(new_item["Name"])
        self.ctx.set_button_icon(equipped_items_dict[slot]["button"], equipped_items_dict[slot]["icon"])
        equipped_items_dict[slot]["button"].setToolTip(self.ctx.format_tooltip_stats(new_item))

        # Remove equipment in other slots if the new combination is not possible
        main_skill_type = equipped_items_dict["main"]["item"]["Skill Type"]
        sub_item_type = equipped_items_dict["sub"]["item"]["Type"]
        if slot == "main":
            main_job_shorthand = self.ctx.state.jobs_dict[self.main_job_combobox.currentText()]
            sub_job_shorthand = self.ctx.state.jobs_dict.get(self.sub_job_selection_combobox.currentText(), "None")
            dual_wield = (main_job_shorthand in ["nin", "dnc", "thf", "blu"] or sub_job_shorthand in ["nin", "dnc"])
            if (main_skill_type in ["Great Sword", "Great Katana", "Great Axe", "Polearm", "Scythe", "Staff"] and sub_item_type not in ["Grip", "None"]) or (main_skill_type=="Hand-to-Hand") or (main_skill_type in ["Axe", "Club", "Dagger", "Sword", "Katana"] and (sub_item_type not in ["Shield", "None"]) and not dual_wield):
                new_sub_item = gear_pyfile.all_gear["Empty"]
                equipped_items_dict["sub"]["item"] = new_sub_item
                equipped_items_dict["sub"]["icon"] = self.ctx.get_equipment_icon(new_sub_item["Name"])
                self.ctx.set_button_icon(equipped_items_dict["sub"]["button"], equipped_items_dict["sub"]["icon"])
                equipped_items_dict["sub"]["button"].setToolTip(self.ctx.format_tooltip_stats(new_sub_item))
                scrollframe["sub"].set_selected("Empty")


            if source=="quicklook":
                self.wpn_type_main = equipped_items_dict["main"]["item"]["Skill Type"]
                self.ctx._set_combo_values(self.ws_selection_combobox, self.ctx.state.ws_dict[self.wpn_type_main] + (self.ctx.state.ws_dict[self.wpn_type_ranged] if self.wpn_type_ranged not in ["None", "Instrument"] else []))
                if old_item["Skill Type"] != new_item["Skill Type"]:
                    if self.ws_selection_combobox.currentText() not in [self.ws_selection_combobox.itemText(i) for i in range(self.ws_selection_combobox.count())]:
                        self.ws_selection_combobox.setCurrentText(self.ctx.state.ws_dict[self.wpn_type_main][0])

        if slot == "sub":
            if (sub_item_type=="Grip" and main_skill_type not in ["Great Sword", "Great Katana", "Great Axe", "Polearm", "Scythe", "Staff"]) or (sub_item_type=="Shield" and main_skill_type not in ["None", "Axe", "Club", "Dagger", "Sword", "Katana"]):
                new_main_item = gear_pyfile.all_gear["Empty"]
                equipped_items_dict["main"]["item"] = new_main_item
                equipped_items_dict["main"]["icon"] = self.ctx.get_equipment_icon(new_main_item["Name"])
                self.ctx.set_button_icon(equipped_items_dict["main"]["button"], equipped_items_dict["main"]["icon"])
                equipped_items_dict["main"]["button"].setToolTip(self.ctx.format_tooltip_stats(new_main_item))
                scrollframe["main"].set_selected("Empty")

        ranged_item_type = equipped_items_dict["ranged"]["item"]["Type"]
        ammo_item_type = equipped_items_dict["ammo"]["item"]["Type"]
        if slot == "ranged":
            if (ranged_item_type=="Gun" and ammo_item_type != "Bullet") or (ranged_item_type=="Bow" and ammo_item_type != "Arrow") or (ranged_item_type=="Crossbow" and ammo_item_type != "Bolt") or (ranged_item_type in ["Instrument", "Equipment"]):
                new_ammo_item = gear_pyfile.all_gear["Empty"]
                equipped_items_dict["ammo"]["item"] = new_ammo_item
                equipped_items_dict["ammo"]["icon"] = self.ctx.get_equipment_icon(new_ammo_item["Name"])
                self.ctx.set_button_icon(equipped_items_dict["ammo"]["button"], equipped_items_dict["ammo"]["icon"])
                equipped_items_dict["ammo"]["button"].setToolTip(self.ctx.format_tooltip_stats(new_ammo_item))
                scrollframe["ammo"].set_selected("Empty")

            if source=="quicklook":
                self.wpn_type_ranged = equipped_items_dict["ranged"]["item"]["Skill Type"]
                self.ctx._set_combo_values(self.ws_selection_combobox, self.ctx.state.ws_dict[self.wpn_type_main] + (self.ctx.state.ws_dict[self.wpn_type_ranged] if self.wpn_type_ranged not in ["None", "Instrument"] else []))
                if (old_item["Skill Type"] != new_item["Skill Type"]) and (new_item["Skill Type"] not in ["Instrument"]):
                    if self.ws_selection_combobox.currentText() not in self.ctx.state.ws_dict[self.wpn_type_main] + self.ctx.state.ws_dict[self.wpn_type_ranged]:
                        self.ws_selection_combobox.setCurrentText(self.ctx.state.ws_dict[self.wpn_type_main][0])

        if slot == "ammo":
            if (ammo_item_type=="Bullet" and ranged_item_type != "Gun") or (ammo_item_type=="Arrow" and ranged_item_type != "Bow") or (ammo_item_type=="Bolt" and ranged_item_type != "Crossbow") or (ammo_item_type in ["Equipment", "Shuriken"]):
                new_ammo_item = gear_pyfile.all_gear["Empty"]
                equipped_items_dict["ranged"]["item"] = new_ammo_item
                equipped_items_dict["ranged"]["icon"] = self.ctx.get_equipment_icon(new_ammo_item["Name"])
                self.ctx.set_button_icon(equipped_items_dict["ranged"]["button"], equipped_items_dict["ranged"]["icon"])
                equipped_items_dict["ranged"]["button"].setToolTip(self.ctx.format_tooltip_stats(new_ammo_item))
                scrollframe["ranged"].set_selected("Empty")

        # Swap the rings if selecting ring1/ring2 to be the item in ring2/ring1 slot.
        if ((slot == "ring1" and (new_item == ring2_item_before)) or (slot == "ring2" and (new_item == ring1_item_before))) and (new_item["Name"] != "Empty"):
            equipped_items_dict["ring1"]["item"] = ring2_item_before
            equipped_items_dict["ring1"]["icon"] = self.ctx.get_equipment_icon(ring2_item_before["Name"])
            self.ctx.set_button_icon(equipped_items_dict["ring1"]["button"], equipped_items_dict["ring1"]["icon"])
            equipped_items_dict["ring1"]["button"].setToolTip(self.ctx.format_tooltip_stats(ring2_item_before))
            scrollframe["ring1"].set_selected(ring2_item_before["Name2"])

            equipped_items_dict["ring2"]["item"] = ring1_item_before
            equipped_items_dict["ring2"]["icon"] = self.ctx.get_equipment_icon(ring1_item_before["Name"])
            self.ctx.set_button_icon(equipped_items_dict["ring2"]["button"], equipped_items_dict["ring2"]["icon"])
            equipped_items_dict["ring2"]["button"].setToolTip(self.ctx.format_tooltip_stats(ring1_item_before))
            scrollframe["ring2"].set_selected(ring1_item_before["Name2"])

        # Swap the earrings if selecting ear1/ear2 to be the item in ear2/ear1 slot.
        if ((slot == "ear1" and (new_item == ear2_item_before)) or (slot == "ear2" and (new_item == ear1_item_before))) and (new_item["Name"] != "Empty"):
            equipped_items_dict["ear1"]["item"] = ear2_item_before
            equipped_items_dict["ear1"]["icon"] = self.ctx.get_equipment_icon(ear2_item_before["Name"])
            self.ctx.set_button_icon(equipped_items_dict["ear1"]["button"], equipped_items_dict["ear1"]["icon"])
            equipped_items_dict["ear1"]["button"].setToolTip(self.ctx.format_tooltip_stats(ear2_item_before))
            scrollframe["ear1"].set_selected(ear2_item_before["Name2"])

            equipped_items_dict["ear2"]["item"] = ear1_item_before
            equipped_items_dict["ear2"]["icon"] = self.ctx.get_equipment_icon(ear1_item_before["Name"])
            self.ctx.set_button_icon(equipped_items_dict["ear2"]["button"], equipped_items_dict["ear2"]["icon"])
            equipped_items_dict["ear2"]["button"].setToolTip(self.ctx.format_tooltip_stats(ear1_item_before))
            scrollframe["ear2"].set_selected(ear1_item_before["Name2"])

        # Can't equip a cloak with a hat
        if slot == "body":
            if ("cloak" in new_item_name.lower()):
                equipped_items_dict["head"]["item"] = gear_pyfile.all_gear["Empty"]
                equipped_items_dict["head"]["icon"] = self.ctx.get_equipment_icon(gear_pyfile.all_gear["Empty"]["Name"])
                self.ctx.set_button_icon(equipped_items_dict["head"]["button"], equipped_items_dict["head"]["icon"])
                equipped_items_dict["head"]["button"].setToolTip(self.ctx.format_tooltip_stats(gear_pyfile.all_gear["Empty"]))
                scrollframe["head"].set_selected("Empty")

        if slot == "head":
            if ("cloak" in equipped_items_dict["body"]["item"]["Name"].lower()):
                equipped_items_dict["body"]["item"] = gear_pyfile.all_gear["Empty"]
                equipped_items_dict["body"]["icon"] = self.ctx.get_equipment_icon(gear_pyfile.all_gear["Empty"]["Name"])
                self.ctx.set_button_icon(equipped_items_dict["body"]["button"], equipped_items_dict["body"]["icon"])
                equipped_items_dict["body"]["button"].setToolTip(self.ctx.format_tooltip_stats(gear_pyfile.all_gear["Empty"]))
                scrollframe["body"].set_selected("Empty")

        # Update the radio button selections based on the newly equipped gear.
        scrollframe[slot].set_selected(new_item_name)


    def update_buffs(self, event: str) -> None:
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
            self.food_selections_dict["combobox"].setToolTip(self.ctx.format_tooltip_stats(self.food_selections_dict["item"]))

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

    def aggregate_buffs(self) -> tuple[dict[str, dict[str, Any]], dict[str, Any]]:
        '''
        Called by quicklook function
        Reads GUI values to determine active buffs and debuffs.
        Returns dictionary containing the sum of enabled buffs.
        '''
        buffs: dict[str, dict[str, Any]] = {"brd":{}, "cor":{}, "geo":{}, "whm":{}, "food":{}}
        debuffs: dict[str, dict[str, Any]] = {"cor":{}, "geo":{}, "whm":{}, "other":{}}

        # BRD buffs
        if self.brd_checkbox.isChecked() == True:
            for song_slot in self.song_selections_dict:
                song_name = self.song_selections_dict[song_slot]["combobox"].currentText()
                if song_name in buffs_pyfile.brd:
                    song_bonus = int(self.song_bonus_combobox.currentText().split("+")[-1]) # "Songs +7" etc
                    song_bonus_limit = buffs_pyfile.brd_song_limits[song_name]   # Some songs are limited to Songs+X due to instrument requirements or other gear limitations.
                    soul_voice = 1.0 + 1.0*self.soul_voice_checkbox.isChecked() # Soul Voice affects all songs. 
                    marcato = 1.0 + 0.5*self.marcato_checkbox.isChecked() if song_slot in ["Song1"] else 1.0 # Marcato only affects song in slot 1
                    for stat, song in buffs_pyfile.brd[song_name].items():
                        buffs["brd"][stat] = buffs["brd"].get(stat, 0) + soul_voice * marcato * (song.base + min(song_bonus_limit, song_bonus)*song.per_point) + 20*("minuet" in song_name.lower() and stat.lower() in ["attack", "ranged attack"]) # +20 Attack to Minuets from Job Point gifts (assuming it applies to all players, not just the BRD)

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
                    for stat, roll in buffs_pyfile.cor[roll_name].items():
                        job_bonus = self.job_bonus_checkbox.isChecked() * roll.job_bonus
                        buffs["cor"][stat] = buffs["cor"].get(stat, 0) + crooked_cards * (roll.potency[roll_value] + roll_bonus*roll.per_roll_bonus + job_bonus)

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
                    for stat, bubble in buffs_pyfile.geo[bubble_name].items():
                        buffs["geo"][stat] = buffs["geo"].get(stat, 0) + bolster * bog * (bubble.base + bubble_bonus*bubble.per_point)

                # GEO Debuffs
                if bubble_name in buffs_pyfile.geo_debuffs:

                    # Debuffing bubbles are frequently reduced to 10~70% of their original potency.
                    bubble_potency = max(0, int(self.bubble_potency_entry.text() or 0)/100)

                    for stat, bubble in buffs_pyfile.geo_debuffs[bubble_name].items():
                        debuffs["geo"][stat] = debuffs["geo"].get(stat, 0) + bolster * bog * (bubble.base + bubble_bonus*bubble.per_point) * bubble_potency

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
        combined_debuffs: dict[str, Any] = {}
        for source in debuffs:
            for stat in debuffs[source]:
                combined_debuffs[stat] = combined_debuffs.get(stat, 0) + debuffs[source][stat]
        # print(buffs)
        return buffs, combined_debuffs

    def update_visible_quicklook_frame(self, slot: str) -> None:
        '''
        When clicking the quicklook equipment icons.
        Raise the selected slot's frame to the top and make it visible.
        Update the self.visible_quicklook_frame_slot variable
        '''
        self.ctx.set_visible_frame(self.quicklook_scrollframes[slot])
        self.visible_quicklook_frame_slot = slot

    def equip_set(self, gearset: Gearset) -> None:
        '''
        Equip a full gear set into the quicklook gear (slot by slot, updating
        icons/tooltips/radio selections) and switch to the Quicklook tab.
        Connected to OptimizeTab.bestSetReady.
        '''
        for slot in gearset:
            self.quicklook_equipped_dict[slot]["item"] = gearset[slot]
            self.quicklook_equipped_dict[slot]["icon"] = self.ctx.get_equipment_icon(self.quicklook_equipped_dict[slot]["item"]["Name"])
            self.ctx.set_button_icon(self.quicklook_equipped_dict[slot]["button"], self.quicklook_equipped_dict[slot]["icon"])
            self.quicklook_equipped_dict[slot]["button"].setToolTip(self.ctx.format_tooltip_stats(self.quicklook_equipped_dict[slot]["item"]))

            best_item_name = gearset[slot]["Name2"]
            self.quicklook_scrollframes[slot].set_selected(best_item_name)

            self.ctx.notebook.setCurrentIndex(0)
