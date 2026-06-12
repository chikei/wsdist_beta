"""
Checkbox and radio frame widgets backed by native Qt list views.

``QListWidget`` virtualizes rendering natively, so these are thin wrappers that
keep selection state and a filterable visible window over a master dataset.
"""

from collections.abc import Callable, Iterable
from typing import Any, cast

from PySide6 import QtCore, QtWidgets


class VirtualRadioFrame(QtWidgets.QGroupBox):
    """
    Single-selection list backed by a native ``QListWidget``.

    Qt's item views virtualize rendering, so no manual row windowing is needed.
    ``N`` is accepted for call-site compatibility but no longer affects sizing
    (parent containers are fixed-size with a stacked layout).
    """

    def __init__(self, parent: QtWidgets.QWidget | None = None, master_data: list[str] | None = None, N: int = 12, command: Callable[..., Any] | None = None, equipment_slot: str | None = None, selection_type: str | None = None, text: str = "") -> None:
        super().__init__(str(text), parent)
        self.N = N
        self.master_data = sorted(master_data or [])
        self.master_set = set(self.master_data)
        self.selected_value = ""

        self.equipment_slot = equipment_slot
        self.command = command
        self.selection_type = selection_type

        self._updating = False

        layout = QtWidgets.QVBoxLayout(self)
        layout.setContentsMargins(4, 4, 4, 4)
        self.list_widget = QtWidgets.QListWidget(self)
        self.list_widget.setSelectionMode(QtWidgets.QAbstractItemView.SelectionMode.SingleSelection)
        self.list_widget.setUniformItemSizes(True)
        self.list_widget.currentItemChanged.connect(self._on_current_changed)
        layout.addWidget(self.list_widget)

        self.set_visible_data(self.master_data)

    def set_visible_data(self, filtered_list: list[str]) -> None:
        """
        Update the visible items by passing a list.
        Show items that are in the input list and the master "all items" list.
        """
        visible = sorted(x for x in filtered_list if x in self.master_set)
        self._updating = True
        self.list_widget.clear()
        self.list_widget.addItems(visible)
        self._restore_selection()
        self._updating = False

    def get_selected(self) -> str:
        return self.selected_value

    def set_selected(self, value: str) -> None:
        if value in self.master_set:
            self.selected_value = value
        self._updating = True
        self._restore_selection()
        self._updating = False

    def _restore_selection(self) -> None:
        if not self.selected_value:
            self.list_widget.setCurrentItem(None)  # pyright: ignore[reportArgumentType]  # None clears selection at runtime
            return
        matches = self.list_widget.findItems(self.selected_value, QtCore.Qt.MatchFlag.MatchExactly)
        self.list_widget.setCurrentItem(matches[0] if matches else None)  # pyright: ignore[reportArgumentType]  # None clears selection at runtime

    def _on_current_changed(self, current: QtWidgets.QListWidgetItem | None, previous: QtWidgets.QListWidgetItem | None) -> None:
        if self._updating or current is None:
            return
        self.selected_value = current.text()
        if self.command:
            self.command((self.equipment_slot, self.selected_value, self.selection_type))


class VirtualCheckboxFrame(QtWidgets.QGroupBox):
    """
    Multi-selection checkbox list backed by a native ``QListWidget``.

    ``selection_state`` over the full master dataset is the source of truth;
    check state of the (filtered) visible rows mirrors it. ``N`` is accepted
    for call-site compatibility but no longer affects sizing.
    """

    def __init__(self, parent: QtWidgets.QWidget | None = None, master_data: list[str] | None = None, N: int = 12, text: str = "") -> None:
        super().__init__(str(text), parent)
        self.N = N
        self.master_data = sorted(master_data or [])
        self.master_set = set(self.master_data)
        self.visible_data = self.master_data.copy()
        self.selection_state = {name: False for name in self.master_data}

        self._updating = False

        layout = QtWidgets.QVBoxLayout(self)
        layout.setContentsMargins(4, 4, 4, 4)
        self.list_widget = QtWidgets.QListWidget(self)
        self.list_widget.setUniformItemSizes(True)
        self.list_widget.itemChanged.connect(self._on_item_changed)
        layout.addWidget(self.list_widget)

        self._populate(self.master_data)

    def set_visible_data(self, filtered_list: list[str]) -> None:
        self._populate(filtered_list)

    def get_selected(self) -> list[str]:
        return [k for k, v in self.selection_state.items() if v]

    def set_selected(self, names: list[str]) -> None:
        for name in names:
            if name in self.selection_state:
                self.selection_state[name] = True
        self._refresh_checks()

    def deselect(self, name: Any) -> None:
        targets = self._resolve_targets(
            name,
            "deselect() expects 'all', 'visible', a string, or a list/tuple/set of strings",
        )
        self._apply(targets, False)

    def select(self, name: Any) -> None:
        targets = self._resolve_targets(
            name,
            "select() expects 'all', 'visible', a string, or a list/tuple/set of strings",
        )
        self._apply(targets, True)

    def _resolve_targets(self, name: Any, error_message: str) -> Iterable[Any]:
        if name == "all":
            return self.selection_state.keys()
        if name == "visible":
            return self.visible_data
        if isinstance(name, str):
            return [name]
        if isinstance(name, (list, tuple, set)):
            return cast("Iterable[Any]", name)
        raise TypeError(error_message)

    def _apply(self, targets: Iterable[Any], value: bool) -> None:
        for k in targets:
            if isinstance(k, str) and k in self.selection_state:
                self.selection_state[k] = value
        self._refresh_checks()

    def _populate(self, visible: list[str]) -> None:
        self.visible_data = sorted(x for x in visible if x in self.master_set)
        self._updating = True
        self.list_widget.clear()
        for name in self.visible_data:
            item = QtWidgets.QListWidgetItem(name)
            item.setFlags(item.flags() | QtCore.Qt.ItemFlag.ItemIsUserCheckable)
            checked = self.selection_state.get(name, False)
            item.setCheckState(QtCore.Qt.CheckState.Checked if checked else QtCore.Qt.CheckState.Unchecked)
            self.list_widget.addItem(item)
        self._updating = False

    def _refresh_checks(self) -> None:
        self._updating = True
        for i in range(self.list_widget.count()):
            item = self.list_widget.item(i)
            checked = self.selection_state.get(item.text(), False)
            item.setCheckState(QtCore.Qt.CheckState.Checked if checked else QtCore.Qt.CheckState.Unchecked)
        self._updating = False

    def _on_item_changed(self, item: QtWidgets.QListWidgetItem) -> None:
        if self._updating:
            return
        name = item.text()
        if name in self.selection_state:
            self.selection_state[name] = item.checkState() == QtCore.Qt.CheckState.Checked


def generate_data(n: int = 5000) -> list[str]:
    import random

    return [f"Item {i:04d} - " + "".join(random.choices("ABCDEFGHIJKLMNOPQRSTUVWXYZ", k=5)) for i in range(n)]


def main() -> None:
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

    def apply_filter(text: str) -> None:
        ft = text.lower()
        filtered_list = [x for x in master_data if ft in x.lower()]
        radio_frame.set_visible_data(filtered_list)
        checkbox_frame.set_visible_data(filtered_list)

    search_entry.textChanged.connect(apply_filter)
    root.show()
    app.exec()


if __name__ == "__main__":
    main()
