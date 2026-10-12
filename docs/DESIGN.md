# Bilskirnir design system

## Direction

The references suggest dark orbital space, substantial metal hulls, pale blue illumination and violet architectural details. Bilskirnir translates that atmosphere into a calm control interface: graphite surfaces, distinct panel boundaries, small illuminated edges and concise English labels.

The ship exteriors inform the steel-blue panels and angular brand mark. Blue engine light informs primary actions and active navigation. The interior reference informs the violet accent and etched header lines. Its warm illumination appears only in pending and warning states.

## Palette

| CSS token | Value | Role |
| --- | --- | --- |
| `--bg` | `#070b12` | Deep-space workspace background |
| `--sidebar` | `#0a101a` | Navigation shell |
| `--panel` | `#101925` | Main metal panels |
| `--panel-raised` | `#162232` | Raised surfaces |
| `--input` | `#0a121e` | Recessed input and code surfaces |
| `--line` | `#2b3b50` | Panel seams and separators |
| `--line-bright` | `#49657f` | Interactive boundaries |
| `--text` | `#e4edf7` | Primary text |
| `--muted` | `#a0afc3` | Secondary text |
| `--accent` | `#92d5f3` | Ice-blue focus, active controls and running state |
| `--accent-soft` | `#142c40` | Primary button surface |
| `--violet` | `#aaa0ed` | Image lineage and secondary instrumentation |
| `--violet-soft` | `#26213d` | Image-type badges |
| `--warning` | `#e3c88f` | Pending and paused states |
| `--danger` | `#f3a0ad` | Errors and destructive actions |

Blue indicates activity and interaction; violet groups image-related information. Status labels and shapes communicate state alongside color. The background gradients stay subtle, and desktop previews retain their original colors.

## Typography and spacing

- System UI typography: Segoe UI Variable / Segoe UI / Arial. No network font dependency.
- Monospace typography: Cascadia Code / SFMono-Regular / Consolas for metrics, image references and logs.
- Desktop page title: 30 px; section title: 16 px; card title: 15 px; body: 14 px; secondary labels: 12 px.
- A 4 px spacing grid, with 8–12 px within controls, 18–22 px inside cards and 24–36 px around sections.
- Panel corners: 6 px. Environment cards and dialogs: 8 px. Chips and preview frames: 3 px.

## Components

**Navigation:** a fixed 232 px shell, a faceted hammer mark and four SVG navigation icons. Active navigation uses a thin blue rail, brighter text and a contained blue surface.

**Page headers:** one title, the relevant actions and a short illuminated rule. Copy uses direct names such as Environments, Images, Volumes and Jobs.

**Metrics:** four distinct metal panels. Monospace values and short labels provide hierarchy; small blue/violet edge details connect them to the visual references.

**Environment cards:** a recessed desktop display, name and state, desktop profile, resource chips, internet switch and actions. The preview frame preserves the full screenshot. Hover strengthens the frame without moving the card.

**Image cards:** violet upper seams distinguish the image library. Parent relationships, dates and user descriptions retain their existing roles.

**Primary controls:** a deep-blue metal fill, pale text and a restrained illuminated border. Destructive controls use muted red. Buttons have a minimum height of 40 px; compact toolbar controls use 34 px.

**Forms and dialogs:** recessed inputs, an etched header separator, a visible focus ring and native dialog focus management. Descriptions explain decisions such as snapshot exclusions and root script execution.

**Tables and logs:** quiet panel framing, monospace table headings, subtle row hover and a darker log surface. Notifications use a colored edge with an explicit message.

**Desktop toolbar:** the same metal surfaces and blue action states. The remote desktop remains a functional canvas with its original application colors.

## Responsive behavior and accessibility

At 1100 px, the sidebar and workspace spacing narrow. At 700 px, navigation becomes a horizontal row, metrics use two columns and environment cards use one column. At 400 px, forms use one column and primary page actions can span the full width.

Interactive controls preserve keyboard focus indicators and descriptive labels. Icons are decorative SVGs with `aria-hidden`; text supplies their accessible names. Sign-out remains available on narrow screens. Text and status colors are chosen for contrast on their actual surfaces. Reduced-motion preferences remove transitions; reduced-transparency preferences simplify the dialog backdrop.

## Implementation

- `web/style.css`: shared tokens, surfaces, component rules and responsive behavior.
- `web/icons.mjs`: consistent 24 px stroke icons.
- `web/mark.svg`: the faceted Bilskirnir hammer mark and favicon.
- `web/app.js`: existing interactions and concise English copy.

Reference images guide the visual language. The application artwork is implemented with CSS and SVG, keeping the bundle self-contained.
