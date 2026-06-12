'''
File containing code to automatically find and return the state of GUI widgets in an application.

Authors: ChatGPT (GPT-5.1), OpenAI.  Kastra (Asura server)
'''

from PySide6 import QtWidgets


def walk_widgets(root):
    """Yield the root widget and all of its descendant widgets."""
    yield root
    yield from root.findChildren(QtWidgets.QWidget)


def get_widget_state(widget):
    '''
    Given a widget object, return the relevant state/value of the widget.
    '''
    if isinstance(widget, QtWidgets.QComboBox):
        return widget.currentText()  # Current selection (not the full list of options)

    if isinstance(widget, QtWidgets.QCheckBox):
        return widget.isChecked()  # True/False

    if isinstance(widget, QtWidgets.QLineEdit):
        return widget.text()  # Current text entered

    if isinstance(widget, QtWidgets.QLabel):
        return widget.text()

    # Quicklook gear icon buttons are not saved.
    # Instead, the currently equipped items are saved from self.quicklook_equipped_dict[slot]["item"]["Name2"]
    # and the icons and tooltips are built from that.
    return None


def set_widget_state(widget, value):
    '''
    Given a widget object and a previously saved value, update the widget object using the saved value.
    '''
    # Check QComboBox first: in Qt it is not a QLineEdit, but keep the explicit order for clarity.
    if isinstance(widget, QtWidgets.QComboBox):
        text = str(value)
        if widget.findText(text) < 0 and text:
            widget.addItem(text)
        widget.setCurrentText(text)

    elif isinstance(widget, QtWidgets.QCheckBox):
        widget.setChecked(bool(value))

    elif isinstance(widget, QtWidgets.QLineEdit):
        widget.setText(str(value))
