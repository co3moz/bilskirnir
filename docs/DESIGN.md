# Interface design

[Back to the README](../README.md)

Bilskirnir's appearance draws on the Asgard ships and interiors that inspired the project: dark neutral metal, with color kept for what it means. That influence belongs in the colors, the type and a few details. The interface itself follows the shadcn/ui `dashboard-01` block, so it reads like a familiar admin dashboard: readable previews, tables you can scan and controls you can find quickly.

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
| `--sidebar` | `oklch(0.205 0 0)` | `oklch(0.985 0 0)` | Sidebar and the frame around the inset page |
| `--background` | `oklch(0.145 0 0)` | `oklch(1 0 0)` | Page surface |
| `--card` | `oklch(0.205 0 0)` | `oklch(1 0 0)` | Cards and panels |
| `--popover` | `oklch(0.205 0 0)` | `oklch(1 0 0)` | Dialogs, drawers and menus |
| `--muted` | `oklch(0.269 0 0)` | `oklch(0.97 0 0)` | Table headers, selected toggles, quiet backgrounds |
| `--border` | white at 10% | `oklch(0.922 0 0)` | Borders and separators |
| `--input` | white at 15% | `oklch(0.922 0 0)` | Input borders and field backgrounds |
| `--foreground` | `oklch(0.985 0 0)` | `oklch(0.145 0 0)` | Main text |
| `--muted-foreground` | `oklch(0.708 0 0)` | `oklch(0.556 0 0)` | Supporting text |
| `--primary` | `oklch(0.922 0 0)` | `oklch(0.205 0 0)` | Primary actions, focus and active states |
| `--violet` | `#aaa0ed` | `#6152c4` | Images and their relationships |
| `--warning` | `#e3c88f` | `#96660f` | Starting, pending or paused states |
| `--destructive` | `oklch(0.704 0.191 22.216)` | `oklch(0.577 0.245 27.325)` | Errors and destructive actions |
| `--success` | `#a9dfcd` | `#1d7f60` | Running desktops and successful outcomes |
| `--chart-1` to `--chart-5` | greys, light to dark | the same greys | Neutral chart series |

The surfaces are the neutral palette of the shadcn preset, without a blue or warm cast. The primary color is near white on dark and near black on light, and it marks what the user can act on. Hue is kept for meaning: violet identifies images and their relationships, and green, amber and red carry states. Always pair a status color with text and an icon: `StatusBadge` does this for every state. A chart whose series are outcomes uses those colors too: the job activity chart draws completed jobs in `--primary` and failed ones in `--destructive`, because two greys cannot be told apart on a dark card. There is no background lighting; leave remote desktop screenshots in their original colors.

## Type and spacing

The interface uses Oxanium for text and Geist Mono for code, both bundled from Fontsource packages so nothing loads from the network. Logs, image references, volume names and container IDs use the mono face. Sizes follow the shadcn components: 14 px body text, 12 px supporting text, 16 px page titles in the header and larger numbers only on summary cards.

The radius is 10 px (`--radius: 0.625rem`); the components derive smaller and larger corners from it. In the Maia style buttons, inputs and badges are fully rounded and cards use the 2xl corner. Pages use the block's spacing: 16 px side padding, 24 px from 1024 px, and 16 to 24 px between sections.

## Components

Components come from shadcn/ui (Radix base, Maia style) in `web/src/components/ui/` and are project code once added. Prefer an existing component to new markup, and new variants to one-off classes. Icons come from Remix Icon (`@remixicon/react`), the preset's icon library: use the `Line` icons, and a `Fill` icon only where the filled shape carries the meaning, as the success state does.

- **Summary cards** show a number, a badge and two lines of context. Use honest numbers from the API; there is no trend data, so do not invent trends.
- **Tables** use `web/src/components/data-table.tsx`: the block's view tabs (a select on narrow pages), the columns menu, row selection where bulk actions exist, sorting on columns where order matters, and pagination that stays on its page while the list refreshes.
- **Detail drawers** open from a table row, from the right, or from the bottom on a phone, as the block's row viewer does. The environment drawer holds the preview, the configuration and every action; the job drawer follows the log while the job runs.
- **Environment cards** show the desktop preview, name, state, profile, resources and actions. Preserve the full screenshot instead of cropping away parts of the desktop. Hover can strengthen the border but should not move the card.
- **Profile cards** have a top border in the primary color; image rows use violet icons and an indented tree for parent relationships.
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
- `components.json` records the shadcn preset (style, base color, icon library), so components added later with the shadcn CLI match the rest.

Verify interface changes in the live browser, in both themes and at a narrow width, before updating screenshots.
