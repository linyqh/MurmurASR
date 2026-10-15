# Murmur installer background

Generated with the built-in **image_gen** tool. The complete generation prompt is
in [prompt.md](prompt.md). The source image is `background@2x.png` (1499 × 1049 px).
`background.tiff` contains the same artwork with 144 DPI metadata, so Finder
displays it at approximately 750 × 525 logical points on Retina and standard displays.

The artwork explains drag-to-install, file transcription, live dictation, subtitle
export, and the first-run model download. App and Applications icons are real
Finder items, not painted into the background.

Regenerate the Finder image representation without changing the artwork:

```bash
sips -s format tiff -s dpiWidth 144 -s dpiHeight 144 \
  assets/dmg/background@2x.png --out assets/dmg/background.tiff
```

`scripts/dmg-settings.py` defines the 750 × 557-point window (including the title
bar), icon positions, hidden support files, and the background reference.
`scripts/create-dmg.sh` builds this layout using pinned dmgbuild dependencies in
the ignored `src-tauri/target/dmg-tools` environment. It writes Finder metadata
directly and does not require Finder automation or AppleScript permission.

To rebuild only the installer from an already signed and stapled application:

```bash
bash scripts/create-dmg.sh \
  src-tauri/target/release/bundle/macos/Murmur.app /path/to/new-installer.dmg
```

That helper only creates the disk image; the new image still needs signing and
notarization. Use `scripts/package-macos.sh` for the complete release workflow.
