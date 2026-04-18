# Pasta Smoke Test Checklist

Use this checklist after each Linux UI-focused phase so we can verify behavior stayed intact and keep the MVP target honest.

## Automated Baseline

Run:

```bash
cargo test
```

Current baseline:

- 41 tests passing on 2026-04-11

## Manual Smoke Pass

### Launcher

- Press `Meta + Space` to show the launcher.
- Press `Escape` to hide it.
- Press `Meta + Space` again repeatedly to ensure show/hide does not get stuck.
- Confirm the launcher can both open and hide via `Meta + Space` without getting stuck in an intermediate transition.

### Tray

- Confirm a tray/status icon appears in the host bar (`waybar`, KDE tray, or equivalent).
- Hover the icon and confirm a tooltip is shown and clearly identifies Pasta.
- Click the tray icon if supported and confirm it does not crash or wedge the app.

### Search

- Type a short query and confirm results filter.
- Backspace quickly through the query and confirm results recover.
- Type a tag-only search with `/tag`.

### Navigation

- Move selection with `Up` and `Down`.
- Move selection with `Ctrl+J`, `Ctrl+K`, `Ctrl+L`, and `Ctrl+;`.
- Confirm scrolling follows the selected row.

### Clipboard Actions

- Press `Enter` on a normal item and confirm it copies.
- Click a row and confirm current click behavior still matches expectations.
- Delete an item with `Delete` or `Ctrl+Backspace`.

### Secret Flow

- Select a secret item and reveal it with `Enter` or `Ctrl+R`.
- Confirm the current Linux auth behavior matches expectations for this build.
- Copy a revealed secret and confirm auto-clear still behaves as expected.

### Editors

- Open info editor with `Ctrl+I`, type, save, and cancel.
- Open tag editor with `Ctrl+T`, type, save, and cancel.
- Open remove-tags flow with `Ctrl+Shift+T`.
- Open parameter editor with `Ctrl+P`.
- Open parameter fill flow by copying a parameterized item.

### Transforms

- Open transforms with `Tab`.
- Run at least one encode transform and one decode transform.
- Exit transforms with `Tab` or `Escape`.

### Visual Pass

- Check the launcher in light mode.
- Check the launcher in dark mode.
- Verify hover, selected row, borders, editor panels, and tag chips remain clearly visible.
