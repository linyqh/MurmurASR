# Murmur DMG background

Mode: built-in image_gen. Use case: ui-mockup (production installer background).

Create a finished, flat 2D raster background for the CONTENT AREA of a macOS Finder DMG installation window. Output exactly 1600 × 1120 pixels (displayed at 800 × 560 points, Retina). This is an actual installer background, not a screenshot or a mockup of a computer. Inspired by the user's reference: minimal, airy native macOS installer with app on the left and Applications on the right. Murmur's existing branding is icy white with bright blue glass ASR lettering, so use a restrained ice-blue/white palette and dark navy typography.

Full-bleed very pale blue-white background. Subtle translucent liquid-glass wave/ribbon in the upper-right corner and a faint blue glow near the lower corners, never under the text. Premium, calm, crisp, highly readable Chinese typography. No window chrome, title bar, traffic lights, borders, device frames, app icons, folder icons, extra file icons, buttons, QR codes, or watermark. No version number.

Layout in 1600 × 1120 coordinates:
- Top-left at (110, 84), small blue wordmark "Murmur".
- At (110, 150), large bold dark-navy Chinese headline exactly "让声音，成为文字。"
- At (110, 244), medium gray-blue instruction exactly "将 Murmur 拖入 Applications，即可安装"
- Middle: two very subtle rounded glass panels, left bounds x=220..620, y=360..760; right bounds x=980..1380, y=360..760. Leave both panels COMPLETELY EMPTY including no placeholder labels. The real draggable Finder app and folder icons will be placed by software, centered at x=420 and x=1180, icon center y=535, with native file labels below. Do not draw those icons or labels into the artwork.
- Between the empty panels, a refined horizontal blue waveform smoothly becoming a right-facing arrow, centered around (800, 535), extending x=690..910. Keep it visually light.
- Below the panels at y=825, a centered single line of readable medium-size Chinese feature text exactly "文件转写  ·  实时听写  ·  字幕导出"
- Bottom informational callout, gently blue-tinted rounded rectangle bounds x=110..1490, y=895..1050. Left-aligned inside at x=155, first line y=925, stronger dark-blue text exactly "首次使用：请在应用内下载语音模型"
- Second line y=982, smaller but legible gray-blue text exactly "安装包不含模型 · 下载完成后可离线转写 · 音频在本机处理"

Preserve the exact wording and punctuation. The bottom explanation is important and must be fully legible, not tiny. Spacious alignment, subtle material detail, no heavy decoration. The only text is the specified five text blocks plus Murmur wordmark. Empty icon panels are essential for real interactive Finder icons.
