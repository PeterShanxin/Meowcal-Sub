"""
User settings persistence for the translation app.
Handles first-time setup detection and user preferences.
"""
import json
import os
from typing import Optional, Dict, Any

SETTINGS_FILE = os.path.join(os.path.dirname(__file__), "user_settings.json")

DEFAULT_SETTINGS = {
    "first_time_setup_complete": False,
    "target_language": "Spanish",
    "ocr_language": "en-US",
    "last_selected_area": None,  # {"x": int, "y": int, "width": int, "height": int}
    "window_position": None,  # {"x": int, "y": int}
}

class UserSettings:
    """Manages user settings with JSON file persistence."""
    
    def __init__(self):
        self._settings = self._load()
    
    def _load(self) -> Dict[str, Any]:
        """Load settings from file, or return defaults."""
        if os.path.exists(SETTINGS_FILE):
            try:
                with open(SETTINGS_FILE, "r", encoding="utf-8") as f:
                    loaded = json.load(f)
                    # Merge with defaults to handle new settings
                    return {**DEFAULT_SETTINGS, **loaded}
            except (json.JSONDecodeError, IOError):
                pass
        return DEFAULT_SETTINGS.copy()
    
    def save(self):
        """Save current settings to file."""
        try:
            with open(SETTINGS_FILE, "w", encoding="utf-8") as f:
                json.dump(self._settings, f, indent=2)
        except IOError as e:
            print(f"Warning: Could not save settings: {e}")
    
    @property
    def is_first_time(self) -> bool:
        """Check if this is the user's first time using the app."""
        return not self._settings.get("first_time_setup_complete", False)
    
    def complete_first_time_setup(self):
        """Mark first-time setup as complete."""
        self._settings["first_time_setup_complete"] = True
        self.save()
    
    @property
    def target_language(self) -> str:
        return self._settings.get("target_language", "Spanish")
    
    @target_language.setter
    def target_language(self, value: str):
        self._settings["target_language"] = value
        self.save()
    
    @property
    def ocr_language(self) -> str:
        return self._settings.get("ocr_language", "en-US")
    
    @ocr_language.setter
    def ocr_language(self, value: str):
        self._settings["ocr_language"] = value
        self.save()
    
    @property
    def last_selected_area(self) -> Optional[Dict[str, int]]:
        return self._settings.get("last_selected_area")
    
    @last_selected_area.setter
    def last_selected_area(self, value: Optional[Dict[str, int]]):
        self._settings["last_selected_area"] = value
        self.save()
    
    def reset(self):
        """Reset all settings to defaults (useful for testing)."""
        self._settings = DEFAULT_SETTINGS.copy()
        self.save()
