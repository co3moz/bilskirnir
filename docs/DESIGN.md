# Interface design

[Back to the README](../README.md)

Bilskirnir's appearance draws on the Asgard ships and interiors that inspired the project: dark metal, pale blue lights and a little violet. That influence belongs in the colors and a few details. The interface itself follows the shadcn/ui `dashboard-01` block, so it reads like a familiar admin dashboard: readable previews, tables you can scan and controls you can find quickly.

## What the interface should communicate

A user should be able to see which desktops are running, open one, and understand whether an operation succeeded. Make those actions and states easy to recognize before adding decoration.

Use familiar names: **Dashboard**, **Environments**, **Images**, **Volumes** and **Jobs**. Add explanatory text only when it helps someone make a decision or understand a result. A snapshot dialog should explain what is saved; a header does not need to explain that the app is a local workspace manager.

Keep visible copy in English. Errors should say what failed and, when possible, what the user can do next. Successful jobs should describe the outcome rather than display an unexplained ID; a job names the environment or profile it worked on.

## Layout

The layout is `dashboard-01`, filled with Bilskirnir's own data:

- **Sidebar** (inset variant, 18 rem): the hammer mark and name, **New environment** with a hammer button for **Build image**, the five pages, a **Running desktops** group where the block lists documents, links to the documentation and agent access, and the account menu with the theme choice and sign-out. It collapses off screen with the header button or Ctrl+B, and is a sheet below 768 px.
- **Site header**: the sidebar button, the page title, the Docker connection and the page's main action.
- **Dashboard**: four summary cards, the job activity chart and the environments table, in that order, as in the block.
- **Environments**: one card per desktop with its preview; **Images**: profile cards above the image table; **Volumes** and **Jobs**: tables.
- **Desktop view**: full screen without the sidebar. A 48 px toolbar leaves as much room as possible for the remote desktop.

Pages live in the URL fragment (`#/images`, `#/desktop/<id>`) so they survive a reload and can be bookmarked.

## Color

Colors are the shadcn theme variables in `web/src/index.css`, with a light and a dark set. Use the variables and their Tailwind utilities (`bg-card`, `text-muted-foreground`, `text-primary`) rather than introducing slightly different colors for each component. Dark is the default; the account menu offers light and system as well.

| Variable | Dark | Light | Use |
| --- | --- | --- | --- |
| `--sidebar` | `#070b12` | `#f2f5f9` | Sidebar and the frame around the inset page |
| `--background` | `#0b111b` | `#ffffff` | Page surface |
| `--card` | `#0f1722` | `#ffffff` | Cards and panels |
| `--popover` | `#111b28` | `#ffffff` | Dialogs, drawers and menus |
| `--muted` | `#1b293c` | `#edf2f7` | Table headers, selected toggles, quiet backgrounds |
| `--border` | `#223146` | `#dae2eb` | Borders and separators |
| `--input` | `#2b3b50` | `#cbd6e2` | Input borders |
| `--foreground` | `#e4edf7` | `#0d1726` | Main text |
| `--muted-foreground` | `#a0afc3` | `#556779` | Supporting text |
| `--primary` | `#92d5f3` | `#1b6f9a` | Primary actions, focus and active states |
| `--violet` | `#aaa0ed` | `#6152c4` | Images and their relationships |
| `--warning` | `#e3c88f` | `#96660f` | Starting, pending or paused states |
| `--destructive` | `#f3a0ad` | `#c42b45` | Errors and destructive actions |
| `--success` | `#a9dfcd` | `#1d7f60` | Running desktops and successful outcomes |
| `--chart-1` to `--chart-5` | blue, violet, green, amber, red | the same hues, darker | Chart series |

Blue draws attention to things the user can act on. Violet identifies images and their relationships. Always pair a status color with text and an icon: `StatusBadge` does this for every state. Keep background lighting subtle (a faint blue glow at the top of the dark page) and leave remote desktop screenshots in their original colors.

## Type and spacing

The interface uses Geist and Geist Mono, bundled from Fontsource packages so nothing loads from the network. Logs, image references, volume names and container IDs use the mono face. Sizes follow the shadcn components: 14 px body text, 12 px supporting text, 16 px page titles in the header and larger numbers only on summary cards.

The radius is 8 px (`--radius: 0.5rem`); the components derive smaller and larger corners from it. Pages use the block's spacing: 16 px side padding, 24 px from 1024 px, and 16 to 24 px between sections.

## Components

Components come from shadcn/ui (Radix base, Nova style) in `web/src/components/ui/` and are project code once added. Prefer an existing component to new markup, and new variants to one-off classes.

- **Summary cards** show a number, a badge and two lines of context. Use honest numbers from the API; there is no trend data, so do not invent trends.
- **Tables** use `web/src/components/data-table.tsx`: the block's view tabs (a select on narrow pages), the columns menu, row selection where bulk actions exist, sorting on columns where order matters, and pagination that stays on its page while the list refreshes.
- **Detail drawers** open from a table row, from the right, or from the bottom on a phone, as the block's row viewer does. The environment drawer holds the preview, the configuration and every action; the job drawer follows the log while the job runs.
- **Environment cards** show the desktop preview, name, state, profile, resources and actions. Preserve the full screenshot instead of cropping away parts of the desktop. Hover can strengthen the border but should not move the card.
- **Profile cards** have a blue top border; image rows use violet icons and an indented tree for parent relationships.
- **Dialogs** open one at a time through `web/src/lib/dialogs.ts`. Explain consequences beside the relevant control: whether a file will be replaced, what a snapshot excludes, why a script runs as root, or which setting restarts the desktop. Destructive confirmations use an alert dialog with a destructive button.
- **File uploads** use a dashed drop target and a compact queue with progress and explicit **Uploaded**, **Failed** or **Cancelled** results. Keep completed outcomes visible and make retry available for unfinished files. Do not offer read-only destinations.

## Small screens and accessibility

Below 768 px the sidebar becomes a sheet, drawers open from the bottom and the chart shows the last 7 days. Summary cards and environment cards use container queries: one column on a phone, two on a tablet and four or three on a wide page. Tables scroll inside their frame. A narrow viewport must not scroll the page sideways to reach a form field or action.

Keep keyboard focus visible, give icon buttons accessible names and keep the Radix dialog focus handling. Icons are decorative unless they are the only content of a control. Sign-out must remain reachable on narrow screens. Check text contrast in both themes against the actual surface, not just the page background. Animations are cut short when the system asks for reduced motion.

## Files to change

- `web/src/index.css` holds the theme variables, fonts and the few global rules.
- `web/src/components/` holds the layout pieces adapted from `dashboard-01` (`app-sidebar.tsx`, `site-header.tsx`, `section-cards.tsx`, `chart-area-interactive.tsx`, `data-table.tsx`) and Bilskirnir's own components; `ui/` holds the shadcn components.
- `web/src/pages/` holds one file per page, `web/src/components/dialogs/` the dialogs and drawers.
- `web/public/mark.svg` is the product mark and favicon.

Verify interface changes in the live browser, in both themes and at a narrow width, before updating screenshots.
