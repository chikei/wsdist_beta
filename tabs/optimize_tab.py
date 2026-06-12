'''
Optimize tab: per-slot gear checkboxes, selection filters, DT requirements,
set metrics, and the buttons that launch optimizer runs.

The tab owns its input widgets and the `optimize_scrollframes` registry. The
optimizer run is driven by SimulateTab (`ctx.simulate_tab.quicklook`). "Equip
best set" emits `bestSetReady`, which QuicklookTab consumes to equip the gear.
Job changes arrive via `apply_slot_refilter` (QuicklookTab.slotsRefiltered).
'''

from typing import Any, cast

import numpy as np

from PySide6 import QtCore, QtGui, QtWidgets

import gear as gear_pyfile
from widgets import WheelIntLineEdit, make_combo
from virtual_frames import VirtualCheckboxFrame


class OptimizeTab(QtWidgets.QWidget):
    '''Gear-selection grid and optimizer controls.'''

    bestSetReady = QtCore.Signal(dict)  # Emitted with the best gearset when "Equip best set" is clicked.

    # Comboboxes attached dynamically via setattr() (keyed by their object_name).
    mastery_rank_combobox: QtWidgets.QComboBox
    ody_rank_combobox: QtWidgets.QComboBox
    soa_ring_combobox: QtWidgets.QComboBox
    tvr_ring_combobox: QtWidgets.QComboBox

    def __init__(self, ctx: Any, parent: QtWidgets.QWidget | None = None) -> None:
        super().__init__(parent)
        self.ctx = ctx
        self.visible_optimize_frame_slot = "main"

        layout = QtWidgets.QGridLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(2)
        layout.setRowStretch(0, 1)
        layout.setColumnStretch(0, 1)

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
        layout.addWidget(optimize_frame_top, 0, 0, align_top)

        optimize_frame_topleft = QtWidgets.QWidget()
        optimize_frame_topleft.setFixedSize(300, 400)
        optimize_frame_topleft_layout = QtWidgets.QGridLayout(optimize_frame_topleft)
        optimize_frame_topleft_layout.setContentsMargins(2, 2, 2, 2)
        optimize_frame_topleft_layout.setSpacing(2)
        optimize_frame_top_layout.addWidget(optimize_frame_topleft, 0, 0, align_top)

        # 4x4 grid of slot buttons that raise the matching scrollframe.
        buttons_grid_frame1 = QtWidgets.QWidget()
        buttons_grid_layout = QtWidgets.QGridLayout(buttons_grid_frame1)
        buttons_grid_layout.setContentsMargins(0, 0, 0, 0)
        buttons_grid_layout.setSpacing(1)
        optimize_frame_topleft_layout.addWidget(buttons_grid_frame1, 0, 0, align_hcenter)

        button_size = 48
        select_buttons_dict: dict[str, dict[str, Any]] = {slot: {} for slot in self.ctx.state.equipment_button_positions}
        for slot in self.ctx.state.equipment_button_positions:
            button = QtWidgets.QPushButton(slot)
            button.setFixedSize(button_size, button_size)
            button.clicked.connect(lambda checked=False, event=slot: self.update_visible_optimize_frame(event))
            select_buttons_dict[slot]["button"] = button
            row, col = self.ctx.state.equipment_button_positions[slot]
            buttons_grid_layout.addWidget(button, row, col)

        # "Select all" family of buttons.
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

        # Conditional selection filters (ranks, rings, mastery).
        select_conditionals_frame = QtWidgets.QWidget()
        select_conditionals_layout = QtWidgets.QGridLayout(select_conditionals_frame)
        select_conditionals_layout.setContentsMargins(0, 0, 0, 0)
        select_conditionals_layout.setSpacing(2)
        select_conditionals_layout.setColumnStretch(1, 1)
        optimize_frame_topleft_layout.addWidget(select_conditionals_frame, 2, 0, align_top)

        ody_selections = ["30", "25", "20", "15", "0", "None"]
        mastery_rank_selections = ["MR10", "MR09", "MR08", "MR07", "MR06", "MR05"]

        for row, (text, values, default, object_name, attr) in enumerate([
            ("Odyssey Rank:", ody_selections, "30", "defaults_ody_rank_combobox", "ody_rank_combobox"),
            ("SoA Ring:", self.ctx.state.soa_rings, "Weatherspoon", "defaults_soa_ring_combobox", "soa_ring_combobox"),
            ("TVR Ring:", self.ctx.state.tvr_rings, "Lehko Habhoka's", "defaults_tvr_ring_combobox", "tvr_ring_combobox"),
            ("Mastery Rank:", mastery_rank_selections, "MR07", "defaults_mastery_rank_combobox", "mastery_rank_combobox"),
        ]):
            label = QtWidgets.QLabel(text)
            label.setMinimumWidth(160)
            select_conditionals_layout.addWidget(label, row, 0, align_left)
            combo = make_combo(values, default, object_name=object_name)
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

        # 16 stacked scrollframes of gear checkboxes, one per slot.
        opt_scrollframe_relative_frame = QtWidgets.QWidget()
        opt_scrollframe_relative_frame.setFixedSize(370, 300)
        optimize_frame_top_layout.addWidget(opt_scrollframe_relative_frame, 0, 1)

        opt_scrollframe_stack = QtWidgets.QStackedLayout(opt_scrollframe_relative_frame)
        self.optimize_scrollframes: dict[str, Any] = {}
        for slot in self.ctx.state.all_equipment_dict:
            equipment_list = sorted([k["Name2" if "Name2" in k else "Name"] for k in self.ctx.state.all_equipment_dict[slot]])
            self.optimize_scrollframes[slot] = VirtualCheckboxFrame(opt_scrollframe_relative_frame,
                                                                    text=f"  Select {slot.capitalize()}  ",
                                                                    master_data=equipment_list,
                                                                    N=22,
                                                                    )
            opt_scrollframe_stack.addWidget(self.optimize_scrollframes[slot])

        # Bottom: DT requirements, set metrics, and optimizer run buttons.
        optimize_frame_bottom = QtWidgets.QWidget()
        optimize_frame_bottom.setFixedSize(630, 320)
        optimize_frame_bottom_layout = QtWidgets.QGridLayout(optimize_frame_bottom)
        optimize_frame_bottom_layout.setContentsMargins(0, 0, 0, 0)
        optimize_frame_bottom_layout.setSpacing(2)
        optimize_frame_bottom_layout.setColumnStretch(0, 1)
        optimize_frame_bottom_layout.setColumnStretch(1, 1)
        layout.addWidget(optimize_frame_bottom, 1, 0, align_top)

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
            combo = make_combo(values, default, width_chars=15)
            optimize_frame_bottomleft_layout.addWidget(combo, row, 1)
            setattr(self, attr, combo)

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
            button.clicked.connect(lambda checked=False, e=event: self.ctx.simulate_tab.quicklook(e))
            optimize_buttons_layout.addWidget(button, pos[0], pos[1])

        self.equip_best_set_button = QtWidgets.QPushButton("Equip best set")
        self.equip_best_set_button.setFixedSize(100, 30)
        self.equip_best_set_button.setEnabled(False)
        self.equip_best_set_button.clicked.connect(lambda checked=False: self.bestSetReady.emit(self.ctx.best_player.gearset))
        optimize_buttons_layout.addWidget(self.equip_best_set_button, 1, 1)

        self.ctx.set_visible_frame(self.optimize_scrollframes["main"])

    def apply_slot_refilter(self, updates: dict[str, Any]) -> None:
        '''
        Slot for QuicklookTab.slotsRefiltered. Refresh each affected optimize
        scrollframe with the job-filtered item list (computed by QuicklookTab).
        updates maps slot -> (filtered_item_list, deselect_spec), where
        deselect_spec is "all", a list of item names, or None.
        '''
        for slot, (data, deselect) in updates.items():
            self.optimize_scrollframes[slot].set_visible_data(data)
            if deselect == "all":
                self.optimize_scrollframes[slot].deselect("all")
            elif deselect is not None:
                self.optimize_scrollframes[slot].deselect(deselect)

    def update_visible_optimize_frame(self, slot: str) -> None:
        '''Raise the selected slot's scrollframe and record it as the visible slot.'''
        self.ctx.set_visible_frame(self.optimize_scrollframes[slot])
        self.visible_optimize_frame_slot = slot

    def select_gear_opt(self, event: Any) -> None:
        '''Select or unselect gear in the optimize scrollframes based on the button pressed.'''
        tvr_ring_names = [k.lower() + " ring" for k in self.ctx.state.tvr_rings]
        soa_ring_names = [k.lower() + " ring +1" for k in self.ctx.state.soa_rings]

        empyrean_names = ["Hattori", "Heathen's", "Wicce", "Lethargy", "Peltast's", "Ebers", "Kasuga", "Arbatel", "Boii", "Chasseur's", "Fili", "Skulker's", "Bhikku", "Maculele", "Nukumi", "Azimuth", "Chevalier's", "Amini", "Hashishin", "Erilaz", "Karagoz", "Beckoner's"]
        relic_names = ["Pedagogy", "Hesychast", "Vitiation", "Mochizuki", "Fallen", "Horos", "Pitre", "Luhlaza", "Plunderer", "Bagua", "Archmage", "Piety", "Agoge", "Caballarius", "Wakido", "Ankusa", "Bihu", "Glyphic", "Lanun", "Arcadian", "Pteroslaver", "Futhark"]
        af_names = ["Academic", "Anchorite", "Atrophy", "Hachiya", "Ignominy", "Maxixi", "Foire", "Assimilator", "Pillager", "Geomancy", "Spaekona", "Theophany", "Pummeler", "Reverence", "Sakonji", "Totemic", "Brioso", "Convoker", "Laksamana", "Orion", "Vishap", "Runeist"]

        input_items_full: list[str] = []  # Full item names
        if event == "select all file":
            filename, _ = QtWidgets.QFileDialog.getOpenFileName(self, 'Select file', './')

            if len(filename) > 0:
                with open(filename, "r") as ifile:
                    for line in ifile:
                        try:
                            item_name_abbreviated = line.split('"')[1]
                            name_match_mask = cast(Any, np.char.lower(self.ctx.state.item_id_dict["name2"]) == item_name_abbreviated.lower())
                            item_index = np.flatnonzero(name_match_mask)
                            if len(item_index) > 0:
                                input_items_full.append(str(self.ctx.state.item_id_dict["name"][item_index[0]]))
                            else:
                                continue
                        except Exception:
                            continue

        for slot in self.optimize_scrollframes:

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

                item = gear_pyfile.all_gear[item_name]

                if event == "select all file":
                    if (item["Name"].lower() in input_items_full):
                        self.optimize_scrollframes[slot].select(item_name)
                    elif (item["Name"].lower().split(" +")[0] in input_items_full):
                        self.optimize_scrollframes[slot].select(item_name)

                # Deselect if the item's Odyssey rank does not match the selected Odyssey Rank.
                if str(item.get("Rank", self.ody_rank_combobox.currentText())) != self.ody_rank_combobox.currentText():
                    self.optimize_scrollframes[slot].deselect(item_name)

                # Swap Nyame R30B for R25B if the checkbox is enabled. Deselect Nyame Paths "not B".
                if "nyame" in item_name.lower():
                    if "B" != item_name[-1]:
                        self.optimize_scrollframes[slot].deselect(item_name)
                    elif self.ody_rank_combobox.currentText() == "30" and self.nyame25_checkbox.isChecked():
                        if "30B" in item_name:
                            self.optimize_scrollframes[slot].deselect(item_name)
                        elif "25B" in item_name and event in ["select all", "select all slot"]:
                            self.optimize_scrollframes[slot].select(item_name)

                if slot in ["main", "sub", "ranged"]:
                    if item_name.split()[0] in self.ctx.state.rema_weapons and "R15" not in item_name:
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
                        if item_name.split()[0] in relic_names + af_names and "+4" not in item_name:
                            self.optimize_scrollframes[slot].deselect(item_name)
                        if item_name.split()[0] in empyrean_names and "+3" not in item_name:
                            self.optimize_scrollframes[slot].deselect(item_name)
                    for limbus_set_name in ["hope", "perfection", "revelation", "trust", "prestige", "sworn", "bravery", "intrepid", "indomitable", "justice", "magnificent", "duty", "mercy", "grace", "clemency"]:
                        if limbus_set_name in item_name.lower() and "R30" in item_name:
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
