import sys
from PyQt6.QtWidgets import (QApplication, QWidget, QRubberBand, QVBoxLayout, QHBoxLayout,
                             QLabel, QMainWindow, QSystemTrayIcon, QMenu, QPushButton,
                             QFrame, QComboBox, QGroupBox, QSizePolicy)
from PyQt6.QtCore import Qt, QRect, QPoint, QSize, pyqtSignal
from PyQt6.QtGui import QColor, QPalette, QFont, QAction, QIcon

# Available languages for translation
SUPPORTED_LANGUAGES = [
    "Spanish", "French", "German", "Italian", "Portuguese", 
    "Chinese", "Japanese", "Korean", "Russian", "Arabic"
]

# Stylesheet constants for modern UI
STYLE_SHEET = """
QWidget#ControlWindow {
    background-color: #1a1a2e;
}
QLabel {
    color: #e0e0e0;
}
QLabel#title {
    color: #ffffff;
    font-size: 22px;
    font-weight: bold;
}
QLabel#subtitle {
    color: #a0a0a0;
    font-size: 12px;
}
QLabel#status {
    color: #4ecca3;
    font-size: 14px;
    font-weight: bold;
}
QLabel#statusIdle {
    color: #a0a0a0;
}
QLabel#statusRunning {
    color: #4ecca3;
}
QLabel#statusError {
    color: #ff6b6b;
}
QPushButton {
    background-color: #16213e;
    color: #e0e0e0;
    border: 2px solid #0f3460;
    border-radius: 8px;
    padding: 12px 24px;
    font-size: 14px;
    font-weight: bold;
}
QPushButton:hover {
    background-color: #0f3460;
    border-color: #4ecca3;
}
QPushButton:pressed {
    background-color: #4ecca3;
    color: #1a1a2e;
}
QPushButton:disabled {
    background-color: #2a2a4e;
    color: #606080;
    border-color: #2a2a4e;
}
QPushButton#primaryButton {
    background-color: #4ecca3;
    color: #1a1a2e;
    border: none;
    font-size: 16px;
    padding: 16px 32px;
}
QPushButton#primaryButton:hover {
    background-color: #3db892;
}
QPushButton#primaryButton:disabled {
    background-color: #2a5a4a;
    color: #1a1a2e;
}
QPushButton#runButton {
    background-color: #4ecca3;
    color: #1a1a2e;
}
QPushButton#stopButton {
    background-color: #ff6b6b;
    color: #ffffff;
}
QGroupBox {
    color: #a0a0a0;
    font-weight: bold;
    border: 1px solid #2a2a4e;
    border-radius: 8px;
    margin-top: 12px;
    padding-top: 8px;
}
QGroupBox::title {
    subcontrol-origin: margin;
    left: 12px;
    padding: 0 6px;
}
QComboBox {
    background-color: #16213e;
    color: #e0e0e0;
    border: 2px solid #0f3460;
    border-radius: 6px;
    padding: 8px 12px;
    min-width: 150px;
}
QComboBox:hover {
    border-color: #4ecca3;
}
QComboBox::drop-down {
    border: none;
    padding-right: 8px;
}
QComboBox QAbstractItemView {
    background-color: #16213e;
    color: #e0e0e0;
    selection-background-color: #4ecca3;
    selection-color: #1a1a2e;
}
QFrame#guidancePanel {
    background-color: #16213e;
    border: 1px solid #0f3460;
    border-radius: 10px;
    padding: 16px;
}
QFrame#areaInfo {
    background-color: #0f3460;
    border-radius: 6px;
    padding: 8px;
}
"""

