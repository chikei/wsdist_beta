'''
Qt scrollable label frame used by the GUI.
'''

from PySide6 import QtWidgets


class ScrollableLabelFrame(QtWidgets.QGroupBox):
    """
    LabelFrame containing a Qt scroll area.

    Callers add child widgets to ``self.interior``'s layout, e.g.
    ``self.interior.layout().addWidget(child)``.
    """

    def __init__(self, parent=None, text=""):
        super().__init__(str(text), parent)

        self._scroll_area = QtWidgets.QScrollArea(self)
        self._scroll_area.setWidgetResizable(True)
        self._scroll_area.setFrameShape(QtWidgets.QFrame.Shape.NoFrame)

        self.interior = QtWidgets.QFrame()
        interior_layout = QtWidgets.QVBoxLayout(self.interior)
        interior_layout.setContentsMargins(0, 0, 0, 0)
        interior_layout.setSpacing(2)
        self._scroll_area.setWidget(self.interior)

        layout = QtWidgets.QGridLayout(self)
        layout.setContentsMargins(0, 0, 0, 0)
        layout.addWidget(self._scroll_area, 0, 0)
