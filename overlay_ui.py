import sys
from PyQt6.QtWidgets import (QApplication, QWidget, QRubberBand, QVBoxLayout, 
                             QLabel, QMainWindow, QSystemTrayIcon, QMenu)
from PyQt6.QtCore import Qt, QRect, QPoint, QSize, pyqtSignal
from PyQt6.QtGui import QColor, QPalette, QFont, QAction, QIcon

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
def create_tray_icon(app, on_select_area):
    tray = QSystemTrayIcon(QIcon.fromTheme("edit-select"), app) # Use system theme icon or placeholder
    
    menu = QMenu()
    
    action_select = QAction("Select Area", app)
    action_select.triggered.connect(on_select_area)
    menu.addAction(action_select)
    
    action_quit = QAction("Quit", app)
    action_quit.triggered.connect(app.quit)
    menu.addAction(action_quit)
    
    tray.setContextMenu(menu)
    tray.show()
    return tray