class SelectionOverlay(QWidget):
    area_selected = pyqtSignal(QRect)

    def __init__(self):
        super().__init__()
        self.setWindowFlags(Qt.WindowType.FramelessWindowHint | Qt.WindowType.WindowStaysOnTopHint | Qt.WindowType.Tool)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        self.setWindowState(Qt.WindowState.WindowMaximized)
        self.setCursor(Qt.CursorShape.CrossCursor)
        
        # Determine the geometry of the virtual desktop (all screens)
        screen_geometry = QApplication.primaryScreen().virtualGeometry()
        self.setGeometry(screen_geometry)

        self.rubber_band = QRubberBand(QRubberBand.Shape.Rectangle, self)
        self.origin = QPoint()
        
        # Instruction Label
        self.label = QLabel("Click and drag to select area. Press ESC to cancel.", self)
        self.label.setStyleSheet("color: white; background-color: rgba(0,0,0,150); padding: 10px; border-radius: 5px;")
        self.label.move(20, 20)

    def mousePressEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            self.origin = event.pos()
            self.rubber_band.setGeometry(QRect(self.origin, QSize()))
            self.rubber_band.show()

    def mouseMoveEvent(self, event):
        if not self.origin.isNull():
            self.rubber_band.setGeometry(QRect(self.origin, event.pos()).normalized())

    def mouseReleaseEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton:
            rect = self.rubber_band.geometry()
            # Convert widget coordinates to global screen coordinates
            global_top_left = self.mapToGlobal(rect.topLeft())
            global_rect = QRect(global_top_left, rect.size())
            
            self.area_selected.emit(global_rect)
            self.close()

    def keyPressEvent(self, event):
        if event.key() == Qt.Key.Key_Escape:
            self.close()


