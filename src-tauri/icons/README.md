# Icons

`icon-source.svg` is the master (signage red plate, a cutaway building, one lit "needs you" window).
`icon-source.png` is its 1024×1024 render with a transparent background (rendered in headless Chromium with `omitBackground`; `qlmanage` paints white corners). Regenerate the set after editing it:

```bash
pnpm tauri icon src-tauri/icons/icon-source.png
```

Then delete the Android, iOS and Square*/StoreLogo outputs: macOS is the only target.
