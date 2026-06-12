'''
Automaton tab (work in progress).

Placeholder pet/attachment/maneuver grid for Puppetmaster. Currently gated off
at the controller call site; the tab still builds standalone for development.
Buttons print their slot id pending real attachment-selection logic.
'''

import numpy as np

from PySide6 import QtCore, QtWidgets


class AutomatonTab(QtWidgets.QWidget):
    '''Pet head/frame, attachment grid, and maneuver column for Puppetmaster.'''

    def __init__(self, ctx, parent=None):
        super().__init__(parent)
        self.ctx = ctx

        layout = QtWidgets.QGridLayout(self)

        self.automaton_equipped_dict = {f"slot{i}": {} for i in range(21)}  # 16 attachments, 3 maneuvers, head, frame

        container = QtWidgets.QFrame()
        container.setFrameShape(QtWidgets.QFrame.Shape.Box)
        container.setFixedSize(350, 350)
        container_layout = QtWidgets.QGridLayout(container)
        layout.addWidget(container, 0, 0, QtCore.Qt.AlignmentFlag.AlignCenter)

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
            self.automaton_equipped_dict[f"slot{i}"]["icon"] = self.ctx.get_equipment_icon(random_pet[i])
            button = QtWidgets.QPushButton()
            self.ctx.set_button_icon(button, self.automaton_equipped_dict[f"slot{i}"]["icon"])
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

        for i, slot in enumerate(self.ctx.state.equipment_button_positions):
            random_attachment = np.random.choice(["Fire Attachment", "Ice Attachment", "Thunder Attachment", "Earth Attachment", "Light Attachment", "Dark Attachment", "Water Attachment", "Wind Attachment"])
            self.automaton_equipped_dict[f"slot{i+2}"]["icon"] = self.ctx.get_equipment_icon(random_attachment)
            button = QtWidgets.QPushButton()
            self.ctx.set_button_icon(button, self.automaton_equipped_dict[f"slot{i+2}"]["icon"])
            button.clicked.connect(lambda checked=False, event=f"slot{i+2}": print(event))
            self.automaton_equipped_dict[f"slot{i+2}"]["button"] = button
            row, col = self.ctx.state.equipment_button_positions[slot]
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
            self.automaton_equipped_dict[f"slot{i+18}"]["icon"] = self.ctx.get_equipment_icon(random_maneuvers[i])
            button = QtWidgets.QPushButton()
            self.ctx.set_button_icon(button, self.automaton_equipped_dict[f"slot{i+18}"]["icon"])
            button.clicked.connect(lambda checked=False, event=f"slot{i+18}": print(event))
            self.automaton_equipped_dict[f"slot{i+18}"]["button"] = button
            maneuver_layout.addWidget(button, i, 0)