class ControlWindow(QWidget):
    """Main control window with first-time guidance and controls."""
    
    # Signals
    select_area_requested = pyqtSignal()
    start_requested = pyqtSignal()
    stop_requested = pyqtSignal()
    language_changed = pyqtSignal(str)
    
    def __init__(self, is_first_time=True, settings=None):
        super().__init__()
        self.settings = settings
        self.is_first_time = is_first_time
        self.selected_area = None
        self.is_running = False
        
        self.setObjectName("ControlWindow")
        self.setWindowTitle("Meowcal Subtitles")
        self.setFixedSize(450, 580 if is_first_time else 420)
        self.setStyleSheet(STYLE_SHEET)
        
        self._setup_ui()
        
    def _setup_ui(self):
        main_layout = QVBoxLayout(self)
        main_layout.setSpacing(16)
        main_layout.setContentsMargins(24, 24, 24, 24)
        
        # Title Section
        title_label = QLabel("🐱 Meowcal Subtitles")
        title_label.setObjectName("title")
        title_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        main_layout.addWidget(title_label)
        
        subtitle_label = QLabel("Real-time on-screen translation powered by local AI")
        subtitle_label.setObjectName("subtitle")
        subtitle_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        main_layout.addWidget(subtitle_label)
        
        main_layout.addSpacing(8)
        
        # First-Time Guidance Panel
        if self.is_first_time:
            guidance_panel = self._create_guidance_panel()
            main_layout.addWidget(guidance_panel)
        
        # Controls Section
        controls_group = QGroupBox("Controls")
        controls_layout = QVBoxLayout(controls_group)
        controls_layout.setSpacing(12)
        
        # Select Area Button
        self.select_area_btn = QPushButton("📐  Select Screen Area")
        self.select_area_btn.setObjectName("primaryButton")
        self.select_area_btn.clicked.connect(self._on_select_area)
        controls_layout.addWidget(self.select_area_btn)
        
        # Area Info Display
        self.area_info_frame = QFrame()
        self.area_info_frame.setObjectName("areaInfo")
        area_info_layout = QHBoxLayout(self.area_info_frame)
        area_info_layout.setContentsMargins(12, 8, 12, 8)
        
        self.area_label = QLabel("No area selected")
        self.area_label.setObjectName("subtitle")
        area_info_layout.addWidget(self.area_label)
        
        self.reselect_btn = QPushButton("Change")
        self.reselect_btn.setFixedWidth(80)
        self.reselect_btn.setVisible(False)
        self.reselect_btn.clicked.connect(self._on_select_area)
        area_info_layout.addWidget(self.reselect_btn)
        
        controls_layout.addWidget(self.area_info_frame)
        
        # Start/Stop Button
        self.run_btn = QPushButton("▶  Start Translation")
        self.run_btn.setObjectName("runButton")
        self.run_btn.setEnabled(False)
        self.run_btn.clicked.connect(self._on_run_toggle)
        controls_layout.addWidget(self.run_btn)
        
        main_layout.addWidget(controls_group)
        
        # Settings Section
        settings_group = QGroupBox("Settings")
        settings_layout = QVBoxLayout(settings_group)
        settings_layout.setSpacing(10)
        
        # Target Language
        lang_row = QHBoxLayout()
        lang_label = QLabel("Translate to:")
        lang_row.addWidget(lang_label)
        
        self.lang_combo = QComboBox()
        self.lang_combo.addItems(SUPPORTED_LANGUAGES)
        if self.settings:
            current_lang = self.settings.target_language
            if current_lang in SUPPORTED_LANGUAGES:
                self.lang_combo.setCurrentText(current_lang)
        self.lang_combo.currentTextChanged.connect(self._on_language_changed)
        lang_row.addWidget(self.lang_combo)
        lang_row.addStretch()
        
        settings_layout.addLayout(lang_row)
        main_layout.addWidget(settings_group)
        
        # Status Section
        status_frame = QFrame()
        status_layout = QHBoxLayout(status_frame)
        status_layout.setContentsMargins(0, 8, 0, 0)
        
        status_prefix = QLabel("Status:")
        status_prefix.setObjectName("subtitle")
        status_layout.addWidget(status_prefix)
        
        self.status_label = QLabel("Ready")
        self.status_label.setObjectName("statusIdle")
        status_layout.addWidget(self.status_label)
        status_layout.addStretch()
        
        # Help Button
        if not self.is_first_time:
            help_btn = QPushButton("?")
            help_btn.setFixedSize(28, 28)
            help_btn.setToolTip("Show quick start guide")
            help_btn.clicked.connect(self._show_help)
            status_layout.addWidget(help_btn)
        
        main_layout.addWidget(status_frame)
        main_layout.addStretch()
        
        # Dismiss guidance button (for first-time users)
        if self.is_first_time:
            dismiss_btn = QPushButton("Got it, let's start!")
            dismiss_btn.clicked.connect(self._dismiss_guidance)
            main_layout.addWidget(dismiss_btn)
    
    def _create_guidance_panel(self) -> QFrame:
        """Create the first-time setup guidance panel."""
        panel = QFrame()
        panel.setObjectName("guidancePanel")
        layout = QVBoxLayout(panel)
        layout.setSpacing(12)
        
        # Welcome Header
        welcome = QLabel("👋 Welcome!")
        welcome.setStyleSheet("font-size: 16px; font-weight: bold; color: #4ecca3;")
        layout.addWidget(welcome)
        
        # Instructions
        instructions = QLabel(
            "Quick Start Guide:\n\n"
            "1️⃣  Click 'Select Screen Area' below\n"
            "2️⃣  Draw a box around the text you want to translate\n"
            "     (e.g., game subtitles, video captions)\n"
            "3️⃣  Click 'Start Translation' to begin\n\n"
            "💡 Tip: The translation appears as a floating overlay!"
        )
        instructions.setWordWrap(True)
        instructions.setStyleSheet("line-height: 1.6;")
        layout.addWidget(instructions)
        
        # Note about first run
        note = QLabel(
            "ℹ️ First run will download the AI model (~2-3 GB).\n"
            "   This is a one-time setup."
        )
        note.setWordWrap(True)
        note.setStyleSheet("color: #a0a0a0; font-size: 11px;")
        layout.addWidget(note)
        
        return panel
    
    def _on_select_area(self):
        """Handle select area button click."""
        self.select_area_requested.emit()
        self.hide()
    
    def _on_run_toggle(self):
        """Handle start/stop button click."""
        if self.is_running:
            self.stop_requested.emit()
            self._set_running(False)
        else:
            self.start_requested.emit()
            self._set_running(True)
    
    def _on_language_changed(self, language: str):
        """Handle language selection change."""
        if self.settings:
            self.settings.target_language = language
        self.language_changed.emit(language)
    
    def _dismiss_guidance(self):
        """Dismiss the first-time guidance and mark setup complete."""
        if self.settings:
            self.settings.complete_first_time_setup()
        # Shrink window by hiding guidance on next show
        self.is_first_time = False
    
    def _show_help(self):
        """Show the help/guidance dialog."""
        # For simplicity, just toggle guidance visibility
        pass
    
    def set_selected_area(self, rect: QRect):
        """Update the UI with the selected area info."""
        self.selected_area = rect
        self.area_label.setText(f"Area: {rect.width()}×{rect.height()} at ({rect.x()}, {rect.y()})")
        self.reselect_btn.setVisible(True)
        self.run_btn.setEnabled(True)
        self.select_area_btn.setText("📐  Area Selected ✓")
    
    def _set_running(self, running: bool):
        """Update UI to reflect running state."""
        self.is_running = running
        if running:
            self.run_btn.setText("⏹  Stop Translation")
            self.run_btn.setObjectName("stopButton")
            self.status_label.setText("Translating...")
            self.status_label.setObjectName("statusRunning")
            self.select_area_btn.setEnabled(False)
            self.reselect_btn.setEnabled(False)
        else:
            self.run_btn.setText("▶  Start Translation")
            self.run_btn.setObjectName("runButton")
            self.status_label.setText("Stopped")
            self.status_label.setObjectName("statusIdle")
            self.select_area_btn.setEnabled(True)
            self.reselect_btn.setEnabled(True)
        # Refresh styles
        self.run_btn.setStyleSheet(STYLE_SHEET)
        self.status_label.setStyleSheet("")
    
    def set_status(self, text: str, is_error: bool = False):
        """Set the status message."""
        self.status_label.setText(text)
        if is_error:
            self.status_label.setObjectName("statusError")
        else:
            self.status_label.setObjectName("statusIdle")
        self.status_label.setStyleSheet("")


