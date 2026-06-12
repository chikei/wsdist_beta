'''
Simulate tab: the Equipped TP set and Equipped WS set gear pickers plus the
DPS-simulation / distribution / compare buttons.

The tab owns the TP and WS gear dicts and their scrollframes, the sim driver
(`quicklook`) and `copy_gearset_dict`. Player/enemy/buff inputs come from
QuicklookTab via `ctx.quicklook_tab.gather_player_inputs()`; the menu toggles,
shared helpers, and `best_player` are reached through `ctx`.
'''

import numpy as np
from PySide6 import QtCore, QtWidgets

import gear as gear_pyfile
import create_player as create_player_pyfile
import actions as actions_pyfile
import wsdist as wsdist_pyfile
import fancy_plot as fancy_plot_pyfile
from virtual_frames import VirtualRadioFrame


class SimulateTab(QtWidgets.QWidget):
    '''TP/WS gear sets and the simulation controls.'''

    def __init__(self, ctx, parent=None):
        super().__init__(parent)
        self.ctx = ctx

        layout = QtWidgets.QGridLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(5)
        layout.setColumnStretch(0, 1)

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
                ("Copy to Clipboard", lambda e=copy_clip_event: self.ctx.quicklook_tab.copy_to_clipboard(e)),
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
                self.ctx.set_button_icon(button, equipped_dict[slot]["icon"])
                equipped_dict[slot]["button"] = button
                button.setToolTip(self.ctx.format_tooltip_stats(equipped_dict[slot]["item"]))
                row, col = self.ctx.state.equipment_button_positions[slot]
                gear_layout.addWidget(button, row, col)

            radio_frame = QtWidgets.QWidget()
            radio_frame.setFixedSize(400, 350)
            outer_layout.addWidget(radio_frame, 0, 1, QtCore.Qt.AlignmentFlag.AlignRight)
            radio_stack = QtWidgets.QStackedLayout(radio_frame)
            for slot in self.ctx.state.all_equipment_dict:
                equipment_list = sorted([k["Name2" if "Name2" in k else "Name"] for k in self.ctx.state.all_equipment_dict[slot]])
                scrollframes[slot] = VirtualRadioFrame(radio_frame, text=f"  Select {slot.capitalize()}  ", equipment_slot=slot, selection_type=set_type, command=self.ctx.quicklook_tab.update_quicklook_equipment, master_data=equipment_list, N=13)
                radio_stack.addWidget(scrollframes[slot])
            return outer

        # Top frame: TP set.
        self.tp_quicklook_equipped_dict = {slot: {"icon": self.ctx.get_equipment_icon(), "item": gear_pyfile.Empty} for slot in self.ctx.state.all_equipment_dict}
        self.tp_quicklook_scrollframes = {}
        simulations_tp_frame = build_simulation_set("tp", "Equipped TP set", self.tp_quicklook_equipped_dict, self.tp_quicklook_scrollframes, self.update_visible_quicklook_frame_tp, "tp to quicklook", "tp")
        layout.addWidget(simulations_tp_frame, 0, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        # Bottom frame: WS set.
        self.ws_quicklook_equipped_dict = {slot: {"icon": self.ctx.get_equipment_icon(), "item": gear_pyfile.Empty} for slot in self.ctx.state.all_equipment_dict}
        self.ws_quicklook_scrollframes = {}
        simulations_ws_frame = build_simulation_set("ws", "Equipped WS set", self.ws_quicklook_equipped_dict, self.ws_quicklook_scrollframes, self.update_visible_quicklook_frame_ws, "ws to quicklook", "ws")
        layout.addWidget(simulations_ws_frame, 1, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        # Simulation buttons.
        simulation_button_frame = QtWidgets.QWidget()
        simulation_button_layout = QtWidgets.QGridLayout(simulation_button_frame)
        simulation_button_layout.setContentsMargins(0, 0, 0, 0)
        simulation_button_layout.setSpacing(5)
        for col in (0, 1, 2):
            simulation_button_layout.setColumnStretch(col, 1)
        layout.addWidget(simulation_button_frame, 2, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

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

    def update_visible_quicklook_frame_tp(self, slot):
        '''Raise the TP set's scrollframe for the selected slot.'''
        self.ctx.set_visible_frame(self.tp_quicklook_scrollframes[slot])
        self.tp_visible_quicklook_frame_slot = slot

    def update_visible_quicklook_frame_ws(self, slot):
        '''Raise the WS set's scrollframe for the selected slot.'''
        self.ctx.set_visible_frame(self.ws_quicklook_scrollframes[slot])
        self.ws_visible_quicklook_frame_slot = slot

    def copy_gearset_dict(self, event):
        '''
        When clicking the Copy to TP/WS/Quickook buttons
        Copy the gearset displayed at the source to the destination.
        '''
        if event=="quicklook to tp":
            source_dict = self.ctx.quicklook_tab.quicklook_equipped_dict
            destination_dict = self.tp_quicklook_equipped_dict
            destination_tab = "2"
            destination_scrollframe = self.tp_quicklook_scrollframes
        elif event=="quicklook to ws":
            source_dict = self.ctx.quicklook_tab.quicklook_equipped_dict
            destination_dict = self.ws_quicklook_equipped_dict
            destination_tab = "2"
            destination_scrollframe = self.ws_quicklook_scrollframes
        elif event=="tp to quicklook":
            source_dict = self.tp_quicklook_equipped_dict
            destination_dict = self.ctx.quicklook_tab.quicklook_equipped_dict
            destination_scrollframe = self.ctx.quicklook_tab.quicklook_scrollframes
            destination_tab = "0"
        elif event=="ws to quicklook":
            source_dict = self.ws_quicklook_equipped_dict
            destination_dict = self.ctx.quicklook_tab.quicklook_equipped_dict
            destination_scrollframe = self.ctx.quicklook_tab.quicklook_scrollframes
            destination_tab = "0"

        for slot in destination_dict:
            destination_dict[slot]["item"] = source_dict[slot]["item"]
            destination_dict[slot]["icon"] = self.ctx.get_equipment_icon(destination_dict[slot]["item"]["Name"])
            self.ctx.set_button_icon(destination_dict[slot]["button"], destination_dict[slot]["icon"])
            destination_dict[slot]["button"].setToolTip(self.ctx.format_tooltip_stats(destination_dict[slot]["item"]))

            source_item_name = source_dict[slot]["item"]["Name2"]
            destination_scrollframe[slot].set_selected(source_item_name)

        self.ctx.notebook.setCurrentIndex(int(destination_tab))

    def quicklook(self, trigger):
        '''
        When clicking the "Quicklook WS" button.
        Compile the player stats from selected buffs and equipment.
        Run create_player() to build a player character with complete stats.
        Run average_ws() with the given player and selected WS parameters.
        '''
        q = self.ctx.quicklook_tab.gather_player_inputs()
        main_job = q["main_job"]
        sub_job = q["sub_job"]
        master_level = q["master_level"]
        ws_name = q["ws_name"]
        ws_type = q["ws_type"]
        spell_name = q["spell_name"]
        tp_entry_value = q["tp_value"]

        special_toggles_dict = q["special_toggles"]
        special_toggles_dict["99999"] = self.ctx.damage_limit99999.isChecked()

        active_buffs, active_debuffs = self.ctx.quicklook_tab.aggregate_buffs()

        equipped_gearset = {slot:self.ctx.quicklook_tab.quicklook_equipped_dict[slot]["item"] for slot in self.ctx.quicklook_tab.quicklook_equipped_dict}
        player = create_player_pyfile.create_player(main_job, sub_job, master_level, gearset=equipped_gearset, buffs=active_buffs, abilities=special_toggles_dict,)

        useful_enemy_stats = q["enemy_stats"]
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
            output = actions_pyfile.average_ws(player, enemy, ws_name, tp_entry_value, ws_type, "Damage dealt")
            self.ctx.quicklook_tab.quicklook_results_damage_label1.setText("Average Damage =")
            self.ctx.quicklook_tab.quicklook_results_tp_label1.setText("Average TP =")
            self.ctx.quicklook_tab.quicklook_damage_value = output[1][0]
            self.ctx.quicklook_tab.quicklook_results_damage_label2.setText(f"{self.ctx.quicklook_tab.quicklook_damage_value:.0f}")
            self.ctx.quicklook_tab.quicklook_tp_value = output[1][1]
            self.ctx.quicklook_tab.quicklook_results_tp_label2.setText(f"{self.ctx.quicklook_tab.quicklook_tp_value:.1f}")

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
            self.ctx.quicklook_tab.quicklook_results_damage_label1.setText("Average Damage =")
            self.ctx.quicklook_tab.quicklook_results_tp_label1.setText("Average TP =")
            self.ctx.quicklook_tab.quicklook_damage_value = output[1][0]
            self.ctx.quicklook_tab.quicklook_results_damage_label2.setText(f"{self.ctx.quicklook_tab.quicklook_damage_value:.0f}")
            self.ctx.quicklook_tab.quicklook_tp_value = output[1][1]
            self.ctx.quicklook_tab.quicklook_results_tp_label2.setText(f"{self.ctx.quicklook_tab.quicklook_tp_value:.1f}")

        elif trigger=="tp":
            output = actions_pyfile.average_attack_round(player, enemy, 0, tp_entry_value, "Time to WS")
            self.ctx.quicklook_tab.quicklook_results_damage_label1.setText("Time per WS =")
            self.ctx.quicklook_tab.quicklook_results_tp_label1.setText("TP/round =")
            self.ctx.quicklook_tab.quicklook_damage_value = output[0]
            self.ctx.quicklook_tab.quicklook_results_damage_label2.setText(f"{self.ctx.quicklook_tab.quicklook_damage_value:.3f}")
            self.ctx.quicklook_tab.quicklook_tp_value = output[1][1]
            self.ctx.quicklook_tab.quicklook_results_tp_label2.setText(f"{self.ctx.quicklook_tab.quicklook_tp_value:.1f}")


        elif "optimize" in trigger:

            # Build the list of equipment to check based on the checkbox selections in each slot.
            # check_gear_dict is a dictionary containing lists of full gear.py item dictionaries

            check_gear_dict = {slot:[gear_pyfile.all_gear[item_name] for item_name in self.ctx.optimize_tab.optimize_scrollframes[slot].get_selected()] for slot in self.ctx.quicklook_tab.quicklook_equipped_dict}

            assert any([len(check_gear_dict[k])>1 for k in check_gear_dict]), "At least two items must be selected in at least one slot to find the best set."

            if "ws" in trigger:
                if not ws_name or ws_name=="None":
                    print("No weapon skill selected.")
                    return

            if "spell" in trigger:
                if not spell_name or spell_name=="None":
                    print("No spell selected.")
                    return

            special_toggles_dict["Verbose Swaps"] = self.ctx.verbose_swaps.isChecked()
            starting_gearset = {slot:self.ctx.quicklook_tab.quicklook_equipped_dict[slot]["item"] for slot in self.ctx.quicklook_tab.quicklook_equipped_dict}

            actions = {
                        "optimize ws":    ["weapon skill", self.ctx.optimize_tab.ws_metric_combobox.currentText()],
                        "optimize spell": ["spell cast", self.ctx.optimize_tab.spell_metric_combobox.currentText()],
                        "optimize tp":    ["attack round", self.ctx.optimize_tab.tp_metric_combobox.currentText()],
                    }

            self.ctx.best_player, _ = wsdist_pyfile.build_set(main_job, sub_job, master_level, active_buffs, special_toggles_dict, enemy, ws_name, spell_name, actions[trigger][0],
                                                            tp_entry_value, check_gear_dict, starting_gearset,
                                                            self.ctx.optimize_tab.pdt_requirements_entry.value(), self.ctx.optimize_tab.mdt_requirements_entry.value(), actions[trigger][1],
                                                            self.ctx.optimize_tab.show_similar_results_checkbox.isChecked(), int(self.ctx.optimize_tab.show_similar_results_entry.text() or 0),
                                                        )
            self.ctx.optimize_tab.equip_best_set_button.setEnabled(True)

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
            ws_output_tp = actions_pyfile.average_ws(tp_player, enemy, ws_name, tp_entry_value, ws_type, "Damage dealt")
            tp_player_wsdmg = int(ws_output_tp[0])
            ws_output_ws = actions_pyfile.average_ws(ws_player, enemy, ws_name, tp_entry_value, ws_type, "Damage dealt")
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
                equipped_gearset = {slot:self.ctx.quicklook_tab.quicklook_equipped_dict[slot]["item"] for slot in self.ctx.quicklook_tab.quicklook_equipped_dict}

            elif "tp" in trigger:
                equipped_gearset = {slot:self.tp_quicklook_equipped_dict[slot]["item"] for slot in self.tp_quicklook_equipped_dict}

            elif "ws" in trigger:
                equipped_gearset = {slot:self.ws_quicklook_equipped_dict[slot]["item"] for slot in self.ws_quicklook_equipped_dict}

            player = create_player_pyfile.create_player(main_job, sub_job, master_level, gearset=equipped_gearset, buffs=active_buffs, abilities=special_toggles_dict,)

            for stat in self.ctx.stats_tab.stats_dict:

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

                self.ctx.stats_tab.stats_dict[stat]["label2"].setText(str(value))
