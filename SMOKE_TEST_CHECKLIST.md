# Pasta Smoke Test Checklist

Use this checklist after each UI-focused phase so we can verify behavior stayed intact.

## Automated Baseline

Run:

```bash
cargo test
```

Current baseline:

- 23 tests passing on 2026-03-26

## Manual Smoke Pass

### Launcher

- Press `Option + Space` to show the launcher.
- Press `Escape` to hide it.
- Press `Option + Space` again repeatedly to ensure show/hide does not get stuck.

### Search

- Type a short query and confirm results filter.
- Backspace quickly through the query and confirm results recover.
- Type a tag-only search with `/tag`.

### Navigation

- Move selection with `Up` and `Down`.
- Move selection with `Cmd+J`, `Cmd+K`, `Cmd+L`, and `Cmd+;`.
- Confirm scrolling follows the selected row.

### Clipboard Actions

- Press `Enter` on a normal item and confirm it copies.
- Click a row and confirm current click behavior still matches expectations.
- Delete an item with `Delete` or `Cmd+Backspace`.

### Secret Flow

- Select a secret item and reveal it with `Enter` or `Cmd+R`.
- Confirm Touch ID gating still works.
- Copy a revealed secret and confirm auto-clear still behaves as expected.

### Editors

- Open info editor with `Cmd+I`, type, save, and cancel.
- Open tag editor with `Cmd+T`, type, save, and cancel.
- Open remove-tags flow with `Cmd+Shift+T`.
- Open parameter editor with `Cmd+P`.
- Open parameter fill flow by copying a parameterized item.

### Transforms

- Open transforms with `Tab`.
- Run at least one encode transform and one decode transform.
- Exit transforms with `Tab` or `Escape`.

### Visual Pass

- Check the launcher in light mode.
- Check the launcher in dark mode.
- Verify hover, selected row, borders, editor panels, and tag chips remain clearly visible.