class SubtitleWindow(QWidget):
    def __init__(self):
        super().__init__()
        self.setWindowFlags(Qt.WindowType.FramelessWindowHint | Qt.WindowType.WindowStaysOnTopHint | Qt.WindowType.Tool | Qt.WindowType.WindowTransparentForInput)
        self.setAttribute(Qt.WidgetAttribute.WA_TranslucentBackground)
        
        layout = QVBoxLayout()
        self.text_label = QLabel("Waiting for text...")
        self.text_label.setFont(QFont("Segoe UI", 16, QFont.Weight.Bold))
        self.text_label.setStyleSheet("""
            QLabel {
                color: #FFFFFF;
                background-color: rgba(0, 0, 0, 180);
                padding: 10px;
                border-radius: 8px;
            }
        """)
        self.text_label.setWordWrap(True)
        self.text_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        
        layout.addWidget(self.text_label)
        self.setLayout(layout)

    def update_text(self, text):
        if text:
            self.text_label.setText(text)
            self.adjustSize()
        else:
            self.text_label.setText("")

    def update_position(self, selection_rect):
        """
        Positions the subtitle window below the selected area.
        If near bottom of screen, places it above.
        """
        screen_geo = QApplication.primaryScreen().geometry()
        x = selection_rect.x()
        y = selection_rect.bottom() + 10 # Default: 10px below
        
        width = max(300, selection_rect.width())
        height = self.height()
        
        # Check if it fits below
        if y + height > screen_geo.bottom():
            # Place above
            y = selection_rect.top() - height - 10
        
        # Center horizontally relative to selection
        center_x = selection_rect.x() + (selection_rect.width() // 2) - (width // 2)
        
        self.setGeometry(center_x, y, width, height)
        self.show()


# Helper for system tray
def create_tray_icon(app, on_show_window, on_select_area):
    """Create system tray icon with menu."""
    tray = QSystemTrayIcon(QIcon.fromTheme("edit-select"), app)
    
    menu = QMenu()
    
    action_show = QAction("Show Window", app)
    action_show.triggered.connect(on_show_window)
    menu.addAction(action_show)
    
    action_select = QAction("Select Area", app)
    action_select.triggered.connect(on_select_area)
    menu.addAction(action_select)
    
    menu.addSeparator()
    
    action_quit = QAction("Quit", app)
    action_quit.triggered.connect(app.quit)
    menu.addAction(action_quit)
    
    tray.setContextMenu(menu)
    tray.activated.connect(lambda reason: on_show_window() if reason == QSystemTrayIcon.ActivationReason.DoubleClick else None)
    tray.show()
    return tray
