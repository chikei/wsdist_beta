"""
Virtual checkbox and radio frame widgets.

Native Qt list widgets that render a fixed window of ``N`` rows over a larger
backing dataset, scrolled with a sidebar/mouse wheel.
"""

from PySide6 import QtCore, QtWidgets


class _VirtualFrameBase(QtWidgets.QGroupBox):
    def __init__(self, parent=None, text=""):
        super().__init__(str(text), parent)

    def _build_list_layout(self):
        self.setLayout(QtWidgets.QGridLayout())
        self.layout().setContentsMargins(4, 4, 4, 4)
        self.layout().setSpacing(2)

        self.inner = QtWidgets.QWidget(self)
        self.inner_layout = QtWidgets.QVBoxLayout(self.inner)
        self.inner_layout.setContentsMargins(0, 0, 0, 0)
        self.inner_layout.setSpacing(0)
        self.layout().addWidget(self.inner, 0, 0)

        self.scrollbar = QtWidgets.QScrollBar(QtCore.Qt.Orientation.Vertical, self)
        self.scrollbar.valueChanged.connect(self._on_scrollbar_value_changed)
        self.layout().addWidget(self.scrollbar, 0, 1)

    def _install_wheel_filter(self, *widgets):
        for widget in widgets:
            widget.installEventFilter(self)

    def eventFilter(self, obj, event):
        if event.type() == QtCore.QEvent.Type.Wheel:
            self._scroll_by_delta(event.angleDelta().y())
            return True
        return super().eventFilter(obj, event)

    def wheelEvent(self, event):
        self._scroll_by_delta(event.angleDelta().y())
        event.accept()

    def _scroll_by_delta(self, delta):
        if self.total_items <= self.N:
            return
        self.start_index += -1 if delta > 0 else 1
        self._clamp()
        self._refresh()

    def _on_scrollbar_value_changed(self, value):
        if getattr(self, "_updating_scrollbar", False):
            return
        self.start_index = value
        self._clamp()
        self._refresh()

    def _clamp(self):
        self.start_index = max(0, min(self.start_index, max(0, self.total_items - self.N)))

    def _update_scrollbar_visibility(self):
        self.scrollbar.setVisible(self.total_items > self.N)

    def _update_scrollbar(self):
        maximum = max(0, self.total_items - self.N)
        self._updating_scrollbar = True
        self.scrollbar.setRange(0, maximum)
        self.scrollbar.setPageStep(max(1, self.N))
        self.scrollbar.setValue(self.start_index)
        self._updating_scrollbar = False


class VirtualRadioFrame(_VirtualFrameBase):
    def __init__(self, parent, master_data, N=12, command=None, equipment_slot=None, selection_type=None, text=""):
        super().__init__(parent, text=text)
        self.N = N
        self.master_data = sorted(master_data)
        self.visible_data = self.master_data.copy()
        self.total_items = len(self.visible_data)
        self.start_index = 0
        self.selected_value = ""

        self.equipment_slot = equipment_slot
        self.command = command
        self.selection_type = selection_type

        self._build_list_layout()
        self.button_group = QtWidgets.QButtonGroup(self)
        self.button_group.setExclusive(True)
        self.radio_buttons = []
        for i in range(self.N):
            rb = QtWidgets.QRadioButton("", self.inner)
            rb.toggled.connect(lambda checked, idx=i: self._on_button_toggled(idx, checked))
            self.inner_layout.addWidget(rb)
            self.button_group.addButton(rb)
            self.radio_buttons.append(rb)
            self._install_wheel_filter(rb)
        self.inner_layout.addStretch(1)
        self._install_wheel_filter(self, self.inner)

        self._update_scrollbar_visibility()
        self._refresh()

    def set_visible_data(self, filtered_list):
        """
        Update the visible items by passing a list.
        Show items that are in the input list and the master "all items" list.
        """
        self.visible_data = sorted([x for x in filtered_list if x in self.master_data])
        self.total_items = len(self.visible_data)
        self.start_index = 0
        self._update_scrollbar_visibility()
        self._refresh()

    def get_selected(self):
        return self.selected_value

    def set_selected(self, value):
        if value in self.master_data:
            self.selected_value = value
        self._refresh()

    def _refresh(self):
        for i in range(self.N):
            idx = self.start_index + i
            rb = self.radio_buttons[i]
            if idx < self.total_items:
                val = self.visible_data[idx]
                rb.blockSignals(True)
                rb.setText(val)
                rb.setProperty("item_value", val)
                rb.setChecked(val == self.selected_value)
                rb.setEnabled(True)
                rb.blockSignals(False)
            else:
                rb.blockSignals(True)
                rb.setText("")
                rb.setProperty("item_value", "")
                rb.setChecked(False)
                rb.setEnabled(False)
                rb.blockSignals(False)
        self._update_scrollbar()

    def _on_button_toggled(self, widget_index, checked):
        if not checked:
            return
        idx = self.start_index + widget_index
        if idx >= self.total_items:
            return
        self.selected_value = self.visible_data[idx]
        if self.command:
            event = (self.equipment_slot, self.selected_value, self.selection_type)
            self.command(event)


