# Interface design

[Back to the README](../README.md)

Bilskirnir's appearance draws on the Asgard ships and interiors that inspired the project: dark metal, pale blue lights and a little violet. That influence belongs in the surfaces and details. The interface still needs to feel like a useful desktop manager, with readable previews and controls you can find quickly.

## What the interface should communicate

A user should be able to see which desktops are running, open one, and understand whether an operation succeeded. Make those actions and states easy to recognize before adding decoration.

Use familiar names: **Environments**, **Images**, **Volumes** and **Jobs**. Add explanatory text only when it helps someone make a decision or understand a result. A snapshot dialog should explain what is saved; a header does not need to explain that the app is a local workspace manager.

Keep visible copy in English. Errors should say what failed and, when possible, what the user can do next. Successful jobs should describe the outcome rather than display an unexplained ID.

## Color

Use the shared tokens in `web/style.css` rather than introducing slightly different colors for each component.

| Token | Value | Use |
| --- | --- | --- |
| `--bg` | `#070b12` | Page background |
| `--sidebar` | `#0a101a` | Navigation background |
| `--panel` | `#101925` | Cards and panels |
| `--panel-raised` | `#162232` | Raised surfaces |
| `--input` | `#0a121e` | Inputs and code backgrounds |
| `--line` | `#2b3b50` | Borders and separators |
| `--line-bright` | `#49657f` | Hovered or emphasized borders |
| `--text` | `#e4edf7` | Main text |
| `--muted` | `#a0afc3` | Supporting text |
| `--accent` | `#92d5f3` | Primary actions, focus and active states |
| `--accent-soft` | `#142c40` | Primary button backgrounds |
| `--violet` | `#aaa0ed` | Image relationships and badges |
| `--violet-soft` | `#26213d` | Image badge backgrounds |
| `--warning` | `#e3c88f` | Pending or paused states |
| `--danger` | `#f3a0ad` | Errors and destructive actions |
| `--success` | `#a9dfcd` | Successful outcomes |

Blue draws attention to things the user can act on. Violet helps identify images and their relationships. Always pair a status color with text or a recognizable shape. Keep background lighting subtle and leave remote desktop screenshots in their original colors.

## Type and spacing

Use the system UI font stack: Segoe UI Variable, Segoe UI and Arial. Logs, image references and metrics use Cascadia Code, SFMono-Regular or Consolas. There is no downloaded font dependency.

Page titles are 30 px, section titles 16 px, card titles 15 px, body text 14 px and supporting labels 12 px. Use a 4 px spacing grid: usually 8–12 px within controls, 18–22 px inside cards and 24–36 px between page sections.

Most panels have 6 px corners; cards and dialogs use 8 px. Small badges and preview frames use 3 px. Keep related elements close enough that their relationship is obvious.

## Components

The desktop navigation is 232 px wide. The pendant-shaped hammer mark follows the supplied reference with a peaked head, tapered stem, flared foot, simple spirals and a blue inset. Active navigation has a blue rail and a stronger background.

Page headers need a title and relevant actions. Metrics should be easy to scan, without competing with environment previews.

Environment cards show the desktop preview, name, state, profile, resources and actions. Preserve the full screenshot instead of cropping away parts of the desktop. Hover can strengthen the border but should not move the card.

Image cards use a violet upper border. Parent relationships, descriptions and dates help users decide which image to start from.

Primary buttons use the blue accent; destructive buttons use muted red. Normal controls are at least 40 px high, with 34 px controls in the compact desktop toolbar. The toolbar should leave as much room as possible for the remote desktop.

Dialogs use recessed inputs and a visible header separator. Explain consequences beside the relevant control: whether a file will be replaced, what a snapshot excludes or why a script runs as root.

File uploads use a dashed drop target and a compact queue with progress and explicit **Uploaded**, **Failed** or **Cancelled** results. Keep completed outcomes visible and make retry available for unfinished files. Do not offer read-only destinations.

## Small screens and accessibility

At 1100 px the navigation and page spacing narrow. Below 700 px navigation becomes a horizontal row, metrics use two columns and environment cards use one column. Below 400 px forms use one column and main page actions can fill the available width.

Keep keyboard focus visible, give icon buttons accessible names and preserve dialog focus handling. Decorative SVGs use `aria-hidden`. Sign-out must remain reachable on narrow screens.

Check text contrast against the actual surface, not just the page background. Respect reduced-motion and reduced-transparency preferences. A narrow viewport should not need horizontal scrolling to reach a form field or action.

## Files to change

- `web/style.css` holds colors, spacing, component styles and responsive rules.
- `web/icons.mjs` supplies the shared 24 px stroke icons.
- `web/mark.svg` is the product mark and favicon.
- `web/app.js` contains the interface and its copy.

The references guide the design; CSS and SVG provide the application artwork. Keep the bundle self-contained and verify changes in the live browser before updating screenshots.
