/**
 * The kinds of things Thoric manages, each with a color of its own so a list or a menu says
 * what an item is before it is read. A concept's color is `--concept-<name>` in both themes of
 * `index.css` and its icon is in `CONCEPT_ICONS` (`components/concept.tsx`); the concepts test
 * checks that every one has both colors, reads on every surface and stays apart from the others.
 */
export const CONCEPTS = ["container", "image", "template", "volume", "mcp"] as const

export type Concept = (typeof CONCEPTS)[number]

/** The CSS color of a concept, for a style. */
export const conceptColor = (concept: Concept) => `var(--concept-${concept})`
