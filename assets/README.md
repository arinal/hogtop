# Brand assets

## Logo

The mark is a terminal **caret peak** (`^` = "top" of the process list) whose
flared tips wink at a pig's ears (the *hog* in "top hog"), beside a **block
cursor** dot. Sleek/utilitarian: it reads as a terminal tool first, hog second.

| File | Use |
|------|-----|
| `logo-full.png`   | 1024² master, full canvas (source of truth) |
| `icon-256.png`    | README header logo (rendered inline at ~38px) |

Need other sizes (favicons, app-bundle icons, social card)? Regenerate them
from the master — the mark sits in a centered 760² crop:

```sh
for s in 16 32 48 180 256 512 1024; do
  magick logo-full.png -crop 760x760+132+132 +repage -resize ${s}x${s} icon-$s.png
done
```

## Palette

Pulled from the TUI accent colors.

| Role            | Hex       |
|-----------------|-----------|
| Primary violet  | `#a78bfa` |
| Light lilac     | `#c4b5fd` |
| Magenta accent  | `#d8b4fe` |
| Charcoal bg     | `#1b1726` |

## Other

| File | Use |
|------|-----|
| `screenshot.png` | TUI screenshot for the README |
