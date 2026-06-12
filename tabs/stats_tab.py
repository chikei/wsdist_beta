'''
Player Stats tab: a read-only grid of the computed gear-set stats.

The tab owns `stats_dict` (the label registry). The controller's `quicklook`
sim driver populates those labels; the three buttons re-run `quicklook` for the
quicklook / TP / WS gear sets.
'''

from typing import Any

from PySide6 import QtCore, QtGui, QtWidgets


class StatsTab(QtWidgets.QWidget):
    '''Displays computed player stats grouped by category.'''

    def __init__(self, ctx: Any, parent: QtWidgets.QWidget | None = None) -> None:
        super().__init__(parent)
        self.ctx = ctx

        layout = QtWidgets.QGridLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.setSpacing(2)
        layout.setColumnStretch(0, 1)

        align_top = QtCore.Qt.AlignmentFlag.AlignTop
        align_hcenter = QtCore.Qt.AlignmentFlag.AlignHCenter

        buttons_frame = QtWidgets.QWidget()
        buttons_frame.setFixedSize(600, 50)
        buttons_layout = QtWidgets.QGridLayout(buttons_frame)
        buttons_layout.setContentsMargins(0, 0, 0, 0)
        buttons_layout.setSpacing(2)
        for col in (0, 1, 2):
            buttons_layout.setColumnStretch(col, 1)
        layout.addWidget(buttons_frame, 0, 0, align_hcenter | align_top)

        for col, (label, event) in enumerate([
            ("Quicklook Gear Stats", "show stats quicklook"),
            ("TP Gear Stats", "show stats tp"),
            ("WS Gear Stats", "show stats ws"),
        ]):
            button = QtWidgets.QPushButton(label)
            button.setFixedSize(150, 30)
            button.clicked.connect(lambda checked=False, e=event: self.ctx.simulate_tab.quicklook(e))
            buttons_layout.addWidget(button, 0, col)

        stats_frame = QtWidgets.QFrame()
        stats_frame.setFrameShape(QtWidgets.QFrame.Shape.Box)
        stats_frame.setLineWidth(2)
        stats_frame.setFixedSize(675, 750)
        stats_frame_layout = QtWidgets.QGridLayout(stats_frame)
        stats_frame_layout.setContentsMargins(0, 0, 0, 0)
        stats_frame_layout.setSpacing(2)
        layout.addWidget(stats_frame, 1, 0)

        stats_subframes: list[QtWidgets.QGridLayout] = []
        for sub_row in range(3):
            sub = QtWidgets.QWidget()
            sub_layout = QtWidgets.QGridLayout(sub)
            sub_layout.setContentsMargins(0, 0, 0, 0)
            sub_layout.setSpacing(2)
            stats_frame_layout.addWidget(sub, sub_row, 0, QtCore.Qt.AlignmentFlag.AlignLeft)
            stats_subframes.append(sub_layout)
        stats_frame1_layout, stats_frame2_layout, stats_frame3_layout = stats_subframes

        useful_stats = [
                        ["STR", "DEX", "VIT", "AGI", "INT", "MND", "CHR"],
                        ["Accuracy1", "Accuracy2", "Attack1", "Attack2", "Ranged Accuracy", "Ranged Attack",],
                        ["Magic Accuracy", "Magic Attack", "Magic Damage", "Magic Burst Damage", "Magic Burst Damage II", "Magic Burst Damage Trait",],
                        ["Daken", "Zanshin", "Kick Attacks", "DA", "TA", "QA", "Double Shot", "Triple Shot", "Quad Shot",],
                        ["Dual Wield", "Martial Arts", "Gear Haste", "JA Haste", "Magic Haste", "Delay Reduction",],
                        ["PDT", "MDT", "DT", "Evasion", "Magic Evasion", "Magic Defense", "Subtle Blow", "Subtle Blow II", ],
                        ["Regain", "Store TP", "Crit Rate", "Crit Damage", "Ranged Crit Damage", "Weapon Skill Damage", "Weapon Skill Damage Trait", "Skillchain Bonus", "PDL", "PDL Trait", "TP Bonus", ]
                        ]
        self.stats_dict: dict[str, dict[str, Any]] = {stat: {} for k in useful_stats for stat in k}

        stat_font = QtGui.QFont("Courier", 10)

        def build_stat_group(parent_layout: QtWidgets.QGridLayout, title: str, stats_list: list[str], size: tuple[int, int], title_align: QtCore.Qt.AlignmentFlag, grid_pos: tuple[int, int], sticky: str | None = None) -> QtWidgets.QGroupBox:
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
