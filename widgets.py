'''
Reusable Qt widget classes and factory helpers shared across the GUI tabs.
'''

from PySide6 import QtCore, QtGui, QtWidgets


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


def make_combo(values, default, object_name=None, width_chars=18, on_selected=None):
    '''Build a QComboBox from values, selecting default (added if absent).

    Args:
        values: Iterable of items to populate the combo box.
        default: Initial selection; appended as an extra item if not already present.
        object_name: Optional Qt object name (used by the defaults save/load system).
        width_chars: Minimum width expressed in approximate character widths.
        on_selected: Optional callable connected to the textActivated signal.

    Returns:
        The configured QComboBox.
    '''
    combo = QtWidgets.QComboBox()
    items = [str(v) for v in values]
    combo.addItems(items)
    if str(default) and str(default) not in items:
        combo.addItem(str(default))
    combo.setCurrentText(str(default))
    if object_name:
        combo.setObjectName(object_name)
    combo.setMinimumWidth(width_chars * 8)
    if on_selected is not None:
        combo.textActivated.connect(on_selected)
    return combo