class VirtualCheckboxFrame(_VirtualFrameBase):
    def __init__(self, parent, master_data, N=12, text=""):
        super().__init__(parent, text=text)
        self.N = N
        self.master_data = sorted(master_data)
        self.visible_data = self.master_data.copy()
        self.total_items = len(self.visible_data)
        self.start_index = 0

        self.selection_state = {name: False for name in self.master_data}

        self._build_list_layout()
        self.checkbuttons = []
        for i in range(self.N):
            cb = QtWidgets.QCheckBox("", self.inner)
            cb.toggled.connect(lambda checked, idx=i: self._update_selection(idx, checked))
            self.inner_layout.addWidget(cb)
            self.checkbuttons.append(cb)
            self._install_wheel_filter(cb)
        self.inner_layout.addStretch(1)
        self._install_wheel_filter(self, self.inner)

        self._update_scrollbar_visibility()
        self._refresh()

    def set_visible_data(self, filtered_list):
        self.visible_data = sorted([x for x in filtered_list if x in self.master_data])
        self.total_items = len(self.visible_data)
        self.start_index = 0
        self._update_scrollbar_visibility()
        self._refresh()

    def get_selected(self):
        return [k for k, v in self.selection_state.items() if v]

    def set_selected(self, names):
        for name in names:
            if name in self.selection_state:
                self.selection_state[name] = True
        self._refresh()

    def deselect(self, name):
        targets = self._resolve_targets(
            name,
            "deselect() expects 'all', 'visible', a string, or a list/tuple/set of strings",
        )
        for k in targets:
            if isinstance(k, str) and k in self.selection_state:
                self.selection_state[k] = False
        self._refresh()

    def select(self, name):
        targets = self._resolve_targets(
            name,
            "select() expects 'all', 'visible', a string, or a list/tuple/set of strings",
        )
        for k in targets:
            if isinstance(k, str) and k in self.selection_state:
                self.selection_state[k] = True
        self._refresh()

    def _resolve_targets(self, name, error_message):
        if name == "all":
            return self.selection_state.keys()
        if name == "visible":
            return self.visible_data
        if isinstance(name, str):
            return [name]
        if isinstance(name, (list, tuple, set)):
            return name
        raise TypeError(error_message)

    def _update_selection(self, widget_index, checked):
        idx = self.start_index + widget_index
        if idx < self.total_items:
            name = self.visible_data[idx]
            self.selection_state[name] = bool(checked)

    def _refresh(self):
        for i in range(self.N):
            idx = self.start_index + i
            cb = self.checkbuttons[i]
            if idx < self.total_items:
                name = self.visible_data[idx]
                cb.blockSignals(True)
                cb.setText(name)
                cb.setChecked(self.selection_state.get(name, False))
                cb.setEnabled(True)
                cb.blockSignals(False)
            else:
                cb.blockSignals(True)
                cb.setText("")
                cb.setChecked(False)
                cb.setEnabled(False)
                cb.blockSignals(False)
        self._update_scrollbar()


def generate_data(n=5000):
    import random

    return [f"Item {i:04d} - " + "".join(random.choices("ABCDEFGHIJKLMNOPQRSTUVWXYZ", k=5)) for i in range(n)]


def main():
    app = QtWidgets.QApplication([])
    root = QtWidgets.QWidget()
    root.setWindowTitle("Virtual Frames External Filtering Demo")
    root.resize(1000, 600)

    master_data = generate_data(5000)

    layout = QtWidgets.QGridLayout(root)
    search_entry = QtWidgets.QLineEdit(root)
    layout.addWidget(QtWidgets.QLabel("Filter externally:", root), 0, 0)
    layout.addWidget(search_entry, 0, 1)

    radio_frame = VirtualRadioFrame(root, master_data, N=15)
    checkbox_frame = VirtualCheckboxFrame(root, master_data, N=15)
    layout.addWidget(radio_frame, 1, 0)
    layout.addWidget(checkbox_frame, 1, 1)

    def apply_filter(text):
        ft = text.lower()
        filtered_list = [x for x in master_data if ft in x.lower()]
        radio_frame.set_visible_data(filtered_list)
        checkbox_frame.set_visible_data(filtered_list)

    search_entry.textChanged.connect(apply_filter)
    root.show()
    app.exec()


if __name__ == "__main__":
    main()
