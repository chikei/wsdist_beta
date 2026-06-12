'''
Simulate tab: the Equipped TP set and Equipped WS set gear pickers plus the
DPS-simulation / distribution / compare buttons.

The tab owns the TP and WS gear dicts and their scrollframes. The sim driver
itself (`quicklook`) and the copy/clipboard helpers still live on the controller
and are reached through `ctx`; they relocate here once QuicklookTab exists.
'''

from PySide6 import QtCore, QtWidgets

import gear as gear_pyfile
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
                ("Copy to Quicklook", lambda e=copy_inputs_event: self.ctx.copy_gearset_dict(e)),
                ("Copy to Clipboard", lambda e=copy_clip_event: self.ctx.copy_to_clipboard(e)),
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
                scrollframes[slot] = VirtualRadioFrame(radio_frame, text=f"  Select {slot.capitalize()}  ", equipment_slot=slot, selection_type=set_type, command=self.ctx.update_quicklook_equipment, master_data=equipment_list, N=13)
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
        dps_simulation_button.clicked.connect(lambda checked=False: self.ctx.quicklook("run dps simulations"))
        simulation_button_layout.addWidget(dps_simulation_button, 0, 0)

        self.plot_dps_checkbox = QtWidgets.QCheckBox("Plot DPS")
        simulation_button_layout.addWidget(self.plot_dps_checkbox, 1, 0, QtCore.Qt.AlignmentFlag.AlignHCenter)

        ws_distribution_button = QtWidgets.QPushButton("Create WS damage\ndistribution plot")
        ws_distribution_button.setFixedSize(150, 30)
        ws_distribution_button.clicked.connect(lambda checked=False: self.ctx.quicklook("build distribution"))
        simulation_button_layout.addWidget(ws_distribution_button, 0, 1)

        compare_sets = QtWidgets.QPushButton("Compare TP & WS stats")
        compare_sets.setFixedSize(150, 30)
        compare_sets.clicked.connect(lambda checked=False: self.ctx.quicklook("compare tp ws stats"))
        simulation_button_layout.addWidget(compare_sets, 0, 2)

    def update_visible_quicklook_frame_tp(self, slot):
        '''Raise the TP set's scrollframe for the selected slot.'''
        self.ctx.set_visible_frame(self.tp_quicklook_scrollframes[slot])
        self.tp_visible_quicklook_frame_slot = slot

    def update_visible_quicklook_frame_ws(self, slot):
        '''Raise the WS set's scrollframe for the selected slot.'''
        self.ctx.set_visible_frame(self.ws_quicklook_scrollframes[slot])
        self.ws_visible_quicklook_frame_slot = slot
