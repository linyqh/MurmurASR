"""Finder layout for the Murmur installer (coordinates are logical points)."""
from pathlib import Path

root = Path(defines["root"]).resolve()
application = Path(defines["app"]).resolve()

format = "UDZO"
filesystem = "HFS+"
files = [str(application)]
symlinks = {"Applications": "/Applications"}
# SetFile's extension flag adds FinderInfo to the app, invalidating codesign.
hide_extensions = []
hide = [".background.tiff", ".VolumeIcon.icns"]
background = str(root / "assets/dmg/background.tiff")
icon = str(root / "src-tauri/icons/icon.icns")

# Finder includes its 32-point title bar in these bounds.
window_rect = ((160, 160), (750, 557))
default_view = "icon-view"
show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
include_icon_view_settings = True
include_list_view_settings = False
arrange_by = None
grid_spacing = 99
scroll_position = (0, 0)
label_pos = "bottom"
text_size = 14
icon_size = 128
icon_locations = {
    application.name: (200, 260),
    "Applications": (550, 260),
    # Keep support files outside the canvas even if Finder shows hidden files.
    ".background.tiff": (1500, 1500),
    ".VolumeIcon.icns": (1500, 1600),
    ".DS_Store": (1500, 1700),
    ".fseventsd": (1500, 1800),
}
