# Murmur app icon

Approved design: the user's attached white tile with blue ASR ligature (concept 02 in the saved numbered set).

- `asr-source.png`: built-in image_gen edge-cleanup pass, retaining the approved lettermark. The generated rim still needs the geometric alpha cleanup below.
- `app-icon.png`: final 1024×1024 RGBA master, with an opaque rounded tile and genuinely transparent exterior. No checkerboard, rectangular matte, exterior glow, or stray pixels.
- `../../src-tauri/icons/`: application PNG sizes, Windows ICO and macOS ICNS generated from this master. Existing Tauri bundle references consume these files.

Regenerate with `node scripts/generate-icons.cjs` from the project root. The script requires the `sharp` Node package (install separately, or expose an existing installation via `NODE_PATH`). It crops the generated image's irregular perimeter, makes the tile interior opaque, applies an antialiased geometric rounded-rectangle mask, and centers an 832×832 tile within the 1024×1024 canvas. Colour and alpha are downsampled separately so sharp lettering does not introduce faint alpha ringing in the transparent margin. Rounded corners are stored in the alpha channel, independently of operating-system masking. Do not flatten the master against white or gray when exporting.

Windows ICO contains 16, 20, 24, 32, 40, 48, 64, 96, 128 and 256 px RGBA frames. macOS ICNS contains standard and Retina representations up to 1024 px. All package formats retain the transparent corners.

Visual QA: `../../output/icon-concepts/2026-10-15/selected-icon-preview.png` compares light/dark backgrounds at 320, 128 and 32 px. macOS `iconutil` successfully decoded the final ICNS; PNG, ICO and decoded ICNS frame dimensions and alpha were also checked. Windows desktop rendering has not been tested on a Windows machine.

## Built-in image_gen edit prompt

Use case: background-extraction.
Asset type: production desktop application icon source, macOS and Windows.
Input images: Image 1 is the edit target, the approved white tile with blue connected ASR lettering.
Primary request: Clean up only the outside edge and background transparency of this approved app icon. Remove every stray white, gray or blue pixel, noisy fringe, accidental protrusion, ghost border and surrounding artifact outside the rounded-square tile. Give the tile a perfectly clean, continuous, symmetric rounded-square silhouette and genuinely transparent corners and surrounding canvas, with no gray/white rectangular matte.
Constraints / invariants: preserve the exact blue ASR connected lettering, letter shapes, proportions, position, optical size, blue color and glass material. Preserve the approved soft white / very pale blue interior tile and its original subtle inner rim. The white tile itself is part of the design and stays opaque. Change only exterior cleanup and edge regularity. One single front-facing square icon, centered with even transparent margins, no caption, no checkerboard baked into the image, no shadow or glow outside the tile. No redesign, no new symbols.
