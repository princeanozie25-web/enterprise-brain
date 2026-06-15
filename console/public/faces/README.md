# Portrait drop-in folder

This folder is **intentionally empty** of images. The console's `<PersonAvatar>`
component resolves portraits at render time with **zero code change** required:

- If `console/public/faces/<principal_id>.jpg` exists, it is shown.
- If it does not, `<PersonAvatar>` falls back to a designed **monogram** — the
  person's initials on a calm disc tinted by their department (from the reserved
  Aperture palette, colorblind-safe).

## Naming scheme

One square JPEG per principal, named by the **principal id** exactly as it
appears in `fixtures/people.json` / `fixtures/company.json`:

```
faces/p001.jpg
faces/p002.jpg
...
faces/p119.jpg
faces/p_void.jpg
```

- **120 human principals**: `p001`–`p119` plus `p_void`.
- Agents (`agent_*`) keep their emblem; they have no portrait.
- **Square**, roughly **256×256 px**, `.jpg`. Larger is fine (the component
  sizes them down); non-square will be center-cropped by `object-fit: cover`.

Drop a synthetic-face pack in here and every avatar across the console (Lens
masthead, diff passports, Atlas/Lane identity headers, and — once AR-2 lands —
the org graph and switcher) upgrades instantly. These must be **synthetic**
faces: Bryremead Distribution Ltd is fictional, and no portrait here may depict
a real person.

> This README is the only tracked file in the folder; it keeps the otherwise
> empty directory in version control until portraits are added.
